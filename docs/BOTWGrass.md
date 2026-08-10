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

`src/visuals/grass.rs` + `grass_records.rs` + `assets/shaders/grass.wgsl`, con
tests de contrato. Grilla rodante de tres niveles centrada en la **cámara** (nunca en el
player: el LOD responde a lo que la pantalla muestra). Desde el Paso 2 **ninguna
brizna es geometría**: cada una es un registro de 16 bytes y el vertex shader la
construye.

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

> ## El veredicto del 2026-08-07, jugando — y qué resultó ser
>
> **"No se siente bien el pasto. Siento que retrocedimos más que avanzamos, y
> siento que es por muchas razones pequeñas."** — el usuario, después de jugar la
> versión con los Pasos 0, 1 y 2 aplicados.
>
> Sus tres quejas eran **dos bugs**, no muchas razones pequeñas:
>
> 1. **"Los billboards están muy cerca."** El chunk decidía, no la brizna:
>    aislando el anillo 3 se plantaban **cartas a un metro de la cámara**.
>    Arreglado con `blade_birth` — el anillo 3 aislado da hoy **0% adentro de los
>    22 m** y 98,8% en 45-64.
> 2. **"El pasto desaparece por cuadrados enteros."** No era el rodado ni el
>    culling: `@builtin(vertex_index)` **no arranca en cero**. Ver abajo.
> 3. **"El sistema de anillos no está bien."** Era el mismo bug: lo que se veía
>    era un nivel entero apagándose.
>
> **La lección que este documento tenía escrita y no se respetó:** *primero se ve
> bien, después se optimiza*. Los Pasos 1 y 2 optimizaron sobre una imagen que
> nunca se aceptó jugando — y el bug del `vertex_index` estaba ahí desde el Paso
> 2, invisible para toda captura desde el mirador fijo porque **el nivel que
> apagaba cambiaba en cada corrida**.

> ## El segundo veredicto del 2026-08-07, jugando
>
> Después del arreglo del `vertex_index`:
>
> **"El problema de los anillos y el crecimiento del pasto a medida que uno
> camina todavía se mantiene, pero está suave, ya no desaparecen chunks de pasto
> agresivamente. Los billboards son muy notorios en comparación con el otro
> pasto, pero creo que estamos más cerca de lograr un buen feeling. No hay nada
> evidente que se pueda capturar en una foto, ahora todos los problemas son en
> movimiento."**
>
> **La última frase cambia el método.** `BOF_SHOT` y su conteo de píxeles ya no pueden
> zanjar lo que queda: miden una imagen fija y lo que sobra es una imagen que
> cambia. De acá en adelante, lo que se pueda poner en una perilla del hub se pone
> —él lo barre jugando y contesta en una sesión— y lo que no, se decide con su
> descripción.

### La primera consecuencia: `grass-growth` es una perilla

Buscando el crecimiento que él ve caminando apareció una contradicción entre el
código y su propio comentario: la rampa dice *"corta, una brizna sola creciendo
es imperceptible"* y la dispersión *"larga, es lo que convierte la ola en un
raleo"* — y **las dos valían 6**. Con la rampa igual de larga que la dispersión,
una brizna tarda seis metros de caminata en salir: más de un segundo, que es
tiempo de sobra para verla crecer.

No se corrigió a ojo. `GRASS_GROWTH_STEPS` pone cinco pares `(rampa, dispersión)`
en el hub, con el índice 0 en el 6/6 que él jugó, para que la comparación arranque
en lo conocido. **Los triángulos no cambian entre pasos**: el selector de chunks
usa el techo de la tabla, así que la grilla es la misma y lo único que se mueve es
cuántas briznas están a altura completa. Lo que sí cambia es el fill — medido
entre el paso 0 y el 4, la cobertura sube de 93,4% a 97,9% en 22-32 m, o sea más
solapamiento y más píxeles pagados.

### Y lo que la perilla contestó: **la rampa no era el problema**

Barrió los cinco pasos jugando y ninguno mató el crecimiento. Su reporte, que es
el dato más importante de la sesión:

> **"Noto los crecimientos entre los anillos 0-1 y 1-2, se nota, siempre se ha
> notado, veo cómo crecen y cómo se achican los pastos, y ésa ha sido mi punto
> más importante. En 0.4 se ve mejor porque los anillos están distribuidos de una
> forma más random en vez de anillos circulares fijos, pero aún se nota el
> crecimiento y desaparición del pasto."**

**Su explicación de por qué 0.4 se ve mejor es el diagnóstico.** No es que la
transición se suavice: es que **se rompe la frontera visible**. Lo que ayuda es
que deje de haber un círculo, no que el círculo tenga borde blando. Está
describiendo la solución, no el ajuste.

Y confirma que la escalera de `grass-growth` es un paliativo. Los anillos son
**cuatro campos de pasto distintos sobre el mismo suelo** —la semilla incluye el
anillo— así que cruzar 0→1 no es la misma brizna con menos detalle: es un campo
que se achica hasta desaparecer mientras otro crece desde cero en su lugar. Una
**sustitución disfrazada de fundido**; la rampa sólo decide cuánto se estira el
disfraz.

### El billboard no era el color: era el suelo pelado

Se midió el color medio por banda desde una cámara de juego (3,2 m, 15° abajo) y
la diferencia no existe: 146,2 de luminancia a 3-8 m contra **149,3** a 45-64 —
el lejano es 2% *más claro*, no más oscuro. Lo que él veía como "otro color" es
la franja de **terreno sin pasto pasados los 64 m**, un verde oliva liso que
ocupa buena parte del horizonte. **No es un bug:** ahí después va la niebla, que
todavía no llega tan cerca. Anotado y cerrado.

Para poder medirlo hizo falta arreglar la herramienta: `write_legend` no escribía
nada con la vista de diagnóstico apagada, así que **la única captura que muestra
el color que el jugador ve era la única sin eje de distancias**.

## El bug que se llevaba el campo: `vertex_index` no arranca en cero

Encontrado el 2026-08-07 después de descartar, midiendo, el rodado, el frustum
culling, el crecimiento, el modo de alfa, el tamaño del buffer y los registros en
CPU. El síntoma: **dos corridas idénticas, misma pose y mismo código, daban
campos distintos** — a veces el anillo 3 cubría el 93% de su banda, a veces el
0%.

El vertex shader ubicaba la brizna así:

```wgsl
let record = blade_records[slot * stride + vertex.vertex_index / 4u];
```

En un draw **indexado**, `@builtin(vertex_index)` vale el índice **más el
`base_vertex`** que Bevy le da a la malla dentro de su buffer compartido
(`MeshAllocator`). Una malla grande se lleva un slab propio y arranca en cero —de
ahí que los tres niveles cercanos anduvieran—, pero **una chica comparte slab y
arranca corrida**: el nivel leía fuera de su casillero, recibía ceros, y con
ceros una carta no se abre (sus cuatro vértices se hornean en el mismo punto).
Nivel entero ausente, sin un solo error.

