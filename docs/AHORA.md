# Ahora — el trabajo presente

Trabajo vivo entre sesiones (≤500 líneas); lo cerrado queda en git. Reglas en
`ARCHITECTURE.md`, visión en `NORTE.md`; visuales en `TEXTURES.md`, `BOTWGrass.md`,
`GraphicalTechniques.md`, `BOTWMovements.md` y `CHARACTER_ANIMATION_IK.md`.
Números crudos de rendimiento, repetidos y con su contexto exacto, en
`GRASS_PERF_DATA.md` — no remedir lo que ya está ahí.

## 2026-08-14 — evaluando volver a Godot para el frontend; el movimiento no está en duda

Nace `../whispers-of-freedom/` (Godot 4.7.1), hermano de este repo en
`~/Programming/uneven/`. **No es un abandono decidido de Bevy** — es una
evaluación en curso, arrancada por la fricción del día: la capa de
instancias del editor F5 (commit `20c52e6`) tardó una sesión entera en
cablear a mano lo que `apply_authored_lod`/`AuthoredVisualRoot` ya resuelven
para árboles, y las dos semanas de pelea con el pasto procedural (`c075c8a`
y anteriores) son la misma fricción de fondo: cero herramientas de motor
para terreno/vegetación/LOD, todo construido a mano acá.

**Lo que NO está en duda: el sistema de movimiento.** `Brain → Broker →
Motors → Body` con arbitraje por prioridad se diseñó y probó originalmente
en Godot (`fbeltran/zelda-druid-godot`, "Druid: Shape-Shifter's Ritual" —
un juego distinto, no éste). Lo que hay en `crates/simulation/src/movement`
es un port directo de ese diseño, afinado acá después. Volver a Godot para
el frontend no reabre esa parte — está resuelta y probada de los dos lados.

**Investigado hoy, con fuentes, no de memoria:** Terrain3D (terreno +
foliage instancing con hasta 10 niveles de LOD integrado, Godot 4.4–4.6+,
reseñas mixtas en rendimiento/export — no un cheque en blanco) y el addon
Blender-Godot Pipeline (colliders/multimesh/materiales desde Blender,
mantenido, actualizado enero 2026) — cubren buena parte de lo que
`build.rs`/`schema.rs` valida a mano en este repo. Godot 4.7.1 está en el
repo `extra` de Arch, al día, ya instalado.

**Estado de `whispers-of-freedom/`:** tiene el `graybox-prototype/` de
Godot completo (motor de movimiento, 55 scripts, 26 escenas) en la raíz, y
`docs/from-breath-of-freedom/` + `docs/from-druid-godot/` con la
documentación de ambos proyectos — sin el sistema de `workflow/`
(stages/personas de IA), que hoy pesa más como bloqueo que como ayuda.

**Todavía no decidido:** si este repo se pausa, se retira, o sigue en
paralelo. La próxima señal es jugable — si Godot destraba terreno/pasto de
verdad, ahí se decide qué pasa con éste.

## Cómo trabajar en este repo

- Validación mínima antes de terminar: `cargo fmt` + `cargo clippy
  --all-targets -- -D warnings` + `cargo test`.
- **Los crates se testean por paquete, no con `--workspace`.** Meter el binario
  en la misma invocación unifica las features de Avian, y el smoke headless de
  `bof_simulation` panickea con esa resolución. La corrida
  buena son tres: `cargo test`, `cargo test -p breath_of_freedom_simulation` y
  `cargo test -p breath_of_freedom_domain`. Con `--workspace` todo lo demás pasa
  y sólo cae ese smoke: si cae otro, es real.
- **Medir: `BOF_BENCH=<suite> cargo run`**, con la ventana visible y sin nada
  más usando la GPU. **Preguntarle al usuario qué tiene abierto antes de la
  primera corrida**: el 2026-08-06 tres barridos dieron 10,89 / 11,88 / 3,83 ms
  para el mismo pasto con Blender y Firefox arriba. Detalle en "La suite de
  medición".
- **Ver sin jugar: `BOF_SHOT=<suite> cargo run`** deja un PNG y su registro RON
  en `target/shots/` y sale; `BOF_SHOT_POSE="x,y,z:dx,dy,dz"` reproduce un
  encuadre y `BOF_KNOBS="grass-view=5,msaa=1"` fija perillas desde el arranque.
  `BOF_SCENE=Pasto cargo run` salta el menú y entra directo a la caja.
  Configuración, pose o escritura inválida terminan con código distinto de cero.
  La estadística de píxeles no la contamina la carga de la máquina, pero tiene
  piso de ruido de **5%** porque el viento mueve las briznas entre disparos.
- **Siempre `cargo run`, nunca `./target/debug/...` a secas.** Bevy busca
  `assets/` junto al ejecutable, así que el binario directo arranca **sin un solo
  shader** y todo lo demás sigue reportando: el 2026-08-07 una corrida así sacó
  una foto de puro cielo declarando 691.200 triángulos de pradera al 95% del
  cuadro. La foto y la tabla ahora avisan, porque el caso ocurrió.
- **Un shader que no compila no dispara ningún guardia — sigue siendo mitigación
  manual (auditado 2026-08-13, sin cerrar a propósito).** `warn_on_broken_assets`
  sólo escucha `UntypedAssetLoadFailedEvent` (fallos de *carga*); un error de
  compilación de pipeline en Bevy 0.19 sólo sale por `error!()` de `tracing`
  (`bevy_render::render_resource::pipeline_cache::process_queue`, verificado
  contra la fuente), sin ningún `Event` enganchable. Existe un hook genérico
  (`LogPlugin::custom_layer`) para contar cualquier `ERROR` de la corrida, pero
  un contador así de amplio marcaría una foto como inválida por un `error!()`
  sin relación con lo que se está fotografiando — cambiaría un guardia tipado
  por ruido de falsos positivos. Se descarta a propósito: **grepear `ERROR`
  además de las líneas `[shot]`** sigue siendo el único chequeo.
- **Contar píxeles: lo hace la misma corrida** (`BOF_KNOBS=grass-view=6`). El
  informe sale en el log junto a la foto: cobertura, reparto por nivel y perfil
  por distancia en metros. **Omite el perfil y dice por qué** cuando la fila de
  pantalla no se puede convertir en distancia (sin perspectiva, con
  `render-scale`, o con más de 20 cm de relieve bajo la mirada).
- **Una curva en una corrida: `BOF_SHOT_SWEEP=<perilla>`** recorre su escalera
  entera, deja una foto por paso e imprime la tabla —fila por paso, columna por
  banda— más el despeje de `C = 1 − e^(−λ·a)` contra la densidad **viva**.
- **Banco hoja/púa/carta: `BOF_SHOT_SWEEP=grass-shape`** (con
  `BOF_KNOBS=grass-view=7` para que cuente). Perilla nueva (2026-08-12): fuerza
  **toda** la corona a una sola forma —no sólo más allá de la hoja— sin tocar
  la densidad de cada forma ni la cámara. `BOF_KNOBS=grass-shape=N` la fija al
  arrancar (0 auto, 1 hoja, 2 púa, 3 carta).
- **Banco de candidatas de carta: `BOF_SHOT_SWEEP=grass-card`** (2026-08-12).
  Las tres texturas que existen —`base`/`legacy`/`v3`— vueltas perilla en vivo:
  recarga el handle sin rehornear la grilla. Combinada con
  `grass-shape=3` ("solo carta") aísla el silueteado de cada PNG del resto.
- **Triángulos por LOD en el reporte** (2026-08-12): toda corrida de
  `BOF_SHOT`/`BOF_SHOT_SWEEP` en una escena con `grass_lab` imprime
  `[shot] pradera por LOD: anillo 0 …tris · anillo 1 … · anillo 2 …` —el mismo
  desglose que F9 muestra jugando, reusando `GrassLabStats` en vez de
  duplicarlo. Detalle en "Banco hoja/púa/carta" más abajo.
- **Antes de explicar una diferencia entre dos configuraciones, sacar la misma
  captura dos veces.** El 2026-08-07 fue lo único que destapó un bug que apagaba
  un nivel entero con resultado distinto en cada corrida, y cada foto suelta
  parecía una configuración con su explicación plausible. La estadística de
  píxeles es determinista salvo el viento (piso 5%), así que **una diferencia
  grande entre dos corridas idénticas es un bug, no ruido** — con el viento
  apagado, el barrido del 2026-08-08 repitió 89 de 90 celdas exactas.
- **F7 en el juego captura lo que el usuario está viendo**, numerada, con la pose
  de cámara impresa ya formateada como `BOF_SHOT_POSE`. Es la única forma de que
  un reporte visual suyo llegue sin pasar por mi interpretación.
- **Medir en dev, no en release**: la diferencia de perfil medida fue 0,38 ms
  contra deltas de 4-12 ms. Release tarda ~9 min y queda para validar un número
  absoluto, o para zanjar si hay un techo de CPU que dev esté inflando.
- El feeling se valida jugando (checkpoint, §10): lanzar **`cargo run` a secas**
  en background para el usuario, redirigiendo a un log; al cerrar la sesión,
  leerlo filtrando `error|panic|took|destroyed` antes de reportar. **No forzar
  X11**: la máquina corre Wayland nativo. Verificado el 2026-07-26 — arranca en
  Vulkan/RADV sobre la Polaris 11.
- Debug in-game: **F1 abre el hub** — canales, perillas de render, acciones y la
  secuencia de medición, todo por click. Sobreviven dos teclas: `[`/`]` ciclan
  clips con el navegador abierto, y **P** vuelca el snapshot al log.
- **El log arranca callado (2026-07-26)**, todos los canales en `off`: el sink de
  consola ponía **208 de 240 líneas** de un playtest. `RUST_LOG` **reemplaza** el
  filtro de Bevy en vez de sumarse, así que a secas devuelve el spam de `wgpu`;
  si hace falta, `RUST_LOG=wgpu=error,naga=warn,breath_of_freedom=debug`.
- **Antes de bindear una tecla, `grep -rhoE "KeyCode::[A-Za-z0-9]+" src/`.**
  Nadie arbitra colisiones (hallazgo C2), así que una tecla ya usada no da error:
  da una función que "no hace nada". **F7 se gastó el 2026-08-06** en la captura
  in-game; quedan **F9, F11, F12**.
- Commits a `main`, mensajes convencionales, sin push sin pedido explícito.
- **El `build-dir` compartido no cumple su premisa:** Cargo cachea por set de
  features y avian3d activa las que otros proyectos no piden — **39 variantes de
  `libbevy_render` y 136 GB** (2026-08-01). Nunca `rm -rf` sobre lo de Cargo.
- **Para listar las ambigüedades de scheduling por nombre**, activá un rato la
  feature `debug` de `bevy_ecs`: sin ella Bevy imprime placeholders y después de
  `Schedule::initialize` el grafo ya no resuelve nombres. Revertila después —
  crea otra variante en el build compartido.

