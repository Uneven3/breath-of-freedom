# Pasto denso estilo BOTW — técnicas y cómo se replican en Bevy

Hoja de ruta del sistema de pasto. Reescrito el **2026-07-25** tras medir el
primer intento: lo que había mezclaba técnicas reales con afirmaciones que la
medición desmintió, y trabajaba sobre la unidad equivocada.

> **Cómo se usa este documento.** Un paso por vez. Cada paso se cierra con su
> entregable **jugado** en la caja `Pasto` (`grass.ron`) y medido con el hub F1
> antes de abrir el siguiente. Un paso que no se puede validar no se implementa.
>
> **Honestidad de fuentes.** Nintendo no publicó su implementación. Lo que sigue
> separa **lo observable en el juego** de **la técnica conocida que produce ese
> resultado**. Donde algo es inferencia, se dice. Ninguna afirmación de
> rendimiento entra acá sin un número medido al lado.

---

## Lo que se observa en BOTW, y qué lo produce

| Lo que se ve | Técnica que lo produce | Por qué |
|---|---|---|
| El suelo no se ve: hay pasto en todas partes cerca del jugador | Densidad alta (decenas de briznas por m²) con geometría baratísima | La densidad es el efecto; todo lo demás existe para poder pagarla |
| El pasto **brota** del suelo al acercarse; nunca aparece de golpe | Escalado vertical con la distancia (*grow*, no *fade*) | Es geometría, no transparencia: sin blending, sin orden, sin overdraw |
| El pasto lejano desaparece sin que se note | El albedo converge al color del terreno antes de apagarse | Si el color ya coincide, la desaparición no tiene borde que delate |
| El campo no termina en una línea | El terreno está pintado del mismo verde que la raíz de la brizna | El terreno *es* el LOD más lejano |
| Olas de viento recorren la pradera entera | Onda en espacio de mundo evaluada en el vertex shader | Una función de la posición XZ: no hay estado por brizna |
| La raíz queda quieta y solo la punta flamea | El desplazamiento se multiplica por la altura normalizada del vértice | El dato de altura viaja en el vértice, no en el CPU |
| Las briznas no se oscurecen cuando el sol pega de lado | Normales hacia +Y (o la normal del terreno), no la de la cara | Una cara plana iluminada por su propia normal se apaga al girar el sol |
| Hay pasto en las laderas pero no en la roca | Filtro por pendiente al generar | Decisión de generación, no de render |
| El pasto se corta con la espada y vuela | Reacción visual a un evento de combate | Presentación pura: la simulación no sabe que hay pasto (§20) |
| Se aplasta al caminar sobre él | Deformación leída de una textura de interacción | Una lectura de textura por vértice, en vez de recorrer actores |

---

## Las tres leyes de este sistema

Salieron de medir el primer intento. Violar cualquiera de las tres reproduce
exactamente el fracaso que ya tuvimos: **matojos aislados sobre tierra pelada**.

### 1. La unidad es la brizna de 2 triángulos

Agrupar briznas en un matojo modelado multiplica por seis el costo de cada
instancia, y la única forma de seguir dentro del presupuesto después de eso es
separarlas — que es cómo una pradera se convierte en arbustos sueltos.

Medido el 2026-07-25, con el mismo gasto de triángulos:

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

### 3. La opacidad es el enemigo

- **Alpha blending** obliga a ordenar, no escribe depth útil y apaga el early-Z.
  Con pasto es letal: cada píxel queda cubierto por decenas de briznas y se
  pagan todas.
- **Alpha test (`Mask`)** también rompe el early-Z: el hardware no puede
  descartar el fragmento antes de ejecutar el shader que hace el `discard`.

Este proyecto ya pagó esa lección: pasar el follaje de `Mask` a `Opaque` fue lo
que llevó el bosque de 13 a 60 FPS (`visuals/foliage.rs:96`).

**Regla:** el pasto es **opaco, siempre**. La silueta va en la geometría, no en
un canal alfa. Las transiciones se hacen con **crecimiento** y con **dithering**
(descarte por patrón de ruido, que conserva el depth y el early-Z), nunca con
mezcla. `AlphaMode::Blend` no entra en este sistema.

---