Cuál nivel caía dependía del orden de asignación de mallas, que cambia entre
corridas. **Verificado por el reverso:** subiéndole la densidad al anillo 3 para
que su malla pasara a tener slab propio, las tres corridas salieron completas.

**El arreglo:** la dirección de la brizna —cuál es y cuál esquina— viaja en
`UV_0`, que la malla índice ya tenía como relleno. Un atributo viaja *con* el
vértice y no sabe nada de dónde vive la malla. Cuesta cero bytes: el atributo ya
estaba. Dos tests lo cobran, uno sobre la malla y otro sobre el shader.

**Lo que esto enseña sobre el instrumental** (ver `docs/AHORA.md`): un bug no
determinista pasa por "cambió el viento" en una sola captura. Lo que lo delató
fue **repetir la misma captura tres veces**, no mirarla mejor.

**Medido desde el mirador canónico, caja Pasto** *(a, 2026-08-07, al cerrar la
sesión)*: 364.200 triángulos de pradera en 4 draws, 4,5 MB de mallas más 8,5 MB
de buffers, 70 chunks. Contra el estado del mediodía —mismo aparato, misma
pose— eran 449.250 triángulos y 84 chunks: **menos geometría y más campo**.

| banda | mediodía (Pasos 0-2) | ahora | |
|---|---:|---:|---|
| 3-4 m | 99,7% | 99,3% | |
| 4-6 m | 99,9% | 99,9% | |
| 6-8 m | 99,8% | 99,7% | |
| 8-11 m | 95,8% | 96,5% | |
| 11-16 m | 88,9% | **94,5%** | |
| 16-22 m | 88,0% | **96,9%** | |
| 22-32 m | 73,6% | **93,4%** | |
| 32-45 m | 61,1% | **99,0%** | |
| 45-64 m | 48,3% | **98,8%** | |

Ninguna banda baja de 93%, y las cinco lejanas eran el agujero del campo. **Las
tres corridas repetidas dan el mismo número** — que es la mitad del resultado.

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
*medir* pintan plano y exacto: la cámara apaga tonemapping y dithering, y **la
misma corrida cuenta los píxeles de cada color** sobre los bytes que van al
archivo (`perf/shot_stats.rs`).

Eso reemplaza los perfiles por detección de bordes que decidieron todo el
2026-08-06: saturan con densidad alta y no distinguen una brizna baja de una
ausente — por eso no vieron el galón de briznas a media altura.

### El eje x: la fila de pantalla, en metros

El primer analizador repartía la imagen en bandas de filas iguales, que **ordenan
por distancia sin medirla** — y una curva sin eje x no es una curva. Ahora la
corrida usa su campo de visión, su viewport y la altura del ojo sobre el suelo
para convertir cada fila en la distancia donde su rayo toca el suelo, y reparte
el conteo en anillos de metros.

**La conversión supone suelo plano, así que la corrida no lo supone:** muestrea
el terreno a lo largo de la línea de vista. Si ondula más de 20 cm, **omite** la
tabla y dice por qué, en vez de imprimir metros creíbles y equivocados. Y de paso
el reparto por filas quedó desmentido:
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

> **El 0,149 quedó corregido el 2026-08-08 y la constante ya no es una sola.**
> Este despeje usó el número de la **perilla** como `λ`, y con eso el raleo por
> distancia se cuela dentro de la huella. Despejando contra la densidad **viva**
> —que la corrida sabe desde que la escalera de alcances es determinista— la
> constante va de **0,082 cerca a 0,114** a 27 m, y **0,185** para la carta. O
> sea la ley pedía 1,8× menos briznas de las que hacen falta en el primer plano.
> No se notaba porque el solapamiento de tres niveles lo compensaba; el Paso 3
> lo quitó y quedó a la vista. Tabla y corrección: `hidden_per_width_per_metre`.

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

## La brizna decide, no el chunk — **HECHO (2026-08-07)**

`ring_cells_with_slack` decidía qué se ve mirando el chunk **entero**: si
cualquier parte de él caía más allá de la frontera de traspaso, se plantaba
completo. Los chunks del anillo 3 miden **32 metros**, así que uno que asomaba la
punta más allá de la frontera traía un cuadrado que **contenía a la cámara**, con
sus cartas a tamaño completo — la ley `1/d` sólo ralea lo lejano, nunca lo
cercano. De ahí *"los billboards están muy cerca"*.

**El arreglo es simétrico al borde de afuera**: `blade_birth` en `grass.wgsl`. La
brizna **nace** con la distancia en vez de morir con ella, con su umbral repartido
por hash sobre la banda `[inner − spread − ramp, inner]`. Tres propiedades que
hacen que funcione donde el intento anterior dejó un pozo:

- **Es una banda, no un corte.** Un umbral duro sería un círculo de cartas
  apareciendo a distancia fija que se mueve con el jugador: el mismo pop con otra
  forma.
- **Termina en el borde interno del anillo**, que es donde el de adentro todavía
  está entero. Por eso el traspaso no deja costura: uno muere mientras el otro
  nace, sobre el mismo tramo.
- **No toca el solapamiento que paga.** El Paso 0 midió que de 8 a 22 m el
  solapamiento vale hasta 22 puntos de cobertura; lo que se recorta es sólo de la
  frontera hacia adentro.

Medido, anillo 3 aislado desde el mirador *(a, 2026-08-07)*: **0,0% adentro de
los 22 m** —antes plantaba cartas a un metro—, 91,8% en 32-45 y 98,8% en 45-64.
Los billboards volvieron a ser la capa más lejana.

### Y el borde de un nivel dejó de ser un cuadrado

El selector medía en Chebyshev, así que el alcance era un cuadrado cuantizado a
la grilla. **Se podía porque el chunk decidía la imagen; ahora que decide la
brizna, el selector sólo dice qué se tiene en memoria** y puede medir en
euclídeas sin que se vea nada. Con eso el borde es un círculo y —esto sí importa—
el test *"todas las briznas de un chunk descartado ya están muertas"* pasa a ser
cierto: en Chebyshev no lo era, porque la esquina de un cuadrado está a √2 de su
lado.

## La decisión tomada: **la brizna pertenece al mundo**

> **"El tema del pasto, según yo, sí pertenece al mundo. Esto es algo que hemos
> discutido varias veces en distintas sesiones; puede que otros agentes se hayan
> equivocado, pero el pasto siempre debió pertenecer al mundo."**
> — el usuario, 2026-08-07, cerrando la sesión

**Esto no es una opción a evaluar: es la dirección del proyecto, y lleva varias
sesiones dicha.** Cualquier agente que retome esto lo hereda decidido. Si una
medición sale en contra, lo que se replantea es *la implementación*, no el
rumbo — y hay que decirlo con esas palabras en vez de proponer volver a anillos
independientes.

**Qué quiere decir, en concreto:** la posición de una brizna sale de una grilla
fija del **mundo**, no del anillo. Cada baldosa tiene su secuencia determinista de
briznas y cada distancia dibuja las **primeras N** de esa secuencia. El anillo
pasa a decidir *cuántas*, nunca *cuáles*.

