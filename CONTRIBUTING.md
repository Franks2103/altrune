# Cómo contribuir a Altrune

Gracias por querer ayudar. Esta guía explica cómo compilar y probar el proyecto y las reglas que debe cumplir cualquier cambio. Todo el trato en el proyecto sigue el [Código de Conducta](CODE_OF_CONDUCT.md).

Antes de empezar, lee el [README](README.md) (qué es Altrune y qué se ha comprobado) y, si vas a tocar código, [`docs/architecture/compatibility-engine.md`](docs/architecture/compatibility-engine.md) (qué hace cada módulo).

## Preparar el entorno

* Un Mac con **chip de Apple** y **macOS** reciente (lo único probado es macOS 27).
* **Xcode Command Line Tools**: `xcode-select --install`.
* **Rust 1.95 o posterior** (lo exige eframe 0.37). El proyecto se comprueba con Rust 1.100.0. Instálalo con rustup (https://rustup.rs) o con Homebrew (`brew install rustup`).

## Compilar y probar

```sh
cargo build            # compilar
cargo run              # abrir la app
```

`cargo run` usa tu carpeta de datos real (su ruta aparece en «Acerca de Altrune», al final de Ajustes) y, si no encuentra un Wine, empieza a descargar uno al arrancar. Para probar cambios de interfaz sin tocar tus datos, compila una vez y usa el modo de capturas con una carpeta personal falsa (regla 2 más abajo). No pongas `HOME=` delante de `cargo`: rompería Rust.

Todo cambio debe pasar estos cuatro comandos, que son los que ejecuta la integración continua:

```sh
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
cargo build --release --locked
```

`cargo test` tarda unos 13 s en un Mac de gama alta (524 pruebas de la app y 77 de los scripts y el paquete; la primera vez compila las dependencias). Si falla una prueba de tiempos solo en una máquina muy cargada, vuelve a ejecutarla sola antes de dar el cambio por roto; no la debilites ni le subas los límites para que pase.

## Reglas del proyecto

1. **Nunca uses `--ignored` ni `--include-ignored`.** Las 5 pruebas ignoradas abren el Wine real, Steam o el launcher de Rockstar, usan un mando conectado o descargan motores: pueden cambiar tus botellas y tus datos. Las pruebas normales no tocan nada tuyo.
2. **Capturas y mediciones, solo con una carpeta personal aislada.** Crea una con `tools/hogar-aislado.sh` y lanza la app con `HOME` apuntando a ella:

   ```sh
   H=$(mktemp -d)
   sh tools/hogar-aislado.sh "$H" botellas
   HOME="$H" target/release/altrune --screenshot "$H/inicio.png" --vista inicio --tema claro
   rm -rf "$H"
   ```

   Nunca lances la app con tu `HOME` real para hacer capturas, `--medir` o `--reposo`, y no publiques capturas con rutas, nombres de usuario o botellas tuyas. La vista `ajustes` enseña la ruta del Wine falso: recórtala o usa otra vista.

   Las imágenes y animaciones del README (`docs/media/`) se rehacen con `sh tools/demo/generar.sh`, que crea su propia carpeta aislada; los dos diagramas SVG se editan a mano.
3. **Cada arreglo lleva una prueba que falla sin él.** Escribe primero la prueba, comprueba que falla y luego arregla. Para comportamientos de la interfaz o de varios pasos, usa un flujo en `src/flows/` (la app real sin ventana, con un Wine falso y una carpeta personal temporal); para decisiones puras, una prueba unitaria. Los detalles están en [`docs/testing/pruebas-aisladas.md`](docs/testing/pruebas-aisladas.md).
4. **`cargo fmt` y `clippy` sin ningún aviso** (`-D warnings`). No añadas `#[allow(...)]` para silenciar un aviso sin explicar el motivo en un comentario.
5. **Idiomas.** Los comentarios del código, los mensajes de commit y los comentarios de scripts y YAML van en **inglés**. Los textos que ve la persona (interfaz, mensajes, documentación) van en **español**, claro y sin jerga. Una prueba (`src/flows/marca_flows.rs`) comprueba que ningún texto visible use otro nombre que Altrune.
6. **No renombres los identificadores en español ni los que ya existen en disco.** `BOTELLA_*`, las opciones `--medir`, `--vista`, `--tema`, `--tamano` y `--reposo`, las rutas de las carpetas de datos y de lanzadores (`src/backend/paths.rs`, `src/backend/launchers.rs`), los identificadores del visor de Finder y de los lanzadores y sus marcas son intencionados: cambiarlos rompería botellas, accesos y asociaciones que ya existen.
7. **Capas en un solo sentido.** `domain` y `diagnostics` no importan nada del resto del crate; `backend` puede usar esas dos; `wine_setup` puede usar `backend`, `domain` y `diagnostics`; `app`, `ui`, `startup` y `measure` pueden usar todo. La prueba `src/flows/capas_flows.rs` falla si una capa de abajo nombra una de arriba.
8. **Sin binarios propietarios ni material descompilado.** No subas Wine, DLL, `.exe`, motores, `D3DMetal`, paquetes de Apple ni nada extraído de software propietario: programas, archivos, datos, código descompilado o desensamblado, pseudocódigo, direcciones de memoria o notas sacadas de ahí. **No se admite ingeniería inversa nueva de software propietario de terceros** ni código escrito mirando ese material: si implementas algo que otro programa ya hace, escríbelo de forma original a partir de lo que la función debe lograr (requisitos y comportamiento observable documentado), y di en la solicitud de cambios de dónde sale la especificación. Las compilaciones de Wine se descargan en tiempo de ejecución y no entran en el repositorio. El `.gitignore` ya bloquea muchas extensiones; no lo esquives.
9. **Sin datos personales en issues, registros ni capturas.** No pegues tu nombre de usuario del Mac, rutas con él, correos, identificadores de Steam, tokens ni registros completos de Wine. Para informar de un problema, usa «Buscar problemas» en la pestaña Diagnóstico de la botella y «Guardar informe…»: el informe oculta datos personales conocidos. Léelo antes de compartirlo; la ocultación no es infalible. Tampoco incluyas datos personales en los commits (el nombre y el correo del autor van con tu identidad pública o con el correo `noreply` de GitHub).
10. **Pequeño y con un solo propósito.** Un cambio por solicitud de cambios. No mezcles refactorizaciones sin relación y no cambies comportamiento ni rutas de datos persistentes sin hablarlo antes.
11. **D3DMetal y el Game Porting Toolkit de Apple.** La versión pública de Altrune no los descarga, no los instala ni los redistribuye, y no se admiten cambios que lo hagan (ni enlaces de descarga a copias de terceros, ni sumas, ni recetas que los instalen). Sí se admite, y se mantiene, **detectar y usar** una capa D3DMetal o un motor «Juegos DirectX 12» que la persona ya tenga en su Mac. Los motivos están en [`THIRD_PARTY_NOTICES.md`](THIRD_PARTY_NOTICES.md).

12. **Si cambia `Cargo.lock`, regenera la página de licencias en el mismo cambio.** Una vez, instala la herramienta con `sh tools/licencias.sh --instalar DIR` (necesita red) y apunta `CARGO_ABOUT` al binario que imprime (o ponlo en el `PATH`). Después ejecuta `cargo fetch --locked` y `sh tools/licencias.sh`, y guarda el nuevo `licencias/terceros.html` en el mismo cambio. Si no, la comprobación del flujo de versión (`sh tools/licencias.sh --comprobar`) falla. Los pasos completos están en la sección «Licencias de terceros» del [README](README.md).

Algunas pruebas leen frases exactas de `README.md`, `docs/roadmap.md`, `docs/distribucion.md` y de los scripts de `tools/` (`tests/paquete.rs` y las demás de `tests/`). Si reescribes esos textos, ejecuta `cargo test` y, si una frase cambia a propósito, actualiza la prueba en el mismo cambio; nunca borres la comprobación.

## Cómo proponer cambios

1. **Antes de un cambio grande**, abre un issue (formulario «Solicitud de función» o «Informe de fallo») y cuenta qué quieres hacer. Para un arreglo pequeño puedes ir directo a la solicitud de cambios.
2. Haz un fork, crea una rama con un nombre que diga qué haces y haz commits pequeños con un mensaje claro en inglés (una línea de resumen en imperativo y, si hace falta, un párrafo con el porqué).
3. Pasa los cuatro comandos de arriba antes de abrir la solicitud de cambios.
4. Abre la solicitud de cambios y rellena la lista de comprobación de la plantilla. Si cambias la interfaz, añade capturas hechas con datos simulados (carpeta aislada).
5. Alguien revisará el cambio y puede pedirte ajustes. Mantén la conversación respetuosa y centrada en el código.

## Qué sí y qué no entra

* **Entra:** arreglos con prueba, mejoras de accesibilidad y de textos en español, nuevas reglas de diagnóstico (deterministas y con prueba), documentación, y comprobaciones en otras versiones de macOS o en Macs distintos (con la versión, el chip y lo que pasó).
* **No entra:** código copiado, descompilado o escrito a partir de software propietario de terceros, descargas de D3DMetal o del Game Porting Toolkit, descargas que no se verifiquen, telemetría, y cualquier cosa que ejecute Wine fuera de la carpeta de botellas de Altrune.

## Seguridad

No abras un issue público por un fallo de seguridad: sigue [`SECURITY.md`](SECURITY.md).

## Licencia de las contribuciones

Altrune se publica bajo la [licencia Apache 2.0](LICENSE). Como dice su sección 5, **salvo que indiques expresamente otra cosa, cualquier contribución que envíes para incluirla en Altrune se licencia bajo los mismos términos de Apache-2.0, sin condiciones adicionales**: lo que entra es lo que sale (*inbound = outbound*). No hay acuerdo de licencia de contribuyente (CLA) ni hace falta firmar nada; conservas el copyright de lo que escribes.

Al enviar un cambio declaras que lo escribiste tú (o que tienes derecho a ofrecerlo bajo Apache-2.0) y que no copia material propietario ni descompilado (regla 8). El nombre y el logotipo de Altrune no entran en esa licencia (sección 6 de Apache-2.0): ver [`docs/marca.md`](docs/marca.md).
