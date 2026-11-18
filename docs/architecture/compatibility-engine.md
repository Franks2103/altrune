# Arquitectura del motor de compatibilidad

Responsabilidades por módulo:

| Módulo | Papel | Notas |
|---|---|---|
| `wine_setup/` — **Engine Manager** | `detect` (Wine que realmente arranca, Rosetta), `engines` (qué motor, de dónde se baja), `download` (descargar y comprobar la suma SHA-256 fijada o la que da GitHub), `swap` (cambiar por renombres sin dejar un motor a medias, recuperar instalaciones interrumpidas, limpiar restos), `install` (los dos instaladores), `layers`/`d3d12`/`video` (añadir DXMT, D3DMetal con `LOADER_SCRIPT`, GStreamer). Dónde vive cada motor lo sabe `backend::engine_layout` | Reinstalar ya no borra el motor que funciona antes de tener el nuevo. `progress::Children` registra `curl` y `tar`: cerrar la ventana los termina (la firma del motor no se corta); una descarga estancada falla a los 60 s; al arrancar se limpian restos de instalaciones interrumpidas, solo con nombres propios de la app |
| `backend/` — **Bottle Manager** | `bottles` (escanear la carpeta de botellas de Altrune, nombres saneados, prefijo completo; lee el archivo de ajustes de la botella, `cxbottle.conf`, para la versión de Windows y `WINEMSYNC`), `settings` (`config.json` escrito aparte y renombrado; uno ilegible se guarda con fecha; los campos que ya no existen se ignoran al leer), `environment` (entorno de Wine de cada botella), copias verificadas, Papelera y errores en español | Las botellas viven solo en la carpeta de datos de Altrune; Wine nunca se ejecuta en un prefijo de fuera |
| `backend/programs/` + `shell_link` — **Biblioteca** | lee los `.lnk` (formato MS-SHLLINK, con límites comprobados), no sigue enlaces fuera de la botella, identidad por destino, ruido por palabras completas, búsqueda sin tildes | Los desinstaladores quedan en «Ocultos» |
| `backend/processes` + `environment` — **Process Manager** | entorno por programa (sin cambios), `is_running` (`wineserver -k0`), `is_running_at` (rápido: mira el socket `/tmp/.wine-<uid>/server-<dev>-<inode>/socket` y solo pregunta a Wine si existe), cierre (`quit_bottle`, 60 s) | Las decisiones (cerrar, reiniciar, copiar) siempre preguntan a Wine |
| `backend/engine_layout` — **Motores en disco** | dónde viven los Wine libres y sus capas (DXMT, D3DMetal, GStreamer, mandos) y el entorno que necesita cada uno (`runtime_env`) | Solo lectura: `backend` lo usa al armar cada orden de Wine y `wine_setup` al instalar; antes vivía en `wine_setup` y obligaba a un ciclo entre las dos capas |
| `app/` — **Estado y acciones** | `state` (el estado de la app), `msg` (lo que avisan los hilos), `action` (las acciones y su despacho, `apply`), `worker` (`App::task` y la recuperación de pánicos), `frame` (lo que pasa en cada cuadro), `programs` (abrir, guardia de doble arranque, juicio de salidas, «Reiniciar Steam»), `engines`, `bottles`, `install` (instalación guiada: `draft` el borrador del diálogo, `flow` el recorrido), `instance` (una sola app: cerrojo en la carpeta de datos y peticiones de Finder en `abrir/`), `diagnose`, `library` (lanzadores de uno en uno), `settings`, `notices`, `messages` (`poll`) | Todo lo que espera a un proceso va por `App::task`, que sobrevive a un pánico y libera la botella, su marca de arranque y la tarjeta de instalación |
| `domain/` — **Modelo** | `engines` (qué pueden hacer los motores y cómo se llaman), `install` (clasificar un archivo por su nombre y sus primeros bytes, proponer botella, etapas de la instalación), `status` (un estado por botella desde hechos, con su tono y su símbolo sin colores: la interfaz los traduce), `step` (los pasos de instalar Wine) | Funciones puras con pruebas; no importan nada del resto del crate |
| `backend/recipes` — **Recipe Manager** | ajustes de registro propios de algunos programas (Steam, el launcher de Rockstar) antes del instalador o del primer arranque | Se anota como aplicada solo cuando se escribió |
| `diagnostics/` — **Diagnósticos** | reglas deterministas con confianza (Comprobado/Probable/Pista), atribución por botella y por intento, coincidencias guardadas fuera de la ventana de 400 líneas, informe con datos personales conocidos ocultos | Sin servicios externos |
| `backend/launchers` — **Integración macOS** | apps por programa en su carpeta de `~/Applications` | Dos programas con el mismo nombre tienen apps distintas |
| `flows/` (solo pruebas) — **Pruebas de flujo** | la app real sin ventana, en un proceso hijo con `HOME` temporal, contra un Wine falso; conducida por acciones, teclado y ratón, leída por AccessKit | Ver `docs/testing/pruebas-aisladas.md` |
| `tools/empaquetar.sh` + `macos/` — **Paquete** | compila con `--locked` sin rutas personales, monta, firma ad hoc y comprueba `dist/Altrune.app` con las licencias de terceros dentro; `tools/crear-dmg.sh` hace y verifica el instalador `.dmg` | No instala nada ni abre la app; firma ad hoc solo para este Mac; la firma con Developer ID y la notarización (`tools/firmar.sh`, `tools/notarizar.sh`) están preparadas, sin ejecutar |
| `main.rs` + `startup.rs` — **Arranque** | `main.rs` solo declara las capas y lo que comparte todo el crate; `startup` lee la línea de órdenes (`--screenshot`, `--medir`, `--reposo`, `--open`), resuelve las peticiones de Finder sin ventana, toma el cerrojo de una sola app y abre la ventana | El orden de arranque es el de siempre |
| `measure/` — **Medición** | `--medir` (tiempos de lectura, detección, registro y memoria sin ventana) y `perf` (tiempos de cada cuadro) | Herramienta de desarrollo dentro del binario |
| `ui/` — **Interfaz** | tema con contraste comprobado (4,5:1), componentes con nombres y roles para VoiceOver (AccessKit), pantallas que solo leen estado | Nada lee el disco por cuadro |