La propiedad que se compra —y es la que arregla su queja de siempre— es que
**acercarse sólo agrega**. Las briznas que ya estaban no se mueven, no se achican
y no se reemplazan, porque son literalmente las mismas. Nada crece en el medio del
campo: sólo aparece detalle al fondo.

Un nivel decidía cuatro cosas a la vez. Tres ya están separadas:

| eje | estado |
|---|---|
| tamaño de chunk | **lo único** que el nivel decide |
| forma de la primitiva | `shape_at(distancia, pantalla)` |
| densidad | `density_at(distancia, forma)` |
| **semilla de la brizna** | **incluye el nivel — el trabajo de mañana** |

### El intento anterior: qué falló, y qué NO falló

**Se intentó una vez y se revirtió, pero el veredicto no fue "la idea no
sirve".** Fueron dos errores concretos de implementación, y conviene tenerlos
presentes mañana porque los dos son evitables.

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
   constante.

**Y los dos primeros son errores separables de la idea, no consecuencias de
ella:**

- **Anidar no exige excluir.** Que cada nivel dibujara *sólo* dentro de su banda
  fue una decisión aparte, y es la que costó el 3×. La pertenencia al mundo dice
  *de dónde salen las posiciones*; no dice que los niveles no puedan pisarse. Se
  puede anidar conservando el solapamiento que el Paso 0 midió que **paga**.
- **El rango no puede salir de un hash.** `anchor/(1-hash)` con rangos chicos
  mata juntas a todas las briznas de un nivel en su borde interno. Necesita ser
  el índice de la brizna dentro de la secuencia de su baldosa — que es
  justamente el dato que la pertenencia al mundo hace existir.

El tercero era lo único que faltaba antes de reintentar, y **está medido**: la
curva de abajo.

## La curva, y la ley que salió de ella (2026-08-08)

`BOF_SHOT_SWEEP=grass-density` recorre la escalera entera de una perilla en una
corrida y deja una fila por paso. Con la vista `medir` y el mirador de la suite
(ojo a 1,6 m, mirando casi horizontal), la cobertura de pantalla por banda:

| briznas/m² | 3-4 | 4-6 | 6-8 | 8-11 | 11-16 | 16-22 | 22-32 | 32-45 | 45-64 |
|---|---|---|---|---|---|---|---|---|---|
| 80 | 100,0 | 100,0 | 100,0 | 99,8 | 99,6 | 99,9 | 99,6 | 100,0 | 99,8 |
| 64 | 100,0 | 100,0 | 100,0 | 99,2 | 98,8 | 99,7 | 98,7 | 99,9 | 99,6 |
| 52 | 99,8 | 100,0 | 100,0 | 98,3 | 97,5 | 99,0 | 97,2 | 99,7 | 99,4 |
| 44 | 99,5 | 99,9 | 99,9 | 97,4 | 95,8 | 97,8 | 95,1 | 99,3 | 99,2 |
| **40** | **99,3** | **99,9** | **99,7** | **96,5** | **94,5** | **96,9** | **93,4** | **99,0** | **98,8** |
| 30 | 98,2 | 99,0 | 98,9 | 92,7 | 89,3 | 93,2 | 87,4 | 97,2 | 97,5 |
| 20 | 94,0 | 95,7 | 96,1 | 84,2 | 77,6 | 83,2 | 74,6 | 91,8 | 93,7 |
| 12 | 82,7 | 84,9 | 86,0 | 67,4 | 59,8 | 65,0 | 56,2 | 78,9 | 83,3 |
| 6 | 47,1 | 59,2 | 63,7 | 44,3 | 34,7 | 40,7 | 32,5 | 52,5 | 61,9 |

(en % de los píxeles de la banda; el paso 0/m² da cero en todas y no se lista)

**Y se deja describir por una sola constante por banda.** Con las briznas como
puntos independientes, `C = 1 − e^(−λ·a)`: cada paso propone su propio `a =
−ln(1−C)/λ`, y todos proponen el mismo. En 22-32 m los nueve pasos dan **0,0682
m²** con extremos 0,0656 y 0,0690 — ±2,5% sobre un rango de densidad de **13×**.

| banda | 3-4 | 4-6 | 6-8 | 8-11 | 11-16 | 16-22 | 22-32 | 32-45 | 45-64 |
|---|---|---|---|---|---|---|---|---|---|
| `a` (m²) | 0,127 | 0,157 | 0,158 | 0,085 | 0,072 | 0,088 | 0,068 | 0,118 | 0,117 |

**Qué es y qué no es.** `a` es *efectiva*: absorbe el solapamiento entre anillos
del reparto actual y el LOD de la primitiva, no es el área física de una brizna.
Lo que sí es, es **predictiva** dentro de este reparto — y eso alcanza para lo
que el Paso 1 necesitaba.

**Lo que contesta:** cuántas briznas necesita una distancia para una cobertura
objetivo, `λ = −ln(1−C)/a`, en vez de elegirse a ojo. Para 97% pareja:

| banda | 3-4 | 4-6 | 6-8 | 8-11 | 11-16 | 16-22 | 22-32 | 32-45 | 45-64 |
|---|---|---|---|---|---|---|---|---|---|
| λ (briznas/m²) | 28 | 22 | 22 | 41 | **49** | 40 | **51** | 30 | 30 |

**La densidad plana de 40/m² está mal repartida:** sobra ~1,5× en el primer
plano y falta ~25% justo en 11-16 y 22-32 m. Y esas dos bandas son las de las
fronteras entre anillos — las mismas donde el usuario ve crecer y achicarse el
pasto al caminar. La queja tiene, además de su causa estructural, un déficit de
cobertura medible.

Determinista: dos barridos idénticos dieron la misma tabla salvo una celda que
difiere en 0,1 punto.

**Todo esto salía de la misma decisión: el LOD horneado en mallas estáticas por
chunk.** De ahí la frontera cuadrada, el reshuffling, el esconder-pero-pagar, los
5-9 ms de horneado y los megabytes. El Paso 2 lo desarmó —la brizna es un
registro— y recién con eso el LOD pudo pasar a decidirse por brizna y por frame,
que es lo que arriba se hizo.

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

### Paso 2 — La brizna deja de ser geometría — **SPIKE HECHO (2026-08-07)**

Convertido **un nivel**, el 3, que ya era casi un vertex pull. Los cuatro puntos
de plomería quedaron verificados corriendo, y **dos de ellos escondían una
trampa que el plan no vio**:

| punto | resultado |
|---|---|
| Malla índice por nivel → batching | ✓ los draws de pradera bajan de **32 a 23** |
| Stride fijo + `MeshTag` como casillero | ✓ con reciclado de casilleros al rodar |
| `vertex_index` en un struct propio | ✓ **pero** ver la trampa 1 |
| `Aabb` a mano | ✓ **pero** ver la trampa 2 |

**Trampa 1: el struct propio pierde los `#ifdef`.** El `Vertex` de `forward_io`
declara cada atributo dentro del suyo, así que se encoge solo hasta calzar con la
malla. Escrito a mano no hay nada que lo encoja, y declarar `normal` en el
location 1 —que el de Bevy tiene— pide a la etapa anterior un `Float32x3` que la
pradera **nunca hornea**: su normal es +Y, calculada. El pipeline no compila y
wgpu lo dice con el número de location, que es lo único que lo hizo barato.