## Cierre del 2026-08-09 — optimización de la pradera, y una alerta para la próxima sesión

**Objetivo:** acercar la pradera al target de `NORTE.md` (900p30), pedido
explícito comparando contra BOTW. Antes de tocar código: `NotShadowCaster`/
`NotShadowReceiver` en `grass.rs` **ya estaban** desde semanas atrás — no hubo
que redescubrirlos.

**Hecho y jugado, en orden:**
1. **Iluminación barata.** El fragment llamaba a `apply_pbr_lighting` —PBR
   completo, luces clusterizadas y sombra— para el 98% de los píxeles del
   cuadro. Reemplazado por difusa direccional + ambiente plano
   (`sun_color`/`ambient_color`, nuevos en `GrassUniform`, alimentados desde
   la luz real). **Baseline GPU 10,53 → 7,11 ms (−32%)**, escala con densidad
   (costo por fragmento, no offset fijo). De paso, bug real no introducido
   hoy: `track_meadow_focus` buscaba el sol sin desambiguar `Sun` de
   `MoonLight` (los dos llevan `DirectionalLight`) — `sun_direction` estuvo
   **congelado desde siempre**. Arreglado con el filtro que ya usa
   `day_night::apply_sun`. **Deuda aceptada:** sin `apply_pbr_lighting`,
   ninguna luz puntual ilumina el pasto — hoy no hay ninguna, así que no
   cuesta nada todavía.
2. **La "V"** (shimmer sólo en movimiento, invisible en captura quieta) se
   diagnosticó como aliasing de geometría sub-píxel con MSAA apagado —
   preexistente, no del cambio de luz. Confirmado jugando: MSAA 2x/4x la
   elimina.
3. **Ancho de brizna** 5,5→5,7 cm en el anillo 0: `minimum_density` pide
   densidad inversamente proporcional al ancho, así que bajó sola, misma
   cobertura al 95%. **−3%**, chico pero real; jugado no se nota. La
   densidad del anillo 0 (332/m²) queda **aceptada, no a revisar** —
   *"esa densidad es la que produce el buen feeling"*.
4. **Cartas reactivadas** (`CARDS_ENABLED = true`) con `AlphaMode::AlphaToCoverage`
   en vez de `Mask`, y **MSAA 2x pasó a ser el default real** (`PerfToggles::default`
   y `BenchmarkStep::baseline`, no sólo `profile_msaa` — ese último sólo fija
   el valor inicial de spawn, `apply_msaa` lo pisa en el primer frame). El
   recorte de silueta dejó el `discard` puro por uno que sólo descarta lejos
   del borde y difumina ~1 px alrededor. Decisión del usuario pese al
   diagnóstico de que el billboard nunca fue un problema de borde duro —
   porque los árboles van a necesitar billboards de todos modos. **Jugado:
   "funciona", pero el anillo lejano sigue leyéndose distinto** — el
   diagnóstico viejo (la carta siempre muestra su ancho completo, a diferencia
   de una brizna real) sigue sin resolverse; se acepta por ahora, con menos
   triángulos que antes (la ley de densidad pide ~4× menos cartas que púas
   para la misma cobertura). Baseline GPU quedó en **~8,16 ms** (sube contra
   el 7,01 más liviano, a cambio de la V arreglada).

**Depth pre-pass: implementado y andando (2026-08-10), retomado tras el
reinicio.** La arquitectura ya estaba confirmada (`DepthPrepass` +
`NormalPrepass`, ver abajo); esta sesión, antes de tocar código, se instaló
`vulkan-validation-layers` (no estaba — Bevy en dev la pide sola si existe, y
sin ella un pipeline mal armado puede no dar un error limpio). Se verificó
además, leyendo `bevy_render::occlusion_culling` (no meshlets — la nota vieja
de "diferido, la Polaris 11 no los soporta" hablaba de otra cosa), que
`OcclusionCulling` de Bevy **exige `DepthPrepass`** para objetos enteros
(no sólo píxeles) — el prerequisito real para cuando entren montañas/piedras.
Implementación: `prepass_vertex_shader`/`prepass_fragment_shader` propios en
`grass.wgsl` (comparten la reconstrucción de la brizna, `build_blade`, con el
vertex principal — nunca pueden divergir), `address`en `location(1)` del
struct de prepass y no `(2)` (confirmado contra el código fuente de Bevy, no
supuesto). Encontrado y arreglado en el camino: un resto muerto de
`deferred_output()` en el fragment que habría fallado al compilar apenas se
activara `prepass_fragment_shader()`. Probado con `cargo build`, luego
`BOF_SHOT`/`BOF_BENCH` con timeout duro antes de involucrar al usuario — sin
cuelgue, sobrevivió el barrido de 11 pasos incluyendo `msaa off`/`msaa 4x`
(reconstrucción de pipeline).

**Medido: el prepass cuesta, no ahorra, en la pradera sola.** A/B en la misma
sesión, mismo `BOF_BENCH=grass`: GPU baseline 9,14 ms sin prepass → 9,99 ms
con. Bevy ya ordena los draws opacos de cerca a lejos, así que el early-Z que
el prepass ofrece ya estaba mayormente cobrado; el vertex de prepass no es
gratis (reconstruye la brizna entera una segunda vez). **Decisión explícita
del usuario: se deja igual.** Es infraestructura para el `OcclusionCulling`
que viene con las montañas/piedras, no una optimización de hoy — pagar el
costo ahora evita repetir esta sesión entera cuando lleguen.

**Lo que sigue sin construirse:** resolución dinámica — sabemos que ahorra
~3 ms (render-scale 50%) pero hoy es sólo una perilla manual, no un sistema
que se ajuste solo. Occlusion culling depende del mismo prepass de arriba.

## Estado (2026-08-04)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/jump/
glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con percepción
gradual (melee + arquero), health/muerte/respawn, horse, espada con combos, arco
de dos fases con carga Bannerlord, cápsula graybox como player, mundo 320×320
con bosque y audio de pasos por superficie. El player es graybox a propósito: no
hay rig ni controlador de animación hasta que exista un personaje propio o con
procedencia compatible.

**CRATES fase 6 cerrada: el juego entero corre sin pantalla.** Tres crates
hermanas (`bof_domain`, `bof_simulation`, y el binario); el smoke headless
levanta el juego completo sin ventana ni GPU.

**Pradera** (ver `BOTWGrass.md`): grilla rodante de **tres niveles, uno por forma
de brizna**, centrada en la cámara; desde el Paso 2 ninguna brizna es geometría
—cada una es un registro de 16 bytes que el vertex shader levanta— y desde el
Paso 3 su posición sale del mundo y no del nivel. Checkpoint tras la auditoría
del 2026-08-09: *"se ve bien en general"*. Reemplazó un intento que afirmaba
"0.0 ms CPU" y "60 FPS estables" mientras medía 35-46. **Regla: ningún número
entra sin salir del medidor.**

**Grass Lab (abierto, 2026-08-12):** `Pasto` es el único contexto de ajuste del
renderer (`SceneDef::authoring.grass_lab`); F1 sigue siendo diagnóstico global.
F9 abre su panel modal: muestra por anillo chunks residentes/frustum y sus
triángulos, y mueve fronteras, umbral de carta y ancho de carta sobre la única
`GrassRendererSettings` que consumen horneado, shader, leyenda y medición. Cada
cambio reconstruye la grilla; no hay valores LOD paralelos ni excepciones por
nombre de escena. El mapa de Pasto declara `TallGrass` en su propio RON: `Soil`
queda tierra desnuda. Esto permite medir, no arregla el relevo por grupos.
**Checkpoint jugado (2026-08-12): cerrado.** F9 abre, sus controles mueven el
campo real y `TallGrass` se ve al pintarlo — *"el laboratorio funciona"*.

## Banco hoja/púa/carta (2026-08-12)

Segunda y tercera herramienta del corte de sesión del 2026-08-11 (la primera
fue Grass Lab, arriba). Pedía comparar las formas **sin confundir asset,
densidad ni cámara**, y de paso —pedido explícito— separar la forma del LOD de
"un anillo" y sumar las tres candidatas de carta que existen en
`assets/textures/props/`, con los triángulos por nivel a la vista.

**Primer intento (retirado el mismo día): mover `spike_min_pixels` a sus
extremos ya legales de F9.** Dejaba el anillo cercano intacto y, medido,
"solo carta" filtraba una franja fina de púa cerca del borde de la hoja —
`leaf_min_pixels` nunca alcanzaba a cubrir el anillo lejano dentro de esos
límites, así que "solo hoja" ni siquiera era alcanzable así.

**Versión actual: saturar los dos umbrales, no nudgearlos.** `shape_at`
(Rust) y `blade_shape_at` (WGSL, vía `spike_from_m`/`card_from_m` en el
uniform) ya derivan la forma de los mismos dos números
(`leaf_min_pixels`/`spike_min_pixels`); llevarlos a pares que los saturan —
`(tiny, tiny)` = hoja, `(huge, tiny)` = púa, `(huge, huge)` = carta, con
`tiny=1e-4`/`huge=1e6` — da una forma pura de punta a punta **sin tocar
`grass.wgsl`**: Rust y el shader siguen clasificando la misma brizna igual
porque saturan la misma fórmula que ya comparten. Medido: cada paso da
100,0% de su color y 0,0% de los otros dos, en toda la pantalla — antes
"solo carta" daba 97,x%/2,x% de púa en el borde.

**Candidata de carta, la tercera que faltaba.** `T_GrassCard_Albedo.png` —la
primera carta del proyecto, descartada para la pradera por alfa binaria y RGB
oculto negro (`BOTWGrass.md` → *Técnica 1*)— se suma como `"legacy"` junto a
`"base"` y `"v3"`. Antes era un env var leído una sola vez al entrar a escena
(`BOF_GRASS_CARD_CANDIDATE`); ahora es `PerfKnob::GrassCardCandidate`
(`grass-card`), recarga el handle en vivo sin rehornear la grilla, y barre en
una corrida como cualquier perilla. Combinada con `grass-shape=3` (solo
carta) mide el silueteado de cada PNG solo: **base 67,7%, legacy 77,2%, v3
73,0%** de la pantalla, misma geometría (271.296 tris) en las tres.

**Triángulos por LOD, en el reporte y no sólo jugando.** `GrassLabStats` ya
lo calculaba cada frame para F9; ahora `BOF_SHOT`/`BOF_SHOT_SWEEP` lo imprime
también (`[shot] pradera por LOD: anillo 0 … · anillo 1 … · anillo 2 …`),
residentes y en frustum, sin sistema nuevo. Confirma en números lo que ya se
sabía: hoja y púa comparten `footprint_m` (mismos triángulos, sólo cambia el
shader), carta cuesta una fracción por su huella más ancha (~270k tris totales
contra ~1,6M de hoja/púa puros, mismo mirador).

**No es conclusión todavía.** Una sola corrida, viento apagado, un solo
mirador (`grass` canónico, no la pose F7 real). Sirve para descartar
experimentos antes de gastar una sesión de juego, no para decidir densidad ni
candidata final.

