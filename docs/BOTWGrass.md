# La pradera

Cómo se construye el pasto de este juego, qué se midió y qué se descartó.
Comprimido el **2026-08-07**: la versión anterior tenía 1.749 líneas, la mayoría
narrando pasos ya cerrados. Lo cerrado vive en `git log -- docs/BOTWGrass.md`.

> **El norte, de `NORTE.md`:** el *feeling* de Breath of the Wild en low-poly.
> No su fidelidad. Cuando una decisión visual esté en duda, la pregunta es
> *"¿se siente como BOTW?"* y se contesta jugando.
>
> **El móvil dejó de ser un veto (2026-08-07).** Sigue siendo el destino, pero no
> el tribunal previo: ninguna técnica se descarta por lo que le pasaría en un
> aparato que nunca se midió. Se construye el feeling, se mide en lo que hay, y
> la adaptación se hace después con un perfil.
>
> **Tres tipos de número, y no se mezclan.** *(a)* medición nuestra, con fecha y
> escena; *(b)* propiedad conocida del hardware, no medida por nosotros; *(c)*
> estimación, con el cálculo al lado. Nada de rendimiento entra acá sin caer
> en (a).

---

## Lo que se observa en BOTW, y qué lo produce

Nintendo no publicó su implementación. Esto separa lo observable de la técnica
conocida que lo produce; donde es inferencia, se dice.

| Lo que se ve | Técnica | Por qué |
|---|---|---|
| Pasto en todas partes cerca del jugador | Densidad alta con geometría baratísima | La densidad es el efecto; todo lo demás existe para poder pagarla |
| El pasto **brota** del suelo al acercarse | Escalado vertical con la distancia (*grow*, no *fade*) | Es geometría: sin blending, sin orden de dibujo |
| El pasto lejano desaparece sin que se note | El albedo converge al color del terreno antes de apagarse | Si el color ya coincide, no hay borde que delate |
| El campo no termina en una línea | El terreno está pintado del verde de la raíz | El terreno *es* el LOD más lejano |
| Olas de viento recorren la pradera | Onda en espacio de mundo en el vertex shader | Función de la posición XZ: no hay estado por brizna |
| Sólo la punta flamea | El desplazamiento se multiplica por la altura normalizada | El dato viaja en el vértice |
| Las briznas no se apagan con el sol de lado | Normales hacia +Y, no la de la cara | Una cara plana con su propia normal se apaga al girar el sol |
| El campo brilla a contraluz | Transmisión: la luz atraviesa la hoja | Separa "hay pasto" de "hay un campo vivo" |
| Hay pasto en laderas pero no en roca | Filtro por pendiente al generar | Decisión de generación, no de render |
| A media distancia aparece una estera repetida | **Card mesh** — cartas que representan matojos | Confirmado con capturas del usuario: se delata por una línea horizontal en la base |

---

## Las leyes que quedan

Las dos primeras salieron de medir y siguen vigentes. Las que eran vetos del
target están abajo, degradadas a consideraciones.

**1. La unidad es la brizna de pocos triángulos.** Agrupar briznas en un matojo
modelado multiplica el costo por instancia y obliga a separarlas — que es cómo
una pradera se convierte en arbustos sueltos. Medido el 2026-07-25 *(a)*, con el
mismo gasto: matojo de 12 tris → 0,48 briznas/m²; brizna de 2 tris → 31/m².

**2. La brizna no es una entidad.** Una entidad por brizna paga transform
propagation, visibilidad y change detection por cada una, todos los frames. Un
chunk hornea sus briznas en una malla; el ECS ve una entidad por chunk.

### Degradadas a consideraciones (2026-08-07)

Eran leyes cuando el móvil vetaba. Siguen siendo ciertas *(b)* y vuelven a
mandar cuando se adapte al target — no antes.

- **`discard` desarma un tiler.** Adreno/Mali/PowerVR apagan su rechazo temprano
  de fragmentos para cualquier draw que pueda descartar. **En escritorio cuesta
  el early-Z de ese draw y nada más**, así que la carta con alfa recortado —la
  que este documento rechazaba— vuelve a estar sobre la mesa.
- **En un tiler un vértice se paga aunque no produzca un píxel.** El chip escribe
  la geometría a memoria y la relee por tile. En modo inmediato no.

---

## Estado actual (2026-08-07)

`src/visuals/grass.rs` + `assets/shaders/grass.wgsl`, 23 tests. Grilla rodante de
cuatro niveles centrada en la **cámara** (nunca en el player: el LOD responde a
lo que la pantalla muestra), briznas horneadas en una malla por chunk.

**La escalera la decide el tamaño en pantalla, no un radio.** Umbrales en
píxeles: 3 px para la hoja, 1,5 para la púa, menos para la carta. Con el viewport
de escritorio caen en ~24 m y ~47 m; a 900p se acercan a ~20 y ~40 sin tocar una
constante. Un radio en metros describe una resolución, no un campo.