**Trampa 2: poner el `Aabb` a mano no alcanza.** `calculate_bounds` tiene *dos*
queries: uno inserta el AABB donde falta, y **el otro lo sobrescribe cuando
`Mesh3d` cambia** (`bevy_camera/src/visibility/mod.rs:568-575`) — y un chunk
recién nacido siempre lo tiene cambiado. Así que Bevy pisaba el AABB del chunk
con el de la malla índice, cuyas posiciones son todas cero: un punto en el
origen, y el nivel entero culleado. La salida está en el mismo código:
`NoAutoAabb`.

**Y así es como se vio.** Con el nivel culleado, la foto normal **se veía bien**:
el terreno está teñido del verde de la raíz, así que a 45-64 m el pasto ausente y
el suelo son casi del mismo color. Lo delató `grass-view=medir` en una línea —
`a3 = 0,0%`, la banda al 8,9%—. Sin esa vista, este bug entraba al repositorio.

**Lo que costó, contado:** *(a)* el nivel 3 pasa de **6,41 MB a 0,98 MB** — 28
mallas de 1766 briznas contra una malla índice (0,23 MB) más el buffer de
registros (0,75 MB). **6,5×**, dentro del 6-8,5× que el plan estimó. La cobertura
queda igual dentro del punto: 96,4% contra 97,4% en la banda de 45-64 m, y la
diferencia es que el hash pasó a salir de la posición en vez del atributo, así
que la ley `1/d` ralea otras briznas.

**Dos cosas quedan pendientes y conviene decirlas ahora:**

- La malla índice **conserva los atributos dummy** (posición, uv, uv1 en cero)
  porque quitarlos cambia los shader defs del pipeline y el fragment es
  compartido con los niveles horneados. Son 0,23 de los 0,98 MB: recortarlos
  llevaría el ahorro a ~8×.
- El horneado inicial dio **2,31 ms por chunk de media** contra los 5,53 medidos
  antes, pero la comparación quedó inválida: entre medio cambió la densidad y
  mezcló chunks grabados con horneados. Las mediciones posteriores usan la suite.

#### Y el medidor mentía sobre la memoria

`SceneCensus` sumaba `vertex_bytes` **por entidad**. Con una malla por chunk eso
era correcto; con una malla índice compartida por diez chunks, contaba diez
copias de algo que existe una vez — o sea que el ahorro entero de este paso era
invisible para el instrumento que tenía que medirlo. Corregido a **por malla
única**; los triángulos siguen por entidad, porque ésos sí se dibujan tantas
veces como instancias haya.

Queda una ceguera **declarada**: el inventario cuenta mallas, no
`ShaderBuffer`s. La memoria que este paso mueve de la malla al buffer no aparece
ahí, y por eso el buffer se loguea aparte en cada corrida. Cuando los cuatro
niveles estén convertidos, el inventario tiene que aprender a verlo o va a
declarar una caída que es en parte mudanza.

### El plan original del Paso 2 *(el desbloqueo grande)*

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

### Paso 3 — La brizna pertenece al mundo — **HECHO (2026-08-08)**

`visuals/grass_tiles.rs`. El suelo se reparte en baldosas de 2 m; cada baldosa
tiene su secuencia determinista y la brizna `j` está siempre en el mismo lugar,
la dibuje quien la dibuje. Los tres cambios que lo hacen funcionar, y que son
exactamente los tres errores del intento revertido:

1. **El alcance sale del índice, no de un hash.** La escalera de alcances es la
   ley de densidad *invertida*: `live_density_at` dice cuántas hacen falta a una
   distancia, y la escalera contesta hasta dónde sigue haciendo falta la número
   `j`. La densidad viva a cualquier distancia *es* la que la ley pide.
2. **Anidar no exigió excluir.** Los niveles se reparten *índices*, no suelo:
   cada brizna la dibuja el nivel más barato que le alcanza. Nadie dibuja dos
   veces y nadie pregunta qué otro nivel cubre este punto — que es lo que costó
   3× la vez anterior.
3. **La curva se midió antes** (arriba), así que cuántas emite cada nivel salió
   de una cuenta y no del ojo.

El shader pierde la ley `1/d`, los dos hashes de umbral y el nacimiento del lado
de adentro: `blade_growth` es una rampa hasta el alcance que la brizna trae en su
registro. **No hay frontera que cruzar.**

**Medido** (vista `medir`, mirador de la suite, dos corridas idénticas):

| banda | 3-4 | 4-6 | 6-8 | 8-11 | 11-16 | 16-22 | 22-32 | 32-45 | 45-64 |
|---|---|---|---|---|---|---|---|---|---|
| antes | 99,3 | 99,9 | 99,7 | 96,5 | **94,5** | 96,9 | **93,4** | 99,0 | 98,8 |
| ahora | 99,6 | 99,9 | 99,9 | 100,0 | **100,0** | 100,0 | **100,0** | 100,0 | 99,6 |

Las dos bandas que se hundían eran las de las fronteras. Cuesta lo mismo:
362.752 triángulos contra 364.200, 4 draws, y los registros bajan de 8,47 a
7,83 MB.

**El gate, jugado el 2026-08-08:**

> **"El crecimiento creo que está mucho mejor que antes, creo que lo que hay que
> hacer ahora es fine tuning."** — el usuario, tras la tercera sesión del día

Es la primera vez que *"veo cómo crecen y cómo se achican los pastos"* —su punto
más importante durante tres sesiones— deja de ser el problema de fondo. No es un
"resuelto": es *mucho mejor*, y lo que sigue es afinar.

Antes, en la misma sesión, había reportado dos cosas que se arreglaron y volvió a
verificar: los billboards cerca (Paso 3b) y el anillo de matojos. Y una
observación que vale registrar porque es método: **con la vista de color ve el
crecimiento y con el color normal casi no.** Es lo esperado —la vista pinta
categorías planas, así que muestra el cambio de *pertenencia*— pero conviene
decirlo cuando se pide un veredicto sobre una vista de diagnóstico.

### Paso 3b — La forma la decide la distancia, no el nivel

**Lo que la captura destapó al terminar el Paso 3:** el nivel de cartas se lleva
el **73% del primer plano**. Sus briznas son las de índice bajo —las que llegan
lejos— y ahora están vivas también a tres metros, donde antes el nacimiento del
lado de adentro las apagaba. La foto quieta se ve bien; un billboard a 3 m gira
con la cámara, y eso sólo se ve caminando.

**Apagarlas cerca no es la salida:** su densidad es parte de la que la ley pide,
así que quitarlas ralea la banda, y hacerlas desaparecer al acercarse rompe
justamente lo que el Paso 3 compró.

La salida es que **la misma brizna cambie de forma con la distancia**: hoja de
cerca, carta de lejos, sin desaparecer. Es la ley que el LOD de este sistema ya
sigue —la escalera la decide la pantalla— aplicada a la primitiva. Pide que
todas las mallas índice lleven dos triángulos por brizna y que el recorte de
silueta se decida por brizna en vez de por material.

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

