//! `--medir`: times the work the app does around its window, without opening one. The same steps
//! run before and after a change, so the numbers can be compared (see `perf.rs`).

pub mod perf;

use crate::backend::{self, Config};
use crate::{domain, wine_setup};
use std::path::Path;

/// `scratch` is where the log-writing step puts its file; `with_wine` also runs the steps that
/// start Wine's own programs (`wine --version`, `wineserver -k0`).
pub fn run(scratch: &Path, with_wine: bool) {
    let started = std::time::Instant::now();
    let cfg = perf::timed("leer config.json", 5, Config::load);
    let bottles = perf::timed("detectar botellas y sus programas", 5, || {
        backend::scan_bottles(&cfg)
    });
    let programs: usize = bottles.iter().map(|b| b.programs.len()).sum();
    eprintln!("  {} botellas · {programs} programas", bottles.len());
    for (i, b) in bottles.iter().enumerate() {
        perf::timed(
            &format!(
                "  biblioteca de la botella {} ({} programas)",
                i + 1,
                b.programs.len()
            ),
            5,
            || backend::scan_programs(&b.path, cfg.bottle_ref(&b.path)),
        );
    }
    perf::timed(
        "estado de los motores",
        5,
        domain::engines::EngineStatus::read,
    );

    if with_wine {
        let state = perf::timed("buscar Wine (arranca wine --version)", 1, || {
            wine_setup::detect(&cfg, &crate::backend::Log::default())
        });
        let wine = state.path();
        let pairs: Vec<_> = bottles
            .iter()
            .filter_map(|b| {
                crate::app::bottle_engine_path(&cfg, b)
                    .or_else(|| wine.clone())
                    .map(|w| (b, w))
            })
            .collect();
        let open = perf::timed(
            &format!(
                "comprobar qué botellas están en uso ({} botellas)",
                pairs.len()
            ),
            5,
            || {
                pairs
                    .iter()
                    .filter(|(b, w)| backend::is_running_at(w, &b.path))
                    .count()
            },
        );
        eprintln!("  {open} en uso");
    }

    let icons: std::collections::BTreeSet<_> = bottles
        .iter()
        .flat_map(|b| b.programs.iter().filter_map(|p| p.icon.clone()))
        .collect();
    let decoded = perf::timed(
        &format!("decodificar {} imágenes de programas", icons.len()),
        1,
        || {
            icons
                .iter()
                .filter(|p| crate::app::decode_icon(p).is_some())
                .count()
        },
    );
    eprintln!("  {decoded} decodificadas");

    let _ = std::fs::create_dir_all(scratch);
    let file = scratch.join(format!("registro-medir-{}.txt", std::process::id()));
    perf::timed("escribir 20000 líneas en el registro", 3, || {
        let _ = std::fs::remove_file(&file);
        backend::log_bench(&file, 20_000)
    });
    let _ = std::fs::remove_file(&file);
    let _ = std::fs::remove_file(file.with_file_name("registro.anterior.txt"));

    let before = perf::process_usage();
    for _ in 0..200 {
        std::hint::black_box(backend::scan_bottles(&cfg));
        std::hint::black_box(domain::engines::EngineStatus::read());
    }
    if let (Some((rss0, cpu0)), Some((rss1, cpu1))) = (before, perf::process_usage()) {
        eprintln!(
            "200 detecciones seguidas: memoria {:.1} → {:.1} MB · procesador {:.2} s",
            rss0 as f64 / 1024.0,
            rss1 as f64 / 1024.0,
            cpu1 - cpu0
        );
    }
    eprintln!("total {:.2} s", started.elapsed().as_secs_f64());
}
