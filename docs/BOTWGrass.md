# Pasto denso estilo BOTW — técnicas y cómo se replican en Bevy

Hoja de ruta del sistema de pasto. Reescrito el **2026-08-04**: la versión
anterior (2026-07-25) tenía las tres leyes correctas y medidas, pero gobernaba
el sistema por la restricción equivocada y contradecía a `NORTE.md`. Las
mediciones se conservan íntegras; el marco que las interpreta cambió.

> **Cómo se usa este documento.** Un paso por vez. Cada paso se cierra con su
> entregable **jugado** en la caja `Pasto` (`grass.ron`) y medido con el hub F1
> antes de abrir el siguiente. Un paso que no se puede validar no se implementa.
>
> **Excepción vigente desde el 2026-08-05, por decisión del usuario:** primero se
> hace que el pasto **se vea bien**, después se optimiza. La regla de medir antes
> de cada paso queda suspendida para la parte estética y vuelve en cuanto la
> imagen esté aceptada. El motivo es de sentido: optimizar algo que todavía no se
> ve como uno quiere es afinar el objeto equivocado. **El target también cambió a
> 900p30**, contra el 1080p60 que dice `NORTE.md` — unas 2,6 veces menos píxeles
> por segundo, así que varios veredictos escritos acá contra el target viejo
> (empezando por el de las cartas de grupo) hay que revisarlos.
>
> **Honestidad de fuentes.** Nintendo no publicó su implementación. Lo que sigue
> separa **lo observable en el juego** de **la técnica conocida que produce ese
> resultado**. Donde algo es inferencia, se dice.
>
> **Tres tipos de número, y no se mezclan.** *(a)* **Medición nuestra**: sale del
> hub F1, lleva fecha y escena. *(b)* **Propiedad del hardware objetivo**: cómo
> se comporta un GPU tile-based; es conocimiento de ingeniería, no medición
> nuestra, y va marcado como tal. *(c)* **Estimación**: lleva la palabra
> *estimado* y el cálculo al lado. Ninguna afirmación de rendimiento de nuestro
> juego entra acá sin caer en (a).

---

## El target manda: Android de gama media, ~2021

`NORTE.md` fija el piso: **móvil de gama media de alrededor de 2021, a 60 FPS**.
Eso es un Snapdragon 695/750G/778G (Adreno 619/642L) o un Dimensity 700/900
(Mali-G57), pantalla de ~1080×2400, memoria LPDDR4X compartida con la CPU. Todo
lo que sigue se justifica contra esa máquina y no contra otra.

Cuatro propiedades de esa clase de hardware *(tipo b)* deciden el diseño entero
de este sistema, y ninguna es el conteo de triángulos:

**1. Son GPU tile-based, y ahí un vértice se paga en bandwidth.** El chip primero
corre un pase de binning que transforma la geometría y **escribe los resultados a
memoria**; después dibuja tile por tile releyendo esos datos. Un vértice no
cuesta solo ALU: cuesta escribirlo y volver a leerlo. La memoria es el recurso
más escaso del aparato, y la comparte con la CPU. Consecuencia directa: **una
brizna cuesta aunque no produzca ni un píxel.**

Y lo que se escribe no es sólo la *entrada* del vertex shader: también su
**salida**. Un `VertexOutput` de PBR ronda los 68 bytes por vértice *(tipo c:
posición de clip + posición de mundo + normal + uv + color)* — más que los 48
bytes de atributos que tenemos hoy, y casi seis veces los 12 que quedarían
después del Paso 2. **Después de adelgazar el vértice, los varyings dominan el
tráfico**, y su tamaño lo fija el pipeline de Bevy, no nosotros. La única palanca
que queda sobre ellos es *tener menos vértices*, que es el Paso 4. Esto decide el
veredicto sobre vertex pulling; ver *Fuera de alcance*.

**2. El overdraw se cobra en cuartetos.** El rasterizador trabaja en bloques de
2×2 píxeles. Un triángulo que en pantalla cubre menos de un píxel igual dispara
cuatro fragmentos. Una pradera vista de lejos es exactamente eso: decenas de
miles de triángulos sub-píxel, cada uno cobrando cuatro. **Este es el modo de
muerte clásico del pasto en móvil**, y no aparece en ningún conteo de
triángulos.

**3. `discard` desarma la arquitectura.** Adreno tiene LRZ, Mali tiene Forward
Pixel Kill, PowerVR tiene HSR: los tres descartan fragmentos ocultos antes de
correr el shader, y **los tres se apagan para cualquier draw cuyo fragment
shader pueda hacer `discard`**. En escritorio perder early-Z es una molestia; en
un tiler es tirar la ventaja principal del chip. Ver la ley 3.

**4. El presupuesto real es el sostenido, no el pico.** Un teléfono baja su reloj
de forma apreciable tras diez o quince minutos de carga *(tipo b: es
característica térmica de la clase, no un número nuestro)*. Un sistema que entra
justo en 16,6 ms en frío no corre a 60 FPS en una sesión de juego. **Se diseña
contra ~11 ms, y los 16,6 son el techo duro, no el objetivo.**

### Y por eso: nada de este documento está medido en el target

Todos los milisegundos de acá salen de la máquina del dev — una **AMD Polaris 11
(RX 460)**, que es un renderer de modo inmediato de escritorio. Frente al target
no difiere en grado sino **en tipo**: los dos costos que más importan arriba
(bandwidth de vértices, `discard` contra LRZ) simplemente no se comportan igual.

No es excusa para no medir: es la razón por la que el **dial de overdraw del hub
F1** (`visuals/diagnostic.rs`) pasa a ser el instrumento principal de este
sistema y no una curiosidad. El overdraw sí se lee en escritorio y sí transfiere.
Hasta que exista una corrida en un teléfono real, ningún milisegundo de acá
autoriza a declarar que el pasto "entra en el target".

---

## Lo que se observa en BOTW, y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| El suelo no se ve: hay pasto en todas partes cerca del jugador | Densidad alta (decenas de briznas por m²) con geometría baratísima | La densidad es el efecto; todo lo demás existe para poder pagarla |
| El pasto **brota** del suelo al acercarse; nunca aparece de golpe | Escalado vertical con la distancia (*grow*, no *fade*) | Es geometría, no transparencia: sin blending, sin `discard`, sin orden |
| El pasto lejano desaparece sin que se note | El albedo converge al color del terreno antes de apagarse | Si el color ya coincide, la desaparición no tiene borde que delate |
| El campo no termina en una línea | El terreno está pintado del mismo verde que la raíz de la brizna | El terreno *es* el LOD más lejano |
| Olas de viento recorren la pradera entera | Onda en espacio de mundo evaluada en el vertex shader | Una función de la posición XZ: no hay estado por brizna |
| La raíz queda quieta y solo la punta flamea | El desplazamiento se multiplica por la altura normalizada del vértice | El dato de altura viaja en el vértice, no en el CPU |
| Las briznas no se oscurecen cuando el sol pega de lado | Normales hacia +Y (o la normal del terreno), no la de la cara | Una cara plana iluminada por su propia normal se apaga al girar el sol |
| El campo **brilla** a contraluz al atardecer | Transmisión: la luz atraviesa la hoja | Es lo que separa "hay pasto" de "hay un campo vivo" |
| Hay pasto en las laderas pero no en la roca | Filtro por pendiente al generar | Decisión de generación, no de render |
| El pasto se corta con la espada y vuela | Reacción visual a un evento de combate | Presentación pura: la simulación no sabe que hay pasto (§20) |
| Se aplasta al caminar sobre él | Deformación leída de una textura de interacción | Una lectura de textura por vértice, en vez de recorrer actores |

---

## Las cuatro leyes de este sistema

Las tres primeras salieron de medir el primer intento; violarlas reproduce el
fracaso que ya tuvimos, **matojos aislados sobre tierra pelada**. La cuarta sale
del target y es la que faltaba.

### 1. La unidad es la brizna de 2 triángulos

Agrupar briznas en un matojo modelado multiplica por seis el costo de cada
instancia, y la única forma de seguir dentro del presupuesto después de eso es
separarlas — que es cómo una pradera se convierte en arbustos sueltos.

Medido el 2026-07-25 *(tipo a)*, con el mismo gasto de triángulos:

| unidad | tris | instancias en 28k tris | densidad en radio 12 m |
|---|---|---|---|
| matojo (primer intento) | 12 | 2.352 | **0,48/m²** |
| brizna | 2 | 14.000 | **31/m²** |

### 2. La brizna no es una entidad

Una entidad por brizna paga transform propagation, visibilidad y change
detection por cada una, todos los frames. A densidad real son decenas de miles
de entidades y el frame se acaba antes de dibujar un triángulo.

**Un chunk hornea sus briznas en una sola malla.** El ECS ve una entidad por
chunk. El costo de una brizna pasa a ser dos triángulos y nada más. Es el mismo
patrón que `visuals/terrain.rs` usa para los 32.768 triángulos del suelo.

### 3. La opacidad es el enemigo, y el `discard` es la opacidad disfrazada

- **Alpha blending** obliga a ordenar, no escribe depth útil y apaga el early-Z.
  Con pasto es letal: cada píxel queda cubierto por decenas de briznas y se
  pagan todas.
- **Alpha test (`Mask`)** también lo rompe: el hardware no puede descartar el
  fragmento antes de ejecutar el shader que hace el `discard`.
- **Y el dithering igual.** El dithering **es** alpha test, solo que sacando el
  umbral de un patrón de ruido en pantalla en vez de una textura. Lleva
  `discard` y apaga LRZ/FPK/HSR exactamente igual. *(La versión anterior de este
  documento afirmaba que el dithering "conserva el depth y el early-Z". La
  primera mitad es cierta y es su única ventaja real sobre el blending — escribe
  depth correcto y no depende del orden de dibujo. La segunda mitad es falsa.)*

Este proyecto ya pagó la lección en escritorio: pasar el follaje de `Mask` a
`Opaque` llevó el bosque de 13 a 60 FPS (`visuals/foliage.rs:96`). En el target
la lección es más cara, no menos.

**Regla:** el pasto es **opaco, siempre**. La silueta va en la geometría. Las
transiciones se hacen con **crecimiento**, que no necesita `discard` en
absoluto. El dithering queda permitido **solo dentro de la banda de transición
entre anillos de LOD**, nunca sobre el campo entero, y sólo si el crecimiento
por sí solo no alcanzó. `AlphaMode::Blend` no entra en este sistema.

### 4. La brizna que no se ve igual se paga

Corolario de que el target es un tiler: **un vértice cuesta aunque su triángulo
sea degenerado**. Toda técnica de LOD para este sistema tiene que reducir el
número de vértices *que se envían*, no esconder los que ya se enviaron.

Esto invalida un plan entero que parecía razonable — colapsar briznas a altura
cero en el vertex shader según la distancia — y lo reduce a lo único para lo que
sirve: suavizar la transición entre dos niveles que **ya difieren en cantidad de
geometría horneada**. La densidad se decide al construir la malla del chunk.

---

## Estado actual (medido el 2026-08-06, en escritorio)

Implementado en `src/visuals/grass.rs` y `assets/shaders/grass.wgsl`, 18 tests.
Grilla rodante de tres anillos: **56 / 28 / 10 briznas por m²** hasta 10, 16 y
32 m, chunks de 5/10/20 m, brizna de 0,45-0,90 m, punta partida en los dos
anillos internos. 489.200 triángulos declarados en los 360° al peor
alineamiento.

### La medición que este documento venía pidiendo desde que se reescribió

Caja `Pasto`, altura de ojo mirando al horizonte, Polaris 11 *(tipo a: medición
nuestra)*. **Tres corridas el 2026-08-06**, y se presentan las tres a propósito,
porque no coinciden:

| paso | corrida A | corrida B | corrida C |
|---|---:|---:|---:|
| baseline (56/m²), GPU ms | 6,08 | 4,87 | 5,43 |
| pasto apagado | 2,31 | 2,31 | 3,07 |
| **costo del pasto** | **3,77** | **2,56** | **2,36** |
| render 50% | −3,90 | −3,10 | −3,04 |
| densidad 12/m² | −2,52 | −1,98 | −1,84 |
| alcance 50% | −2,12 | −1,66 | −1,44 |
| MSAA 4x | +2,10 | +3,17 | +1,81 |

Deriva interna de cada corrida: 0,25 / 0,24 / 0,17 ms. O sea que **dentro** de
una corrida la máquina estuvo quieta, y aun así el costo del pasto varía **entre
corridas de 2,36 a 3,77 ms** — una dispersión del orden de la mitad del efecto.
La causa conocida: el usuario tenía **Blender abierto**, que compite por CPU y
por GPU. De ahí sale una regla del ritual: *cerrar lo que compita por la GPU
antes de medir*, porque el encabezado de contexto del reporte no puede declarar
lo que no ve.

**Lo que aguanta las tres corridas**, que es lo único que se puede afirmar:

**1. La pradera es entre el 45% y el 62% de la GPU de su caja.** Por resta
contra un paso en cero, no extrapolando. El número exacto depende de la corrida;
el orden de magnitud no.

**2. Es fill-bound, y con eso se cierra la pregunta central del documento.** En
las **tres** corridas, bajar la resolución a la mitad —misma geometría, los
mismos 489.200 triángulos— ahorra **más que apagar la pradera entera**. Lo que
cuesta es cuántos píxeles pinta cada brizna encima de otra, no cuántas hay.
Consecuencias:

- El conteo de triángulos es **guardrail, no objetivo** — con la salvedad de
  siempre, que en el target tile-based un vértice se paga en bandwidth aunque no
  produzca un píxel *(tipo b)*, y eso no se manifiesta en esta máquina.
- El cambio de target a **900p30** golpea exactamente la palanca correcta, y no
  por casualidad: es el mismo eje que el paso de render 50%.
- Las técnicas que reducen *overdraw* pasan al frente de la fila; las que
  reducen vértices, atrás.

**3. El alcance ahorra menos que la densidad**, en las tres. Recortar la vista
lejana no es la primera palanca.

**4. MSAA cuesta entre 1,81 y 3,17 ms de GPU**, y en la única corrida con frame
utilizable, **2,48 ms de frame — un 32%**. Ya no hace falta pagarlo: no era la
causa del parpadeo (ver más abajo).

### Lo que estas corridas NO dicen

En dos de las tres el **frame quedó clavado por la presentación** (~16,6 ms en
todos los pasos) y sus deltas no significan nada. En la única con frame
utilizable, apagar la pradera entera bajó la GPU 2,56 ms y el frame sólo 0,31 —
lo que sugiere un techo de CPU alrededor de 7,4 ms. **Pero eso se midió con
Blender abierto y en build dev**, donde nuestro propio código va sin optimizar,
así que no se puede atribuir. Zanjar si el frame es CPU-bound necesita una
corrida en release, con la máquina limpia, y todavía no se hizo.

### El parpadeo: tres diagnósticos, y el que valió fue una frase del usuario

**Resuelto el 2026-08-06** (*"ya no hay parpadeo de pasto"*), y vale escribir
cómo, porque los dos primeros diagnósticos eran razonables y estaban mal.

- **Primero: la histéresis de chunks.** Un chunk en el borde exacto de un anillo
  nacía y moría en frames alternos. Era un bug real y `KEEP_SLACK_M` lo arregló
  — pero no era éste, y el parpadeo siguió.
- **Después: aliasing temporal, o sea MSAA.** Una brizna de 5,5 cm a veinte
  metros es sub-píxel, y el perfil de escritorio corre con `msaa=off`. La
  hipótesis se sostuvo dos días y motivó una perilla nueva. **Era falsa**, y
  costó medirla: MSAA sale entre 1,81 y 3,17 ms de GPU.
- **Lo que era: z-fighting con el suelo.** El shader colapsaba la brizna hacia
  `ground_y`, así que al encogerse **no desaparecía**: sus cuatro vértices
  llegaban a la altura del terreno mientras la punta conservaba su
  desplazamiento horizontal (el lean horneado más el viento). Quedaba un
  cuadrilátero plano, coplanar con el suelo, agitado por el viento. El arreglo
  es una línea: colapsar hacia `ground_y - GROWTH_SINK_M`, 18 cm bajo tierra.

**Lo que hizo la diferencia fue la descripción, no la teoría.** Dos días de
"parpadea" no alcanzaron; *"unos pastos que parecen pegados en el piso que
parpadean"* nombró la causa entera. Cuando un artefacto visual resiste dos
diagnósticos, la pregunta correcta al que lo está viendo no es "¿sigue?" sino
"¿a qué se parece?".

Y de yapa hace lo que la tabla de BOTW de más arriba pedía desde el principio:
la brizna **brota del suelo** en vez de aparecer aplastada sobre él. El primer
quinto de la rampa de crecimiento ocurre bajo tierra.

### Del mismo día, en el Mundo (`BOF_BENCH=general`)

Desde el mirador canónico: la pradera cuesta **1,14 ms de GPU de 4,19** (27%),
render 50% ahorra 2,55, el bosque oculto 0,34 y todas las sombras 0,48. Fill
domina también acá. **Ojo con ese mirador**: que ocultar el bosque entero valga
0,34 ms sugiere que desde ahí casi no se ve bosque, o sea que el punto de vista
"del bosque" no mira al bosque. Autorearlo de verdad sigue pendiente.

---

## Cuatro intentos que fallaron, y qué enseñaron (2026-08-06)

Un día entero apuntando al mismo síntoma —**dónde y cuándo se ve la transición
del pasto**— con cuatro arreglos distintos. Los cuatro fallaron. Están acá para
que nadie los reintente creyendo que son nuevos, y porque el patrón que forman
vale más que cualquiera de ellos.

| intento | hipótesis | resultado | costo |
|---|---|---|---|
| Banda de crecimiento 8 → 3 m | "se ve porque pasa cerca" | **peor**: más lejos pero más brusco | 0 |
| Separar rampa de dispersión | "es un fenómeno, no dos" | **acertó a medias**: el mecanismo quedó bien, se sigue notando | 0 |
| Anillo interior 10 → 16 m | "se ve porque pasa cerca" (otra vez) | **no cambió nada perceptible** | +73% tris (347.600 → 600.000) |
| Textura de pradera en el suelo | "se ve porque destapa tierra" | *"no maquilla ningún problema"* | 0, y el arte quedó |

**Lo que enseñan juntos.** Los cuatro tratan la transición como el problema. No
lo es: es el síntoma de que **el pasto no se sostiene solo**. Un campo que se
sostiene puede terminar donde sea, porque lo que hay antes de terminar ya se lee
como campo. El nuestro no, así que su frontera se nota la pongas donde la
pongas — a 10, a 16 o a 32 metros.

**El veredicto del usuario, que es la línea que gobierna lo que sigue:**

> *"Cuando el pasto se vea bien por sí solo, todo lo demás va a caber bien, lo
> cual no es cierto al revés: poner arbustos, flores y árboles no va a arreglar
> el pasto."*

**Y el contraejemplo que cierra la discusión de presupuesto:** *Flower*
(thatgamecompany, PS3, **2009**) llena la pantalla de pasto con una fracción de
este hardware. Cuando la respuesta a un problema de imagen sea "hace falta más
presupuesto", este renglón dice que no. Es técnica, no plata.

### Dos propuestas mías que el usuario rechazó, y por qué tenía razón

Las anoto porque las dos suenan sensatas y van a volver a tentar:

- **"Velar el borde con niebla / acercar el fog."** Es maquillaje explícito:
  esconde el síntoma sin tocar la causa, y encima cambia la atmósfera de todo el
  juego para arreglar un sistema.
- **"Capas de vegetación: matas a media distancia, arbustos más allá."** Suena a
  ingeniería y hasta tiene aritmética a favor (una mata de 22 tris cubre lo que
  decenas de briznas). Pero es lo mismo: tapar con otros objetos que el pasto no
  llena. Y falla la prueba de Flower, que no tiene ninguna de esas capas.

---

## Cómo lo hacen otros — con fuentes, no de memoria (2026-08-06)

Estudio pedido por el usuario tras el día de los cuatro intentos fallidos. Todo
lo de acá sale de material público citado al final; **lo que es inferencia mía
va marcado**. La regla que lo motivó: *"tu ayuda es realmente valiosa cuando no
inventas técnicas"*.

### Ghost of Tsushima (GDC 2022, Advanced Graphics Summit)

Briznas generadas en GPU por compute, con tiles jerárquicos que se subdividen y
submuestrean del padre altura del terreno, tipo de pasto, factor de clumping,
tamaño y viento.

De ahí, **tres cosas que contradicen decisiones nuestras**:

1. **El terreno lejano se tiñe hacia el color de la PUNTA de la brizna**, para
   *"dar la ilusión de que la densidad a distancia es la misma, aunque esté
   fuertemente culleada"*. **Nosotros teñimos hacia la RAÍZ** (`grass_tint()`
   usa `ROOT_COLOR`), y el comentario de esa función dice explícitamente que lo
   que mantiene legible el pasto es *"que el suelo siga siendo más oscuro que
   él"*. Eso es cierto **debajo** del campo y falso **más allá** de él: un campo
   visto en ángulo rasante se lee del color de sus puntas, así que donde el
   pasto termina, nuestro suelo se oscurece — y un cambio de valor en el
   horizonte es exactamente lo que se ve como borde.
2. **Clumping por celdas de Voronoi**, con un `clump ID` por celda y parámetros
   autorados por tipo: *pull to centre*, *point in same direction*, y variación
   de altura, ancho, tilt y bend por clump. Nuestro scatter es hash uniforme:
   cada brizna es independiente de sus vecinas, que es la definición de alfombra.
3. **La normal de la brizna se interpola hacia la normal del terreno con la
   distancia**, para reducir el granulado, y el AO se desvanece. Nosotros usamos
   +Y fijo a toda distancia.

Su brizna, para calibrar: **15 vértices sobre una curva de Bézier cúbica
evaluada en el vertex shader**, con control de height/width/tilt/bend. Es una
brizna de ~13 triángulos — muy lejos de nuestra ley 1 —, pero es PS5 y no es el
target de este documento.

### La serie de Godot de hexaquo (LOD para llanuras infinitas)

Es el material más cercano a nuestro problema exacto, porque su pregunta es
literalmente "pasto hasta el horizonte":

4. **El LOD es la brizna, no el campo.** Colapsan la brizna de **9 triángulos a
   1** con la distancia — ~90% menos geometría **sin tocar la densidad**.
   Nosotros hacemos lo contrario: mantenemos la brizna y bajamos la densidad,
   que es precisamente lo que abre el vacío.
5. **El impostor es el suelo, no un billboard.** Más allá del alcance renderizan
   *"el suelo —un plano simple, sin geometría extra— y lo hacen actuar como si
   tuviera briznas encima"*. No es "terreno con textura de pasto": es un plano
   que simula el sombreado de un campo.
6. **Compensación por ángulo de visión:** calculan el ángulo entre la dirección
   de cámara y la normal para ajustar albedo y normales, *"evitando que el
   impostor se vea plano a la altura de los ojos"*. **Nuestro terreno no hace
   nada de esto**, y por eso a media distancia se lee como plástico verde.
7. **Normal map horneado del propio pasto**: capturan el buffer de normales de
   la geometría real vista desde arriba y lo usan en el plano. El detalle de
   sombreado sobrevive aunque la geometría no.
8. Transición con solapamiento largo y explícito (empieza a 5, impostor completo
   a 10, geometría desaparecida a 20).

