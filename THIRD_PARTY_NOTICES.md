# Avisos de terceros

Altrune se apoya en trabajo de otras personas y proyectos. Este documento dice qué es, bajo qué licencia llega y cómo. Es un **aviso informativo**, no asesoramiento legal, y se escribió el 2026-11-17 a partir de una revisión de las licencias hecha ese mismo día. Los puntos que esa revisión no pudo comprobar se marcan como «sin verificar».

## La licencia de Altrune

El código propio de Altrune está bajo la **licencia Apache 2.0** (`Apache-2.0`): el texto completo está en [LICENSE](LICENSE) y el aviso del proyecto («Copyright 2026 Franks2103»), en [NOTICE](NOTICE). Esa licencia cubre solo el código y la documentación de Altrune. **No cubre** los componentes de terceros de este documento, que conservan sus propias licencias y de los que Altrune no reclama la propiedad, ni el nombre y el logotipo de Altrune (sección 6 de Apache-2.0; ver [docs/marca.md](docs/marca.md)).

## Resumen

| Qué | Dónde está | Licencia | Cómo llega a la persona |
|---|---|---|---|
| Bibliotecas de Rust (121) | Dentro del binario de Altrune | Permisivas (MIT, Apache-2.0, Zlib, BSD, ISC y similares); ninguna con copyleft efectivo | Se compilan dentro de `Altrune.app` |
| Tipografías de egui | Dentro del binario | Ubuntu Font Licence 1.0, MIT, dominio público y licencia Bitstream Vera | Se compilan dentro de `Altrune.app` |
| Wine (motor «Estándar») | No está en el repositorio ni en la app | LGPL-2.1 o posterior | Lo descarga la app de las compilaciones de Gcenx (sola, sin preguntar, la primera vez que se abre y no hay ningún Wine) |
| Wine 11.0 de highball-engine y su DXMT (motor «Metal») | No está en el repositorio ni en la app | LGPL-2.1 (y las de sus componentes) | Lo descarga la app, a petición, con huella SHA-256 fijada |
| GStreamer 1.28.7 (vídeo de los juegos) | No está en el repositorio ni en la app | LGPL (y las de sus complementos) | Lo descarga la app, a petición, con huella SHA-256 fijada |
| D3DMetal y Game Porting Toolkit de Apple | No están en el repositorio ni en la app | **Propietaria de Apple, sin licencia abierta** | **No se descargan ni se redistribuyen.** Altrune solo detecta y usa una copia que la persona ya tenga en su Mac (ver más abajo) |
| SDL3, sdl2-compat y libusb | No están en el repositorio ni en la app | zlib (SDL3, sdl2-compat); LGPL-2.1 (libusb) | Los compila la propia persona con `mando/compilar-mando.sh` |
| Tipografía de sistema de Apple (San Francisco) | No se copia nunca | Licencia de Apple | La app la lee del Mac de la persona, si existe |

**Regla del proyecto:** solo las dos primeras filas (las bibliotecas de Rust y las tipografías de egui) van dentro del binario `Altrune.app`. Ninguno de los componentes desde la fila de Wine («Estándar») hasta la última se incluye en el repositorio, en `Altrune.app` ni en ningún espejo. Altrune solo descarga algunos de ellos (o los compila la persona) en su propio Mac, y cada uno llega con su licencia original; D3DMetal y el Game Porting Toolkit no se descargan en absoluto. Esto mantiene las obligaciones de redistribución en quien publica cada componente. Si algún día se quisiera incluir uno dentro de la app, cambian las obligaciones (ver la última sección).

## Bibliotecas de Rust dentro del binario