**F1 o F9, decidido (2026-08-12): F1.** El usuario notó que jugando confundía
las dos — F9 sigue afirmando que sus controles rebakean "la pradera real"
mientras `grass-shape`/`grass-card` (F1) pisan esa misma configuración en
silencio. Se queda en F1 porque `BOF_SHOT_SWEEP` sólo sabe barrer
`PerfKnob`: pasarlas a F9 (mensajes a `GrassRendererSettings`) mataría la
única razón de ser de este banco, medir sin jugador. Lo que sí se arregló es
el silencio: el panel de F9 ahora muestra los valores **efectivos** (ya con el
banco aplicado, no los crudos) y una línea de aviso cuando `grass-shape` o
`grass-card` no están en su neutro — *"pisa lo de abajo, volvé a auto/base
para editar acá"*. Antes mover "Umbral de carta" con el banco activo parecía
no hacer nada; ahora dice por qué.

**F1 pasó a pestañas (2026-08-12).** Pedido explícito: bajar por
Medición/Render/Canales/Terreno enteros para llegar a Pradera era un scroll
gigante. Ahora hay una barra fija (Medición, Render, Pradera, Canales,
Terreno, Acciones) y sólo el panel activo se dibuja/mide/scrollea; "Pradera"
además se esconde donde no hay pradera (`Contents::meadow`) y, si se sale de
esa escena con la pestaña puesta, cae sola a "Render" en vez de dejar un panel
vacío. **Deuda anotada para la próxima sesión: migrar `debug_ui` a
`bevy_feathers`** — ya viene con Bevy 0.19 (`bevy_feathers` +
`bevy_ui_widgets`, hoy sin prender), es su kit de widgets pensado justo para
paneles de herramientas, y no se usó desde el principio por no haberlo
revisado. Esta ronda se quedó con lo artesanal (`presentation/debug_ui/`,
~1250 líneas) porque el problema era de organización, no de widgets — migrar
motor de por medio es la tarea aparte.