### El experimento propio: cuánto área proyecta cada forma de brizna

Medido en Blender el 2026-08-06, barriendo el ángulo de cámara de 0° a 90° y
contando píxeles de silueta de cada forma por separado *(tipo a: medición
nuestra, en área proyectada — no en milisegundos)*:

| ángulo | plana (2 tris) | teja (6 tris) | cruz (4 tris) |
|---|---:|---:|---:|
| 0° | 4384 | 4691 | 4384 |
| 45° | 3211 | 3452 | 3211 |
| 90° | **0** | 1182 | 4384 |
| **media** | 2797 | 3180 | 4023 |
| **mínimo** | **0** | 1182 | 3211 |
| **por triángulo** | **1399** | 530 | 1006 |

**Esto refutó la propuesta con la que empecé.** Yo había argumentado que una
brizna curva aporta más masa visual por triángulo; **es la peor de las tres**,
530 px/tri contra 1399 de la plana. La aritmética del ángulo muerto sí se
confirmó —la plana promedia 0,64 de su máximo, que es exactamente 2/π, y a 90°
proyecta **cero**— pero la conclusión que saqué de ella era falsa.

Y hay un corolario que refuerza la ley 1 en vez de romperla: con los mismos 4
triángulos, **dos briznas planas dan 5594 px de media contra los 4023 de una
cruz**. La respuesta a "falta masa" es más briznas planas, no briznas más caras.

**La advertencia que va con esta tabla:** el área proyectada es la métrica
correcta si el sistema es *vertex-bound*, y el 2026-08-06 medimos que es
**fill-bound**. Cuando lo que cuesta son los píxeles pintados, más área
proyectada por brizna es *más caro*, no más eficiente, y la métrica que manda
pasa a ser cobertura útil por píxel pintado — o sea **overdraw**. El dial de
overdraw del hub existe desde siempre y sigue sin usarse.

### Lo que el estudio mandó probar, y qué pasó con cada uno

Se probaron los cuatro el 2026-08-06, en orden. **Ninguno de los dos primeros
sobrevivió**, y las dos razones valen más que las técnicas:

**1. Teñir el suelo hacia la punta y no hacia la raíz** (GoT, punto 1).
Implementado y **rechazado por medición propia**, antes de mostrárselo al
usuario. El horizonte quedó en luminancia 196 contra 172 del pasto del primer
plano — *el fondo más claro que la primera fila* — y la variación de media
distancia bajó de 13,3 a 10,4. Aplanaba la distancia en vez de poblarla.

*Por qué falla acá y funciona en su juego:* la técnica presupone que el color de
la punta **lee como masa de pasto**. La nuestra era `#ADDA81`, un verde casi
blanco; aplicada fuerte borra el grano de la textura y deja un plano liso.

**2. Clumping** (GoT, punto 2). Implementado y **rechazado por el usuario**, con
un argumento que generaliza y que conviene no perder:

> Tsushima es un campo de pasto alto barrido por el viento, con matas y claros a
> propósito. El clumping *compra* estructura pagando con uniformidad. **Nuestro
> objetivo es una alfombra que cubra toda la escena**, así que paga exactamente
> lo que queremos conservar.

La medición coincidió sin explicarlo: +1,0% de variación, o sea nada. Lo que el
efecto realmente producía eran huecos, y el hueco es el defecto.

**3. Que el terreno responda al ángulo de visión** (hexaquo, 6 y 7), con el
normal map que sigue en el repo sin usar. **Sin tocar.**

**4. LOD de la brizna en vez del campo** (hexaquo, 4). **Hecho.** `split_tips:
bool` pasó a `BladeShape` con tres niveles — 3, 2 y 1 triángulo. El anillo
exterior bajó a un triángulo: dos vértices en la base y uno en la punta. Jugado:
*"se siente igual que antes"*, o sea geometría gratis. El anillo medio se queda
en 3 porque sus chunks se solapan hasta ~2 m de la cámara.

Lo empujó el usuario preguntando directo si usábamos LOD de brizna — y este
documento **ya tenía la respuesta escrita** en la sección de hexaquo sin haber
actuado sobre ella.

### Lo que sí movió la aguja, y no estaba en la lista

**La paleta de la brizna.** El campo promediaba luminancia 171,9, que es
exactamente el punto medio de una rampa lineal entre nuestra raíz y nuestra
punta: veíamos el degradado entero, parejo. Dos cambios, los dos derivados:

- **El degradado se sesga hacia la raíz.** Un dosel visto desde parado es casi
  todo puntas. Sustituye a la oclusión ambiental que no calculamos.
- **La paleta sale del suelo donde se paran las briznas** (`T_GroundSoil`, tono
  84°, sat 37%). La raíz estaba a 100° y **22%** — 16° de tono y la mitad de
  saturación contra el suelo del que brota.

| zona de pasto | lum | sd | sat |
|---|---:|---:|---:|
| antes | 171,9 | 16,8 | 32,1% |
| después | 153,7 | 19,3 | 34,7% |

**Y dos errores de método que costaron una ronda de confianza, anotados porque
son más reutilizables que el resultado:**

1. La paleta se leyó de `T_GroundGrass_Albedo.png`, que **no es una de las cuatro
   texturas canónicas** y nunca llega a la escena. Que cayera cerca fue suerte.
2. El objetivo se eligió *a ojo* —"esa malla lee mejor como pasto"— y después se
   midió la distancia hasta él con tres decimales. **La precisión era real y el
   blanco supuesto.** El usuario: *"si no me dices el porqué algo es mejor de lo
   que ya teníamos, no sé si creerte"*. La versión defendible es un criterio
   comprobable —el tono del suelo, que se verifica caminando a un claro— y no un
   promedio de píxeles hacia una preferencia mía.

Aparte y sin arreglar: `T_GroundTallGrass_Albedo.png` está en **tono 113°**, a
29° del suelo que tiene al lado. Dos texturas de suelo tan separadas en la misma
escena es un defecto de arte que la paleta de las briznas no puede tapar.