| nivel | distancia | primitiva | tris/m² |
|---|---|---|---:|
| 0 | 0-13 m | hoja de 2 triángulos | 80 |
| 1 | 13-24 m | hoja de 2 triángulos | 80 |
| 2 | 24-40 m | púa de 1 triángulo | 40 |
| 3 | 40-64 m | carta de 2 triángulos, silueta recortada | 5 |

**Medido desde el mirador canónico, caja Pasto** *(a, 2026-08-07, con la huella
corregida y la carta recortada)*: **449.250 triángulos** de pradera en 32 draws,
31,6 MB residentes, y **ninguna banda de distancia por debajo del 94,7%** —
contra 92,1% de la mañana. Los 368.330 triángulos y 26,0 MB anteriores son los de
la derivación vieja: 22% más barata, con un hueco a 22-32 m y con el horizonte
leyéndose como una hilera de bloques.

### La brizna: dos triángulos unidos por una arista horizontal

```
        ∧              4 vértices, 2 triángulos
       ╱ ╲             arriba: cintura-izq, cintura-der, punta
    ●───────●          ← la arista compartida, a 0,30 de la altura
       ╲ ╱             abajo: punta hundida 6 cm, cintura-izq, cintura-der
        ∨
```

El diseño original del usuario, recuperado el 2026-08-07 tras preguntar cómo
estaban construidos los triángulos. Lo que había era un quad partido por la
**diagonal**, y las dos diferencias importan:

- **Termina en punta por los dos lados**, que es la forma de una hoja. El quad
  era ancho abajo y cortado arriba, y por eso hubo que inventarle una muesca de
  un triángulo extra para que no leyera como tira de papel.
- **Tiene una fila de vértices en el medio.** Sin ella los bordes van rectos de
  la raíz a la punta y **la brizna no puede arquearse**: el `height_factor²` del
  viento daba 0 abajo y 1 arriba igual que lineal, o sea que era un no-op sobre
  una geometría que sólo podía inclinarse rígida.

La punta de abajo se hunde 6 cm: en el suelo mismo sería infinitamente angosta y
dejaría ver tierra donde nace. Medido: **cobertura idéntica con 25.600
triángulos menos**, porque el anillo interior baja de 3 a 2 triángulos.

### La carta, ahora recortada

A 40-64 m, una carta del tamaño de un matojo (0,5 m) que el vertex shader abre
mirando a la cámara — sus cuatro vértices se hornean en el centro de la base.
Gira, y acá corresponde: una carta de canto dejaría un hueco de ese tamaño y a
esa distancia el pivoteo es invisible.

Nació **opaca** porque el móvil vetaba el `discard`; desde el 2026-08-07 recorta
su silueta (Paso 1). La escala salió de una captura de BOTW que trajo el usuario:
los trazos agrupados miden lo mismo que las flores que tienen al lado, no una
pared. La primera versión usaba 1,6 m y era tres veces más grande que la
referencia.

Medido, sólo ese nivel convertido: **688.128 → 86.016 triángulos**, la pradera de
665.600 a 450.560, la memoria de 56,3 a 35,8 MB, el horneado inicial de 420 a
281 ms — y la cobertura **sube**. Cuesta ocho veces menos y pinta más.

---

## Las herramientas (2026-08-07)

Lo que más rindió de toda la sesión, y por lejos.

### Vistas de color: `grass-view` en el hub F1

O `BOF_KNOBS=grass-view=N` al arrancar. **Ninguna cuesta un byte por vértice ni
rehornea nada**: el anillo sale de `floor(uv1.y)`, la brizna de `uv1.x` y el
chunk de `floor(xz / chunk_m)`. Lo que cambia es lo que el shader **pinta**.

| vista | qué muestra |
|---|---|
| `anillo` | un pastel por nivel: dónde cambia el LOD y cuánto se solapan |
| `chunk` | un color por celda = **un draw call** |
| `brizna` | un color por primitiva; se lee de cerca |
| `crecimiento` | qué briznas están creciendo ahora |
| `subpixel` | tres bandas exactas por ancho en píxeles |
| `medir` | plano y exacto, un color por nivel, para contar |

Y dos perillas más, del 2026-08-07: **`grass-density` tiene diez pasos** (de
0,15× a 2×, con los cuatro históricos conservados) porque cuatro puntos no
distinguen una curva de otra, y **`grass-rings`** planta un anillo solo, que es
la única forma de medir cuánta cobertura *aporta* un nivel en vez de cuántos
píxeles *gana*.

Las de *ver* tiñen el color real y dejan la luz puesta — el campo sigue
leyéndose como campo, que es la condición para juzgar si algo *se ve* mal. Las de
*medir* pintan plano y exacto: la cámara apaga tonemapping y dithering, el juego
escribe la paleta en un `.json` al lado del PNG, y `tools/shot_stats.py` cuenta
píxeles de colores **que no conoce de antemano**.

