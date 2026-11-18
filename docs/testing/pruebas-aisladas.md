# Pruebas de flujo aisladas

Las pruebas escriben en una carpeta temporal las mismas rutas que usa la app, también su carpeta de datos de `Application Support`.

`cargo test` recorre los flujos principales de la app de
principio a fin: la app real, sin ventana, contra un **Wine falso** y dentro de una carpeta
personal (`HOME`) temporal. Ninguna prueba puede leer ni cambiar tus botellas, tu configuración,
Steam ni el Wine real.

## Cómo se aíslan

* Cada flujo (`flow_test!` en `src/flows/`) se ejecuta en un **proceso hijo** del propio binario de
  pruebas, con `HOME` apuntando a una carpeta nueva dentro de la carpeta temporal del sistema. Todas
  las rutas de la app salen de `HOME`, así que todo lo que escribe cae ahí.
* El hijo se niega a arrancar si `HOME` no es esa carpeta temporal. El padre exige que se haya
  ejecutado exactamente una prueba (un filtro mal escrito no puede pasar en silencio), espera como
  mucho 90 s y, al terminar bien, borra solo esa carpeta. Si falla, la conserva y dice dónde está.
* El **Wine falso** (`src/flows/fake_wine.rs`) son dos scripts de shell (`wine` y `wineserver`) que
  responden a las órdenes reales de la app: crear la botella (`wineboot --init`), abrir `.exe`,
  `.msi` y `.lnk`, escribir el registro (`reg add`), `winecfg`, cerrar programas y
  `wineserver -k0`. No ejecutan código de Windows. Se niegan a actuar fuera de la carpeta temporal.
  Los instaladores de prueba se distinguen por el nombre: `instalador-prueba.exe` instala «Programa
  de prueba», `instalador-falla.exe` termina con el código 3, `instalador-lento.exe` y
  `programa.exe` siguen abiertos hasta que se cierran, y `se-cierra.exe` falla en un segundo.
* Bajo `cfg(test)` la app **nunca** descarga nada, ni abre Finder, Terminal, diálogos nativos de
  archivo o la contraseña de administrador, ni crea apps en Launchpad, ni busca el Wine instalado en
  el Mac. Estas guardas no existen en el binario normal (`cfg!(test)` vale falso fuera de las
  pruebas).
* Al terminar, cada flujo cierra los programas falsos que quedaran y comprueba que no queda ningún
  proceso zombi.

## Qué cubren

| Archivo | Qué comprueban |
|---|---|
| `main_flows.rs` | primer arranque sin Wine (no descarga nada), crear botella, instalación guiada que encuentra el programa, instalador que falla, abrir y cerrar un programa, doble pulsación, teclado y ratón, ajustes aplicados o no |
| `instalador_flows.rs` | ningún paso marcado antes de tiempo, cerrar el instalador, archivos soltados con un diálogo abierto, resultado en unos 2 s, fallo interno durante la preparación |
| `inicio_flows.rs` | sin Wine «Nueva botella» se niega y dice por qué, una sola acción principal en el primer arranque, textos de la tarjeta de Wine, diálogos que caben en 820×540 |
| `botella_flows.rs` | botella incompleta con explicación y salida, errores visibles en Inicio, «Abriendo…» en la ficha, botones que piden archivo sin diálogo nativo |
| `keyboard_flows.rs` | Tab lleva el foco a la vista, todas las paradas tienen nombre, Mayús-Tab, Escape, flechas en las pestañas, sin trampas de teclado |
| `finder_flows.rs` | una sola app a la vez: la petición de Finder llega a la ventana abierta |
| `programas_flows.rs` | doble clic en Finder nunca abre en una botella a medio crear, marcas de arranque liberadas tras un fallo interno |
| `palabras_flows.rs` | un nombre por motor, sin errores del sistema en inglés |
| `soak.rs` | sesión larga: 100 aperturas y cierres, diálogos, botellas creadas, renombradas, duplicadas y borradas; memoria de la app, descriptores, hilos y zombis vuelven a su nivel |
| `iconos_flows.rs` | la imagen de una botella sale del icono de su programa y deja de usarse cuando el programa se quita |
| `tema_flows.rs` | claro, oscuro o según el sistema: se aplica al momento, se guarda y se conserva; un valor mal escrito en el archivo arranca como el Mac |
| `marca_flows.rs` | la barra lateral dice Altrune y dibuja su símbolo una vez, Ajustes termina con «Acerca de Altrune» y su logotipo cabe en la ventana más pequeña; además, sin flujo: ningún texto visible usa un nombre que no sea Altrune |
| `apple_flows.rs` | D3DMetal y el Game Porting Toolkit de Apple nunca se descargan: donde faltan las pantallas explican por qué, y donde ya están instalados se usan |
| `capas_flows.rs` | el entorno completo de un motor DXMT de prueba no cambia; además, sin flujo: las capas importan en un solo sentido y el escaneo detecta una infracción sembrada |