**Sources:**
- [Procedural Grass in 'Ghost of Tsushima' — GDC Vault](https://gdcvault.com/play/1027033/Advanced-Graphics-Summit-Procedural-Grass)
- [Unity-Grass, implementación documentada de la charla de GoT](https://cainrademan.github.io/Unity-Grass/)
- [hexaquo — Grass Rendering Series Part 4: LOD Tricks for Infinite Plains of Grass in Godot](https://hexaquo.at/pages/grass-rendering-series-part-4-level-of-detail-tricks-for-infinite-plains-of-grass-in-godot/)

---

## Fase 0 — Poder medir, y pagar menos por lo mismo

Nada de esta fase cambia la imagen.

### Paso 0: Los dos diales, en el hub

- **Lógica.** Todo lo que sigue se justifica con mediciones que hoy no se pueden
  tomar. `GrassDensity` y `RenderScale` entran a `PerfKnob` y a la matriz de
  `perf/sequence.rs`; el `KeyCode::F8` de `grass.rs` se borra. El detalle y el
  porqué están en *Presupuesto → Los dos diales tienen que nacer en el hub*.
- **Estado (2026-08-05): implementado.** Los dos diales son `PerfKnob`
  (`GRASS_DENSITY_STEPS`, `RENDER_SCALE_STEPS` en `domain::perf`), F8 y
  `GrassStressState` se borraron, y `STEPS` pasó de 7 a 11 pasos: dos de
  densidad (25 y 10/m²) y dos de resolución (75% y 50%), un cambio por paso.
  Efecto colateral: `grass.rs` salió de `HARDWARE_DEBT` (C2 bajó a 12 archivos).
  `RenderScale` se aplica como **viewport encogido** en la cámara, no como render
  target escalado: no necesita imagen intermedia ni pase de upscale, y al encoger
  los dos ejes por igual conserva el aspecto y por lo tanto el encuadre — lo
  único que cambia es el número de fragmentos. La imagen queda en una esquina de
  la ventana; eso es fealdad de diagnóstico, no un bug.
- **Entregable & validación.** **Pendiente:** una corrida de la secuencia que
  llene la matriz fill-bound / vertex-bound con números, y cuyo paso de baseline
  repetido muestre la deriva. **Ese resultado puede reordenar todo lo que
  sigue** — si el pasto resulta fill-bound, la palanca es la densidad del anillo 0
  y no el shader.

### Paso 1: `ExtendedMaterial` enchufado y shader corregido

- **Lógica.** El viento, el crecimiento, la variación de color y la transmisión
  necesitan un shader propio. Bevy lo permite sin abandonar el PBR:
  `ExtendedMaterial<StandardMaterial, GrassExtension>` conserva luz, sombras y
  niebla, y agrega los uniforms propios.
- **Estado (2026-08-05): implementado.** El material está enchufado y los tres
  bugs corregidos. Un cuarto apareció al usarlo, y no estaba en esta lista: con
  `double_sided`, Bevy **invierte la normal en las caras traseras**
  (`pbr_functions.wgsl:144`), y como la nuestra apunta a +Y, invertida apunta al
  suelo — brizna negra. Con yaw al azar, media pradera. El fragment repone las
  dos normales que el PBR usa (`world_normal` y `N`). *Los tres originales,
  para el registro:*
  1. `out.position = world_position` — tiene que ser clip space.
  2. El fragment **escribe el color directo y nunca llama al PBR**, así que
     saldría plano, sin luz ni sombras ni niebla: justo lo que
     `ExtendedMaterial` existe para conservar.
  3. Ignora `vertex.color`, con lo que el degradado horneado se pierde y lo
     reemplaza el de los uniforms.
  *(Corrección: la versión anterior de este documento decía que el gradiente
  toma `uv.y` "que en nuestras briznas corre al revés". Es falso —
  `grass.rs:258-261` pone `uv.y = 0` en la base y `1` en la punta, que es
  exactamente lo que el shader espera. Ese punto no hay que arreglarlo.)*
- **Entregable & validación.** El campo se ve **idéntico** a hoy con el material
  nuevo puesto. Cambiar el motor de render sin cambiar la imagen es la única
  forma de saber que el cambio fue neutral.

### Paso 2: Adelgazar el vértice

- **Lógica.** Hoy cada vértice lleva posición (12 B) + normal (12 B) + uv (8 B) +
  color (16 B) = **48 bytes**. Tres de esos cuatro atributos no son dato:
  - **La normal es `[0,1,0]` en los cuatro vértices de toda brizna** — constante
    del sistema. Se reconstruye en el shader. **−12 B.**
  - **El color es una función pura de `uv.y`**: `mix(ROOT_COLOR, TIP_COLOR,
    uv.y)`, y los dos colores ya viven como uniforms en `GrassUniform`. **−16 B.**
  - **La uv sale de `vertex_index`.** Una brizna son exactamente 4 vértices en
    orden fijo (base-izq, base-der, punta-der, punta-izq), así que
    `vertex_index & 3` dice qué esquina es cada uno y de ahí salen las dos
    coordenadas. **−8 B**, si el pipeline PBR no exige el atributo igual — hay
    que verificarlo al implementar, no darlo por hecho.

  De 48 a **12** bytes por vértice: sólo queda la posición, que es el único dato
  irreducible. *(La versión de esta mañana decía 20 B y −58%; el −8 de la uv es
  posterior.)*
- **Cuánto vale, honestamente.** Hoy son 5,4 MB de atributos por frame; a 60 fps
  y contando el tráfico doble del tiler, del orden de 650 MB/s contra los ~15-25
  GB/s del aparato: **~4%** *(tipo c)*. Real, pero no es el muro — y este
  documento antes lo llamaba "la mejor relación beneficio/riesgo del sistema",
  que era exagerar. Se hace porque es **casi gratis, no cambia un píxel y es
  prerrequisito de toda la Fase 2**, no porque rescate el frame. Lo que rescata
  el frame es el Paso 4.
- **Estado (2026-08-05): implementado, con un límite verificado.** La normal y
  el color se fueron; la `uv` **no puede irse**: el campo `uv` de `VertexOutput`
  está gateado por el shader def `VERTEX_UVS_A`, que Bevy define desde el
  atributo de la malla, así que sin atributo no hay varying donde pasar la
  altura al fragment. Derivarla de `vertex_index` exigiría un `VertexOutput`
  propio y con eso se pierde `pbr_input_from_standard_material`. El documento
  avisaba que había que verificarlo en vez de darlo por hecho: verificado, no se
  puede. De 48 B se bajó a 20 B, y después subió a **28 B** al agregar el canal
  de dato por brizna que la Fase 2 necesita (hash con el lado en el signo, y
  alcance del anillo empaquetado con la altura).
- **Entregable & validación.** Imagen idéntica; `count_vertices()` igual y buffer
  más chico. Un test que afirme que la malla ya no declara normal ni color — y,
  si la uv también se va, **un test que congele el orden de los 4 vértices por
  brizna**, porque a partir de acá el shader depende de ese orden.

---

## La escalera de LOD, y por qué no hay billboards

Referencia que consumen el Paso 4, el Paso 8 y la sección de Blender.

### Cuánta densidad hace falta de verdad, a cada distancia

La densidad no es una preferencia: se deriva. Una brizna vertical de altura `H`,
vista desde una cámara a altura `h` y distancia `d`, tapa el suelo que tiene
detrás a lo largo de `H·d/h` — porque el ángulo de visión se aplana con la
distancia. Multiplicado por su ancho, eso es el área de suelo que una sola
brizna oculta, y su inverso es la densidad mínima para que no se vea tierra.

Con nuestros valores (`H = 0,4 m`, `h = 1,6 m` de cámara, ancho `0,055 m`)
*(tipo c: estimación, el cálculo está acá)*:

| distancia | suelo tapado por brizna | densidad mínima |
|---:|---:|---:|
| 2 m | 0,028 m² | 36/m² |
| 6 m | 0,083 m² | 12/m² |
| 18 m | 0,248 m² | 4/m² |
| 35 m | 0,481 m² | 2/m² |

**La densidad necesaria cae como 1/d.** Es el resultado más útil de este
documento y contradice la intuición: a 20 m, cuarenta y cinco briznas por metro
cuadrado no son densidad, son **treinta veces la cobertura necesaria** — y esa
redundancia se paga entera en overdraw, que es justo lo que el target no tiene.

**Y el Paso 3 baja este piso otra vez.** Con el terreno teñido, las briznas
dejan de ser responsables de tapar el suelo y pasan a ser responsables de darle
textura y movimiento. Por eso el Paso 3 va **antes** del Paso 4: no es un arreglo
cosmético del borde, es lo que hace pagables las densidades de los anillos.

### El crecimiento: por qué se veía, medido y arreglado (2026-08-06)

Cuatro intentos fallaron porque nadie había mirado la forma de la transición.
Medida por primera vez desde una vista cenital a 40 m, en anillos de 2 m:

| distancia | densidad aparente |
|---|---|
| 0–10 m | plana en ~26 |
| **10–16 m** | **25,7 → 19,6 (−28%)** |
| 16–22 m | plana en ~18-19 |
| 22–32 m | 12 → 9 |

**El 28% está concentrado en 6 metros**, y esa banda viaja con la cámara: el
suelo que estaba ralo a 16 m se ve engordar al llegar a 10. Con la cámara 4 m
detrás del player, son 5-6 m delante suyo — "muy cerca", como se reportó.

Y explica los cuatro fracasos: los dos parámetros que se tocaban —alcance y
dispersión— **mueven la banda pero no la borran**. Alargar el alcance la empuja
(+73% de triángulos, y siguió ahí); acortar la dispersión la concentra.

**La salida estaba escrita en este documento desde el principio y nunca se
aplicó:** la densidad necesaria cae como `1/d`. Se plantaba plano y se recortaba
al borde — una escalera donde correspondía una rampa. Con los umbrales repartidos
como `start / (1 - hash)`, la fracción viva a distancia `d` es `start / d`.

Dos correcciones que hicieron falta y valen por sí solas:

1. **La ley sola mueve el escalón al borde del anillo.** Deja ~25% vivo al llegar
   al alcance y ahí se corta de golpe: medido, −26% en el traspaso de los 16 m.
   Se compone con la banda de borde que ya existía, que ahora sólo tiene que
   apagar ese cuarto.
2. **Los dos umbrales necesitan hashes distintos.** Con el mismo, las briznas que
   la ley perdona son exactamente las que el borde mata primero, y el reparto se
   vuelve un escalón otra vez.

Barrido del punto donde empieza a ralear, midiendo la **desviación de la
pendiente** entre anillos contiguos — menos es más rampa y menos escalera:

| | pendiente media | desviación |
|---|---:|---:|
| escalera (antes) | −7,2%/2m | 7,9 |
| 4 m | −10,0% | **5,1** |
| **8 m** (elegido) | −9,3% | **6,9** |
| 12 m | −8,7% | 8,2 |

Cuatro da la rampa más pareja y deja el campo en 16,5 contra 27 a los 8-10 m, que
es el look registrado como jugado y rechazado. Doce es *peor* que no hacer nada.

**No ahorra un triángulo.** La geometría sigue horneada; esto sólo la encoge en
el vertex shader. Arregla la imagen, no el costo.

### La causa raíz del artefacto más viejo, y el plan (2026-08-06)

**Ocho intentos fallaron contra el mismo artefacto** — "veo crecer el pasto al
caminar" — y el noveno reporte agregó cuadrados visibles en ciertos ángulos.
Ocho intentos que le cambian la forma y no lo borran no son ocho errores de
afinación: es la arquitectura.

| intento | resultado |
|---|---|
| acortar la banda de transición | peor |
| anillo interior 10 → 16 m | +73% de triángulos, sigue |
| textura de suelo | *"no maquilla el problema"* |
| capas de vegetación | rechazado antes de implementar |
| separar `GROWTH_RAMP_M` de `GROWTH_SPREAD_M` | sigue |
| ley `1/d` continua | sigue |
| anclar la ley al borde interno de cada anillo | sigue |
| rampa larga + raleo empujado a 24 m | sigue, y aparecen los cuadrados |

**La causa, verificada en el código.** La semilla de un chunk es
`hash(celda.x, celda.y, anillo)` y su centro es `(celda + 0,5) × chunk_m`. Las
posiciones **sí están ancladas al mundo** —eso desmiente que la pradera "siga a
la cámara", que era el modelo que teníamos— pero **el anillo entra en la
semilla**, y los anillos ni siquiera comparten el tamaño de celda. Entonces:

> No hay una pradera. Hay cuatro praderas independientes apiladas sobre el mismo
> suelo, y se cruza de una a otra según la distancia.

Un pedazo de suelo a 20 m tiene un juego de briznas; el mismo a 10 m tiene
**otro juego distinto**, no las mismas más juntas. Acercarse no es acercarse al
pasto: es cambiarlo por otro pasto. Cualquier cruce entre dos campos distintos es
visible — o se suman (doble densidad), o uno se apaga (el galón de briznas a
media altura), o salta (pop). Los ocho intentos discutían *cómo* cruzar; ninguno
tocaba que hubiera algo que cruzar.

**El plan: praderas anidadas en vez de independientes.**

- La posición de una brizna sale de una **grilla fija del mundo** (baldosas de
  ~1 m), no del chunk. Cada baldosa tiene una secuencia determinista de briznas,
  siempre la misma, independiente del anillo.
- Cada anillo emite **las primeras N** de esa secuencia, con N según su densidad.

El anillo denso emite entonces un **superconjunto** del ralo. Cruzar un borde
deja de reemplazar nada: sólo agrega briznas donde antes había suelo, y las que
ya estaban no se mueven.

- **Arregla** el galón y el barajado: no hay dos campos que promediar, así que
  sobran el solapamiento y la banda de encogimiento.
- **No arregla** que aparezcan briznas nuevas al acercarse — eso es irreducible.
  Pero pasa a ser sólo eso, y para eso la rampa de crecimiento sí sirve.
- Cuesta más CPU al hornear y **cero por frame**. No toca el shader ni el
  presupuesto ni el look ya aceptado.

**Después de eso, por orden, lo que sigue pendiente:**

1. **Bajar el presupuesto desde 2.000.000** con el look ya aceptado, midiendo.
   Es la segunda mitad de la estrategia "empezar arriba y trabajar hacia abajo".
2. **La brizna sigue siendo un plano vertical.** Es la causa común del ángulo
   muerto, del cenital ralo y del Paso 8. Tres ataques posibles y sólo uno
   gratis: subir `BLADE_LEAN`, la brizna curva, o girarla hacia la cámara.
3. **Que el terreno responda al ángulo de visión** (hexaquo 6 y 7), con
   `T_GroundGrass_Normal.png` que sigue en el repo sin usar.
4. **El horizonte**, que nunca se atacó de frente.
5. **Contar píxeles de silueta**, no gradiente, para zanjar si el raleo cenital
   es proyección. El usuario dudó con razón de esa conclusión.

**Y una advertencia sobre el medidor:** el perfil radial por detección de bordes
que se usó todo el día **satura con densidad alta** y no distingue altura de
cantidad. El galón no lo veía justamente por eso — era un anillo de briznas a
media altura, no de menos briznas. Cualquier medición futura de este tipo tiene
que separar las dos cosas.

### El anillo deja de ser un escalón (2026-08-06)

Diagnóstico del usuario, después de mirar mucho: *"el problema siguen siendo los
anillos […] el cambio entre LOD es visible al caminar, y ese ha sido el problema
durante toda la sesión"*. Es correcto, y explica por qué afinar números nunca
alcanzó: **un anillo tenía densidad constante**, así que siempre había un
escalón.

La ley `1/d` estaba en el shader pero **anclada en un punto global**, y eso hace
que cada anillo entregue menos de lo que le toca en su mitad interna. La
corrección tiene dos mitades:

1. Cada anillo se planta a la densidad que la derivación pide en su **borde
   interno** (`C / r_interno`), no en su medio.
2. El shader ancla la ley **en ese mismo borde** — `ring_inner` en `grass.wgsl`
   lo deduce comparando el alcance de la brizna contra los de todos los anillos,
   que viajan en el uniform.

Así la densidad viva es `C/d` exacta en todo el campo, y el anillo pasa a decidir
**sólo el tamaño de chunk**. Sobreplanta a lo sumo 1,6× en su borde externo, que
es justo lo que la ley se come.

Medido desde el cenit, sobre la desviación de la pendiente entre anillos de 2 m
—menos es más rampa y menos escalera:

| | pendiente media | desviación |
|---|---:|---:|
| anillos planos, alcance 32 m | −7,2%/2m | 7,9 |
| cuatro anillos planos, 64 m | −6,6% | 7,8 |
| **anclado al borde interno** | −5,2% | **5,6** |

Y el campo queda **más denso** a media distancia, no menos: 21 contra 19,6 y 12 a
los 14-20 m.

**Lección que generaliza:** el LOD por bandas de densidad constante siempre deja
un escalón; lo que lo borra es que la densidad sea una función continua de la
distancia y que la banda sólo decida el *tamaño del lote*.

### Ver el pasto: las vistas de color, y lo primero que mostraron (2026-08-07)

Pedido por el usuario junto con el arreglo de las herramientas, y con el
argumento correcto: *"quiero ver claramente lo que está pasando en el código con
distintos colores"*. Todo lo que decide la forma del pasto —a qué anillo
pertenece una brizna, de qué chunk salió, cuánto de su altura tiene— es
invisible en la imagen final, porque todo se ve verde. Ocho intentos contra el
mismo artefacto se discutieron sin poder mirar ninguna de esas cosas.

La perilla `grass-view` (hub F1, o `BOF_KNOBS=grass-view=N`) tiene seis pasos en
dos familias:

| vista | qué pinta | para qué |
|---|---|---|
| `anillo` | un pastel por anillo | dónde cambia el LOD y cuánto se solapan |
| `chunk` | un pastel por celda | el mapa de **draw calls**: un chunk es una malla y un draw |
| `brizna` | un pastel por hash | si al acercarse las briznas **se suman** o **se reemplazan** |
| `crecimiento` | rampa de dos colores | dónde está la banda que crece |
| `subpixel` | rojo bajo 1 px de ancho, verde sobre 2 | **la ley 2, por fin mirable** |
| `medir` | plano, exacto, sin luz ni niebla ni tonemapping | contar píxeles |

La de `subpixel` sale del estado del arte y de la ley 2 de este documento: el
rasterizador trabaja en cuartetos de 2×2, así que una brizna más angosta que un
píxel dispara cuatro fragmentos por el uno que aporta. Los motores grandes lo
exponen como *quad overdraw* —Unreal lo tiene como view mode y Unigine lo
documenta con la misma justificación— y es el único costo del pasto que **no
aparece en ningún conteo de triángulos**. Acá sale de las derivadas de pantalla
(`fwidth` de la posición de mundo = metros por píxel) contra `BLADE_WIDTH`, que
viaja en el uniform desde su única fuente.

**Primera lectura, 2026-08-07, desde el mirador canónico:** el rojo está
confinado a una franja fina en el horizonte y el resto del campo es verde. O sea
que el desperdicio de cuarteto **no** es el costo dominante en esta vista — lo
que domina es el solapamiento de anillos en el primer plano, que es geometría de
sobra a tamaño resoluble. Es un descarte útil: cierra una hipótesis que el
documento tenía abierta desde que se escribió.

**Ninguna cuesta un byte por vértice ni rehornea la pradera.** El anillo sale de
`floor(uv1.y)`, la brizna de `uv1.x` y el chunk de `floor(xz / chunk_m)` con los
tamaños viajando en el uniform. Lo que cambia es lo que el shader **pinta**, no
lo que dibuja, así que se encienden jugando y sin perturbar la geometría que se
está juzgando.

Las cuatro primeras tiñen el color real y **dejan la luz puesta** — el campo
sigue leyéndose como campo, que es la condición para juzgar si algo *se ve* mal.
La quinta es otra cosa: la cámara apaga tonemapping y dithering, el juego escribe
la paleta en un `.json` al lado del PNG, y `tools/shot_stats.py` cuenta píxeles
de colores que no conoce de antemano. **Eso reemplaza los perfiles por detección
de bordes** que decidieron todo el 2026-08-06 y que este documento ya marcaba
como defectuosos: saturan con densidad alta y no distinguen una brizna baja de
una brizna ausente.

#### Lo primero que se vio, y es un defecto de arquitectura

**Cuatro anillos plantan sobre el mismo suelo.** No es que se apilen por
distancia: coexisten en todo el rango, incluida la posición de la cámara.

Medido desde el mirador canónico, `grass-view=medir` con bandas horizontales
(fila de pantalla ≈ distancia al suelo):

| banda | anillo 0 | anillo 1 | anillo 2 | anillo 3 |
|---|---:|---:|---:|---:|
| lejos | 0% | 14,2% | 39,2% | 18,4% |
| media | 18,0% | 50,5% | 31,3% | 0% |
| **primer plano** | **32,9%** | **35,8%** | **28,4%** | 0% |

**Dos tercios del primer plano son briznas de anillos lejanos** — y las del
anillo 2 son de **un** triángulo, sin punta partida, pensadas para 24-40 m.

*La causa, en el código:* un chunk se descarta sólo si cae **entero** dentro del
traspaso del anillo interno (`farthest <= handover` en `ring_cells_with_slack`),
y un chunk de 32 m que contiene a la cámara nunca cae entero dentro de 18 m. Se
conserva, y planta sus 40 briznas/m² sobre los pies del jugador. El anillo no
decide "sólo el tamaño de chunk" como dice la sección anterior: decide también
cuánto se superpone con todos los internos.

*Por qué no se arregló de una:* **ese solapamiento es lo que hoy tapa la costura
entre anillos.** Sacarlo sin la reescritura de *praderas anidadas* vuelve a
destapar el artefacto que ocho intentos persiguieron. El sobrecosto está
comprando algo. Lo que cambia es que ahora se sabe cuánto: queda como deuda con
número en `no_patch_of_ground_is_planted_by_more_than_two_rings`, y la
reescritura tiene una medición a favor en vez de una intuición.

#### Y probarlo cambió el diagnóstico (2026-08-07)

Se implementó el recorte interno —cada anillo muerto también por adentro— y se
midió. Tres corridas con distintas densidades de compensación:

| configuración | triángulos | banda cercana | banda 13-24 m |
|---|---:|---:|---:|
| hoy (con solapamiento) | 665.600 | 96,9% | 99,9% |
| recorte, densidades sin tocar | 847.000 | 62,7% | — |
| recorte + anillo 0 a 140/m² | 793.600 | 95,8% | **75,5%** |
| recorte + externos a lo derivado | 365.568 | 95,8% | **64,2%** |

**Dos cosas que ninguna intuición había dado:**

1. **El recorte deja un pozo en cada frontera.** El anillo de afuera nace donde
   el de adentro muere y son **dos juegos de briznas distintos**: no hay forma de
   que una releve a la otra, así que en la banda de traspaso están las dos a
   media población. Se ve en la columna de 13-24 m, que cae del 99,9% al 75,5%
   por más densidad que se le ponga al anillo interior.
2. **El recorte no ahorra**, porque vive en el shader: esconde la brizna pero la
   geometría ya está horneada. Es la ley 4 en acción — *la brizna que no se ve
   igual se paga*. Ahorrar de verdad pide no plantarla, y eso lo decide el
   *baker*, que no puede porque el chunk está anclado al mundo y la cámara se
   mueve.

**Y la corrección de la derivación que salió de ahí.** `minimum_density` pedía
`λ·a = 1`, o sea la densidad con la que las briznas taparían el suelo **si se
ordenaran solas**. Caen sobre un hash: la cobertura de un reparto de Poisson es
`1 − e^(−λ·a)`, así que para el 95% hace falta **tres veces** esa densidad. Con
la fórmula corregida, el anillo interior queda *por debajo* de lo que su
distancia pide — y lo que lo salva es justamente la suma de los anillos que se
pisan.

> El solapamiento no es sólo costo: **está pagando la cobertura**. El defecto no
> es que los anillos se pisen, es que el primer plano se está pintando con
> briznas de anillos lejanos, que son las de un triángulo y sin cintura.

Eso reordena el plan: la reescritura anidada no es una optimización que se pueda
posponer, es lo único que permite tener la densidad **y** la brizna correcta en
el mismo lugar. Y el test de densidad ahora mide la **suma** sobre un punto del
suelo, que es lo que ese punto realmente recibe, en vez de un anillo aislado.

### La brizna de dos triángulos (2026-08-07)

Restaurada del diseño original, después de que el usuario preguntara cómo estaban
construidos los triángulos: **dos triángulos unidos por una arista horizontal**,
uno apuntando abajo y otro arriba.

```
        ∧              4 vértices, 2 triángulos
       ╱ ╲
      ╱   ╲            arriba: cintura-izq, cintura-der, punta
     ╱     ╲
    ●───────●          ← la arista compartida, a 0,30 de la altura
     ╲     ╱
      ╲   ╱            abajo: punta hundida, cintura-izq, cintura-der
       ╲ ╱
        ∨
```

Lo que había era un quad partido por la **diagonal**, y las dos diferencias
importan:

- **Termina en punta por los dos lados**, que es la forma de una hoja de pasto.
  El quad era ancho abajo y cortado arriba, y por eso hubo que inventarle una
  muesca de un triángulo extra para que no leyera como tira de papel.
- **Tiene una fila de vértices en el medio.** Sin ella los bordes van rectos de
  la raíz a la punta y **la brizna no puede arquearse**: el `height_factor²` del
  viento daba 0 abajo y 1 arriba igual que lineal, o sea que era un no-op. El
  comentario del shader decía *"se arquea en vez de inclinarse rígida, que es la
  diferencia entre una hoja y un palo"* sobre una geometría que sólo podía
  inclinarse rígida.

La punta de abajo se hunde 6 cm: en el suelo mismo la brizna sería infinitamente
angosta y dejaría ver tierra donde nace.

**Medido, mismo encuadre:** cobertura 56,60% contra 56,67% —idéntica— con
**665.600 triángulos contra 691.200**. Cubre lo mismo con menos geometría,
porque el anillo interior baja de 3 a 2 triángulos por brizna al no necesitar la
muesca.

*Y explica el fill-bound sin misterio:* el primer plano —donde cada brizna cubre
más píxeles— se está dibujando tres veces.

#### Y la perilla de alcance medía otra cosa

Encontrado el 2026-08-07 auditando qué números del uniform describen algo real.
El vértice lleva `ring_reach(index, escala)` —escalado por la perilla y
redondeado— y el uniform mandaba `RINGS[i].reach_m` **a secas**. A 100% coinciden
y no se nota nada; a 75% una brizna dice "10 m" contra una tabla que dice 13,
`ring_inner` no encuentra ningún alcance menor que el suyo y devuelve **0**, así
que el ancla de la ley `1/d` pasa a ser `growth_start` para todos los anillos.

**Los pasos `reach 75%` y `reach 50%` de la matriz no medían un alcance más
corto: medían otra ley de raleo.** La conclusión *"el alcance ahorra menos que la
densidad"*, de la corrida del 2026-08-06, sale de esas dos filas y hay que
rehacerla.

Arreglado, y con un test que compara la tabla del uniform contra los alcances que
las mallas realmente hornean, para cada paso de la perilla. Verificado plantando
el bug de vuelta: el test cae nombrando el anillo y los dos números.

### Los anillos

**La tabla derivada**, con un factor de seguridad ×2,5 sobre el mínimo:

| anillo | radio | chunk | densidad | briznas | tris |
|---|---:|---:|---:|---:|---:|
| 0 cerca | 0–6 m | 5 m | 40/m² | 4.524 | 9.048 |
| 1 medio | 6–18 m | 10 m | 10/m² | 9.048 | 18.096 |
| 2 lejos | 18–35 m | 20 m | 3/m² | 8.492 | 16.984 |
| 3 | > 35 m | — | — | terreno teñido | 0 |
| | | | **total** | **22.064** | **44.128** |

**Y lo que el código hace hoy (2026-08-06), que es cinco veces más:**

| | derivado | código |
|---|---|---|
| anillos | 0–6 / 6–18 / 18–35 m | 0–16 / 16–24 / 24–32 m |
| densidades | 40 / 10 / 3 | 40 / **20** / **7** |
| tris por brizna | 2 / 2 / 2 | 3 / 3 / **1** |
| triángulos 360° | **44.128** | **224.768** |

El anillo medio corre al doble de lo derivado y el exterior a más del doble. El
interior mantiene 40/m² hasta los 16 m cuando la tabla de arriba dice que a 5 m
alcanzan 15. Cada salto tiene su razón registrada en `visuals/grass.rs` y todos
los decidió el ojo del usuario jugando — pero **la suma nunca se volvió a
comparar contra la derivación**, y esa distancia es el número que hay que mirar
antes del próximo ajuste.

**El disparador que este documento dejó armado ya se cumplió.** Decía: *"si el
barrido de densidad confirma que el pasto es fill-bound, bajar el anillo 0 es la
palanca más barata del sistema entero — más barata que cualquier shader"*. El
barrido corre desde el 2026-08-05 y confirmó fill-bound. La acción sigue sin
tomarse, y ésa es la deuda concreta de esta sección.

*Los radios y densidades son estimaciones derivadas de la tabla de arriba; se
ajustan por ojo en la caja `Pasto`. Lo que no se ajusta es la forma de la curva.*

### Billboards: tres cosas distintas, las tres descartadas

"Billboard" nombra tres técnicas que no tienen nada que ver entre sí, y conviene
separarlas porque sólo una es tentadora de verdad:

**1. Brizna que gira hacia la cámara.** No ahorra nada: sigue siendo un quad de
2 triángulos. Agrega trabajo por frame, y briznas que pivotean al mover la cámara
se leen como un error. Es lo que suele significar "pasto con billboards".

> **Ese rechazo contesta otra pregunta, y el usuario lo cazó el 2026-08-06.**
> Evalúa la técnica como *ahorro*, que no lo es. La razón por la que otros juegos
> la usan es distinta: **que la brizna nunca quede de canto**. Este documento
> describe ese problema con todo detalle en el Paso 8 —yaw uniforme, promedio de
> `|sin|` = 2/π, "las que caen cerca de canto rinden casi cero pagando sus tres
> triángulos igual"— y el barrido en Blender lo midió: **la brizna plana a 90°
> proyecta 0 píxeles**. Pero la única solución que el documento ofrecía era la
> brizna curva, que cuesta más triángulos. Que se pueda atacar lo mismo sesgando
> el yaw —gratis— no estaba considerado en ninguna parte.
>
> **Y no es tan gratis como suena.** El yaw se hornea en las posiciones de los
> vértices cuando se construye el chunk, y la cámara gira después. Para que
> "nunca ortogonal" signifique algo *relativo a la cámara* hay que reconstruir el
> quad en el vertex shader — o sea que las dos técnicas son la misma, y su costo
> real es por frame, no por triángulo.
>
> **El caso extremo es el cenital**, reportado jugando el mismo día: con la
> cámara sobre el player mirando abajo, el campo se lee mucho más ralo. Medido
> con las constantes propias: de costado una brizna tapa 0,0236 m², desde arriba
> 0,0074 — **el 31%**. La cobertura del campo cae de 95% del suelo a 30%.
>
> Ahí hubo dos afirmaciones mías y **una era falsa**. Dije que no se dibujaban
> menos briznas: el contador dice que sí, 233.992 → 197.792 triángulos al
> inclinar la cámara de −0,64 a −0,84. Lo que sí resultó cierto es que esa baja
> es culling correcto — forzar los 41 chunks descartados (415.000 triángulos más)
> cambia la imagen **menos que el propio ruido del viento** (4,53% contra un piso
> de 5,03%, medido con dos corridas idénticas).

**2. Carta de grupo** — un quad texturizado con la silueta de 20-30 briznas.
Es la única que ahorra geometría de verdad, y es la que está en todos los
tutoriales. **Y para nuestro target el negocio va al revés.** La carta necesita
una textura con alfa recortado, o sea `discard`, o sea LRZ/FPK/HSR apagados
(ley 3). Las cuentas a 30 m *(tipo c)*: treinta briznas cubren del orden de 120
fragmentos **conservando el early-Z**; la carta que las reemplaza cubre unos 400
**sin él**. Cambia un recurso que tenemos —vértices, que el Paso 2 recorta un
75%— por el único que no tenemos. En una GPU de escritorio de modo inmediato la
carta gana; por eso la técnica es correcta y por eso está en todas partes. En un
tiler pierde.

**3. Shell texturing** — apilar N capas sobre el terreno, la técnica que volvió a
ponerse de moda. Da volumen con casi nada de geometría, pero es overdraw ×N con
`discard` en cada capa: la forma más cara posible en la arquitectura del target.

**La escalera que las reemplaza: brizna → menos briznas → terreno teñido.** Tres
peldaños, los tres opacos, ni un `discard` en ninguno. El último es gratis porque
el terreno ya se está dibujando de todos modos.

---

## Fase 1 — Alcance: que sea un mundo, no un césped

### Paso 3: El terreno es el LOD más lejano

- **Lógica.** Que el pasto termine no se arregla con más pasto: se arregla
  pintando el terreno del mismo verde que la raíz de la brizna. La transición
  deja de existir porque no hay dos cosas distintas. Requiere que el albedo del
  terreno y `ROOT_COLOR` compartan firma cromática.
- **Estado (2026-08-05): implementado como tinte, y falta la mitad.** El shader
  del terreno mezcla hacia `ROOT_COLOR` allí donde crece pasto, con una regla
  única (`visuals/grass_cover.rs`) que consumen el estampador y el suelo: a WGSL
  no cruza la lógica sino sus parámetros —umbrales como cosenos, kinds con pasto
  como máscara de bits— así que cambiarla mueve las dos mitades juntas. La
  fuerza bajó de 0,8 a 0,55: a 0,8 el suelo quedaba tan cerca en valor de las
  briznas que éstas dejaban de leerse como objetos separados.
  **Lo que falta:** un tinte plano no es una textura de pasto. `T_GroundTallGrass_Albedo.png`
  ya está en el repo y es lo que permitiría recortar el anillo exterior, que hoy
  se lleva casi la mitad de las briznas.
- **Entregable & validación.** Desde 30 m, dónde termina el pasto no se
  distingue. Es el paso más barato del documento y borra el peor artefacto.

### Paso 4: Grilla rodante centrada en la cámara, con densidad por anillo

Este paso absorbe lo que la versión anterior repartía en tres pasos sueltos
(densidad decreciente, cull por distancia, crecimiento anti-pop). Separados no
funcionan: el primero por la ley 4, y los otros dos porque son la misma pieza.

- **Lógica.** El campo deja de ser un cuadrado fijo de 25×25 m y pasa a ser un
  conjunto de anillos que **existen siempre alrededor de la cámara**. Al moverse,
  los chunks que quedaron atrás se re-hornean adelante. El número total de
  briznas es constante, no importa si el mapa mide 25 m o 4 km, y **el
  presupuesto se vuelve por vista en vez de por escena** — que es la única
  definición que tiene sentido en mundo abierto.

  Tres piezas, indivisibles, con los números en *La escalera de LOD*:
  - **Densidad horneada por anillo.** Un chunk lejano nace con pocas briznas; no
    nace con muchas y las esconde (ley 4). Las que sobreviven se ensanchan un
    poco para conservar la cobertura.
  - **Chunks más grandes hacia afuera.** El presupuesto de draws es **100**
    (`perf::budget::MOBILE_DRAWS`). Con chunks de 5 m en todo el radio serían
    cientos; con 5/10/20 m por anillo el total queda en el orden de las decenas.
  - **Crecimiento en la banda de transición.** La altura se multiplica por un
    factor que va de 1 a 0 en el borde de cada anillo, evaluado **respecto a la
    cámara**, no al jugador: si la cámara se aleja o hace zoom, el LOD responde a
    lo que la pantalla muestra. Sin esto, cada re-horneo es un pop.
- **Estado (2026-08-05): implementado.** Tres anillos (0-8 m a 45/m², 8-16 a 16,
  16-32 a 6), chunks de 5/10/20 m, uno horneado por frame al rodar y todos de
  una al entrar a la escena. Tres cosas que costaron una iteración cada una:
  1. **Decidir la pertenencia de un chunk por su centro deja huecos** de tierra
     pelada a pocos metros del jugador — un chunk grande cuyo centro cae dentro
     del anillo interior se descarta entero aunque ese anillo nunca llegue a su
     lado lejano. Ahora entra si *toca* la corona y sale sólo si está *entero*
     adentro.
  2. **La banda de crecimiento va en el borde de cada anillo, no del último.**
     Con una sola banda al final, los anillos internos hacían aparecer chunks de
     45/m² a ocho metros de la cámara, enteros y de golpe. Cada brizna lleva el
     alcance de su anillo y se apaga escalonada por su hash — todas juntas leen
     como una persiana bajando.
  3. **Los anillos se solapan durante la banda**, o la densidad cae a cero antes
     de que el siguiente empiece.
- **Lo que cuesta, y es deuda:** 207.200 triángulos declarados en los 360°, en la
  **peor alineación** de la cámara contra la grilla (en el origen son 82.000; el
  presupuesto declaraba el origen y lo llamaba el peor caso — era el mejor).
  `perf::budget::MEADOW_VIEW_TRIANGLES` lo declara con número y el test falla si
  crece.
- **Entregable & validación.** Caminar 200 m en cualquier dirección con pasto
  siempre alrededor, sin ver el anillo donde baja la densidad, sin ver aparecer
  nada, y con el conteo de draws y de triángulos **plano** durante todo el
  recorrido. `lod_cull` en el hub deja de ser `0/0`.
- **Riesgo propio de este paso:** re-hornear una malla mientras el jugador camina
  es el único trabajo por frame que este sistema va a tener nunca. Se hace de a
  un chunk por frame como máximo, y el criterio de aceptación incluye que el
  frame **no** muestre un pico al cruzar un borde de chunk.

---

## Fase 2 — Que se vea frondoso

Denso no es frondoso. 45 briznas/m² planas y todas del mismo verde se leen como
alfombra de plástico. Esta fase es sobre el aspecto, y toda ella vive en el
shader del Paso 1 — por eso va después.

### Paso 5: Onda de viento en espacio de mundo

- **Lógica.** El desplazamiento sale de una función de la posición XZ del
  vértice y del tiempo: una onda que viaja en la dirección del viento, más un
  segundo armónico de mayor frecuencia y menor amplitud, más micro-jitter.
  Todo multiplicado por `uv.y` — la altura normalizada que ya está en el
  vértice — para que la raíz no se mueva.

  **Y una tercera capa que la versión anterior no tenía:** un ruido de baja
  frecuencia y escala grande que **modula la amplitud** de las otras dos. Sin
  eso el campo entero ondea parejo, que lee como tela. Con eso hay zonas quietas
  y zonas agitadas y una frontera que se desplaza: eso lee como ráfaga.
- **Estado (2026-08-05): implementado.** Las tres capas, incluida la del ruido
  que modula la amplitud. El desplazamiento es cuadrático en la altura
  normalizada, así la brizna se arquea en vez de inclinarse rígida desde la
  base. Cero estado por brizna: una función de la posición y el tiempo, o sea
  una escritura de uniform por frame para todo el campo. *El primer intento lo
  hacía en CPU rotando el `Transform` de 2.352 entidades. Eliminado; no vuelve.*
- **Entregable & validación.** Ráfagas atravesando la pradera, raíces quietas, y
  el frame sin cambios respecto al Paso 2 — el viento debe ser gratis.

### Paso 6: Variación por brizna y normal abombada

- **Lógica.** Dos cosas baratas que atacan lo que más delata a un pasto
  generado:
  - **Color por brizna.** Un hash de la posición que corra tono y valor un poco.
    Va en el espacio que liberó el Paso 2; cuesta cero por frame.
  - **Normal abombada a lo ancho.** La normal sigue apuntando a +Y (ley del
    documento y razón por la que el campo no se apaga con el sol de lado), pero
    se abre levemente hacia afuera a lo ancho de la brizna. Sombrea como un
    cilindro suave en vez de como una superficie plana. Se reconstruye en el
    shader, que es donde ya vive desde el Paso 2.
- **Estado (2026-08-05): implementado.** El lado del quad viaja en el *signo* del
  hash, así que abombar no costó un atributo. La apertura es 0,18 y no 0,35: a
  0,35 un borde entero de la brizna quedaba notablemente más oscuro que el otro
  y el campo se veía moteado.
- **Entregable & validación.** Una captura del mismo punto antes y después: el
  campo deja de leerse como una superficie única. Frame sin cambios.

### Paso 7: Transmisión a contraluz

- **Lógica.** El pasto contra el sol brilla porque la luz atraviesa la hoja. Es
  lo que hace que un campo se sienta vivo a la hora dorada, y es lo que ninguna
  cantidad de densidad compra. Un término de wrap lighting barato en el fragment,
  activado por el ángulo entre vista y sol.
- **Estado (2026-08-05): implementado.** `sun_direction` sale del transform de la
  luz direccional, así que el ciclo día/noche lo maneja gratis. El término va al
  cuadrado para que sea un halo a contraluz y no un lavado general. Y la ley 5 de
  `GraphicalTechniques.md` ya lo autoriza explícitamente: *"un shader de
  rim/transmisión vegetal sólo entra como experimento opt-in, medido, y bajo el
  documento dueño `BOTWGrass.md`"*. Este es ese documento y esta es esa
  autorización.
- **Entregable & validación.** Amanecer y atardecer en la caja `Pasto` con el
  sol bajo de frente. Es fragment cost puro, así que **este paso sí tiene que
  medirse contra el dial de overdraw**, no sólo contra el frame.

### Paso 8: Brizna curva y acentos — **deja de ser condicional (2026-08-06)**

> **Por qué sube de prioridad.** Este paso estaba marcado *condicional* y se
> saltó para ir a la Fase 2, que es la que se ve bien enseguida. El día de los
> cuatro intentos fallidos dio la razón por la que hay que volver: la unidad del
> campo no aporta suficiente masa visual por triángulo que cuesta, y **por eso
> ninguna cantidad de densidad, alcance o textura hace que el campo se sostenga
> solo**.
>
> El número que lo sostiene, y es aritmética, no medición: nuestra brizna es un
> **quad plano con yaw uniforme al azar** (`grass.rs`, `yaw = u3 * TAU`). El
> ancho que proyecta a pantalla es proporcional al seno del ángulo entre su
> plano y la vista, y el promedio de `|sin|` sobre todos los yaws es **2/π ≈
> 0,64**. O sea que una brizna de 5,5 cm rinde en promedio como 3,5, y las que
> caen cerca de canto **rinden casi cero pagando sus tres triángulos igual**.
>
> Una hoja curva no tiene ángulo muerto: siempre hay superficie dando a la
> cámara. Es lo que se ve en las referencias que el usuario trajo (Genshin), y
> lo que permite que ensanchar sirva — hoy `width_scale` está en 1.0 con un
> comentario explicando que ensanchar se probó y se rechazó, y ese rechazo es
> correcto **para un quad plano**: una tira ancha y recta lee como papel.
>
> Curvatura → silueta → se puede ensanchar → hace falta menos densidad. Ése es
> el orden, y es la única cadena del documento que baja el costo mientras mejora
> la imagen.

- **Lógica.** Dos cosas que sólo importan en el anillo 0, donde la brizna ocupa
  píxeles de verdad:
  - **Curvatura.** Un quad plano lee como una carta; una brizna doblada en dos o
    tres segmentos lee como pasto. Es el único paso del documento que **gasta**:
    4-6 triángulos por brizna en vez de 2, y sólo dentro de 6 m — según la tabla
    de anillos, unas 4.500 briznas, o sea +9.000 a +18.000 tris.
  - **Acentos.** Una flor, un tallo seco, una mata alta, estampados al 1-3% entre
    las briznas. **Esto es lo que separa "campo" de "césped"**, y es casi gratis
    justamente porque es raro. Condición dura: comparten el material del pasto y
    se hornean en la misma malla del chunk. Un acento con material propio cuesta
    un draw call por chunk y mata el plan de anillos.
- **Condición de entrada.** **Sólo si el Paso 2 ya aterrizó** — se gasta el
  bandwidth que ese paso liberó, no bandwidth nuevo. Si el Paso 2 no se hizo,
  este paso no se abre.
- **Estado.** No implementado. La curvatura sustituye al *V-split* que la versión
  anterior mandaba a fuera de alcance: apuntaban al mismo problema (de cerca las
  briznas se ven finas) y la curvatura lo resuelve mejor. La punta partida sí se
  hizo, y es media solución al mismo problema por un triángulo.
- **Lo que hay que cuidar al hacerla.** La curvatura no puede ser sólo geometría
  doblada: la **normal** tiene que seguirla, o una hoja arqueada se sombrea como
  una plana y no se gana nada. Hoy la normal se reconstruye en el shader como +Y
  abombada 0,18 hacia el borde (`blade_normal`), y ese abombado es justamente
  una imitación barata de la curvatura que este paso haría real.

---

## Fase 3 — Interacción (después de que el campo se vea bien)

### Paso 9: Mapa de interacción

- **Lógica.** Para que el pasto se aplaste al pisarlo sin que la GPU recorra
  actores: una textura centrada en el jugador donde `Update` escribe las
  posiciones de pies, cascos y ruedas. El vertex shader hace **una** lectura y
  dobla la punta. El muestreo va por coordenada de mundo, no de pantalla, o los
  rastros se deslizan al caminar.
- **Estado.** No implementado. `GrassExtension` ya reserva el binding (101/102).
- **Entregable & validación.** Rastro al caminar que queda fijo en el suelo.

### Paso 10: Corte por espada

- **Lógica.** §20 en su forma más pura: la simulación de combate **no conoce el
  pasto**. Emite su evento de barrido como ya lo hace; presentación lo escucha en
  `Update`, lo escribe en otro canal del mapa, y el shader colapsa la altura.
- **Estado.** No implementado.
- **Entregable & validación.** Cortar pasto sin que `FixedUpdate` cambie.

---

## Presupuesto y cómo se mide

### El overdraw no da un número, y hay que saber qué sí

La vista de overdraw (`visuals/diagnostic.rs` + `assets/shaders/overdraw.wgsl`)
suma con `AlphaMode::Add` una dosis fija de `0.06` por fragmento. Es un **mapa de
calor que se mira**, no un contador: no loguea nada, y **satura a ~17 capas**
(1 ÷ 0,06). Sobre pasto a ángulo rasante se va a ver blanco parejo, que no
distingue diecisiete capas de sesenta. Sirve para *dónde*, nunca para *cuánto*.
Y ojo con la ley 6 de `GraphicalTechniques.md`: el pase de diagnóstico no puede
contaminar la muestra que pretende explicar — `perf/sequence.rs:290` ya lo apaga
durante las secuencias.

El número sale de **dos barridos diferenciales**, no de un contador:

- **Barrido de densidad.** Si el GPU ms escala con las briznas/m², la pradera es
  el costo; si es plano, no lo es. Es la misma lógica que ya zanjó una vez la
  duda de las sombras: quitar la cosa y mirar.
- **Barrido de resolución.** Renderizar al 100 / 75 / 50% y mirar el GPU ms. Es
  la forma estándar de diagnosticar fill-rate.

Los dos existen y **los dos se corrieron el 2026-08-06**. El cuadro que llenan:

| | resolución escala | resolución plana |
|---|---|---|
| **densidad escala** | fill-bound → bajar densidad/overdraw es la palanca | vertex-bound → Paso 2, y ahí sí vertex pulling |
| **densidad plana** | fill-bound, pero no por el pasto | el pasto no es el problema |

**Resultado: la casilla de arriba a la izquierda.** Las dos escalan, y la
resolución más fuerte que la densidad — 3,90 ms contra 1,08 al bajar a 30/m².
Fill-bound, y el pasto es la mayor parte de ese fill.

### Cómo se corre un barrido, desde el 2026-08-06

**Sin tocar el juego:** `BOF_BENCH=grass cargo run`. Arranca en la caja de la
suite, se para en su mirador, mide, escribe la tabla y cierra el proceso. Las
suites son `grass`, `general` y `shadows`; viven en `perf/suite.rs` como tablas,
y agregar una es una variante más una tabla, nunca código en el motor.

También están los botones del hub F1, uno por suite, más "aquí" para medir un
punto malo encontrado jugando.

**Dos trampas de esta máquina, las dos encontradas corriendo:**

1. **La ventana tiene que estar visible.** En Wayland nativo, un compositor no
   manda frame callbacks a una superficie que no se ve, y el juego entero se
   duerme: 1,9 segundos de CPU en 105 de reloj, sin llegar a medir. Para correr
   con la ventana en segundo plano hay que forzar XWayland:
   `WINIT_UNIX_BACKEND=x11 WAYLAND_DISPLAY= BOF_BENCH=grass cargo run`. Eso es
   **sólo para medir** — el juego se juega en Wayland nativo.
2. **El frame time suele quedar clavado por la presentación** y sus deltas no
   significan nada; los de GPU sí. El reporte lo detecta solo y lo avisa: si el
   trabajo de GPU se mueve mucho entre pasos y el frame casi nada, imprime que
   hay que leer `d-gpu` y descartar `d-frame`.

Lo que ese cuadro **no** dice, y hay que seguir repitiendo: nada de esto se midió
en el target. Un tiler cobra el vértice en bandwidth aunque no pinte un píxel, y
esa mitad del problema sigue sin evidencia.
- **Techo de triángulos:** 100.000 por escena (`perf::budget::MOBILE_TRIANGLES`).
  Sigue siendo un contrato útil porque es *dato* — determinista y testeable, a
  diferencia de los milisegundos. Pero es un **guardrail, no el objetivo**: que
  el pasto entre en el conteo no dice nada sobre si corre en un teléfono.
  Después del Paso 4 este techo debería reformularse como presupuesto por vista;
  hasta entonces la escena Mundo declara su exceso en
  `perf::budget::WORLD_SCENE_OVERSHOOT`.
- **Techo de draws:** 100. Es el que gobierna el Paso 4 y hoy nadie lo mira.
- **Techo de frame:** 16,6 ms duro, **~11 ms de objetivo sostenido**. Hoy: 5,78
  ms en escritorio.
- **Cómo se mide:** hub F1 en la caja `Pasto`, secuencia A/B desde el mismo
  punto, dos corridas quedándose con la limpia. **Un delta menor a la deriva
  entre los dos baselines (~0,05 ms) es ruido y no se reporta como mejora.**
- **Prohibido:** anotar acá un número que no salga del medidor, o presentar uno
  de escritorio como si fuera del target. El documento de antes de 2026-07-25
  afirmaba "0.0 ms CPU" y "60 FPS estables" el mismo día en que la medición daba
  35-46 FPS.

### Ver, no sólo medir (2026-08-06)

Medir sin el usuario se resolvió con `BOF_BENCH`. Faltaba la otra mitad y en un
problema visual es la que importa: **el agente no puede ver**, así que cada
cambio de aspecto viajaba hasta el usuario para su veredicto y una idea
equivocada costaba una sesión de juego suya. Tres herramientas cierran eso:

| comando | para qué |
|---|---|
| `BOF_SHOT=<suite> cargo run` | entra a la escena de la suite, se para en su mirador, deja un PNG en `target/shots/` y sale |
| `BOF_SHOT_POSE="x,y,z:dx,dy,dz"` | encuadre a mano — **para reproducir una queja**, no para comparar mediciones |
| `BOF_SCENE=Pasto cargo run` | arranca dentro de la caja, sin pasar por el menú |
| **F7 en el juego** | el usuario captura lo que está viendo; el log imprime la pose ya formateada como `BOF_SHOT_POSE` |

La foto reusa escena y mirador de `BenchSuite` a propósito: *"se ve mejor"* y
*"cuesta 3 ms más"* sólo forman una decisión si son del mismo lugar.

**Lo que una captura sí y no puede decir:**

- **Las estadísticas de píxeles son deterministas.** Luminancia, saturación y
  variación no las contamina que el usuario tenga Firefox abierto — al revés que
  los tiempos, que el mismo día dieron 10,89 / 11,88 / **3,83 ms** para el mismo
  pasto en tres corridas. Antes de cualquier `BOF_BENCH`, **preguntar qué tiene
  abierto**.
- **El piso de ruido de una comparación de imágenes es 5%.** Medido con dos
  corridas idénticas: el viento mueve las briznas entre disparos, así que
  cualquier diferencia visual menor que eso no significa nada. Sin ese control,
  un experimento del día casi se lee al revés.
- **Cada captura imprime el inventario de escena** (mallas visibles, triángulos,
  draws), porque una foto no puede distinguir "se dibuja menos" de "se proyecta
  menos" y esa distinción fue justamente la que hubo que resolver.

**Y buscando eso apareció un bug de meses:** el inventario consultaba mallas por
tipo de material —`StandardMaterial` y `TerrainMaterial`— y `GrassMaterial` es un
tercero, así que **el presupuesto de escena nunca contó la pradera**. En la caja
Pasto declaraba 33.792 triángulos con cien mil briznas en pantalla. Hoy los
triángulos se cuentan por `Mesh3d`, sin mirar el material, y una ley en
`tests/architecture.rs` exige que todo `MaterialPlugin` registrado aparezca en
`collect_scene`.

## Errores que este documento ya cometió — no reintroducir

Se listan porque el archivo lleva varias reescrituras y los tres volvieron a
sonar razonables cada vez:

1. **"El límite real es el conteo de triángulos."** Contradecía a `NORTE.md`, que
   ya decía que en el piso móvil el costo es fill-rate/overdraw, draw calls y
   sombras. Gobernar por el conteo lleva a recortar geometría cerca (donde es
   barata) y a extender lejos (donde es carísima): exactamente al revés.
2. **"El dithering conserva el early-Z."** Falso. Ver ley 3.
3. **"El shader toma el gradiente de `uv.y`, que corre al revés."** Falso;
   `uv.y` es correcto. Los bugs del shader son otros tres (Paso 1).
4. **"Vertex pulling exige pipeline propia y perder PBR."** Falso: `AsBindGroup`
   soporta storage buffers y `ExtendedMaterial` conserva el PBR. La técnica se
   descarta por aritmética, no por incompatibilidad — ver *Fuera de alcance*.
5. **"El medidor principal es el dial de overdraw."** A medias: esa vista no
   produce ningún número y satura a ~17 capas. Los números salen de dos barridos
   diferenciales, y ninguno de los dos es ejecutable todavía.
6. **"El barrido de densidad está disponible hoy con F8, cero código."** Falso.
   La tecla funciona, pero el dial está fuera de `PerfKnob` y por lo tanto fuera
   de la matriz A/B: lo que produce es una impresión cronometrada a mano, que es
   justo lo que `perf/sequence.rs` existe para impedir.

7. **"La brizna curva rinde más área por triángulo."** Propuesta mía el
   2026-08-05 y **refutada por mi propia medición** al día siguiente: barrido de
   cámara de 0° a 90° en Blender contando píxeles de silueta, plana 1399 px por
   triángulo, cruz 1006, curva **530** — la peor de las tres. Corolario que
   refuerza la ley 1: con los mismos 4 triángulos, dos briznas planas dan más
   área que una cruz.
8. **"Teñir el suelo hacia la punta llena el horizonte."** Lo dice Ghost of
   Tsushima y acá lo vacía: probado y medido el 2026-08-06, deja el horizonte más
   claro que el primer plano. La técnica presupone una punta que lea como masa de
   pasto, y la nuestra era casi blanca.
9. **"El clumping es correcto y gratis, así que se queda."** Gratis sí, correcto
   no — *para nuestro objetivo*. Compra estructura pagando uniformidad, y lo que
   este juego quiere es una alfombra. Una técnica se juzga contra el objetivo, no
   contra su prestigio.

El patrón detrás de los cinco primeros es el mismo: **una técnica se descarta con
un número, no con una intuición sobre su complejidad.** Cuando este documento
descartó algo por "es complicado" o "no encaja", se equivocó.

Los tres últimos agregan un patrón distinto, del 2026-08-06: **un número tampoco
sirve si el objetivo contra el que mide se eligió a ojo.** Medir con tres
decimales la distancia hasta un blanco supuesto es precisión, no evidencia — y
las dos veces que pasó ese día, el blanco venía de una preferencia mía o de un
juego que resuelve otro problema.

## Fuera de alcance (a propósito)

Descartado sin discusión: teselado y mesh shaders (no existen en el target),
compute shaders y GPU-driven culling, generación procedural más allá del hash
determinista, pasto que crece con el tiempo, clima que lo moje. Billboards y
shell texturing tienen su propia sección con el porqué.

### Vertex pulling: mejor técnica, no mejor próximo paso

El estado del arte es *vertex pulling* — generar la brizna dentro del vertex
shader a partir de `vertex_index`, leyendo unos pocos bytes por brizna de un
storage buffer en vez de un vértice por esquina. Es lo que hacen las mejores
implementaciones y regala la curvatura del Paso 8.

**Corrección primero.** Este documento decía que no entra porque "en Bevy exige
una pipeline propia y perder PBR, sombras y niebla". **Es falso.** Eso vale para
el hardware instancing por brizna y para un render pipeline custom, no para esto:
`AsBindGroup` soporta `#[storage(n, read_only)]` y `ExtendedMaterial` ya da un
vertex shader propio conservando el PBR entero. La razón estaba mal.

**La razón real son dos números.** Por brizna, en bytes de geometría:

| | bytes/brizna |
|---|---:|
| hoy | 192 |
| después del Paso 2 | 48 |
| vertex pulling puro | 16 |

*(1)* Pulling es 3× mejor que el Paso 2, pero sobre 22.064 briznas esa diferencia
son 0,7 MB por frame — del orden de 85 MB/s contra los ~15-25 GB/s del aparato:
**menos del 1%, o sea ruido.** *(2)* Y sobre todo: después del Paso 2 quien
domina el tráfico de vértices no es la entrada sino los **varyings**, unos 68
bytes por vértice que fija el pipeline de Bevy. **Pulling no toca los varyings** —
produce la misma cantidad de vértices con la misma salida. Lo único que los baja
es tener menos vértices, que es el Paso 4.

O sea: pulling optimiza la mitad chica de un costo que no es el cuello de
botella, y el Paso 4 ataca la mitad grande recortando 22% las briznas mientras
multiplica por 6 el área. Queda un tercer punto menor: el soporte de storage
buffers en la etapa de vértices es despareja en móviles de 2021.

**Disparador para reabrirlo, falsable:** si después del Paso 4 el barrido de
resolución dice que somos *vertex-bound* y no *fill-bound* (ver la matriz en
*Presupuesto*), esta es la puerta y hay que cruzarla. No es "no podemos": es
"todavía no toca".

## Arte en Blender

Hoy la brizna se genera en código: cuatro vértices, forma controlada por
constantes en `grass.rs`. Funciona, pero la forma la decide el programador.

### Cuántas mallas — una, no varias

La tentación es autorar cinco o seis briznas distintas para que el campo no se
vea repetido. **No hace falta y no funciona así.** Una brizna mide 5,5 cm de
ancho: a densidad de pradera nadie va a percibir jamás que la brizna #3 tiene
otra silueta que la #7. Lo que sí se percibe, y lo que delata a un campo
generado, es la uniformidad de **altura, yaw, inclinación y color** — y esas
cuatro ya las produce el estampador con su hash, gratis, sin una malla más.

Entonces el set es corto y cada pieza tiene un porqué distinto:

| pieza | tris | por qué existe | dónde se usa |
|---|---:|---|---|
| `brizna plana` | 2 | la unidad del sistema (ley 1) | anillos 1 y 2 |
| `brizna curva` | 4-6 | de cerca un quad plano lee como carta | anillo 0 (Paso 8) |
| 3-4 acentos | ≤8 | *variedad de especie*, no de forma: flor, tallo seco, mata alta | 1-3% de las posiciones |

Las dos briznas **son la misma planta a dos fidelidades**, no dos plantas. Los
acentos sí son otra cosa, y son los que hacen que lea como naturaleza en vez de
como jardín.

### El contrato de la brizna authored

El estampador lee los vértices del GLB y los planta en la malla del chunk
aplicando posición, yaw, altura e inclinación. Para que eso funcione:

- **Pivote en el origen, apoyado en el suelo**, creciendo hacia `+Y`.
- **Sin material.** El material lo decide el campo, no el asset. Una brizna que
  trae `M_*` propio rompe el batching por chunk.
- **Sin colisión.** El pasto no tiene collider y nunca lo va a tener: el suelo
  debajo ya reporta `Surface(Grass)` para el audio de pasos.
- **Sin vertex colors ni normales autoradas.** Después del Paso 2 ambas se
  derivan en el shader; hornearlas en el GLB sería volver a pagar los 28 bytes
  que ese paso borró.
- **Geometría de una cara.** El material dibuja las dos con `cull_mode: None`;
  duplicar la cara en Blender duplica el costo por nada.
- **Tope de triángulos por pieza**, que es la única regla que de verdad hay que
  hacer cumplir: 2 la plana, 6 la curva, 8 un acento.

### Y acá hay un choque con `ASSET_PIPELINE.md` que hay que resolver

Una brizna estampada **no es un asset de escena**: es una fuente de geometría que
se consume al hornear y que nunca se spawnea como nodo. No encaja en ninguna
regla del contrato vigente — no tiene collider, no tiene material de paleta, no
tiene bandas `VisibilityRange` (su LOD es la densidad del anillo, un mecanismo
completamente distinto), y el sufijo `_LOD0` obligatorio no significaría nada.

Meterla como `prop_` haría que `build.rs` le exija todo eso y que el `LOD1/LOD2`
authored se confunda con el LOD por anillos, que es exactamente el error que este
documento existe para no cometer.

**Recomendación:** una categoría propia en
`domain::asset_pipeline::schema::ASSET_CATEGORIES`, para que `build.rs` haga
cumplir *las reglas correctas* (tope de tris, prohibido `U*_`, `SKT_`, `M_` y
sufijo `_LOD`) en vez de las inapropiadas. Es un cambio chico pero toca el
contrato de assets, así que **no se implementa sin decisión explícita** — queda
anotado acá, no ejecutado.

### Lo que ya existe, medido el 2026-08-06

Siete GLB de pasto viven en `assets/game/authored/props/` y **ningún `.rs` los
nombra**: no se spawnean en ninguna escena. Contados leyendo los GLB:

| pieza | tris | material | atributos |
|---|---:|---|---|
| `prop_grass_a` / `_b` / `_c` | 22 | `M_FoliageCommon` | POSITION, NORMAL, TEXCOORD_0, COLOR_0, COLOR_1 |
| `prop_grass_tall_a` | 22 | `M_FoliageCommon` | ídem |
| `prop_grass_very_short_a` | 22 | `M_FoliageCommon` | ídem |
| `prop_grass_dry_a` | 22 | `M_FoliageDry` | ídem |
| `prop_grass_card_a` | 2 | `M_FoliageCard` | ídem |

Los seis matojos son **11 briznas de costo cada uno** contra las 2 de la unidad
del sistema, y traen puestos los tres atributos que el Paso 2 borró a mano. Como
**unidad del campo la ley 1 los descarta con número**, y eso no cambia.

Pero *acento* es otro trabajo: al 1-3% de las posiciones, 22 triángulos es
barato, y `prop_grass_dry_a` (tallo seco) y `prop_grass_tall_a` (mata alta) son
literalmente dos de los tres acentos que el Paso 8 pide. Ahí sí entran, con dos
condiciones: se **estampan en la malla del chunk** como la brizna —el horneador
lee POSITION e índices y tira el resto— y el `M_*` se ignora, porque el material
lo decide el campo. `prop_grass_card_a` es un caso aparte: 2 triángulos, y el
candidato natural para dar masa al horizonte si las cartas de grupo se
reconsideran contra 900p30.