Eso reemplaza los perfiles por detección de bordes que decidieron todo el
2026-08-06: saturan con densidad alta y no distinguen una brizna baja de una
ausente — por eso no vieron el galón de briznas a media altura.

### El eje x: la fila de pantalla, en metros

`shot_stats.py` repartía la imagen en bandas de filas iguales, que **ordenan por
distancia sin medirla** — y una curva sin eje x no es una curva. Ahora la corrida
escribe su campo de visión, su viewport y la altura del ojo sobre el suelo, y el
analizador convierte cada fila a la distancia donde su rayo toca el suelo. Con
eso el conteo se reparte en anillos de metros (`--metros`).

**La conversión supone suelo plano, así que la corrida no lo supone:** muestrea
el terreno a lo largo de la línea de vista y escribe el perfil al lado del PNG.
Si ondula más de 20 cm, el analizador **omite** la tabla en vez de imprimir
metros creíbles y equivocados. Y de paso el reparto por filas quedó desmentido:
desde el mirador canónico, el **40% superior del cuadro es cielo** y las pocas
filas pegadas al horizonte se llevan de veinte metros al infinito.

### Lo que las vistas destaparon el primer día

1. **Cuatro niveles plantan sobre el mismo suelo.** Dos tercios del primer plano
   eran briznas de niveles lejanos —las de un triángulo, sin cintura—. Explica el
   fill-bound sin misterio: lo que más píxeles cubre se dibujaba tres veces.
2. **El `tint_variation` no era por brizna.** Leía `abs(uv1.x)`, que lleva el
   lado del quad en el signo, así que interpolado a lo ancho barría de +h a −h
   pasando por cero: un degradado simétrico de media cero, no un corrimiento por
   brizna. El efecto que el comentario llamaba "de los que más se notan" no podía
   notarse.
3. **La perilla de alcance medía otra cosa.** El uniform mandaba los alcances
   autorados y el vértice los escalados: a 75% el shader no encontraba el nivel
   de una brizna y anclaba la ley `1/d` en cero. **Los pasos `reach 75%` y
   `reach 50%` de la matriz medían otra ley de raleo**, así que la conclusión
   *"el alcance ahorra menos que la densidad"* hay que rehacerla.

---

## Lo medido, y lo que cada número decidió

Todo *(a)*, escritorio, caja Pasto salvo donde diga.

| qué | número | qué decidió |
|---|---|---|
| Es **fill-bound** (2026-08-06) | bajar la resolución a la mitad ahorra más que apagar la pradera entera | Las técnicas que reducen overdraw van primero |
| Reparto por nivel | los dos lejanos: **77% de los triángulos, 22% de los píxeles** | Convertirlos a cartas |
| Desperdicio de cuartetos | **96,7%** del campo se resuelve entero (≥2 px) | **La ley 2 no aplica acá**: las cartas no ganaron por sub-píxel sino por triángulos y memoria |
| Memoria residente | **56,3 MB** antes de las cartas, 26,0 después | Instancing / vertex pulling vuelven a estar sobre la mesa |
| Horneado por chunk | **5,53 ms de media, hasta 9,5** | El módulo decía "cero trabajo por frame": vale para las briznas, no para la grilla |
| Huella real de una brizna | **0,0082 m² por metro** de distancia, contra 0,0232 supuestos | Se borró el `COVERAGE_MARGIN` de ojo: la derivación ahora pide lo que la imagen entrega |
| Solapamiento de 3 a 8 m | quitarlo cuesta **0,3 puntos** de cobertura | Es desperdicio puro **en el primer plano**, donde los píxeles son caros |
| Solapamiento de 8 a 22 m | quitarlo cuesta hasta **22 puntos** | Ahí paga: el Paso 3 tiene que reponerlo, no sólo quitarlo |

### La derivación de densidad, medida entera (2026-08-07)

La densidad mínima a distancia `d` sale de cuánto suelo tapa una primitiva vista
en ángulo rasante. Tres correcciones; la tercera cerró el Paso 0 y borró las
constantes de ojo.

1. **Poisson, no área.** Las briznas caen sobre un hash, no sobre una grilla, así
   que la cobertura es `1 − e^(−λ·a)` y no `λ·a`. Para el 95% hacen falta **tres
   veces** lo que pedía la fórmula vieja. Ésa es la aritmética detrás de que el
   campo se viera ralo cada vez que se plantaba "según la derivación".
2. **El margen de 2,4**, calibrado a ojo contra la banda de 13-24 m. Tapaba un
   error sin nombrarlo; ver el punto siguiente.
