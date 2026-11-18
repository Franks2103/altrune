# Distribución de Altrune

Esta guía explica qué significa publicar Altrune, qué contiene hoy la app empaquetada, qué reglas
seguir antes de entregar un binario a otras personas y cómo reproducir una compilación. No es
asesoramiento legal: el código de Altrune es Apache-2.0 ([LICENSE](../LICENSE), [NOTICE](../NOTICE))
y las licencias de los componentes de terceros están en [THIRD_PARTY_NOTICES.md](../THIRD_PARTY_NOTICES.md).
Todavía no hay ninguna versión binaria publicada. La política de versiones (apartado 3) deja que un
flujo manual prepare un borrador, pero no que publique nada.

## 1. Publicar el código frente a distribuir una app

Son dos cosas distintas, con obligaciones distintas.

**Publicar el código fuente** es subir el repositorio. El repositorio no contiene ningún programa
de terceros: ni Wine, ni DXMT, ni GStreamer, ni D3DMetal. Solo contiene el código de Altrune (bajo
Apache-2.0), los scripts que descargan o compilan cosas, las imágenes de la marca y la documentación. Quien quiera
usar Altrune lo compila en su Mac (`cargo build --release`) y el programa descarga los motores
cuando la persona lo pide.

**Distribuir una app (`Altrune.app`)** es entregar un binario ya compilado. Eso añade:

- **Firma.** `tools/empaquetar.sh` firma con una firma *ad hoc* (`codesign --sign -`). Esa firma
  solo vale en el Mac que la hizo: en cualquier otro Mac, Gatekeeper (la protección de macOS)
  bloquea una app descargada con ese tipo de firma.
- **Para otros Macs** hacen falta, en este orden (los pasos 3 a 5 ya tienen script; el orden
  completo, las credenciales y los comandos están en
  [firma-y-notarizacion.md](firma-y-notarizacion.md)):
  1. Una cuenta del programa Apple Developer (de pago).
  2. Un certificado **Developer ID Application**, guardado en el llavero de quien firma.
  3. Firmar con `codesign --options runtime --timestamp` (el *runtime reforzado*), con los
     permisos (*entitlements*) que la app realmente necesite: `tools/firmar.sh` lo hace con
     `macos/Altrune.entitlements`, que tiene una sola clave (la automatización de Terminal, para
     el botón que abre una consola de la botella). Hay que comprobar con una app firmada que
     arrancar Wine y cargar sus bibliotecas sigue funcionando bajo el runtime reforzado.
  4. Enviar la app a **notarización** con `xcrun notarytool submit ... --wait` y, si Apple la
     acepta, grapar el resultado con `xcrun stapler staple`: `tools/notarizar.sh`.
  5. Empaquetar el resultado en una imagen de disco: `tools/crear-dmg.sh` hace el `.dmg`, y la
     imagen se firma y se notariza con los mismos dos scripts. Después hay que comprobarla en un
     Mac limpio descargándola como lo haría otra persona.
- **Avisos de terceros.** Un binario contiene código de terceros (ver el apartado 3, punto 4).

**Qué está hecho y qué no.**

- Hecho y comprobado en el Mac de desarrollo: empaquetar sin rutas personales y con las licencias
  dentro; crear y verificar el `.dmg` (formato, montaje de solo lectura, contenido exacto, firma
  de la app de dentro); generar y comprobar la página de licencias; firmar una copia con el runtime
  reforzado en modo de prueba (`tools/firmar.sh --prueba`, firma ad hoc) y arrancarla.
- Escrito pero **nunca ejecutado con una identidad real**: `tools/firmar.sh` (Developer ID),
  `tools/notarizar.sh`, `tools/llavero-ci.sh` y el flujo `.github/workflows/release.yml`. Este Mac no
  tiene ninguna identidad de firma, así que no se ha firmado ni enviado nada a Apple. Esos scripts
  se comprobaron con herramientas simuladas y con pruebas que leen su texto.
- Sin comprobar: que Wine, DXMT y GStreamer arranquen desde una Altrune firmada con Developer ID, que
  el botón de Terminal pida el permiso de automatización en la app firmada, y abrir la imagen
  descargada en un Mac limpio (con y sin notarización).