## El fine tuning, y con qué números empieza (2026-08-08)

Con el crecimiento ya en *"mucho mejor que antes"*, lo que queda es afinar. La
primera vuelta ya está hecha y salió de una intuición suya —*"siento que podemos
lograr un mejor ratio de pastos"*— que resultó medible: la ley pedía de menos
cerca y de más lejos.

| configuración | 3-4 m | 11-16 m | 22-32 m | 45-64 m | triángulos |
|---|---|---|---|---|---|
| ley vieja, dial 40 | 76,4% | 88,8% | 90,1% | 99,0% | 362.752 |
| ley vieja, dial 80 | 94,8% | 98,7% | 99,1% | 99,9% | 728.576 |
| **ley medida, dial 40** | **91,0%** | **94,3%** | **95,0%** | 98,4% | **605.952** |

La ley medida da casi la imagen del dial 80 con **17% menos triángulos**, y la
cobertura queda pareja en 91-95% en vez de repartida entre 76% y 99%.

### Tres niveles, uno por forma (2026-08-08)

> *"Hay muchos anillos, esa es la sensación que realmente me da. Deberían ser
> cero, uno y billboard... con cero la brizna de dos triángulos, el uno la de un
> triángulo, y el billboard después. Incluso siento que el billboard debería
> mezclarse un poco con el LOD uno."*

Eran cuatro y **los dos primeros tenían la misma forma** —los dos hoja—, así que
el segundo no aportaba nada más que una frontera. Desde el Paso 3 el nivel ya no
decide *cuáles* briznas, sólo cuántas caben en su buffer, así que fusionarlos no
cambia el campo: quita un borde. Quedan `16 m` (hoja, chunks de 8), `40 m` (púa,
16) y `64 m` (carta, 32) — y la mezcla carta↔púa que él pide ya existe, porque el
umbral de carta está repartido por brizna entre 34 y 62 m.

**Lo que costó, y por qué se pagó igual.** Un nivel planta su tramo en **todo**
su territorio aunque las briznas mueran antes del borde, así que llevar el
primero de 16 a 24 m multiplica por 3,4 el área donde se plantan las más tupidas.
El primer intento se recortó a 16 m para no pasar el techo; el veredicto del
usuario fue **no recortar el diseño por el techo**:

> *"Olvidémonos del techo por ahora, optimizamos cuando logremos el feeling
> correcto."*

Así que cada nivel llega hasta donde su forma llega, el techo por vista sube de 2
a **3 millones** de triángulos como deuda declarada, y los chunks del primer
nivel pasan a 12 m para que el conteo de draws siga entrando. Es la misma regla
que el proyecto ya tenía escrita —primero se ve bien, después se optimiza— con la
diferencia de que ahora el costo está anotado con su número.

Y el pasto va **más largo** (0,55-0,96 m contra 0,45-0,90), también pedido
jugando: lo que distingue una carta de sus briznas vecinas es sobre todo la masa,
y un campo más alto se le parece más.

Medido: cobertura 91-99% pareja, 654.848 triángulos y **5 draws** en vez de 6.

**Jugado y aceptado** al cerrar el día: el primer plano no se reportó ralo, y las
fronteras quedaron explícitamente fuera de la lista de problemas. Lo que sigue
está en *Por dónde retomar*, al final.

## Por dónde retomar (cierre del 2026-08-08)

**Lo que el usuario dijo al cerrar, textual, porque es el mapa:**

> *"Las fronteras están bien, ese nunca fue el problema, y usé cámara libre para
> ver. Anillo 0 y 1 están bien (sigo pensando que anillos no es la solución
> correcta), los billboards son el problema ahora."*

Tres cosas, y conviene no mezclarlas:

1. **Las fronteras y los niveles 0 y 1 están aceptados.** Verificado por él con
   cámara libre, no de paso. Ahí no hay trabajo pendiente.
2. **El siguiente problema son los billboards.** Es la queja que sobrevivió el
   día entero: *"la diferencia entre el pasto normal y el billboard sigue siendo
   muy notoria"*. Hoy se le atacó el **anillo** (el umbral va repartido por
   brizna entre 34 y 62 m) y el **grano** (la carta bajó de 0,5 a 0,25 m), y aun
   así sigue siendo lo que se nota.
3. **Los anillos siguen sin convencerlo como arquitectura**, dicho tres veces en
   el día. No es un pedido de cambio inmediato —el 0 y el 1 están bien— pero es
   la dirección de fondo, igual que *"la brizna pertenece al mundo"* lo fue ayer.
   Quien retome esto **no lo trata como cerrado**.

### Lo que se sabe del billboard, para no arrancar de cero

- Se abre a partir de `card_from_m` ≈ 48 m con el viewport de referencia —donde
  la brizna mide 1,5 px— y el umbral va **repartido por brizna** entre 0,7× y
  1,3× de eso, o sea 34-62 m. Más cerca, la misma brizna se dibuja como hoja.
- Mide 0,25 m de ancho y su silueta conserva el 58,3% del rectángulo
  (`CARD_SILHOUETTE_AREA`, integral de `card_silhouette` en el shader).
- **El color no es el problema:** medido el 2026-08-07 desde una cámara de juego,
  la luminancia va de 146,2 a 3-8 m a 149,3 a 45-64.
- Lo que **no** se probó: que la altura y el ancho sigan a la distancia como ya
  hace la forma; que la carta tenga variación de tono por instancia; que el
  solape carta↔púa sea más ancho.

### Tres experimentos del 2026-08-09, jugados

1. **La variación de tono ya corría sobre la carta.** No hacía falta
   implementarla: `blade_tint` usa `fract(record.w)`, el mismo identificador
   por brizna que hoja y púa, y la carta sólo pisa `uv_b.x`. No era la causa.
2. **La carta crece con la distancia** (`card_growth_scale`, 1,0×→1,6× dentro
   de su propia banda) — agregado, sin jugar todavía si alcanza por sí solo.
3. **Engordar la púa (idea 3) no cambió nada, jugado.** *"Está igual que
   antes."* La causa más probable: la púa tiene yaw al azar, no mira a
   cámara — triplicar su ancho no ayuda a una brizna que queda de canto según
   el ángulo, porque lo que ves depende del ángulo, no del número. El
   billboard no es "más ancho", es "siempre muestra su ancho completo", y eso
   es lo que compra mirar a cámara. Esto es evidencia a favor de que el
   billboard hace un trabajo real, no sólo caro.

**Ahora en la simulación (`CARDS_ENABLED = false` en `grass.rs`):** en vez de
sacar el código de la carta, se apagó su uso — todo blade que hubiera sido
carta ahora es púa engordada (`spike_growth_scale`, 1,0×→3,0×), por pedido
explícito de comparar las dos sin perder ninguna. `true` vuelve a lo de antes
sin tocar nada más.