3. **La huella estaba 2,83× sobreestimada.** *(a)* La fórmula usaba `ancho ·
   altura_media · d / altura_del_ojo`, que es el área de un **rectángulo
   vertical**. La brizna termina en punta, se inclina y se arquea, así que tapa
   mucho menos: **0,0082 m² por metro de distancia** para 5,5 cm de ancho, o sea
   `0,149 · ancho · d`. Con eso el margen desaparece: la derivación pide
   directamente lo que la imagen entrega.

**Cómo se despejó.** Diez densidades (de 0,15× a 2×) × nueve anillos de
distancia, contando píxeles por anillo con `grass-view=medir`. Dos anillos con
densidades distintas —23,8 y 12,9/m²— y **formas distintas** —hoja de dos
triángulos y púa de uno— dan el mismo coeficiente en diez bandas, entre 0,0077 y
0,0088. La huella depende del ancho y la distancia, no de la forma.

| anillo | banda | densidad | cobertura | huella / distancia |
|---|---|---:|---:|---:|
| 1 (hoja) | 4-6 m | 23,8/m² | 61,8% | 0,00826 |
| 1 (hoja) | 11-16 m | 23,8/m² | 93,2% | 0,00851 |
| 2 (púa) | 4-6 m | 12,9/m² | 42,5% | 0,00877 |
| 2 (púa) | 16-22 m | 12,9/m² | 85,5% | 0,00797 |
| — | la fórmula vieja suponía | | | **0,02320** |

**Y la forma exponencial quedó verificada, no supuesta.** Si la cobertura sigue
a Poisson, `−ln(1−C)/densidad` tiene que ser constante al barrer la densidad. Lo
es: dentro del 1,0-1,3% en cada banda, sobre nueve densidades. **Así que el
modelo no estaba mal "en la forma"** —como este documento afirmaba— sino en la
escala de un solo término.

Dos caminos independientes dan el mismo número. Por un lado, 2,83/2,4 = 1,18×
faltante. Por el otro, la banda de 22-32 m —el borde interno del anillo 2, donde
ningún vecino ayuda— medía 92,1% y pedía **exactamente 1,18×** para llegar al
95%. Aplicada la corrección, esa banda mide **94,7%** *(a, medido, no predicho)*
y ninguna baja de ahí. Cuesta 18% más triángulos: 368.330 → 434.510, y 26,0 →
30,7 MB.

### El solapamiento, repartido (2026-08-07)

La otra mitad del gate. Con cada anillo plantado solo (`grass-rings`), se mide
**cuánta cobertura aporta**, que no es lo que la vista `medir` cuenta sobre el
campo entero: ahí cada píxel lo gana un anillo y el que quedó detrás tapaba
igual. Cuánto cae la cobertura al quitar cada uno:

| banda | todos | sin a0 | sin a1 | sin a2 | sin a3 |
|---|---:|---:|---:|---:|---:|
| 3-4 m | 99,6% | −39,2 | **−0,3** | **−0,2** | 0 |
| 4-6 m | 99,8% | −21,8 | **−0,3** | **−0,1** | 0 |
| 6-8 m | 99,9% | −11,7 | **−0,3** | **−0,1** | 0 |
| 8-11 m | 98,4% | −4,9 | −8,1 | −2,5 | 0 |
| 11-16 m | 98,4% | −0,1 | −21,8 | −4,7 | 0 |
| 16-22 m | 97,9% | 0 | −12,4 | −12,4 | 0 |
| 22-32 m | 92,2% | 0 | −2,2 | −70,1 | 0 |
| 32-45 m | 92,3% | 0 | 0 | −11,0 | −33,6 |
| 45-64 m | 99,6% | 0 | 0 | 0 | −91,8 |

**El veredicto se parte en dos, y el documento tenía razón las dos veces.** De 3
a 8 m el solapamiento es **desperdicio casi puro**: quitar los anillos 1 y 2 de
ahí cuesta tres décimas de punto, y son justo los píxeles del primer plano, los
más caros en un frame fill-bound. De 8 a 22 m **paga**: sin el anillo 1 la
cobertura cae hasta 22 puntos. La primera versión del plan llamaba desperdicio a
todo; la corrección lo sacó entero de la tabla. Lo medido está en el medio.

**Y los niveles se pisan como sucesos independientes.** `1 − Π(1−C_k)` predice la
cobertura del campo entero con un error ≤0,5 puntos en ocho de nueve bandas (la
excepción es 32-45 m, +3,2, donde entran las cartas orientadas a cámara). Con eso
**el costo de quitar cualquier subconjunto de niveles se calcula en vez de
medirse** — que es lo que el Paso 3 necesita para no volver a fallar por
densidad.

---

## El problema abierto: los anillos

**Es el problema de fondo, identificado por el usuario tras tres sesiones.** Un
nivel decidía cuatro cosas a la vez. Tres ya están separadas:

| eje | estado |
|---|---|
| tamaño de chunk | **lo único** que el nivel decide |
| forma de la primitiva | `shape_at(distancia, pantalla)` |
| densidad | `density_at(distancia, forma)` |
| **semilla de la brizna** | **incluye el nivel — pendiente** |

