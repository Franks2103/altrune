# Firma y notarización de Altrune

Esta guía explica cómo se firma y se notariza Altrune para que otros Macs la abran sin avisos
alarmantes, qué necesita la persona que publica y qué pasa cuando algo falla. Va junto con
[distribucion.md](distribucion.md), que explica qué se puede distribuir y qué no.

## Estado

**Hoy ninguna versión de Altrune está firmada con Developer ID ni notarizada.** Los scripts de esta
guía están escritos y comprobados con herramientas simuladas y con una firma *ad hoc* de prueba, pero
no se ha firmado nada con un certificado real ni se ha enviado nada a Apple: el Mac de desarrollo no
tiene ninguna identidad de firma válida (`security find-identity -v -p codesigning` da 0). Por eso:

- Lo que está **probado** en este Mac: que `tools/firmar.sh --prueba` firma una copia con el runtime
  reforzado y los permisos de Altrune, que la copia arranca con el runtime reforzado, tanto
  en modo de medición sin ventana (`--medir --sin-wine`) como dibujando la ventana
  (`--screenshot --vista inicio`), siempre con un `HOME` de usar y tirar, y que Gatekeeper la
  rechaza, como corresponde a una firma ad hoc.
- Lo que **no** está probado: una firma con Developer ID, la notarización, el grapado del ticket,
  abrir la app descargada en un Mac limpio, y que el botón de Terminal de Altrune funcione con la app
  firmada y notarizada (ver «Qué falta por comprobar»).

## Firma ad hoc frente a Developer ID

- **Ad hoc** (`codesign --sign -`). Es lo que hace `tools/empaquetar.sh`. La firma no identifica a
  nadie: solo sella el contenido de la app para detectar cambios. Sirve en el Mac que la hizo.
  En cualquier otro Mac, Gatekeeper (la protección de macOS) la rechaza si la app llegó por
  descarga.
- **Developer ID Application**. Es un certificado que Apple emite a una cuenta del programa Apple
  Developer. La firma dice quién publica la app. Con el *runtime reforzado* y una marca de tiempo
  segura, Apple acepta enviarla a **notarización**.
- **Notarización**. Apple analiza automáticamente la app (no es una revisión manual de contenido) y,
  si la acepta, emite un *ticket*. Con el ticket **grapado** (`stapler`) a la app o a la imagen de
  disco, Gatekeeper la acepta incluso sin conexión.

Sin notarización, según la documentación de Apple, macOS avisa de que no puede comprobar que la app
esté libre de software malicioso y no deja abrirla con doble clic; hay que usar **Abrir igualmente**
(ver el final de esta guía). Con las dos cosas, macOS solo muestra el aviso normal de una app
descargada. Ninguna de las dos situaciones se ha probado todavía en un Mac limpio.

## Qué necesita quien publica

1. Una cuenta del **programa Apple Developer** (de pago, anual).
2. Un certificado **Developer ID Application** (no «Apple Development» ni «Mac App Distribution»),
   creado desde la cuenta y guardado en el llavero de quien firma, con su clave privada.
3. Credenciales de **notarización**, una de estas dos:
   - **Clave de la API de App Store Connect** (recomendada, y la que usa el flujo de GitHub): un
     archivo `AuthKey_XXXXXXXXXX.p8`, su identificador de clave (*Key ID*) y el identificador del
     emisor (*Issuer ID*, un UUID). Se crea en App Store Connect, en Usuarios y acceso, Integraciones,
     con una clave de **equipo** (*Team key*). El `.p8` solo se puede descargar una vez.
   - **Contraseña específica de app** de un Apple ID, guardada una vez en el llavero con
     `xcrun notarytool store-credentials`. Da un *perfil* con nombre, que los scripts leen de
     `ALTRUNE_NOTARIO_PERFIL`.

El certificado, la clave `.p8` y las contraseñas **nunca** se suben al repositorio ni se pegan en
una incidencia. `.gitignore` ya ignora `*.p12`, `*.p8`, `*.pem`, `*.key` y `*.cer`; aun así, guarda el `.p8`
fuera de la carpeta del repositorio.

## Qué se firma y con qué permisos

`tools/firmar.sh` firma `dist/Altrune.app` con `--options runtime` (runtime reforzado),
`--timestamp` (marca de tiempo de Apple) y `--entitlements macos/Altrune.entitlements`.

El archivo de permisos tiene **una sola clave**, `com.apple.security.automation.apple-events`.
Motivo: Altrune lanza `/usr/bin/osascript` para decirle a Terminal que abra una consola de una
botella (`src/backend/tools.rs`). Los eventos los envía `osascript`, pero el proceso responsable es
Altrune, y macOS le pide permiso a él. El texto de esa petición está en `macos/Info.plist`
(`NSAppleEventsUsageDescription`).