## Capas

Las capas importan en un solo sentido: `domain` y `diagnostics` no importan nada del resto del crate; `backend` puede usar esas dos; `wine_setup` puede usar `backend`, `domain` y `diagnostics`; `app`, `ui`, `startup` y `measure` pueden usar todo. Ninguna capa de abajo nombra a una de arriba, así que no hay ciclos y `domain` se prueba sin egui ni disco. Lo comprueba `src/flows/capas_flows.rs` sobre el código que se distribuye (las pruebas sí pueden mirar hacia arriba); la misma prueba fija el entorno completo de un motor DXMT de prueba, variable por variable y en orden.

## Decisiones

1. **Las botellas viven solo en la carpeta de Altrune.** Altrune lista, abre y modifica únicamente
   los prefijos de su carpeta de botellas (`src/backend/paths.rs`).
   Wine nunca se ejecuta en un prefijo de fuera, porque Wine 11
   reescribe el registro y crea perfiles al abrirlo. Duplicar o importar una botella la copia dentro
   de esa carpeta (copia APFS, `cp -c`: instantánea, sin espacio extra hasta que cambian) y la copia
   se verifica (disco C:, tamaños de `system.reg` y `user.reg`). Si el prefijo trae una única carpeta
   de perfil de Windows, el perfil del Mac se enlaza a esa carpeta en lugar de crear un perfil vacío.
2. **Detección de Wine en sitios conocidos.** Solo se buscan compilaciones libres de Wine en una
   lista cerrada de ubicaciones (los motores de Altrune, las apps «Wine Stable», «Wine Staging» y
   «Wine Devel», Homebrew y MacPorts, además de la ruta que la persona indique en Ajustes), y cada
   candidato se ejecuta una vez para comprobar que arranca antes de usarlo.
3. **Un motor por botella, sin bajadas de versión.** Cada botella recuerda el Wine más nuevo con
   el que se abrió (`last_wine`). D3DMetal (motor GPTK, Wine 7.7) solo se ofrece si no supone bajar
   de versión.
4. **Interruptores que hacen algo.** MSync/ESync se detectan leyendo los binarios del Wine de la
   botella. DXMT se ofrece solo donde se ha probado; DXVK aparece como no disponible, con el
   motivo, hasta que se pruebe.
5. **Mensajes de error visibles.** `WINEDEBUG=fixme-all` (antes `-all`) deja pasar los `err:` de
   Wine para el diagnóstico.
6. **Configuración tolerante.** Un `config.json` antiguo con campos que ya no existen se lee sin
   error y sin perder el resto de los ajustes.

## Seguridad

* Procesos con argumentos estructurados; las únicas cadenas de shell son los scripts de los
  launchers, con comillas simples escapadas (`sh_quote`), y el AppleScript de Terminal, con
  escapado propio.
* Extracción: `tar` del sistema sobre carpetas nuevas; las rutas de archivo `.app` resultantes se
  buscan, no se confía en nombres.
* No se desactivan SIP ni Gatekeeper; solo se quita la cuarentena del Wine descargado.