Mientras la semilla incluya el nivel, cruzar una frontera **reemplaza** briznas
por otras en vez de agregarlas, y eso siempre se ve.

### El cuarto eje: implementado, medido y revertido

Se escribió entero —semilla en baldosas del mundo de 1 m, cada nivel emitiendo
las primeras N de la secuencia de cada baldosa, el rango viajando en el vértice,
la ley `1/d` reescrita a su forma correcta para un rango (`d = K/f`) y cada nivel
dibujando sólo dentro de su banda— y se midió:

| | con solapamiento | anidado exclusivo |
|---|---:|---:|
| triángulos | 368.330 | **1.074.208** |
| memoria | 26,0 MB | **75,8 MB** |
| banda 8-13 m | 99,6% | **81,1%** |
| banda 13-24 m | 98,5% | **62,1%** |

Tres hallazgos:

1. **La exclusividad cuesta ~3× la densidad**, por Poisson: de 78% a 99% hay que
   triplicar. El solapamiento estaba **pagando la cobertura**, no sólo costando.
2. **El rango no es un hash.** Con `anchor/(1-hash)` y rangos chicos, todas las
   briznas de un nivel morían juntas en su borde interno.
3. **Calibrar una constante no alcanza**, porque lo que falta no es una
   constante. Antes de reintentarlo hay que medir **la curva de cobertura contra
   densidad a varias distancias**, que las herramientas ya permiten sacar.

### Y el borde de un nivel es un **cuadrado**

`ring_cells_with_slack` usa distancia de Chebyshev, así que el alcance es un
cuadrado cuantizado a la grilla de chunks. Estuvo siempre; se hizo visible el
2026-08-07 cuando las densidades derivadas quedaron 155/70/30/1 en vez de
40/40/40/24 y cada escalón cuadrado pasó a leerse como un parche rectangular.

**Todo esto sale de la misma decisión: el LOD está horneado en mallas estáticas
por chunk.** De ahí salen la frontera cuadrada, el reshuffling, el
esconder-pero-pagar, los 5-9 ms de horneado y los megabytes. La salida es que la
brizna deje de ser geometría y pase a ser **un registro**: `MeshTag` +
`ShaderStorageBuffer` + instancing automático, que Bevy soporta de fábrica
conservando `ExtendedMaterial`. Con eso el LOD se decide por brizna y por frame.

---

## El plan: pradera abundante sin desperdicio (2026-08-07)

Escrito para implementar la próxima sesión, y **revisado por un agente sin
contexto** que encontró once problemas — todos válidos, cuatro de ellos
afirmaciones falsas sobre Bevy que verifiqué contra las fuentes de 0.19. Lo que
sigue es la versión corregida; los hallazgos están al final de la sección.

**Cada paso se valida con un color**, porque mirar es cómo se juzga y contar
píxeles es cómo se zanja.

El desperdicio, tal como está medido:

| # | desperdicio | medido |
|---|---|---:|
| 1 | La brizna se hornea como geometría | 30,7 MB residentes, **5,5-9,5 ms** por chunk, LOD congelado al hornear |
| 2 | Cada chunk es una malla propia, así que **nada batchea** | 32 draws para 32 chunks |
| 3 | Se planta un cuadrado alrededor de la cámara, **incluso detrás** | los chunks de atrás se hornean y se descartan por frustum |
| 4 | ~~La carta opaca gasta píxeles en un rectángulo lleno~~ | **cerrado por el Paso 1** |

**El solapamiento de niveles no está en esta tabla, y ahora se sabe por qué.**
La primera versión del plan lo listaba como desperdicio puro; la revisión lo
sacó entero porque el documento medía que pagaba cobertura. El Paso 0 lo repartió
*(a, 2026-08-07)*: **es desperdicio de 3 a 8 m** —quitarlo cuesta tres décimas de
punto, sobre los píxeles más caros del cuadro— y **paga de 8 a 22 m**, donde
cuesta hasta 22 puntos. Un quinto renglón de desperdicio, entonces, pero acotado
a la franja donde está medido:

| # | desperdicio | medido |
|---|---|---:|
| 5 | Tres niveles plantan sobre el primer plano | quitar los dos de afuera de 3 a 8 m cuesta **0,3 puntos** de cobertura |

### Paso 0 — La curva de cobertura — **HECHO (2026-08-07)**

Bloqueaba al Paso 3 y al spike del Paso 2. Las dos mitades del gate están
arriba: *La derivación de densidad, medida entera* y *El solapamiento,
repartido*. En resumen:

- La forma de Poisson **es correcta**; lo que estaba mal era la huella de la
  brizna, sobreestimada 2,83×. Corregida, el `COVERAGE_MARGIN` de ojo
  desapareció y ninguna banda baja de 94,7%.