No lleva ninguna de las excepciones que debilitan el runtime reforzado: ni `allow-jit`, ni
`allow-unsigned-executable-memory`, ni `allow-dyld-environment-variables`, ni
`disable-library-validation`. El proceso de Altrune no genera código en memoria, no lee variables
`DYLD_*` y solo carga bibliotecas del sistema. Wine y los motores que Altrune descarga son
procesos aparte y, en este Mac, sus ejecutables no están firmados (`codesign -dv` dice «code object
is not signed at all»); el runtime reforzado es una marca de cada proceso y no se da por hecho que
les afecte. Si arrancan bajo una Altrune firmada con Developer ID no se ha probado (ver «Qué falta
por comprobar»).

Hoy el paquete contiene un solo ejecutable. Si algún día lleva código anidado (ayudantes,
bibliotecas, frameworks), `firmar.sh` firma **de dentro hacia fuera**: primero lo anidado, con
runtime y marca de tiempo, y al final el paquete, porque su firma sella lo anterior. Un ayudante
con necesidades propias necesitaría su propio archivo de permisos.

Cualquier clave nueva en `macos/Altrune.entitlements` es una decisión que se revisa: `tests/firma.rs`
fija la lista exacta.

## Orden local

Todo se hace desde la raíz del repositorio. Cada paso se detiene si algo falla.

```sh
# 1. Compilar y empaquetar (firma ad hoc, solo para este Mac)
sh tools/empaquetar.sh

# 2. Firmar la app con Developer ID (sustituye la firma ad hoc)
export ALTRUNE_IDENTIDAD="Developer ID Application: Nombre Apellido (ABCDE12345)"
sh tools/firmar.sh app dist/Altrune.app

# 3. Notarizar la app y grapar el ticket
export ALTRUNE_NOTARIO_PERFIL="altrune-notario"   # o las tres variables de la clave, ver abajo
sh tools/notarizar.sh dist/Altrune.app

# 4. Crear la imagen de disco con la app ya firmada y notarizada
sh tools/crear-dmg.sh

# 5. Firmar la imagen
sh tools/firmar.sh dmg dist/Altrune-0.1.0-alpha.dmg

# 6. Notarizar la imagen y grapar el ticket
sh tools/notarizar.sh dist/Altrune-0.1.0-alpha.dmg

# 7. Reescribir la suma de control: firmar y grapar cambian los bytes de la imagen
(cd dist && shasum -a 256 Altrune-0.1.0-alpha.dmg > Altrune-0.1.0-alpha.dmg.sha256)
```

`ALTRUNE_IDENTIDAD` es el nombre completo del certificado o su huella SHA-1 de 40 cifras
hexadecimales; se ve con `security find-identity -v -p codesigning`. El script rechaza la identidad
`-` (ad hoc) y cualquiera que no sea un Developer ID Application. Si el certificado está en otro
llavero, `ALTRUNE_LLAVERO` es la ruta de ese archivo.

Credenciales de notarización, por una de las dos vías:

```sh
# Perfil guardado una sola vez (la contraseña se pide en pantalla, no se escribe en el comando)
xcrun notarytool store-credentials altrune-notario --apple-id "tu-id@ejemplo.com" --team-id ABCDE12345
export ALTRUNE_NOTARIO_PERFIL=altrune-notario

# O la clave de la API de App Store Connect
export ALTRUNE_NOTARIO_CLAVE="$HOME/claves/AuthKey_XXXXXXXXXX.p8"
export ALTRUNE_NOTARIO_CLAVE_ID="XXXXXXXXXX"
export ALTRUNE_NOTARIO_EMISOR="00000000-0000-0000-0000-000000000000"
```

Sin credenciales, `notarizar.sh` termina con código 2, lo dice y no hace nada. Espera por defecto
hasta 30 minutos el veredicto (`ALTRUNE_NOTARIO_ESPERA`, por ejemplo `45m`). Si se agota, el script
falla con un mensaje de «sin veredicto todavía» (no es un rechazo), imprime el identificador del
envío y no intenta bajar ningún registro: Apple sigue procesando y el estado se consulta con
`xcrun notarytool info <id>`. Si `notarytool` no devuelve ningún identificador, el script muestra lo
que contestó y recuerda `xcrun notarytool history`, donde se ve si la subida llegó a existir.

Lo que hace `notarizar.sh`, en orden: comprueba que el archivo tiene firma Developer ID (y runtime
reforzado, si es una app); si es una `.app`, la comprime con `ditto -c -k --keepParent` solo para
subirla; ejecuta `xcrun notarytool submit --wait --timeout`; si el resultado no es `Accepted`, guarda
el registro en `dist/notary-log-<id>.json` y falla; si lo es, ejecuta `xcrun stapler staple` (hasta 5 intentos, 15 segundos entre ellos,
porque justo después de aceptar el ticket puede tardar en estar disponible; si aun así falla, el
envío ya está aceptado y el grapado se puede repetir a mano) y `xcrun stapler validate` sobre la app o la imagen, y termina con `spctl` (`-t exec` para la app,
`-t open --context context:primary-signature` para la imagen). No escribe nunca una credencial.

