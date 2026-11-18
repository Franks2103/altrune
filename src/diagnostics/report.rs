//! The exported report: a plain-text summary of a diagnosis to share, with personal data hidden.

use super::analysis::Gathered;
use super::findings::Diagnosis;
use super::redact::Redactor;

/// A plain-text report to share. Personal data that is known is hidden; the report says so and
/// asks the reader to check it before sharing.
pub fn report(
    context: &[(&str, String)],
    d: &Diagnosis,
    g: &Gathered,
    redactor: &Redactor,
) -> String {
    let mut s = String::from("Informe de Altrune\n==================\n\n");
    s.push_str(
        "Se ocultaron los datos personales que la app conoce (carpeta de usuario, correos,\nnúmeros de cuenta de Steam, claves y cookies). Léelo antes de compartirlo.\n\n",
    );
    for (k, v) in context {
        s.push_str(&format!("{k}: {}\n", redactor.apply(v)));
    }
    s.push_str("\nQué se analizó\n--------------\n");
    if d.lines == 0 {
        s.push_str("Ningún mensaje: no se abrió ningún programa de esta botella desde la app.\n");
    } else {
        s.push_str(&format!(
            "{} líneas de {} (solo el último intento en esta botella).\n",
            d.lines,
            d.sources_label()
        ));
    }
    s.push_str("\nProblemas reconocidos\n---------------------\n");
    if d.findings.is_empty() {
        s.push_str("Ninguno de los problemas conocidos aparece en lo que se analizó. Eso no quiere decir que todo esté bien.\n");
    }
    for f in &d.findings {
        s.push_str(&format!(
            "* {} [{}] — {} (fuente: {}, {} {})\n",
            f.title,
            f.code,
            f.confidence.label(),
            f.source.label(),
            f.matches,
            if f.matches == 1 { "línea" } else { "líneas" }
        ));
        for e in &f.evidence {
            s.push_str(&format!("  Se vio: {}\n", redactor.apply(e)));
        }
        for c in &f.checked {
            s.push_str(&format!("  Se comprobó: {c}\n"));
        }
        s.push_str(&format!(
            "  Causa probable: {}\n  Qué probar: {}\n",
            f.explanation, f.suggestion
        ));
    }
    let tail = |s: &mut String, title: &str, lines: &[String], n: usize| {
        if lines.is_empty() {
            return;
        }
        s.push_str(&format!(
            "\n{title}\n{}\n",
            "-".repeat(title.chars().count())
        ));
        for l in &lines[lines.len().saturating_sub(n)..] {
            s.push_str(&redactor.apply(l));
            s.push('\n');
        }
    };
    tail(&mut s, "Últimos mensajes de Wine y de la app", &g.ring, 300);
    tail(&mut s, "Registro de Steam", &g.logs.steam, 100);
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diagnostics::{analyze_bottle, Context};
    use std::path::Path;

    #[test]
    fn the_report_is_headed_with_the_apps_present_name() {
        let g = Gathered::default();
        let d = analyze_bottle(Path::new("/b"), &g, &Context::default());
        let r = report(&[], &d, &g, &Redactor::new("/Users/lucia", &[]));
        assert!(
            r.starts_with("Informe de Altrune\n==================\n\n"),
            "{r}"
        );
        assert!(!r.contains("Botella Libre"), "{r}");
    }
}