- El solapamiento **no es una sola cosa**: desperdicio puro de 3 a 8 m (−0,3
  puntos al quitarlo), cobertura pagada de 8 a 22 m (−22 puntos).
- Los niveles se pisan **como sucesos independientes**, así que el Paso 3 puede
  calcular lo que le va a costar la exclusividad antes de escribirla.

Costó tres piezas de instrumental, todas reusables: diez pasos de densidad en
vez de cuatro, la perilla `grass-rings` para aislar un nivel, y el **perfil por
distancia** del analizador — la fila de pantalla convertida a metros, que es lo
que le faltaba al medidor para tener eje x.

### Paso 1 — Carta con alfa recortado — **HECHO (2026-08-07)**

El rectángulo opaco de borde plano ya no existe. En su lugar, una silueta
**procedural** —sin textura y sin `pow`, dos `fract`, dos `abs` y un `max`— que
recorta la carta en puntas: dos capas de dientes triangulares de períodos 7 y 5,
con un piso opaco abajo porque una sola capa deja huecos hasta la tierra y lee
como peine.

Segundo material sólo para los chunks de carta, con `AlphaMode::Mask`: el
`discard` cuesta el early-Z **del draw que lo usa**, y con un material único lo
pagaría también el primer plano, que es donde más fragmentos hay. Cero draws de
más, porque cada chunk ya es el suyo.

**Dos cosas que sólo aparecieron midiendo**, y ninguna estaba en el plan:

1. **La silueta recorta área, y el área es densidad.** La carta pasó a conservar
   el 58% de su rectángulo, así que su huella dejó de ser su ancho. Sin corregir
   `footprint_m`, la banda de 45-64 m se desplomó de 99,8% a **86,8%**.
2. **A esa distancia lo que tapa el suelo es la altura, no el ancho.** Corregida
   la densidad, la banda seguía en 95,9%: el suelo lejano se ve casi de canto, y
   recortar puntas baja la masa. Se arregló subiendo el piso de los dientes
   —silueta igual de irregular, más alta— hasta **97,4%**.

También se probó y **casi no sirvió**: darle a cada carta una fase propia de
silueta, porque todas miran a la cámara y quedan paralelas. Valía 0,5 puntos. La
correlación entre cartas alineadas era una hipótesis razonable y era falsa.

- **Gate:** el bloque dejó de leerse como bloque *(visto)*. La cobertura **no**
  quedó igual o mejor: 99,8% → 97,4% en la banda más lejana, sobre un objetivo
  de 95%. Cuesta 434.510 → 449.250 triángulos (+3,4%). Ataca el desperdicio **4**.

### Paso 2 — La brizna deja de ser geometría *(el desbloqueo grande)*

Los datos por brizna en un `ShaderBuffer` —**ése es el nombre en 0.19**, no
`ShaderStorageBuffer`, que es de una versión vieja— leído vía `#[storage]` en
`AsBindGroup`, que sí combina con los `#[uniform]`/`#[texture]` que
`GrassExtension` ya tiene.

Cinco piezas de plomería que la primera versión del plan daba por gratis y no lo
son. Verificadas contra las fuentes de Bevy 0.19:

1. **Una malla índice por nivel, no una sola.** El batching automático exige el
   **mismo `Handle<Mesh>`**, y una malla por chunk es lo que hoy impide todo
   batching. La salida es que **todos los chunks de un nivel tienen exactamente
   el mismo conteo de briznas** —`blades_per_chunk` depende sólo del nivel y de
   las perillas—, así que una malla índice por nivel los hace compartir handle.
   Cuatro mallas, y los draws deberían caer de 32 a ~4.
2. **Y con eso el allocator del buffer es trivial:** stride fijo por nivel,
   `MeshTag` = el slot del chunk dentro de su nivel. Sin eso haría falta un
   allocator de rangos variables con fragmentación, que es un subsistema entero.
3. **`vertex_index` hay que declararlo.** El `Vertex` de
   `bevy_pbr::forward_io` sólo lo expone bajo `#ifdef MORPH_TARGETS`
   (`forward_io.wgsl:27-29`). Como el vertex shader es nuestro, se declara un
   struct de entrada propio con `@builtin(vertex_index)`: es un builtin, no
   consume location, y el resto del layout no cambia.
4. **El AABB hay que ponerlo a mano.** Bevy lo calcula de las posiciones de la
   malla, y la malla índice no las va a tener. Sin un `Aabb` por chunk el
   culling de Bevy trabaja sobre un volumen falso.
5. **`grass.rs` ya tiene 1.605 líneas** contra el "~300 es señal de dividir" de
   §16. Este paso agrega un subsistema entero: la división del módulo entra en su
   alcance, no se descubre después.