Firmar en CI exige guardar el certificado y las credenciales de notarización como secretos. Por eso
no se hace en `ci.yml`, que no tiene acceso a ningún secreto, sino en un flujo aparte, manual y
protegido: el apartado 5.

## 2. Qué contiene hoy `dist/Altrune.app`

`tools/empaquetar.sh` construye `dist/Altrune.app` (la carpeta `dist/` está en `.gitignore`: nunca
se sube al repositorio). El paquete contiene solo:

| Ruta dentro del paquete | Qué es |
|---|---|
| `Contents/MacOS/Altrune` | El binario de Altrune compilado en modo release, con las rutas de la máquina que lo compila eliminadas |
| `Contents/Resources/Altrune.icns` | El icono, copiado de `assets/branding/` |
| `Contents/Resources/Licencias/` | `LICENSE`, `NOTICE`, `THIRD_PARTY_NOTICES.md` y `terceros.html` (la página de licencias de las bibliotecas y las tipografías, de `licencias/`) |
| `Contents/Info.plist` | La plantilla `macos/Info.plist` con el núcleo numérico de la versión de `Cargo.toml` (`0.1.0` para `0.1.0-alpha`) y el macOS mínimo leído del binario |
| `Contents/_CodeSignature/` | La firma ad hoc que añade `codesign` |

La versión completa (`0.1.0-alpha`) no cabe en el `Info.plist`, que solo admite números y puntos:
la usan el nombre del `.dmg`, la etiqueta `v0.1.0-alpha` de la publicación y la sección «Acerca de Altrune» de Ajustes.
Si falta `licencias/terceros.html`, `empaquetar.sh` avisa y sigue; `crear-dmg.sh` se niega a hacer la
imagen sin esa página (salvo con `--prueba`, que es solo para pruebas y nunca para publicar).

Junto a la app, `dist/` guarda otros archivos que **no** forman parte de ella: `icono-comprobado.png`
(el icono que dibujará Finder, para mirarlo), y las salidas de `tools/crear-dmg.sh`:
`Altrune-<versión>.dmg` y `Altrune-<versión>.dmg.sha256`.

**No incluye** Wine, DXMT, GStreamer, D3DMetal, SDL ni ningún otro motor o capa. Los que se pueden
descargar (Wine, DXMT, GStreamer) los descarga la propia app, en la carpeta de datos de la persona,
y se comprueban con una suma SHA-256 cuando la versión está fijada en el código. D3DMetal no se
descarga nunca (ver el apartado 3, punto 2).

Las tipografías por defecto de egui van **dentro del binario** (Ubuntu-Light, Hack y un
subconjunto de iconos), con sus licencias propias; ver el apartado 3, punto 4. La tipografía del sistema
de Apple (San Francisco) no se incluye: la app la lee del Mac de cada persona y no la copia a
ninguna parte.

## 3. Política de publicación de versiones

1. **No se publica ninguna versión que incluya componentes de terceros sin permiso confirmado
   para distribuirlos.** Hoy no se incluye ninguno y debe seguir así salvo decisión expresa.
2. **D3DMetal y el paquete Game Porting Toolkit de Apple no se descargan, no se instalan, no se
   incluyen y no se reenvían.** La versión pública de Altrune no tiene ninguna descarga de ellos:
   ni del repositorio de Gcenx ni de ningún otro sitio. El acuerdo de Apple (el texto de 2023, el
   único que se pudo leer) limita el uso a desarrollar, probar o evaluar juegos y la distribución a
   fines no comerciales, y no tiene una cláusula de código abierto; ninguna fuente confirma que la
   redistribución de un tercero esté autorizada para este uso, y una casilla de aceptación en la
   app no lo arreglaría. Las descargas actuales de Apple pueden traer un acuerdo más nuevo que no
   se ha podido leer, porque exige iniciar sesión como desarrollador. Esto no es asesoría legal.
   Lo que sí hace la app es **detectar y usar** una capa D3DMetal o un motor «Juegos DirectX 12»
   que la persona ya tenga en su Mac, sin tocarlos. Consecuencia conocida: los juegos de
   Direct3D 12 necesitan D3DMetal, y DXMT solo cubre Direct3D 10 y 11.