Altrune declara 7 dependencias directas (`eframe`, `egui`, `rfd`, `serde`, `serde_json`, `image`, `winit`). Con sus dependencias indirectas, la compilación para macOS usa **121 paquetes de terceros**: 118 enlazados en el binario y 3 que solo se usan al compilar (`autocfg`, `cfg_aliases`, `version_check`). Las compilaciones para Apple Silicon (`aarch64-apple-darwin`) y para Intel (`x86_64-apple-darwin`) usan exactamente el mismo conjunto. `Cargo.lock` lista más paquetes (372), pero el resto solo se compilan para Linux, Windows u otros sistemas y no entran en `Altrune.app`.

Todos los paquetes declaran una expresión de licencia SPDX en sus metadatos. No hay LGPL, GPL pura, AGPL ni MPL en la compilación para macOS. Las licencias de la lista de abajo se leyeron de los metadatos del registro de crates. Los textos completos están en el paquete de licencias: salen del archivo de licencia de cada biblioteca y, para las 27 que no lo traen, del texto estándar con el titular que indica su proyecto (ver «Sin verificar» en «El paquete de licencias: cómo se hace»).

Las licencias están agrupadas tal como las declara cada crate. «A OR B» significa que la persona que usa el crate puede elegir una de las dos; para Altrune basta con que una de las opciones sea aceptable.

### Cómo regenerar esta lista

Desde la raíz del repositorio, con el `Cargo.lock` que se quiera comprobar:

```sh
cargo tree --locked --target aarch64-apple-darwin -e normal,build --prefix none --format '{p}|{l}' \
  | sed 's/ (\*)$//' | grep -v '^altrune ' | sort -u
```

(`{p}` es el nombre con la versión y `{l}` la licencia; el `sed` quita la marca de paquete repetido y el `grep` quita el propio Altrune). Da una línea por paquete (121 hoy). Los grupos de abajo se obtuvieron agrupando esas líneas por licencia y unificando a mano las grafías equivalentes: `MIT/Apache-2.0` y `MIT / Apache-2.0` son «MIT OR Apache-2.0», y el orden de los términos de un «OR» no importa. Para ver también las dependencias que solo se usan en otros sistemas: `cargo metadata --format-version 1 --locked`. Si se instala `cargo-deny`, `cargo deny check licenses` comprueba la lista contra [deny.toml](deny.toml).

### `Apache-2.0 OR MIT` (75)

accesskit 0.25.2, accesskit_consumer 0.39.2, accesskit_macos 0.27.2, ahash 0.8.12, arrayvec 0.7.8, autocfg 1.5.1, bitflags 1.3.2, bitflags 2.13.2, cfg-if 1.0.5, cgl 0.3.2, color 0.3.3, core-foundation 0.9.4, core-foundation-sys 0.8.7, core-graphics 0.23.2, core-graphics-types 0.1.3, crc32fast 1.5.5, document-features 0.2.12, ecolor 0.37.0, eframe 0.37.0, egui 0.37.0, egui-winit 0.37.0, egui_glow 0.37.0, either 1.19.0, emath 0.37.0, epaint 0.37.0, euclid 0.22.14, fdeflate 0.3.7, fearless_simd 0.4.1, flate2 1.1.11, font-types 0.12.7, foreign-types 0.5.0, foreign-types-macros 0.2.4, foreign-types-shared 0.3.1, guillotiere 0.7.0, hashbrown 0.17.1, image 0.25.10, itertools 0.15.0, itoa 1.0.19, kurbo 0.13.1, libc 0.2.191, linebender_resource_handle 0.1.1, litrs 1.0.1, lock_api 0.4.14, log 0.4.35, nohash-hasher 0.2.0, num-traits 0.2.19, once_cell 1.21.4, parking_lot 0.12.5, parking_lot_core 0.9.12, peniko 0.6.1, pin-project-lite 0.2.17, png 0.18.1, polycool 0.4.0, proc-macro2 1.0.107, profiling 1.0.18, quote 1.0.47, read-fonts 0.41.0, scopeguard 1.2.0, serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.154, skrifa 0.44.0, smallvec 1.16.5, smol_str 0.2.2, static_assertions 1.1.0, syn 3.0.10, thiserror 2.0.21, thiserror-impl 2.0.21, unicode-segmentation 1.13.3, uuid 1.29.2, vello_common 0.2.0, vello_cpu 0.2.0, version_check 0.9.5, web-time 1.1.0