**Spike de Feathers (2026-08-12), un solo botón.** Antes de comprometer la
migración completa, un subagente sin contexto revisó el plan y encontró algo
que lo invalidaba tal cual estaba: los widgets de Feathers son
`SceneComponent` — sólo se pueden spawnear con `spawn_scene`/`apply_scene`,
nunca con el `commands.spawn(...).with_children(...)` que usa todo el
proyecto hoy (confirmado leyendo la doc de `bevy_scene` — "spawning them
using `World::spawn` will log an error"). También encontró que prender la
feature `bevy_feathers` es a nivel de workspace compartido con otros juegos
(`beyblade-hitmontop`, `naipes-bevy`, etc.) — aceptado explícitamente por el
usuario, no es un bloqueo. Y que `presentation::theme` tiene **5**
consumidores, no 3: además de `grass_lab`/`debug_ui` están `inventory_ui`,
`editor::hud` y `scene::menu` — el menú principal. Nada de `theme.rs` se
toca en este spike.

Con eso confirmado, se migró **un solo botón** (`CloseButton` del header de
F1) a `bevy_feathers`: `row.spawn(CloseButton).apply_scene(bsn! { @FeathersButton
Children [ (Text("Cerrar (F1)") ThemedText) ] })`, sin tocar el resto del
árbol armado a mano alrededor. Compiló a la primera contra el ejemplo real
que trae el propio crate (`examples/ui/widgets/feathers_counter.rs`, vendido
junto al código fuente). `FeathersPlugins` + `UiTheme(create_dark_theme())`
agregados en `main.rs`. `cargo fmt`/`clippy`/tests, los tres, en verde.

**Prueba el patrón, no reemplaza nada todavía:** confirma que un widget de
Feathers SÍ puede vivir como hijo de un árbol no-BSN vía `apply_scene` sobre
una entidad recién spawneada — la pregunta abierta más grande del plan
original. **Jugado (2026-08-12): el botón se ve bien, pero el primer intento no
cerraba nada al clickear.** Causa real, no hipótesis: `bevy_ui_widgets::Button`
—el widget "headless" que usa Feathers— no llena el `Interaction` clásico
que `handle_clicks` leía por query. Arreglado enganchando el cierre como
observer de `Activate` directo en el `bsn!` del botón (`on(|_activate:
On<Activate>, ...| { super::set_open(...) })`), no desde `handle_clicks` —
ese sistema ya no conoce `CloseButton`, que se borró por no tener más
lectores. **Confirmado jugando de nuevo: cierra.** Esto ya no es hipótesis:
cualquier botón que se migre después necesita el mismo patrón (`on(...)`
inline), no la query de `Interaction` que usa el resto del hub hoy —
mezclar los dos estilos en el mismo sistema (`handle_clicks`) no es viable,
va a haber que decidir, botón por botón migrado, quién lo detecta.

Spike cerrado y validado de punta a punta (visual + click, jugado dos
veces).

## Migración completa de `debug_ui` a Feathers (2026-08-12)

Con el spike probado, se migró **todo** `presentation/debug_ui/` de una vez
(`view.rs` + `hud_menu.rs`; `theme.rs`, `grass_lab.rs` y otros juegos del
workspace siguen fuera de alcance) — pedido explícito del usuario, con la
skill `iterate-safely` (plan → subagente sin contexto → triage → ejecutar) y
sin checkpoint jugado intermedio porque el usuario salió: la validación fue
toda automática más una autoverificación visual mía.

**El subagente de plan encontró dos bugs reales antes de escribir código,
los dos confirmados compilando (no leyendo docs):**
1. `apply_scene` pisa el `Node` entero de la entidad — un botón migrado sin
   recomponer su `Node` dentro del mismo `bsn!` pierde ancho, gaps y
   `justify_content`. Se arregla poniendo `Node {...}` junto a
   `@FeathersButton` en el mismo bloque (mezcla de campos, no reemplazo).
2. Los marcadores de texto que viven dentro de `Children[...]` (`KnobText`,
   `ChannelText`, `TerrainViewText`, `SectionStateText`) necesitan
   `Clone + Default`, y los enums que envuelven (`PerfKnob`, `DebugChannel`,
   `SectionId`) también — agregado en los tres (`crates/domain/src/perf.rs`,
   `crates/domain/src/debug.rs`, `src/debug/channel.rs`), con un comentario
   explicando que el valor por default nunca sobrevive al spawn, sólo
   satisface al macro.

Un segundo subagente (nueva instancia — el primero no se pudo continuar por
un error mío al invocarlo) verificó compilando de verdad, con un arnés
headless propio: composición de `Node` campo a campo, interpolación
`{expr}` de un marcador con datos dentro de `Children`, captura correcta
por iteración en un loop, y que `on(nombre_de_función)` con una función con
nombre (no sólo closures inline) resuelve `On<Activate>::entity` como la
entidad del botón mismo — el mecanismo del que depende toda la migración.

**Patrón final, un observer por *tipo* de botón** (no por instancia): cada
`FeathersButton` lleva `on(activate_x)`, y `activate_x` lee
`On<Activate>::entity` con una `Query<&XButton>` para saber cuál de las N
instancias se clickeó — reemplaza la query centralizada de
`Interaction` que tenía `handle_clicks`/`handle_hud_menu_clicks`, ambas
**eliminadas enteras** porque no les quedó ningún botón que detectar. De
paso, `BenchmarkText` salió: marcador muerto, ningún sistema lo leía.

**Autoverificación visual mía, no sólo `cargo test`.** El primer subagente
ya había avisado que clippy/tests no iban a notar un `Node` roto — cierto:
todo pasaba en verde con el bug #1 sin arreglar. Agregué un hack temporal
(`BOF_DEBUG_UI_OPEN`, forzaba `DebugUiState`/`HudMenuState` abiertos al
arrancar) para sacar capturas con `BOF_SHOT` de las seis pestañas de F1 y
del menú F2, revisé las imágenes, encontré un **tercer bug real** —texto
superpuesto en "Canales": `ThemedText` sólo pone color, no tamaño, y sin un
`TextFont` explícito el default de Feathers es más grande que el `body_font`
del proyecto y las filas apiladas (label + hint) se pisan— lo arreglé
agregando `TextFont { font_size: ... }` en los ~15 sitios migrados, y
repetí la captura para confirmar. El hack se sacó al terminar (`git status`
limpio).

**Validado:** `cargo fmt` / `cargo clippy --all-targets -D warnings` / los
tres `cargo test` en verde, más capturas de las seis pestañas de F1 y de F2.

**Cuarto bug real, encontrado jugando (2026-08-12): scrollear la lista de
una pestaña dejaba de responder a los botones de arriba (cambiar de tab).**
Se ven normales, no reaccionan al click — descartado que fuera sólo visual
(pregunté explícitamente). Un subagente armó un repro compilado real
(picking + scroll + click, con los dos caminos de evento que usa Bevy) y
**no pudo reproducir una falla de picking** con una jerarquía simplificada
— buena noticia a medias: dice que el mecanismo no está roto en general,
pero tampoco explica el síntoma real.

Investigando la jerarquía real encontré la causa: este panel scrolleaba con
un sistema propio que lee la rueda del mouse directo del sistema operativo
— el mismo patrón de "dos caminos de input que no se hablan" que ya había
roto el botón "Cerrar" (`Interaction` vs `Activate`). `DefaultPlugins` **ya
trae activo** el scroll nativo de Feathers (la feature `bevy_feathers`
habilita `bevy_ui_widgets` transitivamente, y ese paquete instala su propio
mecanismo de scroll por rueda — corre sobre el mismo picking que los
botones), sólo faltaba la marca en la entidad para que lo tomara. Se sacó
el sistema propio entero y se agregó la marca nativa a `ScrollPanel`.

**Al arreglarlo, el guardrail C2 (`solo input/ lee hardware crudo`)
encontró una infracción real: mi propio comentario mencionaba el símbolo
del lector de rueda viejo como texto** — la regla es deliberadamente ciega
a si el símbolo está en código o en un comentario (así puede probarse con
texto inventado). Reformulado sin nombrar el símbolo; el guardrail no se
tocó.

**Jugado: el arreglo no funcionó.** El patrón "dos caminos de input" era el
diagnóstico equivocado — el picking seguía roto igual con el scroll nativo
de Feathers. Pregunté qué veía exactamente y su respuesta destrabó todo:
*"hay una sobreposición de los botones que se subieron por el scroll con
los botones para cambiar tabs"*. Confirmado buscando: es una limitación
**del motor**, no de este proyecto — varias issues públicas de Bevy (clip
rects de `bevy_ui` desincronizados con `ScrollPosition` para el picking,
trabajo activo en 2025 sobre esto) describen exactamente esta clase de
bug. El sistema viejo (`Interaction` por polling) resuelve el hit-test por
otro camino y nunca lo tuvo.

**Salida elegida (pedido explícito): que ninguna pestaña necesite
scrollear nunca**, en vez de perseguir el bug del motor. "Render" (14
perillas) y "Pradera" (7) pasaron de lista de una columna a grilla de dos
(`knob_grid`, ancho 48,5% por fila); "Canales" (7, dos líneas cada una)
igual. `ScrollPanel` perdió `ScrollArea`/`ScrollPosition`/el scroll en sí
— ya no hace falta. Casi entra a la primera; probé `Overflow::clip_y()`
como red de seguridad ("si algún día no entra, que se vea cortado y no
que rompa clicks en silencio") pero **el propio clip cortaba contenido
válido** por un problema de timing en su cálculo de altura — confirmado
sacándolo y viendo aparecer la línea que faltaba. Se sacó también; sin
scroll, no hay nada que proteger con un clip.

**Confirmado con capturas las seis pestañas de F1** (con el mismo hack
temporal de antes, revertido al terminar) tras el cambio de grilla: ninguna
corta contenido. `cargo fmt`/`clippy`/tests en verde. **Jugado y aceptado
por el usuario** — la migración completa de `debug_ui` a `bevy_feathers`
queda cerrada por esta sesión.

## Cierre de sesión (2026-08-12)

Resumen de lo construido, en orden: banco hoja/púa/carta de la pradera
(medir sin jugador), banco de candidatas de carta, triángulos por LOD en
`BOF_SHOT`, F1 reorganizado en pestañas por categoría, spike y luego
migración completa de `debug_ui` a `bevy_feathers` (con el bug de
scroll+picking del motor resuelto por diseño, no parcheado). Herramienta
nueva de proceso: la skill `iterate-safely` (global, `~/.claude/skills/`)
— plan propio → crítica de un subagente sin contexto → triage → ejecutar —
usada en las dos decisiones grandes de hoy (qué migrar primero, cómo
migrar el resto) y la que encontró los tres bugs reales antes de que
llegaran a una sesión de juego.

**Deuda para la próxima sesión** (branch `pradera/herramientas-medicion`,
sigue siendo "construir herramientas"): `grass_lab.rs` (F9) y
`presentation/theme.rs` quedaron fuera del alcance de la migración a
Feathers a propósito — `theme.rs` tiene 5 consumidores, dos de ellos UI de
juego real (`inventory_ui`, `scene::menu`), así que migrarlo es una
decisión aparte, no una continuación automática.

*(Cerrado en la sesión siguiente — ver la sección de abajo.)*

## Migración de `grass_lab.rs`, `scene/menu.rs` e `inventory_ui` a Feathers (2026-08-12, sesión siguiente)

Pedido explícito: terminar de migrar lo que había quedado fuera (F9, menú
principal, inventario) y limpiar el código muerto que dejara. Con la skill
`iterate-safely`: plan escrito → subagente sin contexto lo revisó contra el
código y las fuentes reales de `bevy_feathers-0.19.0`/`bevy_ui_widgets-0.19.0`
→ triage → ejecutar.

**El subagente encontró un bug real que el plan no contemplaba, y que ya
afectaba a F1/F2 desde la migración anterior:** `bevy_feathers`/
`bevy_ui_widgets` traen su propio foco de teclado (`bevy_input_focus`),
separado del `ModalInputFocus` del proyecto. Cualquier click sobre un
`FeathersButton` deja esa entidad "enfocada" para el motor — con o sin
`TabGroup` declarado, no hace falta para esto. Cerrar un panel sólo lo
oculta (`Display::None`); nunca despawnea la raíz ni limpia ese foco. Como
Bevy reenvía cada tecla a la entidad enfocada sin mirar si es visible, y el
proyecto no tenía nada tocando `InputFocus`, el último botón clickeado antes
de cerrar un panel seguía "escuchando" Enter/Espacio — y Espacio es Saltar.
Jugar, cerrar el inventario después de equipar algo, y saltar más tarde
podía reenviar el `Activate` de "Equipar" al motor otra vez, en silencio.
El latch `action_sent_this_frame` no lo cubre: protege una carrera dentro
del mismo frame, no un foco que sobrevive frames/minutos. Arreglado en
`src/input/mod.rs::apply_modal_focus_requests`: al liberar el foco modal y
quedar inactivo, además de restaurar el cursor, se limpia
`bevy::input_focus::InputFocus`.

El subagente también encontró que `inventory_ui/view.rs` tenía
`Overflow::scroll_y()` en el panel — el mismo tipo de configuración que
`debug_ui` evitó a propósito por el bug de picking-vs-scroll del motor
(sección de arriba). En la práctica nunca scrolleaba (nadie escribía
`ScrollPosition`, y los 8 slots siempre entran en el 94% de alto), pero
quedaba como una trampa para el día que se sumara un slot. Se sacó.

Y cuestionó si repintar la paleta global de `UiTheme` con los colores del
proyecto era necesario para evitar el choque entre `ButtonVariant`
(automático de Feathers) y escribir `BackgroundColor` a mano — no lo es:
migrar los tres módulos a `ButtonVariant` (en vez de pintar color directo)
ya evita el choque solo, con cualquier tema. Repintar es una decisión de
identidad visual aparte, no una corrección — se hizo de todas formas, al
final, como paso separado (`presentation::theme::feathers_theme`, parcha
`UiTheme` con `PANEL`/`ACCENT`/`ACCENT_DARK`/`ROW_OR_SLOT_BG`/etc. en vez de
dejar el tema oscuro genérico de Bevy), porque dejar el menú principal y el
inventario con gris genérico de Feathers era un cambio de arte real y no
pedido, y porque de paso saca los 4 colores de `theme.rs` que quedaban sin
ningún consumidor tras migrar los `BackgroundColor` manuales a
`ButtonVariant`. Efecto colateral aceptado: esto también repinta F1/F2/F9.

**Mismo patrón que la migración anterior** (`@FeathersButton` + `Node{...}`
compuesto en el mismo bloque `bsn!` + observer por tipo de botón sobre
`On<Activate>`), con dos ajustes de diseño propios de `inventory_ui`: los
estados persistentes (categoría activa, slot seleccionado, acción
disponible) pasan a `ButtonVariant::Primary`/`Normal` en vez de
`BackgroundColor` manual — necesario, no cosmético, porque
`update_button_styles` (el sistema de Feathers) repinta `BackgroundColor`
en cualquier `FeathersButton` cada vez que cambia `Hovered`, y pelearía con
una escritura manual. `BorderColor` en cambio sigue 100% manual (confirmado
leyendo `bevy_feathers::controls::button.rs`: el sistema de Feathers nunca
la toca), así que el resaltado de borde de slot seleccionado no cambió.

**Validado:** `cargo fmt` / `cargo clippy --all-targets -D warnings` / los
tres `cargo test` en verde (187 + 265 + 53). Falta jugar antes de comitear:
menú principal (mouse y dígitos), F9 en escena con pradera, inventario
(mouse y flechas+Enter), y F1/F2 para confirmar que el repintado de paleta
no rompió nada ya aceptado — más el caso puntual del bug de foco: abrir un
panel, clickear algo, cerrarlo, y ahí probar Espacio/Enter jugando.

## Panel de materiales en F1 (pestaña "Materiales", 2026-08-13)

Pedido: seguir construyendo herramientas de medición; el usuario quería poder
*ver* materiales/texturas de forma simple, no sólo leer números. `iterate-safely`
otra vez: plan → crítica → triage → ejecutar.

No hizo falta cómputo nuevo: `debug::material_report::log_material_breakdown`
(la acción "Material breakdown" en Acciones) ya escaneaba las mallas visibles,
agrupaba por "look" cuantizado (color/rugosidad/metalicidad/textura) y lo
tiraba a `info!()`. Se le agregó un segundo sumidero — `MaterialBreakdownSnapshot`,
un `Resource` con hasta `MAX_MATERIAL_ROWS=16` filas ya reducidas a lo que el
panel dibuja (color, mats, meshes, rugosidad, metalicidad, textura) — sin
tocar el escaneo ni su condición de disparo (sigue siendo on-demand, un click,
no continuo). La pestaña nueva reusa el patrón exacto de
`overlay::TerrainLegendRow`/`Swatch`/`Label`: filas fijas spawneadas una vez,
ocultas por índice cuando el snapshot tiene menos looks que el cupo.

**La crítica encontró cuatro cosas reales antes de escribir código:**
1. La función tiene 3 `return` tempranos; sólo 2 debían tocar el snapshot (los
   que corren *después* de confirmar que hubo click). El primero — "sin click
   este frame" — corre casi todos los frames; si tocara el snapshot, el panel
   se habría vaciado un frame después de cada corrida real.
2. El comentario de `ScrollPanel` decía "Deliberately `clip_y`" — desactualizado
   desde que se sacó el clip la sesión pasada (cortaba contenido válido). Se
   corrigió: hoy no hay ningún clip de seguridad, así que un desborde se ve
   roto, no cortado — razón de más para medir antes de confiar en el layout.
3. `terrain_look` no cargaba color (sólo rugosidad/metalicidad/textura); hubo
   que agregar `resolved.base.base_color.to_srgba()` al loop de terreno.
4. La duda del plan entre "queries planas por índice" o navegar `Children`
   ya tenía precedente exacto: `overlay.rs`'s `TerrainLegendRow/Swatch/Label`.
   Se copió tal cual — se evitó decidir de nuevo algo ya resuelto.

**Un quinto bug, no visto por la crítica ni por los tests, lo encontró
correr el juego de verdad**: `sync_material_rows` tenía 4 queries que tocan
`&mut Text` (resumen, omitidas, filas, terreno) sin `Without<>` cruzado entre
ellas — panic de Bevy (`B0001`, queries no comprobadamente disjuntas) apenas
arrancaba, ni bien tocaba ese sistema. `cargo check`/`clippy`/tests no lo
vieron porque el chequeo de aridad de queries es un panic en runtime al
construir el `World`, no un error de tipos. Se arregló con el mismo patrón
que ya usa `inventory_ui::render_inventory` (un alias de filtro `Without<>`
por marcador hermano). **Recordatorio para la próxima vez que algo tenga
varias queries mutables sobre el mismo componente**: `cargo check` en verde
no es suficiente, hay que arrancar el binario al menos una vez.

Verificado con `BOF_SHOT` (hub forzado abierto en Materiales, snapshot
fabricado con las 16 filas + terreno + "3 más omitidas" — el peor caso real,
porque no hay clip que lo salve si no entra): todo cupo sin cortarse. Hack
revertido entero, confirmado con `git status`. `cargo fmt`/`clippy -D
warnings`/tests en verde (190 + 15 + 265 + 53, tres tests nuevos sobre el
recorte a `MAX_MATERIAL_ROWS`). Falta jugar: correr "Material breakdown" en
Acciones y confirmar que la pestaña Materiales muestra colores reales que
coinciden con lo que dice el log.

## El anillo 0 se parte en tiers de buffer (2026-08-13)

`grass_vitality` (sesión anterior) midió que el anillo 0 desperdicia 91,7% de
lo que envía — casi todo chunk lejos del foco reserva el mismo rango de
índices que uno pegado a la cámara, porque todos los chunks de un anillo
comparten una sola malla índice. Dos rondas de crítica (`iterate-safely`)
antes de tocar código: la primera encontró que la asignación de tier tiene
que salir del punto **más cercano** del chunk, no su centro; la segunda
encontró que `chunk_m=8` (la config que parecía mejor en una corrida
exploratoria) casi agotaba `perf::budget::MOBILE_DRAWS` (99 contra el límite
de 100, y subestimado — el conteo de chunks ya no es proxy de draws cuando
cada tier tiene su propio material) y que los límites de tier no pueden ser
metros fijos, porque `AdjustFrontier` mueve el alcance del anillo 0 en vivo
desde F9.

**Implementado con `chunk_m=12` sin cambios (cero riesgo de presupuesto) y 3
tiers**, límites como fracción del alcance vigente, histéresis igual que la
que ya usa el borde de un anillo. `ChunkKey` gana `tier`; `GrassField.records`/
`.materials` pasan a `[[X; MAX_TIERS]; GRASS_RING_COUNT]` — un material por
tier porque un `GrassMaterial` sólo puede tener un buffer bindeado a la vez.
Detalle de diseño completo y los números, en `GRASS_PERF_DATA.md`.

**Medido en vivo, `BOF_SHOT=grass`:** el anillo 0 pasa de 1.107.648 a 357.552
triángulos residentes — **3,1×**, cerca de lo que predijo la corrida
exploratoria. El desperdicio baja de 91,7% a 76,2%, no a cero — esperado, un
chunk sigue necesitando el rango completo si el jugador puede pararse encima
de cualquiera de sus briznas. F9 y `BOF_SHOT` muestran el desglose por tier
del anillo 0, pedido explícito de la sesión.

Validado: `cargo fmt`/`clippy -D warnings`/los tres suites en verde
(202+15+265+53, cinco tests nuevos — el invariante de que ninguna brizna que
debería seguir viva queda sin casillero, histéresis, y que los bordes de tier
siguen al alcance vigente en vez de un valor fijo). **Falta jugar la caja
Pasto**: ningún test puede garantizar al 100% que un borde de tier no dejó un
parche de pasto faltante, y ésa es la clase de bug que ya costó una sesión
entera antes (`vertex_index`, 2026-08-07).

**Jugado esa misma noche: sí había un bug real, encontrado y arreglado.**
Cambiar de tier hoy suelta el chunk viejo en el mismo cuadro pero lo rehornea
recién cuando le toca su turno (`CHUNKS_BAKED_PER_FRAME = 1`) — alcanzaba
para las fronteras de anillo (anchas, raras) pero no para las de tier
(angostas, tres veces más frecuentes en el mismo territorio): varios chunks
cruzando juntos —caminando en línea recta hacia el foco— desaparecían todos
ese cuadro y volvían de a uno. Arreglado: una celda que se suelta por cambio
de tier (no por salir de la grilla) entra a hornear sin esperar el
presupuesto, ese mismo cuadro. Validado en frío otra vez; falta la
confirmación jugada de que el síntoma se fue.

## Sesión de la noche del 2026-08-13: ancho gradual planeado (no ejecutado),
## rampa de crecimiento medida (sin arreglo), F9 gana un control

El usuario se frustró explícitamente por escuchar "eso ya está documentado"
en vez de ver la técnica construida — pedido de cambio de rumbo: actuar, no
anunciar. Se investigaron tres cosas con `/iterate-safely` y datos reales,
mientras dormía:

1. **Ancho gradual por distancia (delgada cerca, ancha lejos), NO ejecutado
   a propósito.** Antes, un experimento de validación (doblar `blade_width_m`
   global, sin gradiente) confirmó que la ley de densidad responde bien —
   mitad de triángulos de toda la pradera, cobertura igual o mejor,
   `BOF_SHOT` medido. Revertido tras validar (quedó sin commitear y sin
   documentar un rato — corregido). El plan de la versión gradual pasó por
   crítica y encontró **dos bugs de corrección reales antes de escribir
   código**: la carta perdería cobertura en silencio si el factor no se gatea
   por forma, y escalar `waist` en el shader corrompería la altura de la
   cintura, no el ancho. Sumado a que está montado sobre el escalonado de
   tiers de esta misma noche, sin confirmar jugado — demasiado riesgo para
   tocar el shader sin que el usuario pueda mirar la pantalla. Plan corregido,
   completo, en `docs/GRASS_PERF_DATA.md`, listo para retomar.
2. **El bug de los chunks apareciendo de golpe (reportado jugando, mismo
   día), diagnosticado con capturas F7 + diff de ImageMagick.** Causa real:
   `growth_ramp` suaviza la muerte de una brizna individual, no el nacimiento
   de un chunk entero — con `chunk_m=64` en el anillo 2, la mayoría de las
   briznas de un chunk recién horneado aparecen a altura completa. Se agregó
   un control en F9 para mover `growth_ramp` en vivo (fuera de
   `GrassRendererSettings` a propósito, para no disparar un rehorneado
   completo por cada click). **Medido, no es el arreglo**: agrandar la rampa
   baja la cobertura en *todas* las bandas, no sólo la lejana — la mayoría de
   las briznas tienen alcance individual corto y pasan más tiempo
   desvaneciéndose. El control queda para explorar en vivo; el arreglo real
   —sospecha: achicar `chunk_m` del anillo 2— sigue sin probarse.
3. **Borde de tier con ruido (Técnica 2, 2026-08-09), implementado y
   apagado por default.** El usuario aclaró que "ameba" no era literal — el
   concepto es ruido en el límite, no una forma nueva. Implementado con la
   misma disciplina que ya costó un bug real esta noche: el ruido sólo puede
   empujar hacia *más* cobertura, nunca menos (`tier_boundary_jitter_m`,
   siempre `<= 0`), con un test que lo barre sobre 900 celdas. Control en F9
   ("Borde de tier: ruido on/off"), apagado por default — con el toggle
   apagado, `BOF_SHOT` da los mismos números exactos que antes, cero
   regresión. **Sin verificar jugando**: si el ruido rompe la lectura
   circular es una pregunta visual, no numérica.

Validado en frío tras cada pieza: `cargo fmt`/`clippy -D warnings`/los tres
suites en verde (206+15+265+53 al cierre). Nada de esto reemplaza que el
usuario juegue la caja Pasto — hay tres cambios de esta noche (el arreglo del
bug de tiers, el control de F9, el ruido de borde) que nunca se vieron en
pantalla.

4. **Jugado a la mañana siguiente: el ruido estaba en el borde equivocado, y
   auditar encontró un bug real distinto.** El usuario reportó que el toggle
   de ruido "no hacía nada" y que seguían viéndose cuadrados aislando un
   anillo. Investigando: el ruido perturba el tier **interno** de ring0
   (diseñado para ser invisible por construcción — sólo cambia cuántos
   índices se someten, nunca lo que se ve), no la frontera **entre anillos**
   (0→1→2), que es la que de verdad cambia densidad y forma de golpe y se lee
   como un círculo. Confirmado con tres `BOF_SHOT` (cada anillo aislado,
   cámara fija, modo normal): campo continuo, sin costuras — el ruido nunca
   tuvo nada visible que romper. Pendiente: mover la Técnica 2 a la frontera
   real de anillos.

   En su lugar se pidió `/iterate-safely` con foco en auditar el código
   completo (`grass.rs`, `grass_tiles.rs`, `grass.wgsl`) buscando fallas
   lógicas — un subagente sin contexto previo encontró que
   `ring0_tier_with_hysteresis` retiene un chunk en su tier hasta 3 m más
   cerca (`KEEP_SLACK_M`) de lo que `tile_ranges` presupuestaba para ese
   tier, dejando sin sus briznas de borde a cualquier chunk retenido cerca de
   una frontera — reproducible en juego normal, sin necesitar el ruido, y
   nunca cubierto por los tests existentes (probaban la asignación limpia,
   no la retenida). Arreglado: el presupuesto de cada tier ahora contempla
   el peor caso que la histéresis realmente permite. Test nuevo
   (`no_tier_boundary_strands_a_blade_retained_by_hysteresis`), los cuatro
   suites en verde (207+15+265+53). Detalle completo en
   `GRASS_PERF_DATA.md`. **Sin verificar jugando** — el bug sólo se
   manifiesta en movimiento, una captura estática no lo prueba.

5. **El ruido se movió a la frontera real de anillos.** Pedido explícito:
   arreglar todo lo que encontró el audit más el error ya conocido del ruido
   mal targeteado. Otra vuelta de `/iterate-safely` — la crítica encontró que
   el plan original habría desperdiciado draws enteros en el borde exterior
   del anillo 2 (más allá de `farthest_reach()`=128 m no vive ninguna brizna
   de toda la pradera, un chunk ahí saldría vacío), dejado un string de F9
   mintiendo ("borde de tier" cuando ahora es de anillo — la misma confusión
   de recién), y agrandado el radio de búsqueda de celdas sin gatear cuando
   el ruido está apagado. Los tres corregidos antes de escribir código.
   `ring_boundary_jitter_m` (signo `>= 0`, opuesto al de los tiers) ahora
   perturba el corte **exterior** de `ring_cells_with_slack` para los
   anillos 0 y 1 — nunca el 2, nunca el `handover` interior (que sigue
   anclado al alcance limpio, así que nunca deja un hueco). El mecanismo
   viejo (ruido en el tier interno, invisible) se eliminó por completo, sin
   dejar código muerto — se reusaron los mismos campos y el mismo botón de
   F9, renombrados. Cuatro tests nuevos, los cuatro suites en verde
   (209+15+265+53). Apagado por default es un no-op algebraico (no sólo
   medido: `ring_boundary_jitter_cap_m` da 0 antes de calcular nada).
   **Sin verificar jugando** — si el ruido activado realmente rompe la
   lectura circular es una pregunta visual. Detalle en `GRASS_PERF_DATA.md`.

6. **Jugado de nuevo, corrección clave: el pop no es del anillo lejano —
   es del cercano y el mediano.** El arreglo de `chunk_m` (abajo) atacaba
   el bug equivocado. Mismo mecanismo de fondo, pero el anillo 0
   (`chunk_m=12`, `reach_m=24`) cruza su frontera todo el tiempo al
   caminar y cada chunk nuevo ocupa gran parte de la pantalla por estar
   cerca — el lejano se cruza rara vez, por eso costó tanto detectarlo
   antes con capturas F7. Confirmado: sólo caminando, nunca parado, con
   `growth_ramp` ya en su default (no un efecto residual de haberla
   bajado a mano).
   - `chunk_m` del anillo 2, 64→48: mitiga (no elimina) el pop lejano ya
     conocido, con margen real de draws medido antes de elegir el valor
     (32, el que daría 4× menos brizna por pop, rompe el presupuesto).
   - **El arreglo real para anillo 0/1**: una segunda dimensión de
     desvanecimiento, por tiempo desde que el chunk nació
     (`total_growth = min(blade_growth, chunk_time_fade)`), independiente
     del desvanecimiento por distancia existente. Un chunk nuevo tarda
     0,35 s en llegar a la altura que le corresponde en vez de aparecer de
     golpe. Nuevo buffer chico por nivel (`chunk_born_at`, un `f32` por
     casillero) y un reloj de uniform sin wrap aparte del que usa el
     viento — reusar el reloj con wrap habría dado una edad negativa
     permanente para un chunk horneado justo antes de una vuelta. Las
     reasignaciones de tier del anillo 0 (histéresis) están exentas por un
     centinela, para no reabrir el pop que la histéresis ya tapa.
   - Un subagente de crítica revirtió sus propios tests de prueba con
     `git checkout --` durante este trabajo, lo que borró **todo** el
     trabajo sin commitear del archivo (no sólo lo suyo). Recuperado sin
     pérdida desde el historial de versiones interno de Claude Code
     (`~/.claude/file-history/`, independiente de git) — confirmado byte a
     byte por los cuatro suites. Lección: pedirle a un subagente que
     revierta con el editor, nunca con git, si hay trabajo sin commitear
     de otra sesión en el árbol.
   - Panel de F9 ensanchado (430→560 px): las líneas de estadística por
     anillo/tier se estaban partiendo por falta de ancho.
   - Validado en frío: los cuatro suites en verde (213+15+265+53). Ningún
     test compila el WGSL — la única verificación de que el shader
     compila es una corrida real (`BOF_SHOT=grass`), hecha dos veces.
     **Sin verificar jugando todavía** si el fundido de 0,35 s alcanza —
     una captura estática no puede mostrarlo. Todo esto commiteado
     (`c075c8a`). Detalle completo en `GRASS_PERF_DATA.md`.

## Escenas: cajas de prueba + mundo

**Las escenas son dato** (`scene::SCENES`): etiqueta, **su propio heightmap** y
qué piezas contiene; el menú se genera de la tabla, así que no puede desviarse de
lo que carga. La mayoría **no son áreas del juego, son cajas de prueba** — el
mundo más chico que permite juzgar *una* cosa mientras la construís. Por eso
`Contents` tiene **un flag por sistema visible**, no por área: "solo el pasto"
tiene que ser expresable o la caja no aísla nada.

| escena | heightmap | piezas |
|---|---|---|
| Traversal | `traversal.ron` | curso + escaleras |
| Combate | `combat.ron` | dianas, pickups, bokobos |
| Pasto | `grass.ron` | solo la pradera |
| Terreno | `sandbox.ron` | ninguna — lienzo limpio y caso de medición |
| Mundo | `world.ron` | todas |

La línea que sostiene todo esto: la **infraestructura** vive todo el proceso
(cámara, paneles, pool de flechas, assets de animación, foco y HUD del editor); el
**contenido de escena** nace y muere con ella, marcado con `DespawnOnExit` para no
mantener un sistema de limpieza sincronizado a mano. **El editor no es una
escena**: F5 esculpe donde estés y `Ctrl+S` escribe en el archivo de esa escena.
**F10** vuelve al menú — no Escape, que ya es de `cursor_control`.

`SceneBuild { Ground, Actors }`: cualquier cosa que se pare sobre el suelo va en
`Actors`, porque en `Ground` el terreno todavía es un comando encolado y **no se
puede leer**. La fase existe desde que el player nacía 6,6 m bajo tierra, y un
heightfield tiene una sola cara: desde abajo la caída no termina nunca.

## La herramienta de mapas

**Autora dato de simulación, no presentación.** En el archivo de nivel no hay un
modelo, ni un material, ni un color: solo significado. Presentación lo lee y
decide cómo se ve (el patrón que ya funciona con `TreeKind` → `VisualCatalog`).
Orden de las capas de autoría, que es también el orden en que se construyen:
**relieve (hecho) → semántica (hecha) → instancias (siguiente)**.

**Dirección (2026-08-11): World Lab dentro de BOF.** Es el modo que permite ver
y editar el mapa con renderer, física y catálogos reales antes de jugarlo;
Blender sigue siendo autor de assets. El RON semántico es canónico y BSN sólo
compone `kind` → presentación. La primera validación vertical pasa a la escena
**Pasto**, que declara el laboratorio de pradera: `Soil` es tierra desnuda y pintar `ShortGrass` o
`TallGrass` debe plantar cobertura en vivo; luego el editor podrá guardar
instancias y volúmenes. Detalle y límites en
`MAP_EDITOR.md`.

Ubicación tras el corte en crates: dato en
`crates/simulation/src/world/terrain.rs`, malla en `src/visuals/terrain.rs`,
autoría en `src/editor/` (`brush` + `paint` + `history` + `persist` + `hud`), y
qué archivo carga cada escena en `src/scene/`.

### Relieve

Heightfield como fuente única de verdad; de esa grilla salen el
`Collider::heightfield` y una malla flat-shaded, sincronizados por change
detection. La navegación **no se hornea**: emerge de la forma + la física.

Seis pinceles con `1..6` —`1` Elevar · `2` Suavizar · `3` Aplanar · `4` Rampa ·
`5` Rugosidad · `6` Terrazas—, elegidos para **hacer mundos que valga la pena
caminar**. MMB suaviza siempre (el borrador de esculpir); rueda = radio,
Shift+rueda o `[`/`]` = fuerza; Ctrl+Z/Y deshacen **por trazo**.

- `Terrain` es dueño del **cómo** cambia la grilla (un método por pincel sobre
  `brush_stroke`, que toma un *segmento* — un círculo es una cápsula de largo
  cero); `editor/` sólo decide **dónde y cuándo**. Un séptimo pincel es un
  método + una fila, nunca un sistema nuevo.
- **La suavidad se arregló en la causa, no en la constante.** Aplicar
  `delta * falloff` a los mismos puntos cada frame *integra la curva de falloff
  en una carpa con pico*; ahora el trazo se relaja a sí mismo mientras sube
  (`RELAX_PER_METRE`). Un test lo fija.

Formato **RON**. Resolución y extent viajan dentro y `apply_ron` **remuestrea en
espacio de mundo**, así cambiar `CELLS` o `WORLD_SIZE` no huerfaniza los niveles.

### Semántica por celda (2026-07-26, jugada y validada)

Se pinta **un significado**, no atributos sueltos: `TerrainKind {Soil,
ShortGrass, TallGrass, Rock, Sand}` y una tabla `KINDS` de la que salen
`surface`, `flammable` y `cuttable` (decisión del usuario — así una celda no
puede ser piedra inflamable, y cambiar qué significa "pasto largo" es una fila,
no repintar los niveles).
**Agua no entra acá**: una laguna necesita altura de superficie, no es propiedad
de una celda.

- **Dos canales en una grilla**: alturas en los `129×129` **vértices** (se
  interpolan), kinds en las `128×128` **celdas** (no). Pintar es **idempotente**
  —sin falloff ni rate, no hay medio camino entre roca y arena— y el borde de un
  parche es la grilla de 2,5 m.
- **Undo es una sola pila** para las dos capas de autoría (`ToolLayer`, **F6**):
  Ctrl+Z es "lo último que hice", no "lo último en el modo en que estoy".
- **Persistencia** run-length en el mismo `.ron`; remuestreo por **vecino más
  cercano** (el camino bilineal habría inventado kinds que nadie pintó), y una
  lista de runs que no suma se **rechaza** en vez de correr el mapa en silencio.
  Medido: 47 runs = 585 bytes contra 105.700 de las alturas.
- **Al terreno se le quitó `Surface`**: uno solo no puede describir 320 m.
- **Pintar no toca la física**: `Terrain` lleva `relief_revision` y el rebuild
  del collider sale temprano si las alturas no se movieron.

Validado jugando (2026-07-26): pintado, `surface=Dirt→Stone→Dirt` bajo los pies,
undo, y guardado en disco. **Falta** la relectura en vivo (F10 → menú → volver).

### Lecciones del relieve que siguen aplicando

- **Resolución 128 celdas (2,5 m/vértice).** Con 64 el pincel cubría 1-2
  vértices y salían carpas puntiagudas. `TerrainVisual` = 32768 tris fijos.
- **El collider se reconstruye en `FixedUpdate`, no en `Update`:** avian sincroniza
  en `FixedPostUpdate`, así que desde `Update` llegaba ≥1 frame tarde y el suelo
  "a veces no colisionaba".
- **`height_at` muestrea el triángulo, no una bilineal.** La superficie bilineal
  se abomba medio metro sobre la triangulación y la depenetración levantaba el
  cuerpo hasta una superficie que no existe.
- **Nada saca a un cuerpo del terreno solo:** `lift_actors_out_of_terrain` corre
  tras los motores. La hipótesis previa (el límite de 60°) era falsa y la captura
  la desmintió.

**Stop-line** (fuera, a propósito): chunks/LOD/streaming, malla adaptativa,
cuevas (= mallas colocadas como instancias, no heightfield), **generación**
procedural del mundo — el pincel de rugosidad es autoría manual, no generación — y
el tuning de wall-climb para pendientes orgánicas, que es tarea de *movimiento*.

## El pasto — el detalle vive en `BOTWGrass.md`

**Norte (2026-08-07):** el feeling de BOTW en low-poly, y **el móvil dejó de ser
un veto** — ver `NORTE.md`. Primero que se vea bien. Target de imagen **900p30**.

> *"Cuando el pasto se vea bien por sí solo, todo lo demás va a caber bien, lo
> cual no es cierto al revés."* — y el contraejemplo de presupuesto: **Flower
> (PS3, 2009)** llena la pantalla con una fracción de este hardware.

**Resuelto y jugado** (detalle en `BOTWGrass.md`): el parpadeo, la altura, la
paleta derivada del suelo, la brizna de dos triángulos, la carta opaca lejana, el
LOD por tamaño en pantalla y **el crecimiento al caminar**.

**Decidido, no a reevaluar (2026-08-07):** *"el pasto siempre debió pertenecer al
mundo"*. La posición sale de una grilla fija del **mundo**; el nivel decide
*cuántas*, nunca *cuáles*. Una medición en contra replantea la implementación,
no el rumbo.

**Jugado y aceptado el 2026-08-08**, tras ocho sesiones de juego suyas en el día:
*"el crecimiento creo que está mucho mejor que antes"* y, al cerrar, *"las
fronteras están bien, ese nunca fue el problema... anillo 0 y 1 están bien"*. La
brizna sale de una grilla del mundo y su alcance del índice; los niveles son tres
—uno por forma— y **coronas**, cada uno dibujando sólo su banda.

**Los billboards, resueltos y jugados (2026-08-09):** *"ahora sí se ve bien, el
problema siempre fueron los billboards."* No era el tamaño (engordar la púa sola
no cambió nada jugado), era la técnica. `CARDS_ENABLED = false` en `grass.rs`
saca la carta de la simulación sin sacarla del código; `true` vuelve a lo de
antes. Detalle en `BOTWGrass.md` → *Tres experimentos del 2026-08-09*.

**La niebla, revertida (2026-08-09):** el intento de esa misma mañana —empujarla
a 40-80 m, tope 70%, para tapar el corte de los 64 m— no funcionó jugado: *"ayer
intentamos tapar con niebla cosas que no estaban bien, y no funcionó"*.
Investigación sobre cómo la usa BOTW (fuentes de análisis técnico por reverse
engineering, no oficiales): reserva la niebla opaca para escenas autoradas
(tormentas de arena, el bosque Korok) y usa un velo apenas perceptible en el
resto, atado al arco de vista, nunca a un borde de LOD puntual. Vuelta a
**45-240 m, tope 30%** — gradual, desacoplada del borde de la pradera.

**El agujero en el suelo, empujado pero no cerrado (2026-08-09, jugado):** el
último nivel llegaba a 64 m y de ahí al horizonte era tierra pelada — anotado
desde el 2026-08-07 con el plan de taparlo con niebla, plan que se probó y no
funcionó. En vez de otra perilla de niebla, el último anillo pasó de **64 a
128 m** (`chunk_m` 32→64 con él). El techo de cordura de la pradera
(`MEADOW_VIEW_TRIANGLES`, guardrail no objetivo) subió de 4 a 5 millones para
admitirlo. Jugado: *"sigo viendo el corte, pero más lejos."* Movió el síntoma,
no lo resolvió — **no seguir empujando el mismo alcance como respuesta**; si
se retoma, es otra técnica, no un número más grande.

**Corte de sesión (2026-08-09): pasa a optimizar, con el corte todavía
abierto.** Pedido explícito del usuario — *"no tiene sentido seguir [con el
pasto] porque hay que hablar de rendimiento, y ahora es donde se pone difícil
la cosa"*. El agujero lejano queda **aparcado, no cerrado**; retomarlo es
trabajo de imagen, posterior a esta fase.