3. **Los motores de Wine, DXMT, GStreamer y SDL no se empaquetan dentro de `Altrune.app`.** Si
   algún día se quisiera, haría falta antes: los textos de licencia, el código fuente
   correspondiente (o una oferta escrita de código fuente) en las licencias LGPL, y una revisión
   de la parte de patentes de los códecs de vídeo. Es una decisión aparte, no un cambio de
   script.
4. **Un binario publicado lleva sus avisos.** Debe viajar con él, y hoy viaja dentro de la propia
   app, en `Contents/Resources/Licencias/`, que `tools/empaquetar.sh` rellena con:
   - `terceros.html`, la página de licencias (`licencias/terceros.html`, generada con
     `tools/licencias.sh`), con los textos completos de las licencias de las bibliotecas de Rust
     (MIT, Apache-2.0 con sus ficheros NOTICE, BSD, ISC, Zlib, Unicode-3.0), su aviso de copyright, el
     aviso de Chromium que traen las bibliotecas de AccessKit y los textos de las licencias de las
     tipografías incluidas: Ubuntu Font Licence 1.0, la de Hack (MIT y Bitstream Vera) y la de
     emoji-icon-font (MIT);
   - `THIRD_PARTY_NOTICES.md`, que explica qué hay y por qué;
   - la licencia propia de Altrune (`LICENSE`, Apache-2.0) y el `NOTICE`, que las licencias Apache
     exigen conservar al redistribuir (sección 4 de Apache-2.0); un binario modificado debe
     además indicar los cambios y usar otro nombre y logotipo (sección 6).

   La página de licencias hay que regenerarla cada vez que cambia `Cargo.lock` (apartado 4).
5. **Compilar sin rastros personales.** El binario de release incluye rutas del registro de
   `cargo` (`/Users/<nombre>/.cargo/...`) en los mensajes de error de las dependencias.
   `tools/empaquetar.sh` compila ya con esas rutas reescritas (`--remap-path-prefix`) y, al terminar,
   mira con `strings -a` si el binario aún contiene `/Users/`, la carpeta personal o el nombre de
   usuario: si es así, falla. Comprobado: sin el remapeo el binario local traía unas 490 rutas con
   `/Users/`; con él, ninguna. Lo ideal sigue siendo compilar la versión en una máquina limpia (un
   ejecutor de CI) y no en el Mac personal. La carpeta `dist/` local no se comparte nunca.
6. **Nada se publica automáticamente.** Un flujo de trabajo manual (`release.yml`, apartado 5) puede
   crear, o renovar, un **borrador** de versión previa (*draft pre-release* en GitHub) con el `.dmg`, su suma
   y las notas; ningún flujo publica una versión, y publicarla es un paso manual de quien mantiene el
   proyecto, después de leer el borrador. Si ya existe una versión publicada con esa etiqueta, el
   flujo se detiene sin tocarla. Ningún otro flujo de trabajo (`ci.yml` incluido) crea versiones,
   sube binarios ni publica paquetes.

## 4. Reproducibilidad

- **Dependencias.** `Cargo.lock` está en el repositorio y las órdenes de CI usan `--locked`: si
  `Cargo.toml` y `Cargo.lock` no coinciden, la compilación falla en vez de cambiar versiones en
  silencio. Para compilar en local conviene usar también `cargo build --release --locked`.
- **Versión de Rust.** `eframe` 0.37 necesita Rust 1.95 o más reciente. El CI instala con
  `rustup` (sin acciones de terceros) la versión fija **1.100.0**, con `rustfmt` y `clippy`: es
  la versión con la que se verificaron todos los resultados, y así una versión nueva de Rust no
  pone en rojo el CI de una contribución con avisos de `clippy` que no tienen que ver con ella.
  Para subirla, se cambia a la vez en `.github/workflows/ci.yml` y en `.github/workflows/release.yml`
  (una prueba comprueba que coinciden) en un commit propio, después de corregir lo que digan los
  avisos nuevos. No hay `rust-toolchain.toml`: fijarlo haría que `rustup`
  descargara esa versión en el Mac de cualquier persona que compile; si el mantenedor prefiere esa
  garantía, es un fichero de tres líneas (`[toolchain]`, `channel = "1.100.0"`,
  `components = ["rustfmt", "clippy"]`). Añadir `rust-version = "1.95"` a `Cargo.toml` haría que
  `cargo` diera un error claro con un Rust demasiado viejo, pero esa versión mínima no se ha
  probado.