## Estado actual (medido el 2026-07-25)

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

Medición en la caja `Pasto`, mismo punto, 7 configuraciones A/B:

| | valor |
|---|---|
| frame | **5,78 ms** (presupuesto 16,6) |
| gpu | 4,16 ms |
| draws / meshes en pantalla | 11 |
| única palanca fuera del ruido | sombras: −0,66 ms |
| deriva entre baselines | 0,05 ms (cualquier delta menor es ruido) |

Presupuesto de triángulos: 56.250 (pasto) + 32.768 (terreno) = **89.018 de
100.000**. Ese es hoy el límite real, no el tiempo de frame.

---

## Fase 1 — Alcance: llegar más lejos sin gastar más

El campo mide 25×25 m y termina en un borde recto. Es lo que más molesta hoy, y
el presupuesto de triángulos impide agrandarlo por fuerza bruta.

### Paso 1: Densidad decreciente con la distancia

- **Lógica.** El nivel de detalle de este sistema **no es cambiar de forma, es
  cambiar de cantidad**. Con briznas de 2 triángulos, sustituir una brizna por
  un billboard de 2 triángulos no ahorra nada: el billboard de BOTW reemplaza un
  *grupo* de briznas, no una. Cada brizna calcula un ruido determinista de su
  posición y colapsa a altura cero si ese ruido supera el umbral que le
  corresponde a su distancia. Un triángulo degenerado se descarta en el clipper:
  cuesta el vertex shader y ni un fragmento. Las que sobreviven se ensanchan un
  poco para conservar la cobertura visual.
- **Estado.** No implementado. La densidad es uniforme en todo el campo.
- **Entregable & validación.** El campo llega a 60 m con el mismo presupuesto de
  triángulos que hoy. Se valida mirando: no debe verse el anillo donde baja la
  densidad.

### Paso 2: Chunks culleados por distancia

- **Lógica.** Los chunks ya son unidades culleables — con la cámara puesta, el
  frustum descarta 17 de 25 (medido: `meshes=11`). Falta el cull por distancia:
  `VisibilityRange` sobre la entidad del chunk, como ya hace
  `visuals/foliage.rs::apply_foliage_lod` con el follaje del bosque.
- **Estado.** No implementado.
- **Entregable & validación.** `lod_cull` en el hub deja de ser `0/0`.

### Paso 3: El terreno es el LOD más lejano

- **Lógica.** Que el pasto termine no se arregla con más pasto: se arregla
  pintando el terreno del mismo verde que la raíz de la brizna. La transición
  deja de existir porque no hay dos cosas distintas. Requiere que el albedo del
  terreno y `ROOT_COLOR` compartan firma cromática.
- **Estado.** El terreno ya carga `T_GroundGrass_Albedo.png`; los colores no
  están calibrados entre sí.
- **Entregable & validación.** Desde 30 m, dónde termina el pasto no se
  distingue.

---

## Fase 2 — Movimiento: el viento en la GPU

### Paso 4: `ExtendedMaterial` enchufado y shader corregido

- **Lógica.** El viento, el crecimiento y el dithering necesitan un vertex
  shader. Bevy lo permite sin abandonar el PBR: `ExtendedMaterial<StandardMaterial,
  GrassExtension>` conserva luz, sombras y niebla, y agrega los uniforms propios
  (tiempo, dirección del viento, posición de cámara).
- **Estado.** `src/visuals/grass_material.rs` y `assets/shaders/grass.wgsl`
  existen y están **registrados pero sin usar**, y el shader está roto: escribe
  `out.position = world_position` (debe ser clip space) y toma el gradiente de
  `uv.y`, que en nuestras briznas corre al revés. Se corrige, no se reescribe.
- **Entregable & validación.** El campo se ve **idéntico** a hoy con el material
  nuevo puesto. Cambiar el motor de render sin cambiar la imagen es la única
  forma de saber que el cambio fue neutral.

### Paso 5: Onda de viento en espacio de mundo

- **Lógica.** El desplazamiento sale de una función de la posición XZ del
  vértice y del tiempo: una onda que viaja en la dirección del viento, más un
  segundo armónico de mayor frecuencia y menor amplitud, más micro-jitter para
  que nunca quede quieto. Todo multiplicado por la **altura normalizada del
  vértice**, para que la raíz no se mueva. Ese dato de altura hay que hornearlo
  en el vértice (hoy el color ya distingue raíz de punta; conviene un canal
  explícito).
