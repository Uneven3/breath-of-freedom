# Ahora — el trabajo presente

Trabajo vivo entre sesiones (≤500 líneas); lo cerrado queda en git. Reglas en
`ARCHITECTURE.md`, visión en `NORTE.md`; visuales en `TEXTURES.md`, `BOTWGrass.md`,
`GraphicalTechniques.md`, `BOTWMovements.md` y `CHARACTER_ANIMATION_IK.md`.

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
- **Contar píxeles: lo hace la misma corrida** (`BOF_KNOBS=grass-view=6`). El
  informe sale en el log junto a la foto: cobertura, reparto por nivel y perfil
  por distancia en metros. **Omite el perfil y dice por qué** cuando la fila de
  pantalla no se puede convertir en distancia (sin perspectiva, con
  `render-scale`, o con más de 20 cm de relieve bajo la mirada).
- **Una curva en una corrida: `BOF_SHOT_SWEEP=<perilla>`** recorre su escalera
  entera, deja una foto por paso e imprime la tabla —fila por paso, columna por
  banda— más el despeje de `C = 1 − e^(−λ·a)` contra la densidad **viva**.
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

## Foco inmediato (2026-08-09)

Al retomar: **empezar a optimizar la pradera** — pedido explícito al cerrar,
*"el feeling recién se está logrando"*. Primer paso: `BOF_BENCH=grass cargo
run`; el último número conocido (12,94 ms de 15,29) es de antes de los tres
niveles y ya no vale.

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

Se pinta **un significado**, no atributos sueltos: `TerrainKind {Soil, Rock,
TallGrass, Sand}` y una tabla `KINDS` de la que salen `surface`, `flammable` y
`cuttable` (decisión del usuario — así una celda no puede ser piedra inflamable,
y cambiar qué significa "pasto largo" es una fila, no repintar los niveles).
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

**La niebla, empujada con techo a propósito (2026-08-09):** llegaba casi
invisible (~3%) al corte de los 64 m; ahora 40-80 m, tope 70% — *"un poco
mejor"*, jugado. **No seguir subiéndola**: el arco alcanza 120 m y BOTW se apoya
en vistas largas — más niebla deja de ser atmósfera y pasa a límite de dibujado.
Si molesta, vestir el terreno más allá de los 64 m es el arreglo pendiente, no
otra vuelta de perilla.

**Abierto:** optimizar (Foco inmediato, arriba); vestir el terreno más allá de
los 64 m (arriba); y **los anillos como arquitectura**, que siguen sin
convencerlo —dicho tres veces el 2026-08-08— aunque con la carta fuera de la
simulación hoy son dos formas activas, no tres.

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

- **`perf/shot.rs` va por 983 líneas** (§16). Hace tres cosas: la máquina de la
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
