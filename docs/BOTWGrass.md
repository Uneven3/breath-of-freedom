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

## Estado actual (medido el 2026-07-25, en escritorio)

Implementado en `src/visuals/grass.rs`, con 4 tests:

- **45 briznas/m²**, 28.125 briznas, 56.250 triángulos.
- **25 entidades** (chunks de 5×5 m en una grilla de 5×5), una malla y un draw
  call cada uno. **Cero trabajo por frame**: nada recorre briznas.
- Brizna generada en código: quad con punta angostada, normales +Y, degradado
  raíz→punta por **color de vértice** (`StandardMaterial` lo multiplica por su
  base, así que el gradiente no cuesta un shader).
- Filtro de pendiente: nada de pasto sobre 45°, briznas más cortas sobre 35°.
- Material **opaco**, `double_sided` (una brizna se ve de los dos lados),
  `NotShadowCaster`.

Medición en la caja `Pasto`, mismo punto, 7 configuraciones A/B *(tipo a)*:

| | valor |
|---|---|
| frame | **5,78 ms** (techo 16,6; objetivo sostenido ~11) |
| gpu | 4,16 ms |
| draws / meshes en pantalla | 11 |
| única palanca fuera del ruido | sombras: −0,66 ms |
| deriva entre baselines | 0,05 ms (cualquier delta menor es ruido) |

**Y el dato que el conteo escondía:** la pradera cubre 625 m² de un mundo de
320×320 m, y ya se lleva el **52% del presupuesto de triángulos de la escena
Mundo** (`AHORA.md`). Extendida al mapa entero serían millones de triángulos.
La forma "campo horneado de tamaño fijo" no llega, y afinarla no la va a hacer
llegar. Ver Fase 1.

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

### Los anillos

Densidades con un factor de seguridad ×2,5 sobre el mínimo, porque las briznas
no se reparten perfecto y algo de solape se quiere:

| anillo | radio | chunk | densidad | briznas | tris |
|---|---:|---:|---:|---:|---:|
| 0 cerca | 0–6 m | 5 m | 40/m² | 4.524 | 9.048 |
| 1 medio | 6–18 m | 10 m | 10/m² | 9.048 | 18.096 |
| 2 lejos | 18–35 m | 20 m | 3/m² | 8.492 | 16.984 |
| 3 | > 35 m | — | — | terreno teñido | 0 |
| | | | **total** | **22.064** | **44.128** |

Contra el campo de hoy (28.125 briznas, 56.250 tris, 625 m²): **6,2× el área por
el 78% del costo.** Chunks totales ~35, de los que el frustum descarta la
mayoría — del orden de 15 draws, contra 11 hoy y un techo de 100.

*Los radios y densidades son estimaciones derivadas de la tabla de arriba; se
ajustan por ojo en la caja `Pasto`. Lo que no se ajusta es la forma de la curva.*

Dos cosas que el anillo 0 obliga a decir en voz alta:

- **Baja de 45/m² a 40/m².** `BLADES_PER_SQUARE_METRE` se eligió por ojo (25 se
  veía delgado, 45 no) y 40 está dentro del ruido de ese juicio, pero es un
  cambio en lo que se ve y se valida mirando, no asumiendo.
- **Y 45 puede estar de más incluso cerca.** La tabla dice que a 2 m alcanzan
  36/m²; a 5 m, 15. Lo que sobra se paga entero en overdraw, así que **si el
  barrido de densidad confirma que el pasto es fill-bound, bajar el anillo 0 es
  la palanca más barata del sistema entero** — más barata que cualquier shader.
  Ese barrido es el Paso 0 del plan y todavía no se puede correr; ver
  *Presupuesto*.

### Billboards: tres cosas distintas, las tres descartadas

"Billboard" nombra tres técnicas que no tienen nada que ver entre sí, y conviene
separarlas porque sólo una es tentadora de verdad:

**1. Brizna que gira hacia la cámara.** No ahorra nada: sigue siendo un quad de
2 triángulos. Agrega trabajo por frame, y briznas que pivotean al mover la cámara
se leen como un error. Es lo que suele significar "pasto con billboards" y es la
peor de las tres.

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

### Paso 8: Brizna curva y acentos (condicional)

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
  briznas se ven finas) y la curvatura lo resuelve mejor.

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

**Y ninguno de los dos es ejecutable hoy**, aunque F8 haga ciclar la densidad.
Ver la sección siguiente antes de intentarlo.

### Los dos diales tienen que nacer en el hub, no en una tecla

`grass.rs` cicla la densidad con un `KeyCode::F8` propio y un `GrassStressState`
local. Es el **único dial visual fuera de `PerfKnob`**, el registro tipado que
tiene los otros doce (`Wireframe`, `Overdraw`, `Forest`, `TreeDetail`…), y viola
dos cosas ya escritas: la Fase 1 de `GraphicalTechniques.md` ("exponer los diales
de comparación en el hub F1") y la regla del menú, que en `hud_menu.rs` está
puesta como *"the one key; everything else inside is a click"*.

Lo que lo vuelve bloqueante y no cosmético: **`perf/sequence.rs` maneja
`PerfToggles`**. Un dial que no es un `PerfKnob` no puede entrar en la matriz
A/B, así que no tiene warmup, ni ventana de asentamiento, ni cámara clavada, ni
chequeo de deriva. Su propio encabezado dice por qué eso arruina el resultado:
*"el operador cronometra mal, las muestras post-cambio siguen asentándose, y la
deriva entre la primera y la última configuración es invisible"*. Un barrido a
mano con F8 no produce una medición: produce una impresión.

Entonces, antes que cualquier paso del plan:

1. **`GrassDensity` entra a `PerfKnob`** y el handler de F8 se borra. Es un dial
   escalonado, igual que `ShadowRange` o `TreeDetail`; no hace falta mecanismo
   nuevo. `PerfKnob::ALL` es un array de tamaño fijo, así que agregarlo obliga al
   compilador a señalar cada sitio que falte.
2. **`RenderScale` entra a `PerfKnob`** — el dial de resolución nace ahí por la
   misma razón, no como otra tecla suelta.
3. **Los dos entran como pasos de `STEPS`**, un cambio por paso para que el delta
   sea atribuible. Eso es lo que llena la matriz de abajo con números que
   sobreviven a comparar hoy contra dentro de un mes.

Sostener MSAA constante durante el barrido de resolución: el perfil móvil lo pone
en `Sample4` (`perf::data::profile_msaa`) y moverlo mezclaría dos variables.

Juntos separan las dos causas, que es lo que ninguno solo puede hacer:

| | resolución escala | resolución plana |
|---|---|---|
| **densidad escala** | fill-bound → bajar densidad es la palanca | vertex-bound → Paso 2, y ahí sí vertex pulling |
| **densidad plana** | fill-bound, pero no por el pasto | el pasto no es el problema |

Hasta que ese cuadro esté lleno con números reales, "el pasto cuesta demasiado"
es una hipótesis, no un hallazgo.
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

El patrón detrás de los cinco es el mismo: **una técnica se descarta con un
número, no con una intuición sobre su complejidad.** Cuando este documento
descartó algo por "es complicado" o "no encaja", se equivocó.

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

### Lo que ya existe y no sirve

Los props actuales (`prop_grass_a` … `prop_grass_tall_a`, 12 triángulos cada
uno) son matojos de cuatro briznas: la unidad que la ley 1 descarta con número.
No se borran, pero la pradera no los usa.
