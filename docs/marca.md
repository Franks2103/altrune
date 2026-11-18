# Marca: Altrune

Este documento recoge la identidad de **Altrune**, dónde va cada recurso y cómo se fabrican. Los recursos están en `assets/branding/` y se reconstruyen con `tools/marca/generar.sh`.

## Identidad

- **Nombre:** Altrune.
- **Símbolo:** una «A» abstracta hecha con una cinta plegada. La pata izquierda es azul (de azul cielo abajo a azul más claro arriba), la derecha violeta, y donde la cinta se pliega queda un solapamiento índigo. Debajo, la palabra «Altrune» en tinta casi negra.
- **Referencia maestra:** `assets/branding/Logotipo de Altrune.png` (copia idéntica byte a byte del original, SHA-256 `6c04dbef66b138e39d69538fbf14dee795d2586ce95886c08bebfa80a15cde3e`). Nunca se redibuja: no se cambia su geometría, proporciones, curvatura, degradados ni el efecto del solapamiento. Todo lo demás se extrae de ese archivo.
- **Colores medidos** (detalle y puntos de medida en `assets/branding/paleta.md`): azul cielo `#0388FE`, azul claro `#5F99FD`, pliegue `#7F9DFC`, índigo `#2C3CCF` a `#4D5DE6`, violeta `#8D81FC`, tinta `#161B23`. Fondo claro de la marca (Frost): blanco a `#F1F3F6`.
- **Personalidad:** premium, simple, fiable, accesible. Nada «gamer»: sin neón, sin runas ni estrellas añadidas, sin brillos. El azul-violeta es un acento puntual, no un degradado para toda la app.

## Licencia del nombre y del logotipo

El código de Altrune está bajo Apache-2.0 ([`LICENSE`](../LICENSE)), pero **esa licencia no cubre el nombre ni el logotipo**:

- **Nombre y logotipo reservados.** La sección 6 de Apache-2.0 dice que la licencia no concede permiso para usar los nombres comerciales, marcas, marcas de servicio o nombres de producto del licenciante, salvo para describir el origen de la obra. Eso incluye «Altrune», el símbolo, el logotipo y el icono de `assets/branding/`.
- **Bifurcaciones y versiones modificadas.** Quien redistribuya una versión modificada de Altrune debe usar **otro nombre y otro logotipo**, y no presentarla como la app oficial ni sugerir que el proyecto la respalda. Sí se puede decir, con verdad, que algo es «una bifurcación de Altrune» o «compatible con Altrune».
- **Origen del logotipo.** El logotipo maestro (`Logotipo de Altrune.png`) se generó con una herramienta de imágenes con IA, según las credenciales de contenido (C2PA) incrustadas en el propio archivo. Por eso **no se reclama copyright sobre la imagen sin modificar** más allá de lo que la ley permita (la Oficina de Derechos de Autor de Estados Unidos considera que el material generado solo con instrucciones de texto no tiene copyright). Los derechos de marca, que sirven para identificar el proyecto, son otra cosa y no dependen del copyright.
- **Sin búsqueda de marcas.** No se ha comprobado si «Altrune» choca con una marca registrada en algún país.

## Dónde va cada recurso

| Lugar | Recurso | Nota |
|---|---|---|
| Dock y Finder | `Altrune.icns` (a partir de `AppIcon-1024.png`) | solo el símbolo, nunca la palabra |
| Barra lateral | `altrune-simbolo-128.png`, pequeño (unos 20 a 28 px) junto al nombre «Altrune» escrito con la tipografía de la app | no usar el logotipo completo |
| Acerca de | `altrune-logotipo-512.png`; en tema oscuro, `altrune-logotipo-simbolo-512.png` más `altrune-logotipo-palabra-512.png` teñida | el único sitio con el logotipo completo |
| Documentación, web, cabeceras de informes | `altrune-logotipo.png` o `altrune-simbolo.png` a resolución original | fondo transparente |
| Tarjetas, botones, listas | ninguno | la marca no se repite en cada tarjeta ni botón |

Tema oscuro: la tinta de la palabra (`#161B23`) casi se funde con el fondo `#1C1E22`. Por eso `altrune-logotipo-palabra*.png` es una máscara blanca con su alfa: la interfaz solo la tiñe con el color de texto, y el símbolo (`altrune-logotipo-simbolo*.png`, misma lona) conserva sus colores exactos.

## Archivos

| Archivo | Contenido |
|---|---|
| `Logotipo de Altrune.png` | copia idéntica del original, 1254 x 1254, RGB, fondo blanquecino |
| `altrune-simbolo.png` | solo la «A», 390 x 331, margen transparente de 16 px, resolución original |
| `altrune-logotipo.png` | logotipo completo, 633 x 482, margen de 16 px, colores originales |
| `altrune-logotipo-simbolo.png`, `altrune-logotipo-palabra.png` | la misma lona en dos capas: símbolo, y palabra como máscara blanca con alfa |
| `altrune-simbolo-128.png` | símbolo, lado mayor 128 px |
| `altrune-logotipo-512.png`, `-simbolo-512.png`, `-palabra-512.png` | capas a 512 px de ancho (512 x 390) |
| `AppIcon-1024.png`, `Altrune.icns` | icono de macOS |
| `paleta.md` | colores medidos y fichas propuestas con contraste |

## Cómo se fabrican

`tools/marca/generar.sh` compila `tools/marca/marca.swift` (solo Swift con ImageIO y CoreGraphics; sin crates nuevos) y lo ejecuta. Ejecutarlo dos veces da archivos idénticos byte a byte (comprobado con SHA-256 de todos los archivos de `assets/branding/`). La ruta del original se puede pasar como argumento o con `ALTRUNE_LOGO`.

