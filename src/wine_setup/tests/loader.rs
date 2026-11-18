//! Tests of the loader script that gives each program its Direct3D layer.

use super::support::*;
use crate::backend::D3D12_PROGRAMS;
use crate::wine_setup::*;

/// Red Dead Redemption 2 needs Direct3D 12 (D3DMetal) and its launcher needs DXMT, in the
/// same bottle and started one by the other: the loader script gives each its own layer.
#[test]
fn loader_gives_each_program_its_layer() {
    let dir = std::env::temp_dir().join(format!("bl-loader-{}", std::process::id()));
    let on = [
        ("BOTELLA_CAPA", "/m/dxmt"),
        ("BOTELLA_D3DMETAL", "/m/d3dmetal"),
        ("BOTELLA_D3DMETAL_PROGRAMAS", D3D12_PROGRAMS),
    ];
    let game = [
        "/m/d3dmetal/wine",
        "/m/d3dmetal/external/libd3dshared.dylib",
    ];

    // The game, as its launcher starts it; its arguments arrive untouched.
    let got = loader_run(
        &dir,
        &on,
        &[
            r"C:\Program Files\Rockstar Games\Red Dead Redemption 2\RDR2.exe",
            "-uno",
            "dos tres",
        ],
    );
    assert_eq!(got[..2], game);
    assert_eq!(
        got[2..],
        [
            r"C:\Program Files\Rockstar Games\Red Dead Redemption 2\RDR2.exe",
            "-uno",
            "dos tres"
        ]
    );
    assert_eq!(loader_run(&dir, &on, &["rdr2"])[..2], game);
    assert_eq!(loader_run(&dir, &on, &["/juegos/Rdr2.EXE"])[..2], game);
    // The Last of Us Part II's launcher uses "./tlou-ii.exe", not a full Windows path.
    assert_eq!(loader_run(&dir, &on, &["./tlou-ii.exe"])[..2], game);
    assert_eq!(loader_run(&dir, &on, &["./tlou-ii-l.exe"])[..2], game);

    // Everything else keeps DXMT, also when the game itself starts it.
    let mut from_game = on.to_vec();
    from_game.push(("WINEDLLPATH_PREPEND", game[0]));
    from_game.push(("CX_APPLEGPTK_LIBD3DSHARED_PATH", game[1]));
    for other in [
        r"C:\Program Files\Rockstar Games\Launcher\Launcher.exe",
        r"C:\juegos\notrdr2.exe",
        r"C:\rdr2\crash.exe",
    ] {
        assert_eq!(
            loader_run(&dir, &from_game, &[other])[..2],
            ["/m/dxmt", ""],
            "{other}"
        );
    }

    // A program that ships Microsoft's Direct3D 12 runtime beside it (the Agility SDK's
    // `D3D12\D3D12Core.dll`) is a Direct3D 12 program, whatever its name: Resident Evil
    // Requiem, started by Steam with its full Windows path.
    let prefix = dir.join("botella");
    let requiem = prefix.join("drive_c/Program Files (x86)/Steam/steamapps/common/RE requiem");
    write_exe(&requiem.join("D3D12/D3D12Core.dll"), "MZ");
    write_exe(&requiem.join("re9.exe"), "MZ");
    let other = prefix.join("drive_c/Juegos/Otro");
    write_exe(&other.join("otro.exe"), "MZ");
    std::fs::create_dir_all(prefix.join("dosdevices")).unwrap();
    let _ = std::os::unix::fs::symlink("../drive_c", prefix.join("dosdevices/c:"));
    let mut in_bottle = on.to_vec();
    let prefix_text = prefix.to_string_lossy().to_string();
    in_bottle.push(("WINEPREFIX", &prefix_text));
    let started = loader_run(
        &dir,
        &in_bottle,
        &[
            r"C:\Program Files (x86)\Steam\steamapps\common\RE requiem\re9.exe",
            "-steam",
        ],
    );
    assert_eq!(started[..2], game);
    assert_eq!(started[3], "-steam");
    assert_eq!(
        loader_run(&dir, &in_bottle, &[r"c:\juegos\otro\otro.exe"])[..2],
        ["/m/dxmt", ""]
    );
    // A name alone, or a drive that does not exist, keeps DXMT.
    for name in ["re9.exe", r"Q:\x\re9.exe", r"C:\nada\re9.exe"] {
        assert_eq!(
            loader_run(&dir, &in_bottle, &[name])[..2],
            ["/m/dxmt", ""],
            "{name}"
        );
    }

    // Without Direct3D 12 added, the engine's layer is passed on as it came.
    let plain = [("WINEDLLPATH_PREPEND", "/m/dxmt")];
    assert_eq!(
        loader_run(&dir, &plain, &["RDR2.exe"])[..2],
        ["/m/dxmt", ""]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// An engine without Direct3D 12 added keeps Wine's own loader untouched.
#[test]
fn loader_is_refreshed_only_where_it_was_replaced() {
    let dir = std::env::temp_dir().join(format!("bl-refresh-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let wine = fake_engine(&dir.join("Motor DXMT"));
    assert!(!refresh_loader(&wine));
    let loader = dir.join("Motor DXMT/engine/lib/wine/x86_64-unix/wine");
    assert_eq!(std::fs::read_to_string(loader).unwrap(), "LOADER");
    let _ = std::fs::remove_dir_all(&dir);
}
