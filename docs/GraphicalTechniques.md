# Técnicas gráficas — estándar y hoja de ruta

Contrato objetivo para LOD, culling, batching, shading y presupuesto gráfico de
**Breath of Freedom** en Bevy 0.19. El código y `AHORA.md` describen el estado
vivo; este documento define lo que se quiere construir y cómo se acepta.
Revisado el **2026-08-05** contra el código y el target.

Los contratos especializados siguen teniendo un único dueño:

- `ASSET_PIPELINE.md`: nombres, jerarquía Blender→GLB y bandas LOD.
- `BOTWGrass.md`: representación, densidad y respuesta de la pradera.
- `TEXTURES.md`: formatos, alfa, procedencia y presupuesto de texturas.
- `LIGHTING.md`: luces, sombras y atmósfera.
- `PARTICLES.md`: VFX transitorios y su presupuesto de fill.

---

## Resultado buscado

| Resultado visible | Técnica | Criterio |
|---|---|---|
| La silueta se conserva al alejarse | LOD solapado con `VisibilityRange` | No hay *popping* legible a distancia de juego |
| Bosque y pradera no saturan CPU | Handles compartidos y geometría agrupada | El número de entidades/draws no escala por brizna |
| El follaje no pierde el frame | Geometría opaca y cota de sombras | Sin alpha-test en vegetación baseline |
| Los objetos fuera de alcance desaparecen sin afectar gameplay | Frustum/distance culling sólo visual | Colliders y resultados de `FixedUpdate` no cambian |
| La escena cabe en el target móvil | Guardrails estáticos + inventario runtime | ≤100k tris, ≤100 draws, ≤64 materiales |
| El fill-rate no se dispara con vegetación densa | Dial de overdraw del hub F1 | Sin `discard` en el baseline; el overdraw se mira, no se supone |
| El estilo sigue siendo legible y barato | `StandardMaterial` mate | Sin toon shader ni outline fullscreen baseline |
| Los bordes no serruchan en el teléfono | MSAA 4× en el perfil móvil, off en desktop | Es el AA que un tiler resuelve on-chip (ley 7) |

---

## Las siete leyes

### 1. El presupuesto es un contrato, no una advertencia

Cada GLB authored se rechaza en build si su LOD0 supera el presupuesto de su
categoría:

| Categoría | LOD0 máximo |
|---|---:|
| `prop` | 1.500 tris |
| `weapon` | 2.000 tris |
| `tree` | 3.000 tris |
| `structure` | 6.000 tris |
| `char` / `creature` | 15.000 tris |

La escena completa apunta además a `MOBILE_TRIANGLES = 100_000`,
`MOBILE_DRAWS = 100` y `MOBILE_MATERIALS = 64`. `SceneInventory` grada por el
**peor** de los tres cocientes y sólo avisa al cruzar o recuperar un límite.

**Los tres son guardrails, no el objetivo.** El target es un Android de gama
media ~2021 (`NORTE.md`), y esa clase de chip es tile-based: el costo dominante
es fill-rate/overdraw, bandwidth de vértices y draw calls — no el conteo de
triángulos. Estos números están acá porque son *dato* (deterministas,
testeables, rompen el build); pasarlos no demuestra que la escena corra en el
target. El techo de frame que importa es el **sostenido** (~11 ms), no el pico:
un teléfono baja su reloj tras diez o quince minutos. `BOTWGrass.md` desarrolla
las consecuencias para vegetación densa, que es donde muerden primero.

### 2. `ASSET_PIPELINE.md` es el único contrato de LOD

`LOD0` es obligatorio y cubre `0–30 m`; `LOD1` y `LOD2` son opcionales, pero
contiguos, con bandas default `20–58 m` y `50–70 m`. El loader aplica márgenes
de solapamiento dentro de esas bandas.

No se inventan rangos por categoría en otro documento. Si una medición exige
variantes, se agregan al catálogo como política explícita y se validan en build.