**Reactivadas con `AlphaToCoverage` (2026-08-09, jugado):** el usuario decidió
volver a probar pese al diagnóstico —*"siempre muestra su ancho completo"*,
arriba— porque los árboles van a necesitar billboards de todos modos. El
recorte de silueta dejó el `discard` puro por uno que sólo descarta lejos del
borde y difumina ~1 px alrededor, que `AlphaMode::AlphaToCoverage` (con MSAA
2x, ya default) convierte en cobertura de muestras en vez de un salto binario.
**Jugado: "funciona", pero el anillo lejano sigue leyéndose distinto** — el
borde suave no tocó la causa real (mirar siempre a cámara), tal como se
sospechaba. Se acepta por ahora — con menos triángulos que antes, no es la
misma cuenta que perdía la sesión pasada. Detalle de rendimiento en
`AHORA.md` → *Cierre del 2026-08-09*.

### El agujero pasados los 64 m: empujado, no cerrado (2026-08-09)

El plan de siempre había sido *"ahí después va la niebla"* (§*El billboard no
era el color: era el suelo pelado*, 2026-08-07). Se probó —niebla empujada a
40-80 m, tope 70%— y jugado no funcionó: una niebla puesta a propósito para
esconder algo se nota como eso, no como profundidad (investigación de cómo la
usa BOTW: reserva la opacidad real a escenas autoradas, no a tapar bordes de
LOD). La niebla volvió a lo gradual (`camera/mod.rs`).

Segundo intento: el último nivel pasó de **64 a 128 m** (`chunk_m` 32→64 con
él, misma proporción que los otros dos). `MEADOW_VIEW_TRIANGLES` subió de 4 a
5 millones para admitirlo. **Jugado y no alcanzó:** *"sigo viendo el corte,
pero más lejos."* Correr el número más lejos mueve el síntoma sin tocar la
causa — **no seguir por ahí**; si se retoma, hace falta una técnica distinta
(vestir el terreno con otra cosa que no sea brizna, o una transición que no
dependa de un alcance). Aparcado a pedido del usuario para pasar a optimizar.

### Los anillos como arquitectura, confirmados (2026-08-09/10)

El usuario venía quejándose de los anillos desde el 2026-08-08 sin que
ninguna captura lo mostrara con claridad. Pedido explícito: *"ejecuta el
juego y te saco una foto, porque eso está clarísimo con la herramienta de
colores"* — `grass-view=medir` (color plano por anillo, hecho a propósito
para contar píxeles) mostró tres bandas con borde nítido y horizontal,
prácticamente un círculo alrededor de la cámara.

**Contra la tabla de BOTW observado** (arriba, *Lo que se observa en BOTW*):
su LOD también cambia con la distancia a cámara —brota, cambia de forma,
converge de color— así que geométricamente también tiene que tener algún
límite centrado en la cámara; no hay forma de que un LOD screen-space-driven
no lo tenga. La lectura, sin fuente oficial: BOTW no evita el círculo, lo
**disimula** (ruido, densidad decreciente, color convergiendo al terreno).
Un dato de la misma tabla reencuadra la queja del billboard: *"a media
distancia aparece una estera repetida... se delata por una línea horizontal
en la base"* — el card mesh de BOTW también se nota, así que "se lee
distinto" puede no ser un bug nuestro sin resolver, sino la misma limitación
de la técnica real.

**Plan acordado, tres técnicas en orden, checkpoint entre cada una:**
1. Mezclar los tres assets (2 tris / 1 tri / card mesh) para que el card mesh
   se vea igual al pasto de 1 triángulo.
2. Ruido perturbando la distancia de cada anillo — ataca el círculo en sí.
3. Sesgo de LOD por posición en pantalla (más barato en los bordes de cámara)
   — más caro/riesgoso (puede hacer que una brizna cambie de forma al girar
   la cámara, no al caminar), al final a propósito.

### Técnica 1: mezclar los tres assets (2026-08-10, "suficientemente bien
por ahora")

**Siguiente incremento — laboratorio `Card mesh` (abierto, 2026-08-10).** Antes
de tocar de nuevo la pradera se construye una caja de prueba aislada: una escena
nueva, terreno plano propio, sin pradera rodante, y sólo unas pocas referencias
LOD0/hoja y LOD1/púa junto a variantes explícitas de carta. El laboratorio es
presentación desechable, marcada al salir de escena. No modifica la selección
de LOD, densidad, buffers ni shader de la pradera. Es un flag exclusivo
—apagado incluso en Mundo—, con heightmap plano
propio. Sus mallas se crean una vez y se comparten entre reentradas. Las
referencias se nombran hoja y púa, no por anillo. Tests canario exigen que la
caja no cree `GrassChunk` ni que Pasto cree el laboratorio. Criterio de salida:
checkpoint jugado que elija una dirección visual o descarte la carta; recién
entonces se planea un cambio de la pradera y se vuelve a medir.

**Ajuste de laboratorio (2026-08-10, abierto).** El primer checkpoint mostró
dos errores del propio banco, no de la técnica: sus láminas proyectaban sombras
y la textura de la carta estaba invertida verticalmente. Se corrigen sólo en
`Card mesh`; la pradera sigue intocada hasta que el usuario elija una silueta.
La imagen `T_GrassCard_Albedo.png` tiene alfa binaria, pero su RGB oculto es
negro; el laboratorio estaba además anulando la textura, por lo que no servía
para juzgar una carta dibujada. No se cambia el material ni el shader de
producción.
El checkpoint posterior siguió viéndolas negras: queda prohibido inferir color
desde el handle. Antes de otro cambio se inspecciona la entidad/material que
llega al render y la orientación de sus normales bajo la luz real.

**Aislamiento de lectura (2026-08-10, abierto).** La inspección confirmó que
las nueve entidades del banco sí usan `FoliageCommon`, verde y doble cara, pero
ese material es PBR: no proyectar sombras no impide que luz, normales y
exposición lo oscurezcan. Para decidir sólo la silueta, el laboratorio ahora
clona ese material una vez, lo deja verde/opaco/doble cara y lo marca `unlit`;
referencias y cartas comparten esa copia. No se muta el handle canónico (también
usado por el bosque) ni se afirma que este banco valide el aspecto final de
producción. Canarios cubren asociación de las nueve entidades al material del
lab y winding coherente con sus normales. El checkpoint pendiente es verlo
verde al girar; recién después se compara silueta y triángulos.

**Carta de mata amplia (2026-08-10, abierto).** Se descartaron la carta cruzada
y la mata de tres láminas: más paneles sólo devolvían los triángulos visibles,
no más lectura de pasto. El banco ahora deja una única carta amplia de 2 tris,
un quad UV completo de 4,8 × 4,8 m, para que una ilustración de mata larga tape
mucha pantalla sin pagar más geometría. Su albedo propio
`T_GrassCardLab_Albedo.png` representa hojas largas superpuestas, raíz continua
y huecos grandes; es `unlit`, doble cara y `Mask(0.4)`, mientras hoja y púa
siguen opacas. Es una excepción de laboratorio para poder juzgar el coste y la
lectura de alpha: `Mask` ejecuta el fragment shader sobre todo el rectángulo y
no valida todavía el baseline de producción sin alpha. Canarios fijan 2 tris,
4 vértices, UVs completos, los dos materiales separados y 7 especímenes sin
sombras. El checkpoint debe mirar la carta de frente, al girar y al alejarse:
sin negro/halo/shimmer, raíz al suelo y textura que se lea como pasto antes de
llevar cualquier decisión a la pradera.