Lo que compra: **memoria 6-8,5×** — contado contra el layout real (28 B/vértice
más índices `u32`: 136 B por hoja, 96 por púa, contra 16 del registro; el "~5×"
de la primera versión era de ojo), **horneado casi nulo**, y sobre todo el **LOD
decidido por brizna y por frame**, que disuelve la frontera cuadrada, el
reshuffling y el esconder-pero-pagar.

- **Color:** `chunk` (un color por draw) para confirmar que el batching mejora, y
  una vista nueva **`rango`**, que colorea por el número de la brizna en la
  secuencia de su baldosa — es la que hace visible el anidado del Paso 3.
- **Riesgo, y §21:** el plan describe una **combinación** de features que Bevy da
  por separado. El spike de un solo nivel no es para medir memoria: es para
  **verificar los cinco puntos de arriba** antes de convertir el resto.
- **Gate:** mismo aspecto, memoria y horneado abajo, draws abajo, frontera
  cuadrada desaparecida. Ataca los desperdicios **1** y **2**.

### Paso 3 — Praderas anidadas y exclusivas

Con la brizna como registro, "emitir las primeras N de la secuencia de una
baldosa" es trivial y rehornear es barato. Cada nivel emite un **superconjunto**
del siguiente, así que cruzar una frontera cambia la copia, no el campo.

Ya se escribió una vez y se revirtió. **Por dos cosas, no una:** la densidad
—que el Paso 0 corrige— y un bug real, que la ley `1/d` estaba escrita para un
hash y recibía un rango, así que todas las briznas de un nivel morían juntas en
su borde interno. La forma correcta es `d = K/f`. Ese arreglo entra en el gate.

**Y ahora el costo se calcula antes de escribirlo.** El Paso 0 verificó que los
niveles se pisan como sucesos independientes, así que la cobertura de un campo
exclusivo sale de la aritmética: con la huella medida, el nivel que quede solo en
cada banda tiene que llegar por sí mismo a la cobertura que hoy dan tres. En las
bandas de 8 a 22 m eso es reponer entre 8 y 22 puntos, que en densidad es
`−ln(1−C)` y no una regla de tres — el error que hizo fallar el primer intento.
De 3 a 8 m, en cambio, no hay nada que reponer: el anillo 0 ya llega solo al 99%.

- **Color:** `rango` (las mismas briznas conservan su color al cruzar) y `medir`
  (un solo nivel por banda).
- **Gate:** caminar sin ver crecer nada, **y** verificar que el mapeo rango↔ley
  no volvió a entrar mal.

### Paso 4 — Plantar sólo lo que la cámara mira

Con la existencia de una brizna decidida por frame, la grilla puede sesgarse
hacia adelante en vez de ser un cuadrado completo. Hay que cuidar el caso de
girar rápido, y por eso **depende del Paso 2**: hornear tiene que ser barato.

- **Color:** `chunk`, para ver qué se hornea y no se ve.
- **Gate:** menos primitivas horneadas con la misma imagen y sin agujeros al
  girar. Ataca el desperdicio **3**.

### Paso 5 — Devolver el viento, y el arqueo que nunca hubo

`wind_strength` está en 0 desde que se apagó para diagnosticar. Con la fila del
medio de la brizna, el `height_factor²` **por fin hace algo**. Cero geometría, e
independiente de todo lo demás.

- **Color:** ninguno — es feeling, se juega.

### Paso 6 — Interacción

El mapa de interacción: los actores estampan su huella en una textura centrada en
el jugador y el vertex shader la lee y aplasta. Independiente.

### Lo que deliberadamente no entra

- **Meshlets / mesh shaders:** la Polaris 11 del dev no los tiene.
- **Pasto generado por compute cada frame** (el método de GoT): Bevy 0.19 lo
  permite, pero el Paso 2 ya da decisiones por brizna y por frame con mucho menos
  riesgo. Horizonte, sólo si una medición lo pide.
- **Profiler propio:** después del feeling (`NORTE.md`).

### Lo que la revisión cambió

Un agente sin contexto leyó el plan y el código y encontró once problemas. Los
once válidos; las cuatro afirmaciones sobre Bevy las verifiqué a mano.

| corregido | era |
|---|---|
| `ShaderBuffer` | `ShaderStorageBuffer`, tipo de una Bevy vieja — escrito de memoria |
| Una malla índice **por nivel** | "una malla compartida", que no batchea con conteos distintos |
| Stride fijo + `MeshTag` como slot | un allocator de rangos variables que el plan no nombraba |
| Struct de vértice propio | `vertex_index` dado por gratis; sólo existe bajo `MORPH_TARGETS` |
| `Aabb` insertado a mano | culling "sigue funcionando", sobre posiciones que ya no existen |
| Memoria **6-8,5×** | "~5×", cifra de ojo |
| Paso 0 bloquea 2 y 3 | "bloquea a todo lo demás" |
| El gate del Paso 3 incluye el bug rango↔ley | "falló por la densidad, no por el diseño" |
| El solapamiento sale de la tabla de desperdicio | listado como desperdicio puro, contra lo que el propio doc mide |
| Dividir `grass.rs` entra en el Paso 2 | 1.605 líneas contra el "~300" de §16 |
| El spike verifica, no sólo mide | §21: se estaba planeando sobre una combinación no verificada |