### Extracción con transparencia real

1. **Fondo medido.** El fondo del original no es blanco puro ni tiene degradado: es uniforme, media R 254,252, G 254,080, B 254,101 (`#FEFEFE`), con ruido de ±1 y un ringing suave (estilo JPEG, hasta unos 14 niveles) cerca de los bordes.
2. **Interior exacto.** Un píxel es interior si todos los de su ventana 5 x 5 están lejos del fondo (distancia por canal mayor que 30). Ahí el alfa es 1 y el color es el del archivo, sin tocarlo. Así los azules claros no salen semitransparentes, como ocurriría con un «color a alfa» de máximo por canal.
3. **Bordes sin halo.** En los píxeles del borde `p = a*F + (1-a)*B`. El color local `F` se propaga anillo a anillo desde el interior más cercano. El alfa es lo que `p` se aleja de `B` a lo largo de `F-B`, y el color es `(p - (1-a)*B)/a`. Con alfa bajo (menos de 0,25) se usa `F` para no amplificar el ruido. El borde conserva su color real y no queda borde blanco.
4. **Ruido recortado.** Alfa menor que 0,04 pasa a 0; el alfa menor que 0,10 que no toca un borde real (ringing suelto a 2 px o más) también. El alfa final es de 8 bits.
5. **Capas.** El símbolo y la palabra se separan por la fila del hueco vacío entre ambos (y = 723 del original). Las dos capas de la misma lona suman exactamente el logotipo completo (0 píxeles discrepantes de 305 106).
6. **Tamaños de interfaz.** Reducción con Lanczos-3 en espacio premultiplicado (sin fleco oscuro en los bordes), una sola vez desde la resolución original.

### Fidelidad medida

Se componen las capas guardadas (releídas del disco) sobre el fondo medido, se redondea a 8 bits y se compara con el original:

| Comparación | Diferencia máxima R/G/B | Media R/G/B | Píxeles con algún canal > 4 |
|---|---|---|---|
| Logotipo, solo píxeles con alfa > 0 (75 402) | 4 / 7 / 7 | 0,024 / 0,028 / 0,026 | 31 |
| Logotipo, imagen completa 1254 x 1254 | 14 / 14 / 14 | 0,385 / 0,347 / 0,344 | 4 532 |
| Símbolo contra las filas del símbolo, imagen completa | 13 / 11 / 7 | 0,208 / 0,185 / 0,183 | 889 |

En lo que cubren las capas, el resultado es prácticamente idéntico al original. La diferencia de la imagen completa viene del fondo: el original lleva ruido y ringing que se descartan a propósito (se componen sobre un fondo uniforme), no de la forma.

### Icono de macOS

Lona de 1024 x 1024. Cuerpo cuadrado redondeado de 824 x 824 centrado (margen de 100 px), con esquinas continuas tipo «squircle» (rectas, un tramo de Bézier que entra suave y un arco, suavizado de 0,6, radio del 22,5 % del lado), sin segunda máscara dentro. Fondo Frost: blanco arriba a `#F1F3F6` abajo (un degradado vertical casi imperceptible), sin brillos. Sombra suave debajo (desplazamiento 10 px, desenfoque 24 px, negro al 26 %). El símbolo mide el 55 % del ancho del cuerpo (453 px; 1,27 veces el original, sin pasar de 1,3) y va centrado ópticamente: se mide el centro de masa (una «A» pesa abajo) y se centra a medio camino entre el centro de su caja y su centro de masa, lo que lo sube unos 11 px en el original (14 px en el icono).

- Los tamaños 64 a 1024 se componen de nuevo en cada tamaño (no se encogen desde el maestro).
- Los de 16 y 32 px tienen su propia composición: sin sombra, cuerpo casi a sangre (margen de 1 y 2 px), un filo interior de 1 px (negro al 14 %) para que el cuerpo se distinga sobre fondos blancos, y símbolo más grande (74 % y 70 % del cuerpo) para que se lea.
- `Altrune.icns` se construye en una carpeta temporal `.iconset` (que no se guarda) con `iconutil`, con las diez representaciones (16, 32, 128, 256, 512, cada una a 1x y @2x). La comprobación del propio script: `iconutil -c iconset` devuelve diez imágenes del tamaño correcto, `sips` las lee y `file` dice «Mac OS X icon».

## Límites

- El original es un PNG de 1254 px con artefactos de compresión (ruido y ringing); la extracción no puede recuperar vectores. Los bordes son tan nítidos como el original; en el icono el símbolo se amplía 1,27 veces, y ampliarlo más (o usar las capas a un tamaño mayor que el original) haría visibles sus píxeles.
- La tinta de la palabra no es plana (R 15-35, G 19-40, B 24-50): `altrune-logotipo.png` la conserva tal cual; la máscara de la palabra solo guarda su alfa.
- El alfa de los píxeles de borde se estima con el color local propagado desde el interior; en un borde que cruza un cambio brusco de color podría haber un error pequeño (la medida de fidelidad de arriba lo acota a 7 niveles como máximo).
- La forma del icono es una aproximación de la plantilla de Apple (curva de esquina con suavizado 0,6), no un calco píxel a píxel.
- En 16 px cada pata de la «A» tiene apenas dos o tres píxeles de ancho: el símbolo se reconoce, pero el solapamiento índigo casi no se distingue.
- Las fichas de color de `paleta.md` son una propuesta; este paquete no cambia el código de la app ni el nombre dentro de ella.