**Paleta de la carta (2026-08-10, abierto).** La ilustración inicial era más
oliva/amarilla que las briznas. El primer recolor apuntó al gradiente real de
la pradera, pero era el objetivo incorrecto para este banco: sus referencias
LOD0/LOD1 usan la copia `unlit` opaca de `FoliageCommon`, `#529438`. El RGB de
la carta queda anclado a ese verde y conserva el relieve de la ilustración sólo
como claro/oscuro; su alpha quedó idéntico byte a byte. Así la caja responde
honestamente *"¿la carta se lee junto a estas briznas?"*. No valida todavía
producción: allí intervienen el gradiente lineal, variación por brizna,
iluminación, niebla y transmisión de la pradera real.

**Recorte de base y separación (2026-08-10, abierto).** Se modifica sólo el
alpha de la carta. El primer intento extendió cuatro raíces hacia filas sin
arte RGB y reveló bloques verdes; se descarta. La máscara vuelve a su silueta
base y se le tallan únicamente tres entrantes anchos y asimétricos desde el
borde inferior existente hacia arriba. No se pinta alpha nuevo: RGB queda
idéntico y `alpha_nuevo ≤ alpha_base` píxel a píxel. La mata puede flotar un
poco, por decisión del checkpoint, pero la línea horizontal se rompe sin
rectángulos ni microdientes. Esto no ahorra el coste del rectángulo alpha ni
compensa cobertura de pradera: es únicamente el checkpoint de lectura de la
carta.

**Adopción de la carta ilustrada en Pasto (2026-08-10, abierto).** Aprobada la
lectura del banco, la imagen pasa a `T_GrassMeadowCard_Albedo.png`; el banco la
carga como consumidor, nunca al revés. **No** se llevó su quad ECS de 4,8 m a
la pradera: habría roto el batching de `GrassChunk`, los registros de 16 B y la
transición por brizna. La producción conserva su malla índice instanciada de
dos triángulos, tamaño, anillos y draws; sólo las briznas que `blade_is_card`
ya seleccionaba muestrean la ilustración en `grass.wgsl`.

La imagen no reemplaza la luz de la pradera: su RGB aporta variación de
luminosidad normalizada y el shader conserva degradado, tinte determinista,
luz, niebla y transmisión. Su alpha reemplaza la silueta procedural. Color y
prepass llaman a la misma muestra; el prepass descarta bajo 0,5, el mismo cutoff
con que `AlphaToCoverage` cae a `Mask` sin MSAA. El checkpoint jugado rechazó el
fallback heredado que bajo 5 px volvía la carta un rectángulo sólido. Se retira:
el alpha de la ilustración manda a toda distancia. Esto puede reabrir shimmer o
puntos de cielo subpíxel; es un riesgo explícito de checkpoint, no una razón
para esconder la silueta aprobada detrás de un bloque.

La huella/densidad queda **provisional**: `CARD_SILHOUETTE_AREA = 0,583` es la
calibración de la carta procedural, no una afirmación sobre el PNG. La máscara
visible de la fuente a alpha ≥0,4 es 35,63% del rectángulo, pero su área efectiva
depende de la distancia y del filtrado alpha. Antes de tocar la escalera se mide
`grass-view=medir` por anillo y distancia; bajar o subir densidad sin esa tabla
sería una regresión disfrazada de reducción de triángulos. El PNG aún no tiene
mips: es deuda explícita del pipeline, a vigilar por shimmer durante el
checkpoint jugado. La captura técnica válida conserva 5 draws (3 de pradera);
no se declara una mejora de ms ni de triángulos hasta comparar A/B repetible.

**Carta más ancha, menos instancias vivas (2026-08-10, abierto).** Por pedido
del checkpoint se ensancha `CARD_WIDTH` de 0,25 a **0,30 m** (+20%). No hay una
segunda perilla de densidad: `footprint_m = ancho × 0,583` alimenta
`minimum_density`, así que donde una brizna ya es Card la ley pide 0,25/0,30 =
**16,7% menos**. AABB, uniform de media anchura y la variación ±30% derivan de
la misma constante. Es una primera escala moderada: 0,35 m ampliaría 40% el
salto duro púa→carta antes de medirlo.

No confundir “menos cartas vivas lejos” con “menos triángulos enviados”: la
malla índice del anillo exterior se dimensiona por su borde interior, que aún
usa púas, y manda 2 tris por registro incluso cuando el shader colapsa una
brizna fuera de alcance. El criterio de este paso es la lectura en 35–65 m y
el coste de fragmentos; bajar el conteo de triángulos requiere otro cambio de
representación, no falsear el presupuesto. La huella alpha del PNG sigue
provisional y exige `grass-view=medir` antes de dar esta densidad por calibrada.

**Varianza de ancho por carta.** El diagnóstico de siempre: la carta mira
siempre a cámara y por eso muestra *siempre* su ancho completo, mientras que
una púa real tiene orientación fija en el mundo y casi siempre se ve de
perfil. No se tocó el eje (`camera_right`): girar la carta reintroduce el
hueco de cobertura que el billboard existe para evitar. En cambio, ancho por
brizna (`hash_position(record.xy, 3u)`, ±30% centrado en 1.0 — no desplaza la
media, no toca la ley de densidad). Efecto real pero sutil en captura: rompe
el empedrado uniforme, no lo resuelve solo.

**Bug propio, encontrado y arreglado en el camino: puntos de cielo en el
anillo lejano.** Reportado por el usuario jugando, confirmado con F7. Causa:
`card_silhouette` mete ~6 dientes a lo ancho de la carta; cuando la carta
entera mide un par de píxeles en pantalla, cada píxel ya cruza varios
dientes y su valor puntual cae cerca de algún borde **casi siempre** — no es
que la banda de antialiasing sea ancha, es que el patrón entero se volvió
ruido de muestreo. `AlphaToCoverage` convierte ese ruido en cobertura
fraccional de MSAA, y lo que se cuela por el hueco es el cielo. Dos
intentos:
- Acotar `card_aa_width` (`CARD_AA_MAX`) — no alcanzó solo, queda como
  defensa de segunda línea.