El orden importa: la app se firma y se notariza **antes** de meterla en la imagen, para que la app
de dentro ya lleve el ticket grapado, y después se firma y notariza la imagen.

### Comprobación local sin certificado

En un Mac sin identidad se puede comprobar que el runtime reforzado no impide arrancar a Altrune.
Siempre sobre una **copia**; el script se niega a tocar `dist/Altrune.app`:

```sh
mkdir -p /tmp/prueba-firma && ditto dist/Altrune.app /tmp/prueba-firma/Altrune.app
sh tools/firmar.sh --prueba app /tmp/prueba-firma/Altrune.app
```

Firma con la identidad ad hoc `-` pero con `--options runtime` y los mismos permisos, y comprueba
que `codesign -dv` muestra `flags=0x10002(adhoc,runtime)`. Es solo para esta comprobación: nunca
sirve para repartir la app. Para arrancarla sin tocar tus datos hay que usar un `HOME` de usar y
tirar (`tools/hogar-aislado.sh`).

## Flujo de GitHub

El flujo `.github/workflows/release.yml` se lanza a mano (`workflow_dispatch`, con la versión como
entrada) y **solo crea un borrador de versión previa**: no publica nada. Hay que abrir el
borrador, revisarlo y publicarlo a mano.

### Entorno `release`

1. En el repositorio: Settings, Environments, New environment, nombre `release`.
2. Activar **Required reviewers** y añadir a quien deba aprobar cada firma. Los secretos del
   entorno solo se entregan al flujo cuando alguien lo aprueba.
3. En **Deployment branches and tags**, elegir **Selected branches and tags** y permitir solo
   `main`. Hazlo **antes de añadir ningún secreto**: es la única barrera que un archivo de flujo
   modificado en otra rama no puede saltarse (la comprobación de rama dentro del flujo vive en un
   archivo que esa rama puede editar), y sin ella un flujo editado podría imprimir los secretos.
4. Dentro del entorno, añadir estos seis secretos:

| Secreto | Qué es |
|---|---|
| `MACOS_CERTIFICADO_P12_BASE64` | El certificado Developer ID Application y su clave privada, en un `.p12`, codificado en base64 |
| `MACOS_CERTIFICADO_CLAVE` | La contraseña de ese `.p12` |
| `ALTRUNE_IDENTIDAD` | El nombre completo del certificado, `Developer ID Application: Nombre (ABCDE12345)`, o su SHA-1 |
| `NOTARIO_CLAVE_P8_BASE64` | El archivo `AuthKey_XXXXXXXXXX.p8`, codificado en base64 |
| `NOTARIO_CLAVE_ID` | El identificador de la clave (*Key ID*) |
| `NOTARIO_EMISOR` | El identificador del emisor (*Issuer ID*) |

**Exportar el `.p12`.** En Acceso a Llaveros, en «Mis certificados», selecciona el certificado
«Developer ID Application: ...» **con su clave privada desplegada**, Archivo, Exportar ítems, formato
`.p12`, y elige una contraseña larga y única. Ese `.p12` es lo más sensible de todo el proceso:
quien lo tenga y conozca su contraseña puede firmar como tú.

**Codificarlo y guardarlo como secreto**, sin dejar copias en el repositorio:

```sh
base64 -i ~/Escritorio/certificado.p12 | gh secret set MACOS_CERTIFICADO_P12_BASE64 --env release
gh secret set MACOS_CERTIFICADO_CLAVE --env release          # pide el valor en pantalla
base64 -i ~/claves/AuthKey_XXXXXXXXXX.p8 | gh secret set NOTARIO_CLAVE_P8_BASE64 --env release
gh secret set ALTRUNE_IDENTIDAD --env release
gh secret set NOTARIO_CLAVE_ID --env release
gh secret set NOTARIO_EMISOR --env release
```

Después, borra el `.p12` y su copia en base64 del escritorio. Se puede hacer lo mismo desde la web
(Settings, Environments, `release`, Add secret).

Si **falta cualquier secreto**, el flujo no firma: produce una compilación de prueba sin firmar y lo
dice en el título y en las notas del borrador.

### El llavero temporal

En el ejecutor de GitHub, `tools/llavero-ci.sh crear` crea un llavero temporal en `$RUNNER_TEMP` con
contraseña aleatoria, importa el `.p12`, permite que `codesign` use la clave sin ventana de
confirmación y lo añade a la lista de búsqueda; `borrar` restaura la lista y elimina el llavero. El
script **se niega a ejecutarse fuera de CI** (exige `CI=true` y `RUNNER_TEMP`): cambia la lista de
llaveros e importa una clave privada, y eso no debe pasar nunca en el Mac de nadie.