### `MIT` (18)

block2 0.5.1, block2 0.6.2, cfg_aliases 0.2.2, dispatch 0.2.0, glutin-winit 0.5.0, harfrust 0.12.0, objc-sys 0.3.5, objc2 0.5.3, objc2 0.6.5, objc2-app-kit 0.2.2, objc2-encode 4.1.0, objc2-foundation 0.2.2, objc2-foundation 0.3.2, rfd 0.18.0, simd-adler32 0.3.10, tracing 0.1.44, tracing-core 0.1.36, zmij 1.0.23

### `Apache-2.0 OR MIT OR Zlib` (11)

bytemuck 1.25.2, bytemuck_derive 1.12.1, cursor-icon 1.2.0, dispatch2 0.3.1, glow 0.17.0, miniz_oxide 0.8.9, miniz_oxide 0.9.1, objc2-app-kit 0.3.2, objc2-core-foundation 0.3.2, objc2-uniform-type-identifiers 0.3.2, raw-window-handle 0.6.2

### `Apache-2.0` (4)

accesskit_winit 0.34.2, glutin 0.32.3, unicode-general-category 1.1.0, winit 0.30.13

### `(MIT OR Apache-2.0) AND Unicode-3.0` (2)

unicode-ident 1.0.27, unicode-properties 0.1.5

### `Apache-2.0 OR BSD-3-Clause` (2)

moxcms 0.8.1, pxfm 0.1.30

### `MIT OR Unlicense` (2)

byteorder-lite 0.1.0, memchr 2.8.3

### `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0` (1)

epaint_default_fonts 0.37.0

### `0BSD OR Apache-2.0 OR MIT` (1)

adler2 2.0.1

### `Apache-2.0 AND MIT` (1)

dpi 0.1.2

### `Apache-2.0 OR BSD-2-Clause OR MIT` (1)

zerocopy 0.8.63

### `Apache-2.0 OR GPL-2.0-only` (1)

self_cell 1.3.0

### `ISC` (1)

libloading 0.8.9

### `Zlib` (1)

foldhash 0.2.0


### Notas sobre algunas licencias

- **`self_cell`** declara `Apache-2.0 OR GPL-2.0-only`. Altrune toma la opción **Apache-2.0**; no hay copyleft. Conviene dejar esa elección anotada en el archivo de licencias de cualquier binario que se distribuya.
- **`winit`, `glutin`, `accesskit_winit` y `unicode-general-category`** son solo Apache-2.0. Al distribuir un binario hay que incluir el texto de la licencia y los archivos `NOTICE` que tengan.
- **`dpi`** es `Apache-2.0 AND MIT`: la parte MIT es `src/libm.rs` (derivada de rust-lang/libm), que solo se usa sin la opción `std`. Altrune compila `dpi` con las opciones por defecto, así que esa parte no se usa, pero el paquete de licencias reproduce los dos textos por prudencia.
- **`accesskit`, `accesskit_consumer` y `accesskit_macos`** declaran `MIT OR Apache-2.0`, pero parte de su código deriva de Chromium y lleva en la cabecera de esos archivos fuente un aviso de «The Chromium Authors» bajo una licencia de estilo BSD de tres cláusulas (el repositorio de AccessKit la guarda en `LICENSE.chromium`). Esas bibliotecas no incluyen ese archivo, y su aviso debe acompañar al binario: el paquete de licencias lo trae.
- **`unicode-ident` y `unicode-properties`** añaden la licencia Unicode-3.0, que exige conservar su aviso al distribuir.
- **`epaint_default_fonts`** declara `(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0`. Ver la sección siguiente: la parte OFL no llega al binario.
- Los crates que declaran `MIT/Apache-2.0` o `Apache-2.0/MIT` usan la grafía antigua con barra para decir «o».