**Los anillos, confirmados como el problema de fondo (2026-08-09/10), con un
plan de tres técnicas en orden.** Una captura con `grass-view=medir` (pedida
por el usuario, *"eso está clarísimo con la herramienta de colores"*) mostró
tres bandas de color planas con borde nítido y horizontal — la corona de cada
`Ring` es geométricamente un círculo alrededor de la cámara, no una
percepción. Comparado contra la tabla de BOTW observado (`BOTWGrass.md`): el
LOD de BOTW también es screen-space-driven (por definición, un círculo
también), pero lo disimula; nosotros no. Plan acordado, en orden, con
checkpoint del usuario entre técnica y técnica:
1. **Mezclar los tres assets** (2 tris / 1 tri / card mesh) para que el card
   mesh se vea igual al pasto de 1 triángulo — en curso, ver abajo.
2. Ruido perturbando la distancia de cada anillo, para romper el círculo.
3. Sesgo de LOD por posición en pantalla (bordes de cámara), más caro/riesgoso
   — al final de la lista a propósito.

**Técnica 1, primer intento con dientes procedurales (2026-08-09), superado
el mismo arco por un segundo incremento (2026-08-10) — auditoría de
consistencia en `AUDIT_GRASS_2026-08-11.md`.** El primer intento
(recorte dentado 7/5→3/2 columnas, `CARD_SILHOUETTE_AREA` sin recalibrar) fue
un paso intermedio, no el estado final: en la misma sesión del 2026-08-10 se
retiró por completo y **ya no existe en el código** (`grass.rs:1450-1481`
tiene un test que falla si el WGSL vuelve a traer ese mecanismo). Lo
reemplazó una carta **texturizada**: laboratorio aislado `card_mesh_lab.rs`
(escena exclusiva, no toca la pradera de producción) para aprobar
`T_GrassMeadowCard_Albedo.png`, adoptada después en `grass.wgsl` — fragment y
prepass muestrean la misma textura, alpha reemplaza la silueta procedural
(discard bajo 0,5), RGB sólo aporta variación de luminosidad. `CARD_WIDTH`
subió 0,25→0,30 m por pedido del checkpoint (16,7% menos densidad de carta
vía `footprint_m`). Detalle completo, con todos los pasos intermedios y sus
checkpoints, en `BOTWGrass.md` → *Técnica 1: mezclar los tres assets*.