## Cómo comprobar el resultado

Sobre la app, ya grapada:

```sh
codesign --verify --deep --strict --verbose=2 dist/Altrune.app
codesign -dv --verbose=4 dist/Altrune.app 2>&1 | grep -E 'flags|Authority|TeamIdentifier|Timestamp'
codesign -d --entitlements :- dist/Altrune.app
xcrun stapler validate dist/Altrune.app
spctl -a -vv -t exec dist/Altrune.app
```

Se espera `flags=0x10000(runtime)`, `Authority=Developer ID Application: ...`, un `TeamIdentifier`,
una línea `Timestamp=`, la única clave de permisos de arriba, `The validate action worked!` y
`source=Notarized Developer ID` en la salida de `spctl`.

Sobre la imagen:

```sh
codesign --verify --strict --verbose=2 dist/Altrune-0.1.0-alpha.dmg
xcrun stapler validate dist/Altrune-0.1.0-alpha.dmg
spctl -a -vv -t open --context context:primary-signature dist/Altrune-0.1.0-alpha.dmg
(cd dist && shasum -a 256 -c Altrune-0.1.0-alpha.dmg.sha256)
```

El archivo `.sha256` que escribe `tools/crear-dmg.sh` queda **desfasado** en cuanto la imagen se
firma y se graba el ticket, porque cambian sus bytes. Por eso el paso 7 de «Orden local» lo reescribe
a partir de la imagen final (el flujo de GitHub hace lo mismo), y es esa suma, y no la primera, la
que debe aparecer en las notas de la versión.

La prueba que de verdad cuenta es abrir la imagen descargada (con el atributo de cuarentena de una
descarga) en un **Mac limpio**, que nunca haya tenido Altrune. Eso todavía no se ha hecho.

## Si la notarización se rechaza

`notarizar.sh` termina con error y deja el registro en `dist/notary-log-<id>.json` (la carpeta
`dist/` y los `notary-log*.json` los ignora git). El registro lista los problemas con la ruta del
archivo afectado. En el flujo de GitHub el ejecutor se descarta, así que el registro se imprime
en la salida de la ejecución (un grupo plegado con el nombre del archivo). Los más habituales:

- **«The binary is not signed with a valid Developer ID certificate»**: se firmó con otra
  identidad, o ad hoc. Repite el paso de firma con `ALTRUNE_IDENTIDAD`.
- **«The signature does not include a secure timestamp»** o **«does not have the hardened runtime
  enabled»**: se firmó sin `--timestamp` o sin `--options runtime`. `firmar.sh` los pone siempre;
  suele pasar al firmar a mano.
- **«The signature of the binary is invalid»** o un archivo anidado sin firmar: apareció código
  nuevo dentro del paquete que `firmar.sh` no firmó. Revisa la lista de archivos anidados que
  imprime.
- **Un SDK antiguo** o un ejecutable que Apple no admite: se corrige al compilar.

Se corrige la causa, se vuelve a empezar desde el paso de firma y se envía de nuevo. Cada envío tiene
su identificador; `xcrun notarytool history` los lista y `xcrun notarytool log <id>` descarga el
registro de uno cualquiera. Un rechazo no deja nada publicado ni grapado.

## Qué ve una persona con una compilación de prueba sin notarizar

Una imagen o una app **sin notarizar** (por ejemplo, la compilación de prueba que produce el flujo
cuando faltan secretos) no se abre como una app normal. Según la documentación de Apple, macOS
muestra un aviso de que no puede verificar que la app esté libre de software malicioso y no deja
abrirla con doble clic. Quien confía en la compilación puede abrirla así: intentar abrirla una vez,
ir a **Ajustes del Sistema, Privacidad y seguridad**, bajar hasta el aviso sobre Altrune y pulsar
**Abrir igualmente**, y confirmar con la contraseña de la sesión.

Esto es el comportamiento documentado por Apple; **todavía no se ha probado en un Mac limpio**, y
las notas de la versión de prueba deben decir claramente que no es una versión final. Una versión
notarizada no necesita ese paso.

## Qué falta por comprobar

- Firmar con un certificado Developer ID real y notarizar de verdad (nada de eso se ha hecho).
- Que el runtime reforzado no estorba a Wine, DXMT ni GStreamer cuando los lanza una Altrune firmada
  y notarizada.
- Que el botón de Terminal funciona en la app firmada: debe aparecer la petición de permiso de
  automatización con el texto de `NSAppleEventsUsageDescription`. Si el permiso no se pide o se
  deniega, se revisa `macos/Altrune.entitlements`.
- Abrir la imagen descargada en un Mac limpio, con y sin notarización.