- **El real:** apagar el recorte dentado por completo bajo
  `CARD_SILHOUETTE_MIN_PIXELS` (ancho de la carta en píxeles), y dibujar un
  bloque sólido — la misma lógica que ya usa la escalera hoja→púa→carta.
  Primer valor (16 px) estaba mal calculado: una carta recién nacida
  (`card_from_m`, ~35-65 m con el reparto por brizna) ya mide 5-9 px, por
  debajo de 16 — apagaba el dentado **siempre**, no sólo lejos, y volvió las
  cartas un bloque liso reportado por el usuario (*"se nota mucho la
  diferencia entre el cardmesh y el pasto generado"*). Corregido a 5 px
  (`326/5 ≈ 65 m` con el cálculo de `card_from_m`) — recupera el dentado en
  el rango donde las cartas realmente viven.

**Los dientes nunca se habían recalibrado contra el ancho actual de la
carta.** Con 7 y 5 columnas sobre `CARD_WIDTH = 0,25 m` (bajó de 0,5 el
2026-08-08 y estos números quedaron como estaban — advertido en el propio
código), cada diente medía 3,6 y 5 cm — **más angosto que una brizna real**
(`BLADE_WIDTH` = 5,7 cm). Un diente más chico que una brizna no puede leer
como "unas briznas juntas": es detalle de sub-píxel garantizado. Bajado a
**3 y 2** columnas (coprimos, igual que antes) — 8,3 y 12,5 cm por diente,
más anchos que una brizna.

**Cambiar el conteo de dientes cambia la integral, no sólo el ancho —
medido, e intentado remedir sin éxito.** El propio código ya avisaba: *"si
cambiás estos números, actualizá `CARD_SILHOUETTE_AREA`"*. Con el valor viejo
(0,583) tras bajar los dientes, la cobertura de la banda 45-64 m
(`grass-view=6`, mirador `0,20,0` mirando `0,-0.2,0.98`) cayó a **46,7%**.
Dos pasadas de la inversión de Poisson que usa `shot_stats::estimate_density`
(`a = −ln(1−C)/λ`) llevaron el valor a 0,583 → 0,122 → 0,098, con la banda
midiendo **91,9%** — pero **rompió `every_distance_gets_the_density_it_demands`
/ `the_living_density_is_the_one_the_law_asks_for`**: a 50 m la escalera
(`grass_tiles::reach_ladder`) sólo entrega 10/m² cuando la ley con esa área
pide 13,2/m². Medir una banda de pantalla no prueba que la ley se cumpla en
*todas* las distancias — el test sí las mide todas, y el `cargo test` de
cierre de sesión lo agarró. Una lectura de código durante el intento —que la
escalera reparte una capacidad fija y sólo "redistribuye" en vez de faltar—
quedó **refutada** por el propio test. Revertido a 0,583 para no dejar un
test roto: **la carta queda rala con los dientes nuevos**, deuda declarada,
no resuelta esta sesión. Antes de reintentar: entender por qué la escalera
no alcanza a 50 m con una demanda mayor, no sólo remedir una banda distinta.

**Estado:** el usuario lo dio por suficiente por ahora, sin cerrar la
técnica del todo — quedó pendiente ver si el card mesh sigue leyéndose
distinto jugando (no sólo en captura estática), y la densidad de la carta
sigue rala (ver arriba). Técnica 2 (ruido en el límite del anillo) y 3
(sesgo por pantalla) quedan para la próxima sesión.

### Detrás, y sólo después

- **Medir los milisegundos.** Todo lo de hoy son conteos, y la geometría casi se
  duplicó: 1.264.384 triángulos en cuadro contra 654.848. El pasto se llevaba
  12,94 ms de GPU de un cuadro de 15,29 **antes** de eso. `BOF_BENCH=grass`.
- **El horneado va a 1 chunk por frame.** `CHUNKS_BAKED_PER_FRAME = 1`, con un
  comentario que ya no describe el sistema.
- **El Paso 5, el viento**, aplazado desde el principio: *"el viento sigue siendo
  algo que sólo vamos a hacer cuando todo lo demás esté bien"*.

**Cerrado, no reabrir:** el color de los billboards; la cobertura del primer
plano; revertir el Paso 2; y el techo de triángulos mientras el feeling no esté
—*"olvidémonos del techo por ahora"*—.

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
12. **Un medidor que se escribió para una técnica deja de medir cuando la
    técnica cambia.** `vertex_bytes` por entidad era correcto mientras cada chunk
    tuviera su malla; con una malla compartida contaba diez copias de una. El
    instrumento no falló: siguió dando un número creíble del mundo anterior.
13. **Poner el AABB a mano no alcanza si algo lo puede pisar.** El plan decía
    "insertarlo" y era la mitad: `calculate_bounds` lo sobrescribe en cuanto
    `Mesh3d` cambia. Verificar una API leyendo su firma no es lo mismo que leer
    su cuerpo.
14. **Una captura desde un mirador fijo no reemplaza jugar.** Las tres quejas del
    2026-08-07 —billboards encima, cuadrados que desaparecen, el campo que no se
    siente— **no aparecen en ninguna captura** desde el mirador canónico: dos de
    las tres necesitan movimiento y la tercera necesita una altura de cámara que
    el mirador no tiene. Se optimizó dos pasos seguidos contra una imagen que
    parecía correcta y no lo era. El medidor de píxeles zanja *cuánta cobertura
    hay*; no contesta *cómo se siente el campo*, y esa era la pregunta.
15. **Optimizar antes de que la imagen esté aceptada es construir sobre arena.**
    Está escrito en este documento como ley —*primero se ve bien, después se
    optimiza*— y se incumplió igual, porque los números del Paso 2 eran buenos y
    los buenos números se sienten como progreso.




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

16. **Un bug no determinista se disfraza de "cambió el viento".** El del
    `vertex_index` apagaba un nivel entero, y cuál nivel cambiaba en cada corrida.
    Cada captura suelta parecía una configuración distinta con una explicación
    plausible: "el anillo 3 no llega", "el primer plano perdió densidad". Lo que
    lo delató fue **repetir la misma captura tres veces con todo idéntico** — y
    eso cuesta tres minutos. Antes de explicar una diferencia entre dos
    configuraciones, verificar que la misma configuración se repite a sí misma.

17. **Cuando algo desaparece, sospechar del direccionamiento antes que del
    contenido.** Se descartaron, midiendo, el rodado, el frustum culling, el
    crecimiento, el modo de alfa, el tamaño del buffer y los registros en CPU —
    todos "el dato está mal"— antes de mirar *cómo se lo busca*. Con vertex
    pulling, un índice corrido no da error: da ceros, y los ceros se ven como
    "no se plantó".

18. **Un builtin del shader es una API, y hay que leerla.** `@builtin(vertex_index)`
    parece un contador desde cero y no lo es: en un draw indexado incluye el
    `base_vertex`. Lo que hizo el bug caro es que **funciona igual** mientras la
    malla sea grande, porque una malla grande se lleva un slab propio.

19. **Una perilla no arregla una decisión de arquitectura.** El 2026-08-07 se
    puso `grass-growth` en el hub para atacar "veo crecer el pasto al caminar", y
    los cinco pasos lo suavizaron sin matarlo — porque lo que se ve no es un
    parámetro mal calibrado sino que los anillos son **campos distintos**. Antes
    de ofrecer una perilla, preguntarse si lo que molesta es un valor o una
    estructura. Si es estructura, la perilla compra tiempo y confunde el
    diagnóstico.

20. **Cuando el usuario dice "esto ya lo discutimos varias veces", es una
    decisión, no una opinión a reevaluar.** La pertenencia de la brizna al mundo
    lleva sesiones dicha y se siguió tratando como una opción abierta, en parte
    porque una medición previa salió en contra. Una medición en contra replantea
    *la implementación*; el rumbo lo fija él.

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