## Tipografías que egui incluye en el binario

Altrune activa en `eframe` la opción `default_fonts` (y **no** `monochrome_emoji_fonts`; se comprobó con `cargo tree -e features`). Con eso el binario lleva dentro estas tipografías, que vienen del crate `epaint_default_fonts` 0.37.0:

| Archivo | Qué es | Licencia |
|---|---|---|
| `Ubuntu-Light.ttf` | Ubuntu Light | Ubuntu Font Licence 1.0 |
| `Hack-Regular.ttf` | Hack (de Source Foundry) | MIT para el trabajo de Source Foundry; los añadidos de DejaVu son de dominio público; los glifos de Bitstream Vera van con la licencia Bitstream Vera (nombres reservados «Bitstream» y «Vera») |
| `egui-icons.ttf` | 5 glifos extraídos de emoji-icon-font | MIT, copyright 2014 John Slegers |

El crate también trae `NotoEmoji-Regular.ttf` (OFL-1.1) y la versión completa de `emoji-icon-font.ttf`, pero **no se compilan** en Altrune, así que la licencia OFL-1.1 que aparece en la expresión del crate no aplica al binario (aunque sí aparece en cualquier escáner de licencias).

Qué hay que cumplir: la Ubuntu Font Licence permite incrustar las tipografías en una aplicación, pero las tipografías y sus derivadas siguen bajo esa licencia, y el texto de la licencia y el aviso de copyright deben acompañarlas. No se deben renombrar ni modificar. Los textos están en el crate, en la carpeta `fonts/` (`UFL.txt`, `OFL.txt` y los de Hack y emoji-icon-font).

### Tipografía San Francisco de Apple

Altrune **no incluye** ninguna fuente de Apple. En tiempo de ejecución, `src/ui/theme.rs` intenta leer `/System/Library/Fonts/SFNS.ttf` y `SFNSMono.ttf` del Mac de la persona, y si no existen usa las de egui. No guarda copia en disco. La licencia pública de Apple para San Francisco (en developer.apple.com/fonts) trata del paquete descargable, para maquetas de interfaces; no se encontró ningún texto que permita o prohíba que una app lea la copia del sistema. Es una práctica habitual, pero es una valoración, no una certeza. Por eso: no copiar nunca esos archivos al repositorio, a la `.app` ni a pruebas, y tener presente que las capturas de pantalla que muestren esa tipografía pueden requerir cuenta de desarrollador de Apple si se publican (riesgo bajo; se pueden hacer capturas con la tipografía de egui).

## Componentes que Altrune descarga o compila en el Mac de la persona

Ninguno de estos componentes está en el repositorio ni dentro de `Altrune.app`. Las direcciones y huellas salen del código (`src/wine_setup/` y `mando/compilar-mando.sh`); las licencias, de los repositorios y páginas oficiales consultados el 2026-11-17.

### Wine, motor «Estándar»

- **Qué:** compilaciones `wine-stable` de Wine para macOS publicadas por Gcenx.
- **Origen:** `https://api.github.com/repos/Gcenx/macOS_Wine_builds/releases` (la app elige la primera publicación `wine-stable` para macOS 64 bits; si no hay ninguna entre las últimas 40, prueba con `wine-staging` y luego `wine-devel`, que llevan la misma licencia LGPL).
- **Licencia:** Wine es LGPL-2.1 o posterior. El repositorio de Gcenx no tiene archivo `LICENSE` (GitHub no declara licencia); su README dice que las compilaciones usan bibliotecas de MacPorts y que incluyen wine-gecko y wine-mono. **Sin verificar:** la licencia de cada biblioteca de MacPorts empaquetada.
- **Cómo llega:** la app lo descarga sola la primera vez que se abre y no encuentra ningún Wine (se puede cancelar), o cuando la persona pide instalarlo desde Ajustes; lo comprueba con la huella SHA-256 que publica GitHub (si GitHub no da ninguna, se instala sin esa comprobación), lo descomprime en su carpeta de datos y le quita la cuarentena de macOS.

