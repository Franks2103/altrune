## Qué cambia y por qué

<!-- Describe el cambio y el motivo. Enlaza el issue si lo hay. -->

## Cómo se comprobó

<!-- Qué prueba nueva falla sin este cambio, y qué más probaste. -->

## Lista de comprobación

- [ ] `cargo fmt --all --check` sin cambios
- [ ] `cargo clippy --locked --all-targets -- -D warnings` sin avisos
- [ ] `cargo test --locked` pasa (sin `--ignored` ni `--include-ignored`)
- [ ] `cargo build --release --locked` compila
- [ ] Cada arreglo lleva una prueba que falla sin él
- [ ] Comentarios y commits en inglés; textos visibles en español
- [ ] No renombré identificadores en español ni los que ya existen en disco (`BOTELLA_*`, `--medir`, `--vista`, identificadores de los lanzadores, carpetas de datos)
- [ ] No hay datos personales (nombre de usuario del Mac, rutas con él, correos, cuentas, tokens) en el código, los commits, los textos ni las capturas
- [ ] No hay binarios propietarios ni material descompilado o de ingeniería inversa de software propietario de terceros: código, datos, pseudocódigo, direcciones
- [ ] No añade descargas de D3DMetal ni del Game Porting Toolkit de Apple
- [ ] Si cambia `Cargo.lock` (dependencias), regeneré `licencias/terceros.html` con `sh tools/licencias.sh`
- [ ] Acepto que mi contribución se licencia bajo Apache-2.0 (sección 5 de la licencia)
- [ ] Las capturas, si las hay, se hicieron con datos simulados (`tools/hogar-aislado.sh`)
- [ ] Probé con una carpeta personal aislada, nunca con mis botellas reales