- **Estado.** El primer intento lo hacía en CPU rotando el `Transform` de 2.352
  entidades. Eliminado hoy; no vuelve.
- **Entregable & validación.** Olas atravesando la pradera, raíces quietas, y el
  frame sin cambios respecto al Paso 4 — el viento debe ser gratis.

### Paso 6: Crecimiento anti-pop

- **Lógica.** La altura de la brizna se multiplica por un factor que va de 1,0
  cerca a 0,0 en el borde del campo, evaluado **respecto a la cámara**, no al
  jugador: si la cámara se aleja o hace zoom, el LOD debe responder a lo que la
  pantalla muestra.
- **Estado.** El primer intento lo hacía en CPU y además pisaba la escala
  autorada, con lo que todo el pasto tenía la misma altura. Eliminado.
- **Entregable & validación.** Caminar hacia el borde del campo sin ver aparecer
  nada.

---

## Fase 3 — Interacción (después de que el campo se vea bien)

### Paso 7: Mapa de interacción

- **Lógica.** Para que el pasto se aplaste al pisarlo sin que la GPU recorra
  actores: una textura centrada en el jugador donde `Update` escribe las
  posiciones de pies, cascos y ruedas. El vertex shader hace **una** lectura y
  dobla la punta. El muestreo va por coordenada de mundo, no de pantalla, o los
  rastros se deslizan al caminar.
- **Estado.** No implementado. `GrassExtension` ya reserva el binding.
- **Entregable & validación.** Rastro al caminar que queda fijo en el suelo.

### Paso 8: Corte por espada

- **Lógica.** §20 en su forma más pura: la simulación de combate **no conoce el
  pasto**. Emite su evento de barrido como ya lo hace; presentación lo escucha en
  `Update`, lo escribe en otro canal del mapa, y el shader colapsa la altura.
- **Estado.** No implementado.
- **Entregable & validación.** Cortar pasto sin que `FixedUpdate` cambie.

---

## Presupuesto y cómo se mide

- **Techo de triángulos:** 100.000 por escena (perfil móvil). El terreno se lleva
  32.768 fijos. El pasto tiene ~56k y no debería crecer: si hace falta más
  alcance, sale del Paso 1, no de subir el conteo.
- **Techo de frame:** 16,6 ms. Hoy: 5,78 ms.
- **Cómo se mide:** hub F1 en la caja `Pasto`, secuencia A/B desde el mismo
  punto, dos corridas quedándose con la limpia. **Un delta menor a la deriva
  entre los dos baselines (~0,05 ms) es ruido y no se reporta como mejora.**
- **Prohibido:** anotar acá un número que no salga del medidor. El documento
  anterior afirmaba "0.0 ms CPU" y "60 FPS estables" el mismo día en que la
  medición daba 35-46 FPS.

## Fuera de alcance (a propósito)

Teselado, compute shaders, GPU-driven culling, generación procedural del campo
más allá del hash determinista, pasto que crece con el tiempo, clima que lo
moje. Y **el V-split de briznas cercanas**: apunta a un problema real (de cerca
las briznas se ven finas), pero es la última pieza, no la primera — con
densidad alta y ensanchado por distancia puede que no haga falta.

## Arte en Blender

Hoy la brizna se genera en código: cuatro vértices, forma controlada por
constantes en `grass.rs`. Funciona, pero la forma la decide el programador.

El camino para devolverle el control al artista **sin perder nada** del
rendimiento actual: autorar **una sola brizna** (`prop_grass_blade_a.glb`, 2
triángulos, 4 vértices, pivote en la base, normales +Y, ~0,4 m) y que el sistema
estampe sus vértices en la malla del chunk aplicando posición, yaw, altura e
inclinación. Sigue siendo una malla por chunk y cero trabajo por frame.

Los props actuales (`prop_grass_a` … `prop_grass_tall_a`, 12 triángulos cada
uno) son matojos de cuatro briznas: la unidad que este documento descarta. No se
borran, pero la pradera no los usa.