### Wine 11.0 «Metal» y DXMT (highball-engine)

- **Qué:** un Wine 11.0 compilado a partir de fuentes LGPL, más una compilación de DXMT (traductor de Direct3D 10/11 a Metal).
- **Origen (con huella fijada en el código):**
  - `https://github.com/gauthierpiarrette/highball-engine/releases/download/engine-wine-11.0-20261112T101620Z-gst-rpath-0030/engine-wine-11.0-20261112T101620Z-gst-rpath-0030.tar.gz`
  - `https://github.com/gauthierpiarrette/highball-engine/releases/download/dxmt-highball-20261017T231707Z-331c552/dxmt-highball-20261017T231707Z-331c552.tar.gz`
- **Licencia:** el repositorio highball-engine es LGPL-2.1. Su `NOTICE.md` indica que el Wine sale de fuentes LGPL-2.1 o posterior, y enumera componentes con sus licencias (MoltenVK Apache-2.0, DXVK zlib, winetricks LGPL-2.1, wine-mono y wine-gecko MIT/MPL, DXMT). DXMT original (3Shain/dxmt) es LGPL-2.1 o posterior, con el aviso de copyright de su autor, Feifan He (las versiones hasta la 0.80 eran MIT); la compilación usada sale de una bifurcación pública con la misma licencia. **Sin verificar:** que el contenido del archivo publicado coincida con esa bifurcación, y que el Wine salga realmente de esas fuentes (es una afirmación de highball-engine).
- **Cómo llega:** descarga a petición, verificada contra la huella SHA-256 fijada antes de descomprimir. Cada publicación de highball-engine lleva las fuentes correspondientes.

### GStreamer 1.28.7 (vídeos de los juegos)

- **Qué:** el paquete oficial de GStreamer para macOS, del que Altrune conserva los componentes de reproducción y 32 complementos, y deja fuera los de licencia GPL, los codificadores y las herramientas de red y de desarrollo.
- **Origen:** `https://gstreamer.freedesktop.org/data/pkg/osx/1.28.7/gstreamer-1.0-1.28.7-universal.pkg` (huella SHA-256 fijada, igual a la publicada junto al archivo).
- **Licencia:** GStreamer es LGPL. FFmpeg tal como lo compila GStreamer se declara LGPL-2.1 o posterior (se leyó la rama principal de su receta, no la etiqueta 1.28.7). **Sin verificar:** los textos de licencia dentro del paquete (no se descargaron los 153 MB para leerlos).
- **Cómo llega:** descarga a petición, se expande con `pkgutil --expand-full` dentro de la carpeta del motor; no se instala nada en el sistema.
- **Patentes (aviso, sin verificar):** los decodificadores H.264 (libav y VideoToolbox) pueden estar sujetos a patentes en países que las admiten para software. La licencia LGPL no concede licencia de patentes. No se comprobó el estado actual de ningún consorcio de patentes. Por eso no se debe incluir este paquete en un `.app` distribuido sin consejo legal.

### SDL3, sdl2-compat y libusb (compilados localmente)

- **Qué:** bibliotecas que dan al motor acceso al mando de Xbox por cable (ver `mando/`).
- **Origen:** `mando/compilar-mando.sh` clona `https://github.com/libsdl-org/SDL.git` (etiqueta `release-3.4.20`) y `https://github.com/libsdl-org/sdl2-compat.git` (etiqueta `release-2.32.74`), y descarga `libusb-1.0.30.tar.bz2` de `https://github.com/libusb/libusb/releases` (con huella SHA-256 fijada). Las dos etiquetas de Git no están fijadas por commit.
- **Licencia:** SDL3 y sdl2-compat son zlib; libusb es LGPL-2.1. El script enlaza libusb de forma estática dentro de SDL3, de modo que una `libSDL3.dylib` distribuida sería un trabajo enlazado con LGPL.
- **Cómo llega:** la compila la persona que ejecuta el script; la app no lo ejecuta. El resultado queda en la carpeta de datos de la app, nunca en el repositorio.