**Los diales de medición son otra cosa.** `CULL_STEPS` puede apagar árboles a 45
m, o sea antes de que termine la banda de `LOD1`. Eso no es política de LOD: es
un instrumento para saber cuánto cuesta lo que se apagó. Un dial nunca define el
contrato; si un valor de dial resulta ser el correcto, se muda a
`ASSET_PIPELINE.md` como política y deja de ser dial.

### 3. El culling de presentación no cambia simulación

Frustum culling y `VisibilityRange` pueden ocultar mallas; nunca eliminan
colliders, cambian `SurfaceKind` ni alteran un resultado de `FixedUpdate`. El
sensing LOD sí puede espaciar trabajo costoso de IA, pero conserva el último
hecho válido y calcula distancia al jugador más cercano, no a una entidad
singular asumida.

### 4. Compartir material no garantiza batching

Reusar `Handle<Mesh>` y `Handle<StandardMaterial>` reduce variantes y permite a
Bevy agrupar entidades compatibles. El batching exige además geometría, pipeline
y estado compatibles.

La pradera no promete hardware instancing: `BOTWGrass.md` agrupa miles de
briznas en una malla por chunk. Ese diseño paga una entidad/draw por chunk, no
por brizna.

**Y su escalera de LOD no lleva billboards.** Ni cartas de grupo ni shell
texturing: las dos formas que ahorran geometría de verdad necesitan `discard`, y
en un GPU tile-based eso apaga LRZ/FPK/HSR y cuesta más fill del que ahorra en
vértices. La escalera es brizna → menos briznas → terreno teñido, opaca en los
tres peldaños. El detalle y las cuentas, en `BOTWGrass.md`.

**Compartir handle tiene un límite que no es de rendimiento:** un visual cuyo
material se muta en runtime (hit flash, tinte de estado) necesita instancia
propia, o el efecto se propaga a todos los que comparten el handle. Ver
`PARTICLES.md`, ley 7.

### 5. El baseline es opaco y mate

El estilo usa `StandardMaterial`, `perceptual_roughness ≥ 0.8`,
`metallic = 0.0` y reflectancia baja. No hay toon shader, ramp shading ni
outline fullscreen baseline. Las excepciones de alfa pertenecen a
`PARTICLES.md`/`TEXTURES.md` y siempre tienen cota.

Un shader de rim/transmisión vegetal sólo entra como experimento opt-in, medido,
y bajo el documento dueño `BOTWGrass.md`.

**El `discard` cuenta como alfa**, venga de `AlphaMode::Mask` o del dithering.
Son la misma operación con distinto origen del umbral, y las dos apagan el
early-Z del tiler. Hoy hay `discard` vivo en dos lugares del baseline: las hojas
de los árboles Quaternius y la banda de cross-fade de `VisibilityRange`
(`LOD_FADE = 12 m`, `visuals/foliage.rs`). Están permitidos y acotados, pero
**son excepciones, no el default**, y su costo en el target es desconocido.

### 6. Toda optimización conserva una comparación atribuible

Los cambios se prueban A/B con el mismo recorrido, cámara, hora y escena. El
modo de diagnóstico que agrega pases —wireframe u overdraw— nunca contamina la
muestra de rendimiento que pretende explicar.

**Y eso tiene un requisito mecánico:** la matriz A/B de `perf/sequence.rs` opera
sobre `PerfToggles`. **Un dial que no está en `PerfKnob` no entra en la matriz**,
así que no tiene warmup, ni asentamiento, ni chequeo de deriva — lo que produzca
no es atribuible, por construcción. Hoy hay exactamente uno así: el
`KeyCode::F8` que cicla la densidad del pasto en `visuals/grass.rs`. Es también
la única tecla suelta de ajuste visual que sobrevive al "una sola tecla, el
resto es click" del hub. Su reemplazo es el Paso 0 de `BOTWGrass.md`.

### 7. El AA se elige por arquitectura, no por gusto

MSAA es la técnica de antialiasing correcta para el target: un tiler resuelve
las muestras dentro del tile, sin escribirlas a memoria, así que 4× cuesta
mucho menos que en un GPU de escritorio *(tipo b)*. Por eso el perfil móvil usa
`Msaa::Sample4` y el de escritorio `Msaa::Off` (`perf/data.rs`).

Dos consecuencias que hay que tener presentes:

- **La máquina donde se mide no corre la configuración que se apunta.** Ninguna
  medición de escritorio incluye el costo de MSAA, y ninguna incluye su
  beneficio.
- **MSAA no suaviza un borde hecho con `discard`.** Un recorte por alpha-test
  sigue serruchando con MSAA activo salvo que se use alpha-to-coverage. Otra
  razón —independiente del early-Z— para que la silueta viva en la geometría.

---

## Orden de implementación

### Fase 1 — Guardrails y observabilidad

- Validar nombres, LOD contiguos y presupuestos LOD0 en `build.rs`. **Hecho.**
- Mantener un `SceneInventory` con tris, draws y materiales visibles. **Hecho.**
- Exponer el inventario y los diales de comparación en el hub F1. **Casi:**
  falta la densidad del pasto (ley 6).
- Mantener pruebas deterministas del costo estático de escenas declaradas.
  **Hecho** (`perf/budget.rs::static_cost`).
- **Pendiente: que el dial de overdraw dé un número.** Hoy es un mapa de calor
  aditivo que satura alrededor de las 17 capas y no publica ninguna cifra, así
  que responde "¿dónde?" pero no "¿cuánto?" ni "¿mejoró?". Es el instrumento
  principal para el target y es el único que todavía no se puede poner en una
  tabla A/B.

### Fase 2 — LOD y culling visual

- Aplicar las bandas de `ASSET_PIPELINE.md` mediante `VisibilityRange`.
- Usar margen de transición (`LOD_FADE = 12 m`). **No es alpha blend, pero sí es
  `discard`**: Bevy cruza esa banda dithereando. Aceptado como excepción acotada
  (ley 5) y a medir en el target; si resulta caro, la alternativa es una banda
  más angosta o un cambio de LOD sin cruce.
- Acotar la distancia de mallas y shadow casters por separado. **Hecho**
  (`CULL_STEPS`, `SHADOW_CASTER_STEPS`).
- Confirmar que los colliders y la simulación son invariantes ante cada dial.

### Fase 3 — Batching de vegetación

- Reusar mesh/material en proxies de bosque compatibles con instancing.
- Construir la pradera por chunks de geometría, según `BOTWGrass.md`. **Hecho.**
- Mantener hojas y briznas baseline opacas; la silueta vive en geometría.
- Marcar briznas finas como `NotShadowCaster` cuando la medición confirme que
  sus cascadas compran ruido y no profundidad. **Hecho** para el pasto.

### Fase 4 — LOD de sensing y animación

- Sustituir cualquier ancla `Single<Player>` por la distancia mínima a todos los
  jugadores relevantes.
- Espaciar `SenseWorld` de actores lejanos sin borrar el último dato válido.
- Escalar reproducción de locomoción con
  `k_speed_node = V_real / V_autorada_node`, protegido cuando
  `V_autorada_node < 0.05`.
- Medir animación/IK con el presupuesto de `CHARACTER_ANIMATION_IK.md`.

### Fase 5 — Validación de target

**Bloqueada, y conviene decirlo:** no existe build de Android en el repo. Ningún
script, ninguna receta, ninguna corrida. Todo lo que este documento llama
"target" es hoy un razonamiento sobre una arquitectura, no una máquina donde algo
haya corrido. El paso 0 de esta fase es que el juego **arranque** en un teléfono;
recién después tienen sentido los demás:

- Ejecutar un flythrough reproducible con el perfil móvil, warmup excluido y
  secuencia A/B/A con al menos tres muestras por condición.
- Verificar ≤100k tris, ≤100 draws y ≤64 materiales en los puntos críticos.
- Registrar mediana/p95 de frame time; FPS sólo como traducción secundaria.
- Medir el frame **sostenido** tras diez minutos de juego, no el de los primeros
  treinta segundos.
- Jugar el savepoint: una optimización que rompe silueta, lectura o control no se
  acepta aunque reduzca milisegundos.

---

## Fuera de alcance

Occlusion culling complejo, Nanite/virtualized geometry, impostores de alta
memoria, toon shading global y optimizaciones sin una medición A/B atribuible.
