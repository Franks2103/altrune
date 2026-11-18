# Hoja de ruta

Qué hace Altrune hoy, cómo se ha comprobado, qué falta por comprobar y qué viene después. Cada punto de «Siguiente» debe terminar con una prueba automática o con una observación real anotada en [`docs/testing/`](testing/compatibility-matrix.md).

Contenido: [Hoy](#hoy) · [Cómo se comprueba](#cómo-se-comprueba) · [Comprobado con la app real](#comprobado-con-la-app-real) · [Medidas](#medidas) · [Pendiente y sin verificar](#pendiente-y-sin-verificar) · [Siguiente](#siguiente)

## Hoy

Altrune es una app de Mac, en Rust con eframe/egui, que ejecuta programas de Windows con Wine libre. Es un único binario, sin servicios ni telemetría.

| Área | Qué hace |
|---|---|
| Botellas | Crear, renombrar, duplicar, copiar y borrar botellas; elegir la versión de Windows; detectar una botella incompleta y explicar qué falta. Viven solo en la carpeta de datos de Altrune y Wine nunca se ejecuta en un prefijo de fuera. Cada botella recuerda el Wine más nuevo con el que se abrió y D3DMetal se bloquea si supondría bajar de versión |
| Instalador guiado | Un punto de entrada por bytes del archivo (`.exe`, `.msi`); botella propuesta con su motivo; ningún paso se marca hecho antes de terminar y comprobarse; programas nuevos con «Abrir» o «no apareció nada nuevo» |
| Biblioteca | Lee los accesos `.lnk`, no recorre el Escritorio del Mac, evita duplicados por destino, busca sin tildes y permite ocultar y volver a mostrar programas |
| Motores | «Automático», «Steam y launchers» (DXMT, DirectX 10/11 sobre Metal), «Juegos DirectX 12» (D3DMetal, solo si ya está en el Mac) y «Wine normal». Un solo nombre por motor en toda la app. MSync y ESync se activan solo si el Wine de la botella los lee. Wine solo se busca en sitios conocidos de compilaciones libres (los motores de Altrune, las apps «Wine Stable», «Wine Staging» y «Wine Devel», Homebrew y MacPorts) y se comprueba que arranca antes de usarlo |
| Descargas | Las compilaciones de Wine, DXMT y GStreamer se verifican con una suma SHA-256 antes de desempaquetarse; un cambio de motor se hace por renombres, sin dejar un motor a medias; una descarga estancada falla a los 60 s |
| Procesos | Estado «En uso» sin lanzar procesos para botellas apagadas; cierre ordenado con espera de 60 s; sin zombis; al arrancar se limpian restos de instalaciones interrumpidas (solo nombres propios de la app) |
| Diagnóstico | Reglas deterministas con confianza (Comprobado, Probable, Pista), atribución por botella y por intento, e informe que oculta datos personales conocidos |
| Finder y Launchpad | «Abrir con» para los `.exe` con una sola app abierta a la vez, y una app de Mac por programa si se activa |
| Accesibilidad | Teclado completo (el orden de Tab es el visual), nombres y roles para VoiceOver (AccessKit) y contraste mínimo de 4,5:1 en tema claro y oscuro |
| Marca y paquete | Nombre, logotipo e icono de Altrune; versión `0.1.0-alpha`. `sh tools/empaquetar.sh` compila con `--locked` sin rutas personales, monta, firma ad hoc y comprueba `dist/Altrune.app`, con las licencias de terceros dentro; `sh tools/crear-dmg.sh` hace y verifica el instalador `.dmg` y su suma SHA-256 (sin firmar) |
| Firma y publicación | Scripts de firma con Developer ID y de notarización, guía ([`firma-y-notarizacion.md`](firma-y-notarizacion.md)) y un flujo de GitHub que solo crea un borrador de versión previa: preparados, sin ejecutar con una identidad real ni en GitHub |

Se conservan a propósito las rutas de las carpetas de datos y de lanzadores, los identificadores que ya existen en disco y `LOADER_SCRIPT`: cambiarlos rompería botellas, accesos y asociaciones ya creados.

## Cómo se comprueba

1. **Pruebas automáticas** (`cargo test`): cientos de pruebas unitarias y de flujo. Los flujos arrancan la app real, sin ventana, contra un Wine falso y una carpeta personal temporal; no tocan botellas ni Wine reales ([`testing/pruebas-aisladas.md`](testing/pruebas-aisladas.md)). Las pruebas que usan Wine real, Steam o Rockstar están marcadas como ignoradas y solo se ejecutan si la persona lo pide.
2. **Calidad estática:** `cargo fmt --all --check` y `cargo clippy --locked --all-targets -- -D warnings` sin avisos. Cero `unsafe`, un solo `unwrap()` real en producción, bloqueos que recuperan el envenenamiento y siete dependencias directas sin runtime asíncrono.
3. **Capas:** los módulos importan en un solo sentido (`domain` y `diagnostics` no importan nada del crate, `backend` usa esas dos, `wine_setup` usa `backend`) y una prueba lo vigila sobre el código que se distribuye.
4. **Comprobación con la app real, a mano:** lo que depende de macOS, de la pantalla o del Wine real. Se anota abajo, con la fecha y lo que quedó sin ver.

## Comprobado con la app real

Con la app instalada en el Mac de pruebas (chip de Apple, macOS 27), 2026-11-17:

* `Altrune.app` abre las botellas y los programas existentes sin cambiarlos: las fechas de las carpetas y `config.json` quedan intactos.
* El menú de la app dice «Altrune» («About Altrune», «Quit Altrune»). Los elementos «About», «Hide» y «Quit» salen en inglés porque los crea winit.
* El Dock muestra el icono del paquete (estilo de iconos «Teñido») y Finder lo ve (`NSWorkspace`). eframe pintaba su propio logo encima del icono; dentro de `Altrune.app` ya no lo hace.
* El visor «Abrir con» conserva su identificador, para no perder la asociación con los `.exe`, y se muestra como «Altrune» gracias a un nombre localizado.
* `sh tools/instalar.sh` instala en `/Applications/Altrune.app` con una copia verificada, solo reemplaza Altrune y se niega si la app está abierta. Desde ahí se abre, el Dock la muestra y Spotlight la encuentra. Conviene abrir siempre la de Aplicaciones, porque el visor de Finder sigue a la última copia que se abrió.
* VoiceOver ve nombres y roles según la API de accesibilidad de macOS.
* Una sesión larga (100 aperturas y cierres, diálogos, instalación guiada, botellas creadas, renombradas, duplicadas y borradas) deja la memoria de la app, los descriptores, los hilos y los zombis en su nivel inicial. Encontró y se corrigieron cuatro fugas.
* Las 18 capturas de todas las vistas, en tema claro, oscuro y a 1280×800, coinciden con las del binario anterior a la refactorización por capas salvo ruido de dibujo (ningún píxel cambia más de 40 de 255).

Con el instalador, en el mismo Mac (macOS 27.0.1, apariencia Oscura, estilo de iconos «Teñido»):

* `sh tools/empaquetar.sh` compila con `--locked` y `strings` no encuentra en el binario `/Users/`, la carpeta personal ni el nombre de usuario.
* `sh tools/crear-dmg.sh` crea `dist/Altrune-0.1.0-alpha.dmg` (5 805 568 bytes) y su `.sha256`, y la imagen pasa sus comprobaciones: `hdiutil verify`, montaje de solo lectura, contenido exacto, `codesign --verify --deep --strict` de la app de dentro, mismo ejecutable y sin rutas personales.
* Una captura real de la ventana de Finder (`tools/dmg/captura.sh`) sale limpia y simétrica. No se guarda en el repositorio: con «Teñido» el icono de la app sale rosado.
* `sh tools/licencias.sh --comprobar` pasa: la página de licencias guardada coincide con la que sale hoy.
* Una copia de la app firmada con `tools/firmar.sh --prueba` (ad hoc con runtime reforzado) arranca en una carpeta personal falsa.

Los resultados con programas concretos (Steam, el launcher de Rockstar) están en [`testing/compatibility-matrix.md`](testing/compatibility-matrix.md).

## Medidas

Binario optimizado, Steam cerrado, en una carpeta personal aislada con varias botellas y programas. Mediana de 6 ejecuciones alternando binarios. Son cifras aproximadas: se repiten con `--medir` en cada Mac.

| Medida | Resultado |
|---|---|
| `--medir`, total | 0,12 s |
| Detectar botellas y programas | 0,12 ms |
| ¿Qué botellas están en uso? (ninguna abierta) | 0,03 ms |
| Escribir 20 000 líneas de registro | 29 ms |
| Reposo de 20 s (carpeta aislada) | 23 cuadros, 0,40-0,50 % de CPU, unos 161 MB, sin hilos, archivos ni zombis de más |
| Cuadro en caliente, 1120×740 | 38-82 µs |

Las tomas por cuadro son bimodales (unos 40 µs o unos 65 µs según el núcleo donde caen), así que las diferencias entre vistas están dentro del ruido. La comprobación «en uso» con Steam abierto (5,5 ms) no se repitió desde que Steam está cerrado; ese código no cambió.

## Pendiente y sin verificar

Necesita a una persona o un Wine real; las pruebas automáticas no lo cubren.

**Del paquete y la marca**

* Pendiente: ver el Dock y Finder con el estilo de iconos normal (este Mac usa «Teñido»), Launchpad y el panel «Acerca de» del sistema.
* Pendiente: ver la ventana del instalador con el estilo de iconos normal y con la apariencia Clara (en este Mac se vio con «Teñido» y Oscura; Finder dibuja las etiquetas en negro sobre el fondo claro), y en otras versiones de macOS.
* Firma con Developer ID y notarización para otros Macs: los scripts están escritos (`tools/firmar.sh`, `tools/notarizar.sh`) pero **no se han ejecutado**, porque este Mac no tiene ninguna identidad de Developer ID. La firma ad hoc solo vale para este Mac. Falta comprobar con una app firmada que el botón de Terminal pide el permiso de automatización, que Wine, DXMT y GStreamer arrancan bajo el runtime reforzado y que la imagen descargada se abre en un Mac limpio.
* El flujo de versión (`.github/workflows/release.yml`) está preparado y no se ha ejecutado nunca: falta crear el entorno `release` con sus seis secretos y una primera ejecución real (ejecutores de GitHub, `dmgbuild` y `cargo-about`, la ruta firmada). Solo crea un borrador; publicarlo es un paso manual.
* Decidir si se quiere migrar las carpetas de datos a los nombres de Altrune.

**De Wine real**

* Instalación con un instalador real en cada motor.
* Descargas reales: cancelar, cerrar la ventana a mitad, conexión lenta.
* Alta resolución, versión de Windows, herramientas (`winecfg`, `rundll32 …Control_RunDLL`), exportar e importar, con Wine escribiendo en una botella.
* Rendimiento de MSync y ESync (la detección está probada, la mejora no está medida).
* Steam y los juegos: no se han vuelto a abrir desde los últimos cambios.

**De macOS y de la interfaz**

* Doble clic en Finder con la app cerrada y con la app abierta (traer la ventana al frente).
* VoiceOver con el lector en marcha: con un diálogo abierto, la página de detrás sigue en el árbol de accesibilidad.
* La contraseña de Rosetta en un Mac sin Rosetta.
* Apps de Launchpad con nombres repetidos.
* Ventana oculta mucho tiempo: macOS deja de despertarla y un programa que termina mientras tanto queda como zombi hasta que la ventana vuelve.
* Macs con Intel y versiones de macOS anteriores a la 27: sin probar.

**Deuda técnica, sin urgencia**

* `wine_card` (unas 210 líneas) y `diagnostics_tab` (unas 170) siguen enteras.
* `app/messages.rs` es un solo archivo de unas 670 líneas, ordenado por secciones.
* Los archivos de `app/` y `ui/` siguen usando `use crate::*`.
* `cargo doc` da 9 avisos de enlaces.

## Siguiente

1. **Steam y Rockstar sobre una botella de pruebas.** Matriz de un cambio por vez (ver [`testing/compatibility-matrix.md`](testing/compatibility-matrix.md)), con la persona presente para autorizar abrir Steam. Después Epic, GOG y Battle.net.
2. **DXMT con Wine 11 libre.** Comprobar con un programa Direct3D 11 pequeño que la capa carga en el Wine descargado (`WINEDLLPATH` y sustituciones `d3d11`, `dxgi`, `d3d10core` en `n,b`) y documentar el resultado antes de ofrecer más programas con ese motor.
3. **Dependencias de programas.** Instalar el Visual C++ redistribuible oficial de Microsoft con verificación de hash y estado idempotente.
4. **Wine Mono y Gecko.** Comprobar si el Wine descargado los pide al crear la botella y ofrecer su instalación con hash verificado.
5. **Prueba integrada con Wine real** en una carpeta temporal: los flujos ya cubren crear, instalar, abrir y cerrar con un Wine falso; falta repetirlo con el real (cambiar Windows, alta resolución).
6. **Distribución.** Hecho: versión `0.1.0-alpha`, paquete sin rutas personales y con las licencias dentro, instalador `.dmg` verificado, página de licencias, y scripts y flujo de borrador escritos. Pendiente: conseguir un certificado Developer ID, firmar y notarizar de verdad, hacer una primera ejecución real del flujo, comprobar la imagen en un Mac limpio y publicar a mano la primera versión descargable (ver [`distribucion.md`](distribucion.md) y [`firma-y-notarizacion.md`](firma-y-notarizacion.md)).
7. **Migración opcional** de las carpetas de datos a los nombres de Altrune, solo a petición de la persona y con camino de vuelta.