### Wine Mono y Wine Gecko

Altrune no los descarga. El motor Estándar los trae dentro del paquete de Gcenx; en el motor Metal, highball-engine los incluye, y cuando faltan es Wine (no Altrune) quien los pide. Wine Mono se declara disponible como LGPL o MIT (con excepciones menores que no se usan). La licencia de Wine Gecko no se pudo leer directamente; la etiqueta «MPL» viene del `NOTICE.md` de highball-engine (sin verificar).

## D3DMetal y Game Porting Toolkit de Apple (componentes propietarios)

**D3DMetal no es software libre.** Es de Apple y se rige por el «Software License Agreement for Game Porting Toolkit» de Apple. Ninguna licencia abierta lo cubre.

- **Política de la versión pública: Altrune no descarga, no instala ni redistribuye D3DMetal ni el Game Porting Toolkit.** Ni el repositorio ni `Altrune.app` contienen ningún archivo de Apple, y la app no lo descarga de ningún sitio (tampoco de la redistribución de Gcenx). Tampoco debe hacerlo nunca ningún `.app` ni espejo del proyecto.
- **Qué sí hace Altrune:** si el Mac de la persona ya tiene una capa D3DMetal instalada o un motor «Juegos DirectX 12» que ya existía, Altrune lo **detecta y lo usa** sin tocarlo. La persona los consigue por su cuenta y bajo la licencia de Apple.
- **Por qué:** se eligió la opción más conservadora. La licencia de Apple (texto de 2023, leído íntegro) solo permite usar el software para «desarrollar, probar o evaluar videojuegos» para productos de Apple; permite distribuirlo únicamente con fines no comerciales; prohíbe separar los componentes del paquete para distribuirlos (salvo el marco y los «redistribuibles», también solo sin fines comerciales); prohíbe usarlo en hardware que no sea de Apple, descompilarlo, modificarlo y crear obras derivadas (salvo lo que permita la ley o licencias abiertas); y los derechos terminan si se incumple algún término. Ninguna fuente confirma que la redistribución de Gcenx esté autorizada para este uso (solo dice «asegúrate de cumplir la licencia de Apple»), y un cuadro de aceptación en la app no lo arreglaría.
- **Consecuencia (limitación conocida):** los juegos de Direct3D 12 necesitan D3DMetal; DXMT cubre Direct3D 10 y 11.
- **Puntos que quedaron sin resolver** (y que ya no importan para la versión pública, porque no se descarga nada): el paquete de Gcenx añade componentes a lo que publica Apple (por ejemplo un GStreamer.framework con libx264, que es GPL); es dudoso que jugar quede dentro de «desarrollar, probar o evaluar»; Apple ofrece ya Game Porting Toolkit 4, cuya licencia no se pudo leer (exige iniciar sesión de desarrollador).
- **Reglas para quien contribuya o publique:** no añadir descargas, sumas ni recetas que instalen D3DMetal o el kit; no incluirlos en el repositorio, en la app ni en un espejo; no describirlo como «gratis para jugar». Mantener la detección de las copias que ya existen.

## Marcas registradas y no afiliación

Altrune es un proyecto independiente. **No tiene relación con Apple, Valve, Rockstar Games ni el proyecto Wine**, y ninguna de esas organizaciones lo respalda. Los nombres se usan solo para indicar con qué programas funciona Altrune (uso nominativo) y pertenecen a sus dueños:

- «Wine» es nombre del proyecto Wine (WineHQ).
- «Apple», «macOS», «Metal», «Rosetta», «San Francisco», «Game Porting Toolkit» y «D3DMetal» son nombres o marcas de Apple Inc.
- «Steam» y «Valve» son marcas de Valve Corporation.
- «Rockstar Games» y «Rockstar Games Launcher» son marcas de Rockstar Games, Inc. / Take-Two Interactive.
- «GStreamer», «SDL», «libusb», «DXMT», «DXVK» y «MoltenVK» son nombres de sus proyectos.
- «Windows» es marca de Microsoft Corporation.

El nombre «Altrune» y su logotipo **no** están cubiertos por la licencia del código (sección 6 de Apache-2.0; ver [docs/marca.md](docs/marca.md)). El logotipo maestro se generó con una herramienta de IA (ver `docs/marca.md`); no se buscó si el nombre «Altrune» choca con alguna marca existente.

## Qué añadir si se distribuye un binario (`Altrune.app`)

Publicar solo el código fuente apenas crea obligaciones sobre terceros: el repositorio no contiene código ni binarios ajenos (salvo lo que baja `Cargo.lock` al compilar). **Distribuir un `.app` ya compilado sí las crea**, porque el binario lleva dentro las 118 bibliotecas y las tipografías. Estado de cada paso:

1. **Hecho: licencias dentro de la app.** `sh tools/empaquetar.sh` copia a `Altrune.app/Contents/Resources/Licencias/` el `LICENSE` (Apache-2.0), el `NOTICE`, este documento y el paquete de licencias de terceros `licencias/terceros.html`, que lleva los textos completos de todas las licencias (MIT, Apache-2.0, Zlib, ISC, Unicode-3.0, Ubuntu Font Licence 1.0, Bitstream Vera y SIL OFL 1.1), con el aviso de derechos de autor de cada biblioteca y el aviso de Chromium. Este documento solo nombra las licencias; **no basta** como aviso de un binario, por eso existe la página.
2. **Hecho: cómo se genera.** Con `cargo-about` (versión fijada en `tools/licencias.sh`), a partir de [about.toml](about.toml) y de la plantilla `licencias/plantilla.hbs`. Ver el apartado siguiente. Se ejecuta a mano antes de cada versión y se comprueba con `sh tools/licencias.sh --comprobar`, no en cada prueba.
3. **Hecho: elecciones anotadas.** La página dice que de `self_cell` se toma Apache-2.0, qué opción se toma cuando una biblioteca ofrece varias, y que las tipografías no se han renombrado ni modificado.
4. **Se mantiene: la `.app` sin motores dentro.** Incluir D3DMetal o el paquete de Gcenx está excluido por la política del proyecto (y exigiría permiso escrito de Apple); incluir el motor de highball-engine, el GStreamer o libusb obligaría a ofrecer su código fuente, los textos de licencia y la posibilidad de reemplazar las bibliotecas LGPL, y a responder por sus actualizaciones; el H.264 de GStreamer añade el riesgo de patentes.
5. **Pendiente: firma y notarización.** Hoy la app está firmada «ad hoc», que solo sirve en el Mac que la compiló. Los scripts para firmar con un ID de desarrollador (Developer ID) y notarizar están preparados, pero **no se han ejecutado**: este Mac no tiene ninguna identidad de Developer ID. Ver `docs/distribucion.md`.
6. **Hecho: compilación sin rutas del autor.** `sh tools/empaquetar.sh` compila con `--locked` y con las rutas del Mac del autor eliminadas, y falla si el binario conserva alguna.

### El paquete de licencias: cómo se hace

Archivos (todos en el repositorio, nada se genera en la máquina de quien compila la app):

