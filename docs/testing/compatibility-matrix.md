# Pruebas y resultados

Ejecutar desde la raíz del repositorio:

```
cargo fmt --all --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked
```

Todas las pruebas deben pasar; las ignoradas son las que usan Wine real y no se ejecutan nunca sin que la persona lo pida. Una parte son flujos aislados con un Wine falso: ver [`pruebas-aisladas.md`](pruebas-aisladas.md).

## Pruebas automáticas

| Prueba | Qué demuestra |
|---|---|
| `backend::bottles::tests::sanitize_rules_one_by_one` | saneado de nombres de botella, una fila por regla |
| `backend::tests::reserved_names_are_refused` | nombres reservados y vacíos rechazados |
| `backend::tests::quit_wait_is_sixty_seconds` | espera de cierre = 60 s |
| `backend::profile_tests::*` | el perfil del Mac se enlaza a la única carpeta de perfil que ya tiene la botella; una carpeta apartada no se toma por el perfil |
| `backend::steam_tests::steam_starts_from_its_folder_without_web_helper_options` | Steam (acceso, .exe, `steam://`) arranca desde su carpeta y sin opciones del web helper; otros programas no cambian |
| `backend::engine_tests::sync_switch_follows_what_wine_reads` | MSync/ESync solo cuando el Wine lo lee; si no, la variable se quita |
| `backend::engine_tests::real_wine_builds_on_this_mac` | en este Mac: Wine 11 de WineHQ sin MSync/ESync; un Wine que sí lo lee, con `WINEMSYNC` |
| `backend::engine_tests::wine_versions_and_downgrades` | detección de bajada de versión (11.0 → 7.7) |
| `backend::engine_tests::*` | Wine solo se ejecuta dentro de la carpeta de botellas de Altrune |
| `backend::engine_tests::copies_are_complete_and_checked` | copia con espacios en rutas; no sobrescribe; detecta copia incorrecta |
| `diagnostics::tests::*` | síntomas reconocidos, salida limpia sin falsos positivos, borrado de datos personales, informe |
| `recipes::tests::*` | comodines de instaladores y receta de Steam como `reg add` |
| `launchers::tests::launcher_runs_program_with_bottle_settings` | script del launcher con entorno, argumentos y limpieza |
| `tests::bottle_name_from_installer` | nombre de botella desde el instalador |

## Aplicaciones reales

| Aplicación | Botella | Motor | Arranca | Ventana usable | Login | Red | Cierre | Estado |
|---|---|---|---|---|---|---|---|---|
| Steam | de Altrune, sin opciones del web helper | Wine 11.0 | sí | **no** (negra) | no ("Transport Error: 2") | parcial (`Unknown error 10045`) | — | Falla: con el Wine normal la ventana se queda negra. Las opciones del web helper que apagan la GPU dejan la ventana negra en el motor DXMT, por eso Steam arranca sin ellas. |
| Steam | de Altrune | «Steam y launchers» (DXMT), sin opciones del web helper | sí | sí | sí | sí | — | Pasa en el Mac de pruebas (comprobado a mano: la ventana se dibuja, inicia sesión, muestra la biblioteca y abre un juego). Sin repetir desde los últimos cambios. |
| Rockstar Games Launcher | de Altrune | Wine 11.0 | sí | no ("failed to initialize") | — | — | — | Falla con un perfil de Windows nuevo y vacío. Pendiente repetirlo con el motor «Steam y launchers». |
| `cmd /c ver` | temporal en `$TMPDIR` | GPTK 3.0-3 | sí | n/a | n/a | n/a | sí | Pasa (Windows 10.0.19043). |

## Qué tiene prueba automática

* Con prueba automática: nombres de botella, cierre, MSync y entorno, Steam, recetas, lanzadores y
  diagnósticos.
* Implementadas sin prueba (necesitan Wine real escribiendo en una botella): alta resolución,
  versión de Windows, herramientas (`winecfg`, `rundll32 …Control_RunDLL`, reinicio), exportar e
  importar, iconos.
* No disponibles todavía: catálogo en línea, DXVK. D3DMetal no se descarga: solo se detecta y se usa
  si ya está en el Mac (ver `THIRD_PARTY_NOTICES.md`).