**Sigue abierto, no cerrado:** `CARD_SILHOUETTE_AREA` (0,583) es la
calibración de la carta *procedural* vieja, no una medición del PNG —
`grass-view=medir` por anillo/distancia queda pendiente antes de tocar la
escalera de densidad. El PNG no tiene mips todavía (riesgo de shimmer/moiré
en las cartas lejanas — vigilar jugando). Y **todas** las secciones de
`BOTWGrass.md` sobre este incremento están marcadas `(abierto, 2026-08-10)`:
el checkpoint jugado de la carta texturizada en producción (no del
laboratorio) no se cerró formalmente. Técnica 2 y 3 de los anillos esperan
ese cierre — asumen la carta actual como base.

**Técnica 2, experimento retirado y siguiente dirección (2026-08-11,
abierto).** Adelantar la carta, bajar su escala inicial y perturbar el relevo
no quitaron la línea: la carta cercana se leyó como otra textura de suelo. Se
retiraron del runtime esos cambios, incluido el relevo hoja→púa que reabría un
tramo aceptado jugando. El baseline vuelve a 24/40/128 m, 12/16/64 m por chunk,
umbral de carta de 1,5 px y dos triángulos enviados por brizna. La pose F7 real
(`y=4,32`, pitch −0,278) **sí** queda como mirador canónico.