- **Las imágenes de la marca.** `tools/marca/generar.sh` regenera todo `assets/branding/`
  (símbolo, palabra, versiones de 128 y 512 píxeles, `AppIcon-1024.png` y `Altrune.icns`) a
  partir del logotipo maestro, que solo lee. Está comprobado que ejecutarlo sobre el árbol limpio
  da ficheros idénticos byte a byte: `git status` queda vacío después. Necesita macOS (Swift,
  `iconutil`, `sips`); no se ejecuta en CI.
- **Las órdenes que ejecuta el CI**, las mismas que debe pasar cada cambio:
  `cargo fmt --all --check`, `cargo clippy --locked --all-targets -- -D warnings`,
  `cargo test --locked` y `cargo build --release --locked`. Nunca se usa `--ignored`: esas
  pruebas descargan motores reales o necesitan un Wine real y se ejecutan solo a mano.
- **Binarios idénticos bit a bit** no están garantizados (cambian con la versión de Rust y de
  Xcode). Lo reproducible es el resultado de las pruebas y el contenido del paquete.
- **El paquete.** `tools/empaquetar.sh` compila con `--locked` en su propia carpeta,
  `target/empaquetar` (no toca `target/release`), y quita las rutas de la máquina con
  `--remap-path-prefix`, que pasa por `CARGO_ENCODED_RUSTFLAGS` (la opción `trim-paths` de Cargo
  sigue siendo inestable en Rust 1.100.0). Por eso se niega a seguir si `RUSTFLAGS`,
  `CARGO_ENCODED_RUSTFLAGS` o `CARGO_BUILD_RUSTFLAGS` están definidas: las descartaría sin avisar.
- **La página de licencias.** `licencias/terceros.html` se genera con `cargo-about` 0.9.3, fijado en
  `tools/licencias.sh`, sin red y con la versión de cada biblioteca de `Cargo.lock`: dos
  ejecuciones dan el mismo archivo byte a byte. Cada cambio de `Cargo.lock` (también una
  actualización de Dependabot) obliga a regenerarla con `sh tools/licencias.sh` y a guardar el
  resultado. `sh tools/licencias.sh --comprobar` falla si la página guardada ya no coincide; el
  flujo de versión y `ci.yml` lo ejecutan (así un cambio de `Cargo.lock` que deja la página vieja se
  ve en la solicitud de cambios, no al preparar una versión); `tests/licencias.rs` solo lee la
  página, no la regenera. En una máquina limpia hay que ejecutar antes `cargo fetch --locked`, porque la
  generación no usa la red; la herramienta se instala con `sh tools/licencias.sh --instalar DIR`.
- **El instalador.** `tools/crear-dmg.sh` usa `dmgbuild` 2.0.0 y sus dependencias, fijados por suma
  en `tools/dmg/requirements.txt` e instalados solo desde ruedas ya compiladas
  (`--require-hashes --only-binary=:all:`) en `target/dmg-venv`; necesita Python 3.11 o posterior y
  red la primera vez. La imagen **no** es idéntica byte a byte entre dos compilaciones (cambian las
  fechas y el UUID del volumen HFS+ y la fecha de creación del fondo), pero su contenido sí. Por eso
  la suma SHA-256 se calcula del archivo final y no se puede predecir; si la imagen se firma y se
  notariza después, la suma se vuelve a escribir (ver
  [firma-y-notarizacion.md](firma-y-notarizacion.md)). El fondo sale de `assets/dmg/fondo.svg` con
  `sh tools/dmg/generar-fondo.sh`, que sí da los mismos archivos cada vez.

### `tools/empaquetar.sh`

Compila (`cargo build --release --locked`, en `target/empaquetar`, con las rutas de la máquina
eliminadas), arma `dist/Altrune.app` con los textos de licencia dentro y lo firma ad hoc. Solo
sustituye `dist/Altrune.app` y deja junto a ella `dist/icono-comprobado.png` (el icono que dibujará
Finder, para mirarlo); ese PNG no forma parte de la app y no debe repartirse. No instala nada, no
abre la app y no toca `/Applications`. Después comprueba:

- que el binario no contiene `/Users/`, la carpeta personal ni el nombre de usuario como carpeta
  de una ruta;
- que el `Info.plist` es válido, con su identificador (`com.altrune.app`), versión y macOS mínimo;
- que la firma verifica y sella los recursos;
- que el icono y el ejecutable están donde dice el plist;
- que el binario empaquetado arranca con `--medir --sin-wine` en un `HOME` desechable.

Termina pidiendo al sistema el icono que dibujará Finder (necesita Swift y una sesión de
escritorio). Si lo ejecuta una persona, un fallo ahí detiene el script; en un ejecutor de CI
(variable `CI`) solo avisa por la salida de error y sigue, porque es un paso para mirar y no hay
sesión de escritorio garantizada. La primera ejecución real en GitHub será la primera prueba de
ese paso.

### `tools/crear-dmg.sh`

Hace `dist/Altrune-<versión>.dmg` y su `.sha256` a partir de `dist/Altrune.app` (u otra app con
`--app`), tal como esté: firmada o no, la copia sin tocarla. Formato UDZO (zlib, solo lectura)
sobre HFS+, con un volumen del tamaño de la app más 24 MiB. Dentro hay solo `Altrune.app`, un
acceso a `/Applications` y los archivos de diseño de la ventana. Después comprueba: `hdiutil verify`,
el montaje de solo lectura, que el contenido es exactamente ese y sin basura, `codesign --verify
--deep --strict` sobre la app montada, que el ejecutable tiene el mismo modo y SHA-256 que el original,
que no hay rutas personales, y desmonta el dispositivo que montó. Nunca firma, nunca abre Finder y
nunca copia nada a `/Applications`. Se niega sin `terceros.html` en la app (salvo con `--prueba`).
No hay captura de la ventana en el repositorio; `tools/dmg/captura.sh` la hace en un Mac con sesión
de escritorio, permiso de grabación de pantalla y una ventana de Finder visible.

### `tools/licencias.sh`

Regenera (sin argumentos), comprueba (`--comprobar`) o instala la herramienta (`--instalar DIR`)
de la página `licencias/terceros.html`. Ver el apartado 4.

### `tools/firmar.sh`, `tools/notarizar.sh` y `tools/llavero-ci.sh`

Firman con Developer ID (runtime reforzado, marca de tiempo y los permisos de
`macos/Altrune.entitlements`), notarizan y graban el ticket, y crean y borran el llavero temporal de
un ejecutor de CI. **No se han ejecutado con una identidad real.** `firmar.sh --prueba` firma una
copia con la firma ad hoc más el runtime reforzado, solo para comprobar que Altrune arranca así.
`llavero-ci.sh` se niega a ejecutarse fuera de un ejecutor de CI. Todo en
[firma-y-notarizacion.md](firma-y-notarizacion.md).

### `tools/instalar.sh`

Copia `dist/Altrune.app` a una carpeta de aplicaciones (por defecto `/Applications`). Antes
comprueba que el origen es Altrune y que su firma verifica; solo sustituye una app que sea
también Altrune, y se niega mientras esa app está abierta. Hace la copia al lado, la verifica y
solo entonces la intercambia; si algo falla, la app instalada queda como estaba. No abre la app
ni lee los datos de nadie.

### Revisión de los scripts

Se revisaron (antes de añadir el instalador, la firma y las licencias) `tools/empaquetar.sh`, `tools/instalar.sh`, `tools/hogar-aislado.sh`,
`tools/marca/generar.sh` y `mando/compilar-mando.sh` buscando datos privados o de una máquina
concreta. No hay nombres de usuario, rutas `/Users/...`, correos, claves ni cuentas: las rutas se
construyen con `$HOME`, `$ROOT` o `mktemp`.

Cambio de comportamiento: `tools/empaquetar.sh` ya no lee `./target/release`: compila en
`target/empaquetar` con `--locked`, fijando él mismo `CARGO_TARGET_DIR`. Quien dependiera de que
refrescara `target/release/altrune` debe usar `target/empaquetar/release/altrune`.

