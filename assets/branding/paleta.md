# Paleta de Altrune

Valores medidos en el logotipo oficial (`Logotipo de Altrune.png`, 1254 x 1254, SHA-256 `6c04dbef66b138e3...`). Cada muestra es la media de 5 x 5 píxeles en coordenadas del original (x, y). Se regeneran con `tools/marca/generar.sh` (orden `paleta` de `tools/marca/marca.swift`).

## Colores medidos

| Zona | Media | Muestras (x, y) |
|---|---|---|
| Pata izquierda, abajo (azul cielo) | `#0388FE` | (480, 675) `#018EFE`; (500, 685) `#0386FE`; (520, 688) `#0682FD` |
| Pata izquierda, centro | `#1589FE` | (500, 640) `#0789FE`; (520, 620) `#1288FE`; (540, 595) `#278BFE` |
| Pata izquierda, arriba (azul más claro) | `#5F99FD` | (600, 435) `#619BFD`; (590, 470) `#5898FD`; (610, 460) `#659AFD` |
| Pliegue en la cima | `#7F9DFC` | (650, 428) `#789DFD`; (665, 440) `#7B9BFC`; (680, 455) `#899EFC` |
| Solapamiento índigo, arriba (lo más oscuro) | `#2C3CCF` | (650, 545) `#2237BC`; (690, 545) `#303CDD`; (670, 555) `#3142D3` |
| Solapamiento índigo, centro | `#4D5DE6` | (680, 580) `#4655E3`; (700, 600) `#5A67EF`; (660, 590) `#475BDF` |
| Pata derecha, arriba (violeta claro) | `#8B99FD` | (700, 500) `#8A97FD`; (680, 470) `#7F97FC`; (715, 520) `#979DFE` |
| Pata derecha, abajo (violeta) | `#8D81FC` | (780, 690) `#9181FC`; (760, 685) `#8F86FC`; (770, 670) `#887CFB` |
| Tinta del logotipo (palabra «Altrune») | `#161B23` | mediana de 11 743 píxeles de trazo sólido; por canal R 15-35, G 19-40, B 24-50 (la tinta tiene una variación leve, no es plana) |
| Fondo del original | `#FEFEFE` | media del marco exterior: R 254,252, G 254,080, B 254,101 (uniforme, con ruido de ±1 y algo de ringing alrededor de los trazos) |

Resumen de la identidad: azul cielo `#0388FE`, azul claro `#5F99FD`, índigo `#2C3CCF`-`#4D5DE6`, violeta `#8D81FC`, tinta `#161B23`. El azul-violeta es un acento de la interfaz; no se usa como degradado general.

## Fichas propuestas para la interfaz

Derivadas del matiz azul del símbolo (entre el azul `#5F99FD` y el índigo `#4D5DE6`), oscurecidas lo justo para llegar a contraste en cada uso. Las razones son WCAG 2.x (luminancia relativa); se calculan en `marca.swift paleta`.

Superficies de referencia: tema claro `#F5F6F8` (fondo) y `#FFFFFF` (tarjetas); tema oscuro `#1C1E22` (fondo) y `#26282D` (tarjetas).

### Tema claro

| Ficha | Valor | Uso | Contraste |
|---|---|---|---|
| `acento_boton` | `#2F5FE3` | relleno de botón principal, texto blanco encima | blanco sobre él 5,43:1; el relleno contra `#F5F6F8` 5,02:1 y contra `#FFFFFF` 5,43:1 (borde de control, mínimo 3:1) |
| `acento_texto` | `#2A55D2` | texto e iconos de acento (enlaces, pestaña activa) | sobre `#F5F6F8` 5,83:1; sobre `#FFFFFF` 6,31:1 (mínimo 4,5:1) |
| `acento_foco` | `#2F6BFF` | anillo de foco y selección (no texto) | sobre `#F5F6F8` 4,16:1; sobre `#FFFFFF` 4,50:1 (mínimo 3:1) |
| `seleccion_fondo` | `#D6E2FF` | fila seleccionada | texto `#15191F` encima 13,59:1; texto de acento `#2A55D2` encima 4,86:1 |

### Tema oscuro

| Ficha | Valor | Uso | Contraste |
|---|---|---|---|
| `acento_boton` | `#3D68F5` | relleno de botón principal, texto blanco encima | blanco sobre él 4,67:1; el relleno contra `#1C1E22` 3,57:1 y contra `#26282D` 3,16:1 (borde de control, mínimo 3:1) |
| `acento_texto` | `#7AA5FF` | texto e iconos de acento | sobre `#1C1E22` 6,88:1; sobre `#26282D` 6,08:1 (mínimo 4,5:1) |
| `acento_foco` | `#5B8CFF` | anillo de foco y selección (no texto) | sobre `#1C1E22` 5,28:1; sobre `#26282D` 4,66:1 (mínimo 3:1) |
| `seleccion_fondo` | `#263870` | fila seleccionada | texto blanco encima 11,16:1; texto de acento `#7AA5FF` encima 4,60:1; anillo de foco `#5B8CFF` 3,53:1 (no texto, mínimo 3:1) |

Notas de uso:

- Un solo acento por pantalla: el botón principal, el enlace y el foco comparten matiz. Los estados (correcto, aviso, error) conservan sus colores semánticos y no salen de esta paleta.
- En el tema oscuro la tinta de la palabra `#161B23` es casi del color del fondo (contraste 1,06:1 con `#1C1E22`): por eso existe `altrune-logotipo-palabra.png`, una máscara blanca con su alfa que la interfaz tiñe con el color de texto (por ejemplo `#E8EAEE`), sin tocar el símbolo.
- Estas fichas son una propuesta: este paquete no cambia el código de la app.
