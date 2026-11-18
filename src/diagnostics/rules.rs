//! The table of known symptoms. Each rule is a literal text match, listed with why it exists.

use super::findings::{Confidence, Remedy};

pub(super) struct Rule {
    pub(super) code: &'static str,
    /// All of these (lower-case) must appear in one line.
    pub(super) all: &'static [&'static str],
    pub(super) title: &'static str,
    pub(super) explanation: &'static str,
    pub(super) suggestion: &'static str,
    pub(super) confidence: Confidence,
    /// The line names a DLL: it goes in the title and each DLL is its own finding.
    pub(super) names_dll: bool,
    /// The advice is to move the bottle to the DXMT engine.
    pub(super) dxmt_advice: bool,
    pub(super) remedy: Option<Remedy>,
}

/// Known symptoms. Each pattern is text Wine, macOS or the program really prints.
pub(super) const RULES: &[Rule] = &[
    Rule {
        code: "rosetta-missing",
        all: &["bad cpu type"],
        title: "Falta instalar Rosetta en el Mac",
        explanation: "Wine para Mac usa programas de Intel y este Mac necesita Rosetta para abrirlos.",
        suggestion: "Instala Rosetta: la app la pide al instalar Wine, o ejecuta «softwareupdate --install-rosetta».",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "missing-dll",
        all: &["err:module:import_dll", "not found"],
        title: "Falta un componente que el programa necesita",
        explanation: "El programa necesita una librería de Windows (un archivo DLL) que no está en la botella.",
        suggestion: "Suele ser el «Visual C++ Redistributable» o .NET. Instálalo en la misma botella con «Instalar un programa».",
        confidence: Confidence::Comprobado,
        names_dll: true,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "dll-load-failed",
        all: &["err:module:load_dll", "failed to load"],
        title: "No se pudo cargar un componente del programa",
        explanation: "Wine no pudo cargar una librería que el programa pide.",
        suggestion: "Instala el componente que falta (a menudo Visual C++ o .NET) en esta botella.",
        confidence: Confidence::Probable,
        names_dll: true,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "wrong-arch",
        all: &["is not a valid win32 application"],
        title: "Programa no compatible",
        explanation: "El archivo no es un programa de Windows que Wine pueda abrir (o está dañado).",
        suggestion: "Comprueba que bajaste la versión de Windows (no la de Mac) y que la descarga terminó.",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "prefix-broken",
        all: &["could not load kernel32.dll"],
        title: "La botella está dañada",
        explanation: "Faltan archivos básicos de Windows dentro de la botella.",
        suggestion: "Crea una botella nueva y reinstala el programa.",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "prefix-newer",
        all: &["wine prefix", "was created by a newer"],
        title: "La botella es de una versión más nueva de Wine",
        explanation: "Esta botella se usó con una versión de Wine más nueva que la que la abre ahora.",
        suggestion: "Ábrela con «Automático» (Wine 11) o crea una botella nueva.",
        confidence: Confidence::Comprobado,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    // Seen on an Apple M5 with WineHQ 11.0 (Rockstar Games Launcher 1.0.108.2970): wined3d reports
    // "GPU maximum feature level 0x9300" on Apple's OpenGL 4.1 and on MoltenVK alike, so every
    // Direct3D 10/11 device is refused. `fixme:winediag` is kept visible for this (base_env).
    Rule {
        code: "graphics-init",
        all: &["wined3d_select_feature_level", "none of the requested d3d feature levels"],
        title: "El programa no pudo dibujar sus gráficos",
        explanation: "El programa pidió un dispositivo Direct3D 10 o superior y este Wine, en el Mac, solo llega a Direct3D 9: ni su traductor a OpenGL ni el de Vulkan alcanzan ese nivel.",
        suggestion: "En los ajustes de esta botella elige Gráficos: Steam y launchers (DXMT). Descarga un motor libre que dibuja DirectX 10/11 con Metal. Cambiar la versión de Windows o reinstalar no lo arregla.",
        confidence: Confidence::Comprobado,
        names_dll: false,
        dxmt_advice: true,
        remedy: Some(Remedy::UseDxmt),
    },
    // The same failure one layer up: Direct2D draws through a Direct3D 10 device and gets none,
    // so the program has nothing to draw its window with (visible with WINEDEBUG=warn+d2d).
    Rule {
        code: "direct2d-init",
        all: &["d2d_factory_get_device", "failed to create device"],
        title: "No se pudo dibujar la ventana del programa",
        explanation: "Direct2D dibuja sobre un dispositivo Direct3D 10 y este Wine no pudo crearlo, así que el programa no tiene con qué dibujar su ventana.",
        suggestion: "En los ajustes de esta botella elige Gráficos: Steam y launchers (DXMT). El Wine normal no trae DirectX 10/11 en el Mac; DXMT sí.",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: true,
        remedy: Some(Remedy::UseDxmt),
    },
    Rule {
        code: "gl-too-old",
        all: &["requested gles version", "greater than max supported"],
        title: "El programa pide gráficos que este Wine no da",
        explanation: "El programa usa un navegador interno (Chromium) que pide gráficos que este Wine no da en Mac.",
        suggestion: "En los ajustes de esta botella elige Gráficos: Steam y launchers (DXMT). El Wine normal no da esos gráficos en el Mac; DXMT sí.",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: true,
        remedy: Some(Remedy::UseDxmt),
    },
    Rule {
        code: "black-window",
        all: &["disabling gpu acceleration"],
        title: "Ventana en negro",
        explanation: "Steam apagó su aceleración gráfica porque falló; su ventana puede quedar negra.",
        suggestion: "Steam solo dibuja su ventana con DXMT: en los ajustes de esta botella elige Gráficos: Steam y launchers (DXMT) y ábrelo otra vez. Si ya lo usa, deja activado en Steam «Renderizado acelerado por GPU en vistas web» (Parámetros, Interfaz).",
        confidence: Confidence::Probable,
        names_dll: false,
        // The text already covers a bottle that uses DXMT, so it is not replaced.
        dxmt_advice: false,
        remedy: Some(Remedy::UseDxmt),
    },
    Rule {
        code: "no-video-filter",
        all: &["winegstreamer: failed to create deinterlace"],
        title: "Falta una pieza para reproducir videos",
        explanation: "Wine ya tiene el decodificador, pero le falta el filtro de video «deinterlace» que usa GStreamer.",
        suggestion: "Cierra Steam y el juego. Pulsa «Añadir video para juegos» para completar GStreamer y vuelve a abrir Steam.",
        confidence: Confidence::Comprobado,
        names_dll: false,
        dxmt_advice: false,
        remedy: Some(Remedy::AddVideo),
    },
    Rule {
        code: "no-video",
        all: &["gstreamer doesn't support"],
        title: "El programa no puede reproducir videos",
        explanation: "El programa quiso reproducir un video y este Wine no tiene con qué (GStreamer). Muchos juegos se cierran en ese momento.",
        suggestion: "En los ajustes de esta botella, con Gráficos: Steam y launchers (DXMT), pulsa «Añadir video». Luego cierra y vuelve a abrir el launcher (Steam).",
        confidence: Confidence::Comprobado,
        names_dll: false,
        dxmt_advice: false,
        remedy: Some(Remedy::AddVideo),
    },
    Rule {
        code: "network",
        all: &["transport error"],
        title: "El programa no pudo conectarse",
        explanation: "El programa no pudo hablar con sus servidores (inicio de sesión o descarga).",
        suggestion: "Comprueba internet. Si el resto funciona, puede ser una función de red que este Wine no tiene: prueba otra vez más tarde.",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "network-unsupported",
        all: &["unknown error 10045"],
        title: "Una función de red no está disponible",
        explanation: "Wine respondió «operación no admitida» a una función de red del programa.",
        suggestion: "Suele no impedir el uso. Si el programa no conecta, prueba otra vez más tarde.",
        confidence: Confidence::Pista,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "init-failed",
        all: &["failed to initialize"],
        title: "El programa no pudo iniciar",
        explanation: "El mensaje es muy general: a menudo es porque el programa no encuentra sus datos (perfil de usuario de Windows) o le falta un componente.",
        suggestion: "Mira los mensajes anteriores en «Detalles técnicos»; suelen decir qué faltaba. Si necesitas ayuda, guarda el informe.",
        confidence: Confidence::Pista,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "permission",
        all: &["permission denied"],
        title: "macOS no dejó acceder a un archivo",
        explanation: "macOS no dejó leer o escribir un archivo que el programa necesita.",
        suggestion: "Revisa Ajustes del Sistema › Privacidad (Archivos y carpetas / Acceso total al disco).",
        confidence: Confidence::Pista,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
    Rule {
        code: "no-display",
        all: &["nodrv_createwindow"],
        title: "No se pudo abrir la ventana",
        explanation: "Wine no logró conectar con la pantalla del Mac.",
        suggestion: "Cierra todo con «Forzar cierre» y vuelve a abrir el programa.",
        confidence: Confidence::Probable,
        names_dll: false,
        dxmt_advice: false,
        remedy: None,
    },
];

/// The DXMT advice when the bottle already runs on that engine: changing engine will not help.
pub(super) const ALREADY_DXMT: &str = "Esta botella ya usa Steam y launchers (DXMT), así que cambiar los gráficos no lo arregla. Guarda el informe para pedir ayuda.";

/// The rules that match one line (by index into `RULES`). Cheap enough to run on every line
/// Wine prints: one lower-casing and a few substring checks.
pub fn rules_hit(line: &str) -> Vec<usize> {
    let mut hits = Vec::new();
    let low = line.to_lowercase();
    for (i, rule) in RULES.iter().enumerate() {
        if rule.all.iter().all(|p| low.contains(p)) {
            hits.push(i);
        }
    }
    hits
}