| Archivo | Para qué sirve |
|---|---|
| [about.toml](about.toml) | Configuración de `cargo-about`: los dos destinos de macOS, las licencias aceptadas (las de [deny.toml](deny.toml)), sin dependencias de desarrollo, y las aclaraciones de `epaint_default_fonts` y `dpi` |
| `licencias/plantilla.hbs` | Plantilla de la página: HTML con estilos propios, sin scripts, tipografías ni peticiones externas; legible en claro y en oscuro |
| `licencias/terceros.html` | La página generada; **se guarda en el repositorio** y es la que se copia a la app |
| `tools/licencias.sh` | Regenera, comprueba o instala la herramienta |
| `tests/licencias.rs` | Comprueba el contenido de la página (bibliotecas clave, sin rutas del equipo, sin licencias desconocidas, tipografías) |

Pasos:

```sh
# Una vez, en una máquina nueva (es lo único que usa la red): instala la versión fijada de cargo-about
sh tools/licencias.sh --instalar "$HOME/herramientas-altrune"      # imprime la ruta del programa
export CARGO_ABOUT="$HOME/herramientas-altrune/bin/cargo-about"    # o ponlo en el PATH

cargo fetch --locked            # trae las bibliotecas de Cargo.lock a la caché de cargo (usa la red)
sh tools/licencias.sh           # regenera licencias/terceros.html (sin red); luego se guarda el cambio en el repositorio
sh tools/licencias.sh --comprobar   # falla si la página guardada no coincide con lo que sale hoy
```

Cuándo regenerar: cada vez que cambie `Cargo.lock`, `about.toml` o la plantilla. Dos ejecuciones seguidas dan el mismo archivo byte a byte (la página no lleva fechas ni rutas).

Qué hay que saber:

- **Elección entre licencias.** `about.toml` da prioridad a MIT, luego Apache-2.0, Zlib, ISC y el resto: cuando una biblioteca ofrece «A o B» se reproduce la primera de la lista. `self_cell` («Apache-2.0 OR GPL-2.0-only») usa Apache-2.0; la GPL no está en la lista y la generación falla si fuera la única salida.
- **Bibliotecas sin archivo de licencia (27).** Sin archivo, `cargo-about` pone el texto estándar con la marca `<year> <copyright holders>`. La plantilla lo sustituye por grupos con el titular real: egui (Emil Ernerfeldt), AccessKit (The AccessKit contributors), objc2 (Mads Marquart y Steven Sheldon), dispatch (Steven Sheldon) y profiling (Philip Degarmo y colaboradores). Una biblioteca nueva sin archivo de licencia que no esté en ningún grupo sale como «SIN CLASIFICAR» y el test falla hasta que se añada al grupo correcto de la plantilla.
- **Tipografías.** `epaint_default_fonts` se aclara con las sumas SHA-256 de sus cuatro archivos de licencia (Ubuntu Font Licence 1.0, Hack con Bitstream Vera, MIT de emoji-icon-font y SIL OFL 1.1). Si una versión nueva del crate cambia alguno de esos archivos, la aclaración deja de valer y la generación falla en vez de omitir una licencia. La OFL-1.1 corresponde a `NotoEmoji-Regular.ttf`, que el crate trae pero Altrune no compila.
- **Sin verificar.** Los titulares de las bibliotecas sin archivo de licencia se tomaron del archivo de licencia del repositorio de cada proyecto (rama principal, porque GitHub no resolvió las etiquetas exactas de algunas versiones) o, cuando el proyecto no tiene ninguno (`dispatch`, y AccessKit, cuyo `LICENSE-MIT` no lleva línea de derechos de autor), del campo `authors` de `Cargo.toml`. Solo se buscaron avisos de código derivado dentro de los archivos fuente de los crates de AccessKit (donde apareció el de Chromium); no se revisó el código fuente de los demás crates en busca de avisos propios.

## Cómo mantener este documento

Cada vez que cambie `Cargo.lock`, un componente descargado (`src/wine_setup/`) o el script `mando/compilar-mando.sh`, volver a generar la lista de bibliotecas (comando arriba) y el paquete de licencias (`sh tools/licencias.sh`), y revisar las direcciones y huellas. Un cambio de versión de un componente descargado obliga a volver a comprobar su licencia.