Además, `tests/paquete.rs` (sin Wine ni ventana) comprueba los archivos que forman `Altrune.app`: claves de `macos/Info.plist`, que el identificador `com.altrune.app` no reutiliza los de los lanzadores y del visor de Finder, que el icono nombrado está en `assets/branding`, que la plantilla rellena es un plist válido (`plutil`), que `tools/empaquetar.sh` firma al final, solo reemplaza `dist/Altrune.app` y no instala ni abre nada, y que `/dist/` está ignorado. El paquete en sí lo construye `sh tools/empaquetar.sh`, que no forma parte de `cargo test`.

Las demás pruebas de `tests/` tampoco usan Wine ni ventana ni red, y no firman con una identidad real, no notarizan ni publican nada. Leen archivos del repositorio. Las que ejecutan un script lo hacen en una carpeta temporal, y la notarización se prueba con herramientas simuladas. Una prueba de `tests/firma.rs` firma ad hoc un paquete mínimo de prueba (solo macOS; necesita `cc`, de las herramientas de línea de órdenes de Xcode).

| Archivo | Qué comprueba |
|---|---|
| `tests/dmg.rs` | el instalador: `tools/crear-dmg.sh` y `tools/dmg/` (dependencias fijadas por suma; el script no usa `SetFile` (herramienta de Apple en desuso), no firma, no abre Finder y no toca `/Applications`; el nombre de la imagen sale de `Cargo.toml`), el fondo de `assets/dmg/` (tamaños y frase acordada) y que la ventana coincide con el diseño; no construye ninguna imagen (eso lo hace `sh tools/crear-dmg.sh`) |
| `tests/firma.rs` | `tools/firmar.sh`, `tools/notarizar.sh`, `tools/llavero-ci.sh`, los permisos de `macos/Altrune.entitlements` y `docs/firma-y-notarizacion.md`: ninguna credencial se imprime, la identidad ad hoc se rechaza, el llavero solo se toca en CI y la notarización se prueba con `codesign`, `xcrun` y `spctl` falsos |
| `tests/licencias.rs` | `licencias/terceros.html`, `about.toml`, la plantilla y `tools/licencias.sh`: bibliotecas y tipografías presentes, sin licencias desconocidas ni rutas de la máquina, una sola versión fijada de `cargo-about`; que la página coincida con lo que sale hoy lo comprueba `sh tools/licencias.sh --comprobar` |
| `tests/release.rs` | `.github/workflows/release.yml` leído como texto (solo disparo manual, permisos de solo lectura salvo el trabajo del borrador, acciones fijadas por commit, nunca publica) y `tools/notas-version.sh` sobre una imagen falsa |

## Ejecutar

```
cargo test --locked   # unos 13 s: los flujos corren en paralelo
```

Nunca `cargo test -- --ignored`: esas pruebas abren el Wine real, Steam o Rockstar.

## Lo que necesita una persona

Las pruebas no pueden comprobar lo que depende del Wine real, de la pantalla o de macOS. Ver la
lista «Pendiente y sin verificar» en [`docs/roadmap.md`](../roadmap.md#pendiente-y-sin-verificar).
