# Política de seguridad

## Cómo avisar de una vulnerabilidad

**No abras un issue público** para un fallo de seguridad.

La forma recomendada es el **aviso privado de vulnerabilidades de GitHub**: https://github.com/Franks2103/altrune/security/advisories/new («Report a vulnerability», en la pestaña «Security» del repositorio). Quien mantiene el proyecto la activa cuando el repositorio se hace público; si el enlace no funciona todavía, usa la alternativa de abajo.

**Alternativa, sin correo electrónico:** abre un [issue público](https://github.com/Franks2103/altrune/issues/new/choose) **mínimo** con el formulario «Pedir un canal privado» (no tiene campos obligatorios), con el título «Quiero avisar de un problema de seguridad». **No pongas ningún detalle** de la vulnerabilidad, ni pasos, ni capturas, ni el nombre del componente afectado. Quien mantiene el proyecto te propondrá un canal privado (por ejemplo, activar el aviso privado de GitHub) y, desde entonces, el intercambio se hace por ahí. Cuando haya un canal privado, el issue se cierra o se borra.

Incluye, si puedes:

* qué versión o commit de Altrune usas, el modelo de Mac (chip) y la versión de macOS;
* qué pasa y por qué crees que es un problema de seguridad;
* los pasos para reproducirlo, con una carpeta personal aislada si es posible (`tools/hogar-aislado.sh`);
* el impacto que imaginas (por ejemplo, ejecutar una orden con tus permisos, escribir fuera de la carpeta de datos).

No incluyas datos personales, contraseñas ni registros completos. El informe de «Buscar problemas» oculta datos personales conocidos, pero revísalo antes de enviarlo.

## Versiones con soporte

Altrune todavía no tiene versiones publicadas. Solo se da soporte a la rama **`main`** (el último commit). Los arreglos de seguridad se hacen sobre `main`.

## Qué entra en el alcance

Lo que Altrune hace con tu Mac y con lo que descarga:

* **Scripts de los lanzadores y entorno.** Las apps por programa son scripts de shell que Altrune escribe, y la Terminal recibe variables de entorno de la botella. Cualquier forma de colar órdenes o variables a través de nombres de botellas, de programas, de rutas o de archivos de configuración (por ejemplo un archivo de ajustes de la botella, `cxbottle.conf`, manipulado) está en el alcance.
* **Descargas fijadas y comprobación de sumas.** El «Wine normal» se descarga solo, sin preguntar, la primera vez que se abre la app sin ningún Wine; los demás motores, solo cuando la persona lo pide. Qué se descarga, desde dónde, cómo se verifica la suma SHA-256 y qué pasa cuando no coincide o GitHub no la da.
* **Desempaquetado e instalación de motores.** Rutas fuera de la carpeta de destino, enlaces, permisos, la cuarentena que se quita al Wine descargado.
* **El visor de Finder («Abrir con»)** y las peticiones de otras instancias: abrir un archivo desde Finder no debe ejecutar nada inesperado ni abrir otra copia de la app.
* **Ocultación de datos personales** en los informes de diagnóstico: si un dato personal conocido (usuario del Mac, correo, identificador de Steam, token, contraseña) sale en un informe guardado.
* **Gestión de procesos:** cerrar o forzar el cierre de botellas, limpieza de restos de instalaciones interrumpidas (solo debe tocar nombres propios de la app, nunca botellas ni copias de seguridad).
* **Operaciones sobre las botellas:** copiar, borrar, mover a la Papelera. En especial, que Altrune nunca ejecute Wine ni escriba fuera de su propia carpeta de datos.

## Qué queda fuera del alcance

* **Wine en sí**, sus fallos y que un programa de Windows haga cosas dentro de su botella. Un programa de Windows ejecutado con Wine **no está aislado** del resto de tu Mac: tiene acceso a tus archivos con tus permisos. Ese es el diseño de Wine, no una vulnerabilidad de Altrune. Ejecuta solo programas en los que confíes.
* **Motores y componentes de terceros**: las compilaciones de Wine de Gcenx, highball-engine, DXMT, GStreamer, SDL, libusb, y D3DMetal y el Game Porting Toolkit de Apple. Avisa a sus proyectos; si una vulnerabilidad de ellos afecta a cómo Altrune descarga (Wine, DXMT, GStreamer) o usa lo que ya hay en el Mac, eso sí cuenta. Altrune no descarga D3DMetal ni el Game Porting Toolkit: solo detecta y usa las copias que la persona ya tiene.
* Las vulnerabilidades de las dependencias de Rust que no se pueden alcanzar desde Altrune (avísalas a su proyecto; se actualizan con la integración continua).
* Ataques que requieran que la persona ya haya perdido el control de su Mac, o acceso físico.
* Programas de Windows concretos (Steam, el launcher de Rockstar…) y sus servicios.
* Pérdida de datos por un fallo de uso que no implique un atacante (ábrelo como issue normal).

## Qué esperar

Quien mantiene Altrune es una persona, no una empresa, y el proyecto es joven. **No se prometen plazos de respuesta ni de arreglo.** Lo que sí se intenta:

* acusar recibo cuando se pueda;
* decirte si se acepta el informe como vulnerabilidad;
* avisarte antes de publicar el arreglo y, si quieres, citarte;
* no dar detalles públicos de un fallo hasta que haya arreglo o una mitigación.

Si no has recibido respuesta tras un tiempo razonable, puedes volver a escribir por el mismo canal.

## Buena fe

No hagas pruebas con las botellas, la cuenta o los datos de otra persona, ni con servicios de terceros. Hazlas en tu propio Mac, con una carpeta personal aislada y datos simulados. Se agradece y se valora la investigación hecha de buena fe y con estas precauciones.