La carta v3 aprobada en `Card mesh` queda como candidata de laboratorio, no en
producción. Su primera medición de huella está abajo; todavía falta la curva que
permita recalibrar densidad y escalera juntas. La siguiente técnica requiere una
identidad de **grupo** estable en mundo: una carta reemplaza las púas de su
propia huella, no una púa 1:1. Sus tests deberán cobrar estabilidad, dueño
complementario, cobertura de chunks y presupuesto; el checkpoint jugado decide
la integración.

**Medición por forma, lista (2026-08-11).** `grass-view=7` (`medir-forma`)
pinta hoja/púa/carta con colores planos y conserva el alpha de la carta, por lo
que `shot_stats` cuenta su cobertura efectiva. F7 canónica, 1920×1072, MSAA 2x
y perfil de suelo plano: asset base, carta 58,4% y total 67,8% en 45–64 m;
candidata v3 (`BOF_GRASS_CARD_CANDIDATE=v3`, sólo la corrida), **65,5%** y
**73,8%**. La mejora no autoriza cambiar `CARD_SILHOUETTE_AREA`: falta medir la
curva completa y recalibrar densidad/escalera juntas. El siguiente paso sigue
siendo el prototipo de grupos, no mover otra vez el alcance.

**Corte de sesión (2026-08-11).** No se prototiparon grupos: el usuario pidió
parar tras dejar baseline, evidencia y candidata reproducibles. Hacen falta
herramientas antes de intentar el feeling final: visualización de dueños y
huellas de grupo, una tabla de cobertura por distancia completa y un banco que
compare púa/card sin confundir asset, densidad ni cámara. No seguir ajustando
radios, alpha o ruido como sustituto de esas herramientas.

**De paso, sin relación con el pasto:** `day_night.rs` ganó
`shadow_casters()` para que sólo una luz direccional (sol o luna, nunca las
dos) castee sombra cerca del horizonte — barrido minuto a minuto en test.
Detalle en `MVPS.md`; alimenta `grass_data.sun_color`/`sun_direction` pero es
un fix de iluminación general, no de la pradera.

## La suite de medición (2026-08-06)

`BOF_BENCH=<suite> cargo run` entra a la escena, espera, mide y cierra. Las suites
en `perf/suite.rs` exigen baseline al inicio/final y un eje por paso;
rechazan escena o inventario ajenos. Cerrar consumidores de GPU y mantener la
ventana visible: Wayland duerme superficies ocultas. Con presentación fija,
leer `d-gpu`.

## Rendimiento: lo que sigue informando decisiones

El feeling precede a la adaptación al target (`NORTE.md`). El graybox debe ser
honesto sobre el costo y `VisualCatalog` mantiene ese costo en la representación,
no en la identidad. Los tiempos GPU sólo suman spans instrumentados: ausencia en
la tabla exige A/B, no una conclusión. En las tres suites el frame resultó
fill-bound; bajar resolución ahorró más GPU que apagar la pradera.

### Presupuesto de polígonos como contrato (2026-07-25)

**Conteos sí, milisegundos no.** Tris/materiales son dato exacto; `draws~` es la
cota inferior determinista de pares `(malla, material)`, no draw calls del
render world. Los tiempos son medición: un test de ms muere por ruido.

- `build.rs` cuenta triángulos por LOD al importar cada GLB; el presupuesto vive
  en `schema.rs::lod0_triangle_budget` y **falla el build nombrando el asset** —
  no un `warn!` que se lee después, que es como pasaron unos pies de 9172 tris.
- Tests de escena (`perf/budget.rs::static_cost`) suman lo que cada fila de
  `SCENES` declara. Es lo que el contador de runtime no puede ser: él grada lo
  que la cámara ve, así que una escena pasada puede leer "bien" desde un rincón.
- **Y las herramientas ya no conocen los materiales por una lista** (2026-08-07).
  El inventario, la vista de overdraw y el desglose de materiales tenían cada uno
  la suya, escrita a mano; `GrassMaterial` faltó en las tres, en dos de ellas
  durante meses. Ahora hay una sola llamada —`add_instrumented_material::<M>()`
  en `visuals::material_registry`— que engancha el render y las herramientas en
  el mismo acto, y una ley de arquitectura prohíbe la puerta de atrás. El
  inventario además **atribuye**: pradera / bosque / terreno / resto, en
  triángulos y batches~, que es el número que cada ajuste del pasto necesitaba.
- El terreno son **32768 tris fijos** en toda escena — un tercio del presupuesto
  móvil antes de poner nada encima. Subir `CELLS` es una decisión de presupuesto.
- Un draw call exacto **no** es testeable sin cámara; lo testeable es la cota.
- **El conteo es guardrail, no objetivo**, y desde el 2026-08-07 los
  presupuestos móviles **no vetan**: el target es destino, no tribunal previo
  (`NORTE.md`). Los conteos siguen porque son testeables. El inventario cuenta
  además los **bytes residentes**, que es el costo que ningún conteo de
  triángulos muestra: la pradera son 26 MB.

### Cámara y flythrough

Un solo `Camera3d`; los modos son comportamientos gateados por `CameraMode` **en
la entidad cámara**, no entidades distintas — re-spawnear rompería los
`Single<With<Camera3d>>`. **Orbit** (gameplay) y **Freecam** (F3; **F4** loguea la
pose como `Waypoint` pegable). El flythrough acumula frame/gpu/tris/draws~/mats
**por tramo**, así correr la misma ruta hoy y en un mes compara peras con peras.