Observaciones, ninguna cambiada porque ninguna es una fuga de datos:

- `mando/compilar-mando.sh` menciona en un comentario el modelo del Mac y del mando con los que
  se verificó (Apple M5, macOS 27.0.1, un mando con cable de 8BitDo). Es información de «probado
  en», sin números de serie.
- Los scripts usan la misma carpeta de datos que la app (`src/backend/paths.rs`).
- `mando/compilar-mando.sh` fija SDL y sdl2-compat por etiqueta de Git, no por código de
  *commit*: una etiqueta se puede mover. libusb sí está fijado por SHA-256.
- `tools/empaquetar.sh` y `tools/hogar-aislado.sh` solo aceptan carpetas temporales del sistema
  (`/tmp`, `/var/folders`...) por seguridad; por eso el CI no redefine `TMPDIR`.
  `hogar-aislado.sh` no acepta `/private/var/folders`, que `empaquetar.sh` sí.
- `tools/marca/generar.sh` trabaja en una carpeta temporal de `mktemp` que borra al terminar.

## 5. Publicar una versión

`.github/workflows/release.yml` (nombre «Release») es el único flujo que puede preparar una versión.
Se lanza **a mano** (`workflow_dispatch`) desde la rama `main`, con la versión completa como entrada
(por ejemplo `0.1.0-alpha`). Esa versión debe ser igual a la de `Cargo.toml` y tener sus notas en
`docs/releases/<versión>.md`. **Todavía no se ha ejecutado nunca**: el repositorio es privado y no
se publica nada sin la persona dueña del proyecto.

Tres trabajos, en orden:

1. **Versión y comprobaciones** (`check`). Rechaza una rama que no sea `main`, una versión ya
   publicada y una etiqueta que apunte a otro commit; después ejecuta `cargo fmt --all --check`,
   `cargo clippy --locked --all-targets -- -D warnings` y `cargo test --locked`.
2. **Compilar, firmar y empaquetar** (`package`, en el entorno `release`). Instala `cargo-about`,
   trae las bibliotecas con `cargo fetch --locked`, ejecuta `sh tools/licencias.sh --comprobar`,
   `tools/empaquetar.sh` y `tools/crear-dmg.sh`, escribe la suma SHA-256 y las notas
   (`tools/notas-version.sh` con `docs/releases/<versión>.md`). Si existen los seis secretos del
   entorno, antes de la imagen firma y notariza la app y después firma y notariza la imagen
   (con un llavero temporal que borra al final) y vuelve a escribir la suma. Los secretos solo
   existen en este trabajo.
3. **Borrador** (`release`). El único trabajo con permiso de escritura. Crea un borrador de
   versión previa `v<versión>` con el `.dmg`, el `.sha256` y las notas, o, si ya hay un borrador de
   esa etiqueta, sustituye sus archivos y sus notas. Si ya existe una versión **publicada** con esa
   etiqueta, falla sin cambiar nada. Nunca la publica.

**Sin los seis secretos** el flujo no falla: produce una compilación de prueba **sin firmar** y lo
dice en el título del borrador («Altrune 0.1.0-alpha (versión de prueba sin firmar)») y al principio
de las notas. Esa imagen no se abre con doble clic en otro Mac (ver
[firma-y-notarizacion.md](firma-y-notarizacion.md)).

**Qué hace la persona dueña.** Crear el entorno `release` en la configuración del repositorio, con
al menos una persona que deba aprobar cada ejecución y con las ramas permitidas limitadas a `main`;
guardar allí los seis secretos (la lista y cómo obtenerlos están en la guía de firma); lanzar el
flujo; abrir el borrador en GitHub, leer las notas y, solo si todo está bien, **publicarlo a mano**.
Para preparar una versión nueva: subir la versión en `Cargo.toml` (y `Cargo.lock`), escribir
`docs/releases/<versión>.md`, regenerar `licencias/terceros.html` si cambió algo y pasar las
comprobaciones. `sh tools/notas-version.sh <versión> <imagen> firmado|sin-firmar` imprime las notas
en local, sin subir nada.