## Errores que este documento ya cometió — no reintroducir

1. **"El pasto cuesta 0.0 ms de CPU y corre a 60 FPS estables."** Escrito el
   mismo día en que el medidor marcaba 35-46 FPS. Ningún número entra sin salir
   del medidor.
2. **"La brizna curva rinde más área por triángulo."** Refutado por medición
   propia al día siguiente: plana 1399 px/tri, cruz 1006, curva **530** — la
   peor. Con los mismos 4 triángulos, dos briznas planas dan más área que una
   cruz.
3. **"Teñir el suelo hacia la punta llena el horizonte."** Lo dice Ghost of
   Tsushima y acá lo vacía: dejó el horizonte más claro que el primer plano. La
   técnica presupone una punta que lea como masa de pasto.
4. **"El clumping es correcto porque lo hace GoT."** Compra estructura pagando
   uniformidad, y este juego quiere una alfombra. Una técnica se juzga contra el
   objetivo, no contra su prestigio.
5. **"El quad overdraw es el modo de muerte."** Traído de investigación y medido
   en contra el mismo día: es el 3,3% del campo. La ley describe un modo de falla
   real de los tilers; este campo no está en él.
6. **"La carta de grupo no sirve."** El rechazo suponía alfa recortado. La carta
   **opaca** nunca se había considerado, y ganó en los cuatro ejes.
7. **Un número no sirve si el objetivo contra el que mide se eligió a ojo.**
8. **Una herramienta que no puede fallar tampoco puede avisar.**
9. **"El modelo `C/d` está mal en la forma, no en la escala."** Al revés: la
   forma exponencial de Poisson quedó verificada sobre nueve densidades, y lo
   que estaba mal era la escala de un término —la huella de la brizna, 2,83×
   sobreestimada—. El síntoma que motivó la frase (81% donde se predecía 95%)
   era real; el diagnóstico, no. Un modelo que falla por un factor constante se
   parece mucho a uno con la forma equivocada hasta que se lo barre.
10. **Un margen calibrado a ojo esconde el error que lo hizo necesario.** El 2,4
    no era el precio de que "la fórmula no captura todo": era el 2,83 de un
    término mal calculado, redondeado hacia abajo. Cada constante de ajuste es
    una medición que no se hizo.
11. **Recortar una silueta no es un cambio de aspecto: es un cambio de
    densidad.** La carta con alfa se escribió como "el rectángulo se lee como
    bloque, dale forma", y al recortar el 42% de su área se llevó puesta la
    cobertura de la banda más lejana — 99,8% → 86,8%, sin que nada en el código
    lo dijera. Toda primitiva que descarta fragmentos tiene una huella menor que
    su geometría, y la derivación de densidad lee la geometría.


---

## Fuera de alcance (a propósito)

Generación procedural más allá del hash determinista, pasto que crece con el
tiempo, clima que lo moje.

**Mesh shaders y meshlets:** no por el target, sino por la máquina del dev — una
Polaris 11 de 2016 no los tiene.

**Vertex pulling / instancing: reabierto.** El rechazo se apoyaba en el tráfico
por frame (<1% del bandwidth, ruido) y **no miraba la memoria residente ni el
horneado**, que son 26-76 MB y 5-9 ms por chunk. Con esos dos números sobre la
mesa el veredicto se cae.

---

## Interacción (después de que el campo se vea bien)

- **Mapa de interacción**: una textura centrada en el jugador donde los actores
  estampan su huella; el vertex shader la lee y aplasta. Una lectura por vértice
  en vez de recorrer actores.
- **Corte por espada**: reacción visual a un evento de combate. Presentación
  pura — la simulación no sabe que hay pasto (§20).

---

**Fuentes:**
- [Procedural Grass in 'Ghost of Tsushima' — GDC Vault](https://gdcvault.com/play/1027033/Advanced-Graphics-Summit-Procedural-Grass)
- [hexaquo — Grass Rendering Series](https://hexaquo.at/pages/grass-rendering-series-part-4-level-of-detail-tricks-for-infinite-plains-of-grass-in-godot/)
- [shaders-botw-grass — Daniel Ilett](https://github.com/daniel-ilett/shaders-botw-grass)
- [Optimization View Modes — Unreal Art Optimization](https://unrealartoptimization.github.io/book/profiling/view-modes/)
- [Analyzing Quad Overdraw — Unigine](https://developer.unigine.com/en/docs/2.21/content/optimization/geometry/quad_overdraw/)
- [Instancing — ejemplo oficial de Bevy](https://bevy.org/examples/shaders/automatic-instancing/)