Pendiente: **autorear la ruta canónica real** con F4 — hoy los tramos se llaman
`spawn→clearing` en cajas que no tienen ni claro ni bosque, y una regresión de
~1 ms de GPU pasó dos días inadvertida. El nombre de un tramo es una afirmación
que nada verifica, igual que lo era el mirador del bosque antes de que el
reporte midiera qué hay en cuadro.

Diferido: impostores, streaming por chunks y **occlusion culling** — el de Bevy
es experimental vía meshlets, y la Polaris 11 del dev no los soporta.

## Malla de 1 triángulo/brizna en "solo púa" (2026-08-13)

Reporte del usuario: "la brizna son 2 triángulos, y hay otra de un
triángulo — si cambio a la de uno, el conteo debería bajar a la mitad, y no
baja". `iterate-safely` una vez más.

**Primer error, mío, corregido antes de tocar código**: al investigar
pensé que la forma degenerada (la de "1 triángulo efectivo") era **Hoja**.
Es al revés — releyendo `grass.wgsl::blade_vertex` con calma: en
`SHAPE_SPIKE` las esquinas 2 y 3 devuelven las dos `out.offset = tip`
(mismo punto), así que el segundo triángulo tiene área cero. En la Hoja
(el `else` final) las 4 esquinas —raíz hundida, cintura×2, punta— son
puntos distintos: nada degenera. **Es Púa la de 1 triángulo efectivo, no
Hoja.** El subagente de crítica confirmó esto releyendo el shader por su
cuenta antes de tocar nada — si no lo hubiera hecho, el fix habría ido a
la forma equivocada y el "experimento" habría medido otra cosa.

**Por qué el conteo no bajaba, aunque el shader ya "resolvía" bien**: no
hay tres mallas por forma. Hay una sola malla índice por nivel, siempre
`VERTICES_PER_BLADE=4`, y hasta hoy siempre `SUBMITTED_TRIANGLES_PER_BLADE=2`
sin condicionar a la forma — hoja/púa/carta es una diferencia puramente del
*vertex shader* (dónde pone las esquinas), no de la malla. El segundo
triángulo de la Púa se manda igual aunque no pinte nada. El contador
(`collect_grass_lab_stats`, `log_meadow_lod`, el HUD de F9) lee esa misma
constante fija — nunca midió lo que de verdad se manda por forma.

**El arreglo, acotado a un solo caso seguro**: `grass_records.rs::ring_index_mesh`
ya soportaba `triangles_per_blade=1` (con test propio desde antes), sólo
que nadie lo pedía nunca. Nuevo helper
`submitted_triangles_per_blade(perf)` en `grass.rs`: devuelve 1 sólo si el
banco (`grass-shape`) está en "solo púa", 2 en cualquier otro paso
(incluido juego normal, `"auto"`). Sólo es seguro ahí porque
`shape_bench_settings` satura `leaf_min_pixels`/`spike_min_pixels` **juntos
y sin tabla por anillo** — con el banco en "solo púa" no hay forma de que
sobreviva una hoja en ningún nivel; en juego normal un mismo nivel mezcla
formas por brizna según distancia sobre la misma malla compartida, así que
bajarla ahí le cortaría la cintura real a cualquier brizna-hoja.

Usado en los 4 sitios que antes leían la constante fija: donde se
construye la malla, `collect_grass_lab_stats`, un `debug!` de diagnóstico,
y `ring_facts()` — este último lo encontró el subagente de crítica, no yo;
alimenta el `.ron` que acompaña cada captura de `BOF_SHOT`, así que sin
arreglarlo la evidencia de antes/después habría quedado mintiendo justo en
el experimento que la pide. También descartó (con motivo) guardar el valor
en `RingRecords`: como es el mismo para todos los anillos a la vez cuando
el banco está activo, guardarlo por anillo era complejidad de más — los 4
sitios llaman al helper directo.

**Validado**: test nuevo (`only_solo_spike_drops_the_second_triangle`,
recorre los 4 pasos del banco), `cargo fmt`/`clippy -D warnings`/tests en
verde (191+15+265+53). Jugado por el usuario: confirmó que el conteo ahora
sí baja en "solo púa" — el mecanismo está arreglado.

**Pero el rendimiento sigue mal, y el conteo de triángulos sigue siendo
mucho incluso con el fix.** Palabras del usuario: *"vamos a tener que
cambiar estrategia, y la verdad ya no sé qué hacer"*. No es una conclusión
de esta sesión, es un punto de partida para la próxima: el problema no era
(sólo) que el conteo mintiera — era que el conteo real, aun corregido, ya
es demasiado. La próxima sesión arranca ahí, no desde cero.

## Pipeline authored de assets

El contrato Blender→GLB→Bevy vive en `ASSET_PIPELINE.md`; texturas en
`TEXTURES.md`. El scanner authored genera la autoridad espacial build-time y la
carga hace fallback/swap atómico. `tree_pine_a` espera checkpoint antes de
retirar `Pine_1`. El contrato de clips ya rechaza GLB incompletos, pero el rig
runtime espera un personaje propio; blending e IK viven en sus docs de dominio.

### Colisiones e hitboxes para assets finales (decisión 2026-07-19)

Se toma el *feeling* de BotW, no una implementación supuesta: las fuentes
públicas no documentan sus hurtboxes. Contrato para cuando lleguen los assets:

1. **Locomotion body:** cápsula elegida por traversal, no generada del mesh. La
   forma se separa del envelope semántico (pies, cabeza, radio de soporte) que
   consumen ledges/stairs/ladders — un cambio cosmético no altera `FixedUpdate`.
2. **Hurtboxes:** sensores hijos con `owner` + región, sin respuesta física; las
   posturas cambian desde simulación, nunca desde el esqueleto.
3. **Hitboxes:** sweeps de capacidad fija por arma/ataque y fase autoritativa.
4. **Mundo/assets:** colisión simplificada y semántica en nodos GLTF propios;
   nunca trimesh visual automático.

Antes del primer asset final: separar layers Body/Hurtbox, deduplicar hits por
Actor, y separar `LocomotionShapeSet` de `BodyEnvelope`. Tests obligatorios: el
swap visual no cambia simulación; varias hurtboxes dan un hit por ataque;
self-hit imposible; los sensores no bloquean locomoción; ningún caché crece en el
tick.

## BSN: verificado, y no todavía (§21)

`world/layout.rs` **es** un BSN artesanal. Reverificado en `bevy_scene 0.19` el
2026-08-05: `bsn!`, `SceneComponent` y el parcheo por campo existen y se usan
dentro de Bevy; el formato de archivo `.bsn` **no**, y nada serializa una
`Scene`. Por eso BSN no puede ser el archivo de nivel, y sí la forma correcta de
resolver `kind` → entidades. Plan en `MAP_EDITOR.md`; la prueba barata para
decidir cuándo migrar es reescribir `spawn_stair_segment` como `bsn!` y ver si
queda más legible.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **`perf/shot.rs` va por ~1.000 líneas** (§16). Hace tres cosas: la máquina de la
  captura, la leyenda que se escribe al lado, y el barrido de perillas. El corte
  natural es sacar el barrido, que ya casi no toca a las otras dos.
- **Paleta/IDs Rust↔WGSL sin unificar, y `terrain_material.rs` sin dividir**
  (§1, §16). Viene de la auditoría del 2026-08-02; no bloquea, pero cada color
  nuevo hay que escribirlo en dos lados que nadie obliga a coincidir.
- **`InventorySet` y `MountsSet::PostMove` comparten crate** y su orden mutuo
  sigue sin declarar: es una línea al lado de las otras cuatro.
- **C1 — allocation en `FixedUpdate`:** `rebuild_terrain_collider` arma un
  `Vec<Vec<f32>>` que Avian vuelve a aplanar, ~130 allocations por tick
  esculpido. La vía barata exige `parry` como dep directa.
- **C2 — hardware leído fuera de `input`,** en **12 archivos** (eran 15). El
  test impide que la lista crezca; falta el dueño único que traduzca bindings a
  acciones tipadas.
- **113 pares de sistemas ambiguos en `FixedUpdate`**, auditados uno por uno y
  congelados en `scheduling_audit::FIXED_UPDATE_AMBIGUITIES` (desglose completo
  en el doc-comment de esa constante). Re-verificado 2026-08-08: los 113 están
  explicados, cero acción de código pendiente — el número es un guardrail, no
  una tarea.
- **La pradera es el 92% de los triángulos del frame** y su techo por vista subió
  a 3 millones el 2026-08-08 como deuda declarada —*"olvidémonos del techo por
  ahora"*—. Es fill-bound, así que **la palanca es el overdraw**: un nivel planta
  su tramo en todo su territorio aunque las briznas mueran antes del borde
  (1,5-3,4× según el alcance, medido).
- **`GroundFacts.surface` se publica y nadie la consume.** El sensor la
  resuelve por punto de contacto y el HUD la muestra, pero ningún motor la usa:
  correr sobre arena, roca o pasto largo da exactamente el mismo movimiento.
  Encontrado el 2026-08-04 investigando la montura. Es el enganche natural de
  "la lluvia moja y afecta el agarre" (`NORTE.md`) y de la tracción por
  superficie; la tabla `KINDS` ya tiene dónde colgar el dato. Toca el feeling de
  locomoción, así que no entra sin checkpoint jugado.
- **Audio real:** el paso es un `debug!`; falta cargar `.ogg` y reproducirlo en
  el cue `Step`. Y el timing por **foot-plant**: el acumulador de zancada es un
  stopgap hasta que la animación emita eventos de pisada.
- **Escalar a N enemigos = dato, no código.** El spawn es hardcodeado. Dos
  costuras gemelas *andamiaje→dato*: (a) roster como tabla de arquetipos
  (capacidades + Brain + stats + `AppearanceKey`), (b) visuales de enemigo al
  `VisualCatalog` (hoy cápsula hardcodeada). La señal de que se está torciendo:
  tentación de copiar un cuarto `visuals/enemyN.rs`.
- **Facciones:** `Perceivable` es un bit; reemplazar por facción cuando haya
  hostilidad entre no-jugadores.
- **Cortar árboles → madera real:** `Inventory`/`ItemKind::Material` ya existen;
  falta la tala (el patrón destructible ya existe: `PracticeTarget` + `Health`).
- **Escudo/parry:** siguiente pieza de combate.
- **Durabilidad de arco y espada montada:** ninguna pasa por `WeaponDurability`
  equipable. Y `combat::motors::attack::ProposeQuery` exige `WeaponProfile` no
  opcional, así que romper el arma a pie bloquea el combate montado hasta
  re-equipar (quirk aceptado).
- **Respawn no restaura arma:** morir desarmado sin repuesto deja al jugador
  incapaz de atacar hasta encontrar otra. Decidir si el respawn garantiza un
  arma mínima.
- **Apilado de comida por igualdad exacta de `f32`:** una fuente futura que
  calcule `heal` en runtime puede fallar el apilado por redondeo.
