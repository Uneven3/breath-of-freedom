# Ahora — el trabajo presente

Conversación de trabajo entre sesiones y agentes. Presupuesto: **≤500
líneas**; lo cerrado se borra (queda en git), no se acumula. Léelo antes de
continuar; actualízalo tras cada decisión aceptada, checkpoint jugado o
cambio de foco. Reglas en `ARCHITECTURE.md`, visión en `NORTE.md`.

## Cómo trabajar en este repo

- Validación mínima antes de terminar: `cargo fmt` + `cargo clippy
  --all-targets -- -D warnings` + `cargo test`.
- **Medir en `cargo run` (dev), no en release.** Las deps ya compilan en
  `opt-level 3` en dev (`[profile.dev.package."*"]`), y el cuello es GPU, así que
  la diferencia de perfil medida en el punto menos dependiente de la vista es
  0.38 ms contra deltas de 4-12 ms. Release tarda ~9 min por `codegen-units = 1`
  + `lto`; se reserva para validar el número absoluto antes de dar por cumplido
  un objetivo. Correr la secuencia **dos veces** y quedarse con la limpia: la
  primera a veces trae outliers.
- El feeling se valida jugando (checkpoint, §10): lanzar con
  `env -u WAYLAND_DISPLAY DISPLAY=:1 cargo run` en background para el
  usuario; al cerrar la sesión, leer el log filtrando
  `error|panic|took|destroyed` antes de reportar.
- Debug in-game: **F1 abre el hub** (`presentation/debug_ui/`) — canales,
  perillas de render, acciones y la secuencia de medición, todo por click.
  Sobreviven dos teclas: `[`/`]` ciclan clips con el navegador abierto, y **P**
  vuelca el snapshot al log sin abrir un modal sobre lo observado.
- Commits a `main`, mensajes convencionales, sin push sin pedido explícito.

## Estado (2026-07-19)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/
jump/glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con
percepción gradual (melee + arquero), health/muerte/respawn, horse (montar
F8/E, carga con sweep, inmunidad de dueño), espada con combos, arco de dos
fases con carga Bannerlord, Ranger Quaternius femenino + UAL2 (43 clips,
navegador F7), mundo 320×320 con graybox central y bosque. Rendimiento cerrado
2026-07-21 (60 FPS estables, ver arriba).

Auditoría adversarial de arquitectura (2026-07-17): 4 hallazgos reales, 4
corregidos el mismo día (input a PreUpdate, patrón CapacityPending
eliminado, `Perceivable`, test del veto ForbidSprint). 187 tests.

## Cierre de rendimiento (2026-07-21): 13 → 60 FPS estables

En el peor punto del bosque (dentro, al ras del suelo) el frame pasó de ~72 ms
(13 FPS) a **nunca bajar de 60 FPS**, con vsync. El camino, medido con la
secuencia:

1. **Materiales de tronco a `OPAQUE`/single-sided** (`visuals/foliage.rs`): la
   corteza es 100% opaca pero venía `MASK`+`doubleSided`; restaurar early-Z
   sobre el ~70% de la geometría del bosque. Sin cambio visual.
2. **Mapa de sombras 2048→1024 y hojas sin sombra por default**: el costo del
   sol dentro del bosque pasó de dominar el frame a ~2.7 ms. El sol no se apaga,
   se presupuesta.
3. **La decisión de raíz (2026-07-21): el graybox tenía que ser honesto sobre el
   costo.** Los árboles Quaternius (miles de triángulos, hojas alpha-masked)
   eran placeholder que fingían ser baratos y nos daban un número falso. Se
   reemplazaron por **proxies procedurales** (cilindro+copa por familia, mallas
   compartidas e instanciadas) como default; el modelo detallado quedó como tier
   opt-in (`tree-detail`). Esto es lo que llevó el peor caso a 60 estables.
4. **Baseline PBR nativo (2026-07-22):** mundo, pickups y proxies usan un perfil
   mate de `StandardMaterial`; la atmósfera se construye con luz, paleta y
   entorno. Toon y outline fullscreen se descartaron también en desktop: no son
   la dirección visual y el outline era incompatible con el MSAA del perfil móvil.
5. **Bruma atmosférica nativa:** `DistanceFog` lineal no toca los primeros 45 m,
   transiciona hasta 240 m y mezcla como máximo 30%; sigue el color del cielo
   día/noche. Da profundidad sin ocultar navegación ni sumar un pase fullscreen.

Arquitectura que sostiene esto (ver `ARCHITECTURE.md`):

- **El costo es propiedad de la representación, no de la identidad.** `TreeKind`
  resuelve a dos tiers en `VisualCatalog` (proxy barato / escena detallada);
  impostores e instancing se enchufan ahí sin tocar simulación.
- **Watchdog de polígonos** (`visuals/budget.rs`): cuenta triángulos de cada
  malla al cargar y avisa sobre presupuesto. Agnóstico de asset — ya delató que
  el Ranger femenino es igual de pesado (pies: 9172 tris; ver deudas).
- **La atmósfera parte del pipeline estándar.** El baseline comparte
  `StandardMaterial` con actores/assets; no hay pipeline toon ni outline global.

Instrumentación clave que hizo esto medible: secuencia automática con
precalentamiento de pipelines, dos modos de vantage ("aquí" para zonas lentas /
canónico para comparar), detección de movimiento que invalida pasos, y overlay
en pantalla.

## Referencia de rendimiento (cerrado 2026-07-21)

- **Máquina de destino:** AMD Polaris 11 (RX 460/560), 2 GB VRAM, 2016 — low-end
  real. El costo escala con lo que se **ve**, no con el tamaño del mundo (Bevy
  hace frustum culling), mientras la distancia de dibujo esté acotada.
- **Herramientas** (`src/perf/`, hub F1): split CPU/GPU con timestamps; ~11
  perillas A/B por click; secuencia automática (precalienta pipelines, vsync
  off, dos vantages, invalida al moverse, tabla al log con deriva); overlay de
  progreso. Cascadas se fijan al arrancar (`BOF_CASCADES=1..4`): cambiarlas en
  vivo panica la contabilidad de visibilidad de Bevy.
- **Ceguera medida:** el total `gpu:` suma solo spans registrados; los pases de
  sombra usan `info_span!`, no el grabador. "El gpu medido no cambió" **no**
  implica "no es GPU" — indujo un diagnóstico equivocado una vez. Lo no
  instrumentado se mide por A/B.
- **Pendientes de rendimiento** (no urgentes, hay margen): comprimir texturas
  del bosque a BCn/KTX2 (~88 MB RGBA8 hoy); LOD/impostores cuando la densidad
  suba; streaming por chunks para el mundo grande: la costura ya existe en
  `world/layout.rs`.

## Suite de rendimiento (2026-07-23)

Antes de agrandar el juego: instrumentación que diga, siempre, si se aplican las
técnicas correctas. Principio: **el medidor dice *cuándo* una técnica vale la
pena; no se aplican todas siempre** (eso es cargo-culting y frena al dev, no al
juego). Piso objetivo: **móvil gama media ~2021**; arte propio en **Blender**
(low-poly, ver NORTE).

Instrumentación cerrada (2026-07-22): FPS/frame-time y GPU por passes vía
`gpu_pass_costs` (sombras fuera), watchdog de tris por malla, frustum culling
(default de Bevy) y cull por distancia (`VisibilityRange`), 12 perillas A/B, la
sección `scene` del debug (tris/draws/mats/lod_cull, `debug/collect.rs`), las
vistas `wireframe`/`overdraw` en F1, y presupuestos móviles automáticos con
`BOF_PROFILE=mobile` (2 cascadas / shadow 512 / MSAA 4x / cull 70 m). Último
perfil móvil medido: **37.3k tris, 62 draws, 53 mats → "medio", por materiales.**

### Modos de cámara (2026-07-23)

Un solo `Camera3d`; los modos son comportamientos gateados por `CameraMode`
(componente `CameraControl` **en la entidad cámara**, `camera/data.rs`), no
entidades distintas — re-spawnear rompería los `Single<With<Camera3d>>` (discos
sol/luna, park del benchmark, juice). Hechos y probados (`camera/freecam.rs`, 3 tests):

- **Orbit** (gameplay, default): la follow-cam de 3ª persona de siempre; sus
  sistemas corren sólo en este modo.
- **Freecam** (debug, **F3**): vuela desacoplada del jugador (WASD + Space/Ctrl,
  Shift boost, look con hold-RMB que agarra el cursor sólo mientras se sostiene).
  Al entrar adquiere foco modal **multi-dueño** → congela al jugador y suelta el
  cursor reusando la máquina de `input`, con el hub F1 operable encima; al salir
  libera el foco y restaura el grab. **F4** loguea la pose actual como una línea
  `Waypoint {..}` pegable — la mitad de autoría del flythrough.

### Flythrough de perf por tramos (2026-07-23)

Herramienta reproducible para medir *por zona* y trabajar con confianza: correr la
misma ruta hoy y en un mes y comparar peras con peras (`perf/flythrough.rs`, 4 tests).

- **Ruta como constantes** (`ROUTE`): se autorea volando la freecam y capturando
  poses con **F4** (captura→constantes); vive en código, versionada, idéntica entre
  sesiones/máquinas. Hoy sembrada con una ruta placeholder; falta autorear la real.
- **Corre desde el hub** (F1 → "Correr flythrough", `FlythroughRequest`): lap de
  warmup que prima pipelines de toda la ruta, luego lap medido que interpola la
  cámara por cada tramo (`MEASURE_SECS_PER_LEG`) y **acumula por tramo** frame/gpu/
  tris/draws/mats. Reusa el seam de pose (`park_scripted_camera`), `SceneInventory`
  (fresco a 4 Hz) y `gpu_pass_costs`. Restaura toggles al terminar/abortar; guard
  cruzado con el benchmark (uno scriptea la cámara a la vez); overlay muestra el
  tramo en curso.
- **Reporte**: tabla por tramo (frame mean/max, gpu, tris, draws, mats) clasificada
  con el presupuesto móvil (`scene_budget_grade`) y el peor tramo marcado.

Siguiente / diferido:

- **Autorear la ruta canónica real** (jugando, con F4) — la placeholder solo prueba
  el flujo.
- **Modos de gameplay pendientes** (mismo `CameraMode`, reusan spring/boom/
  proyección): 1ª persona, fija tipo Dota (zoom in/out), tipo WoW.
- **Compartir handles de materiales / atacar el "medio"**: recién si el flythrough
  confirma que mats/draws se acercan al presupuesto por zona.
- **Diferido, solo si el profiler lo pide:** impostores (hoy fog+VisibilityRange
  ya cullean lo lejano); compresión de texturas a BCn/KTX2; streaming por chunks;
  **occlusion culling** — el de Bevy es experimental vía **meshlets**, no
  mobile-friendly; confirmado **no implementado** (2026-07-23).

## Cierre del graybox (decisión del usuario, 2026-07-17)

Hecho, probado en conjunto y con rendimiento cerrado (2026-07-21):

- **Ciclo día/noche con identidad por transición** (`world/day_night.rs`):
  amanecer coral/dorado, atardecer magenta/naranja, cielo y ambiente con
  `smoothstep`; luna direccional independiente (400 lux + sombras) y
  ambiente azul nocturno (40) para mantener volumen y navegación. Sol/luna
  cruzan el horizonte sin salto de dirección. Cinco tests enfocados verdes;
  medir en playtest el costo de dos shadow maps.
- **Inventario con UI en capa propia**
  (`presentation/inventory_ui/`): overlay modal con categorías, ocho slots,
  cantidades, arma equipada, durabilidad, detalle y acciones equipar/
  consumir por mouse o teclado. Presentación solo lee; emite mensajes por
  slot que `InventoryPlugin` valida y aplica en `FixedUpdate`. Input posee
  el foco modal, libera el cursor y neutraliza movimiento/cámara/ataque.
  Tras los últimos ajustes (trigger descartado al abrir, acción única por
  frame, swap atómico, queries disjuntas y layout adaptable), `cargo check`
  y `cargo build` pasan limpios usando el build-dir compartido. Suite completa
  verde; la validación de feeling queda subordinada al checkpoint de rendimiento.
- **Mundo 320×320 + bosque Quaternius** (`world/forest.rs`,
  `visuals/forest.rs`): 179 árboles deterministas alrededor de una clearing de
  42 m, camino N/S libre, 15 variantes Common/Pine/Twisted y colliders de
  tronco cilíndricos authored independientes del mesh. `TreeKind` vive en
  mundo; presentación lo resuelve a `Stylized Nature MegaKit` mediante
  `VisualCatalog`. Las raíces visuales cargan como hijos descartables y las
  carpetas vendor quedan intactas.

El inventario de simulación conserva swap/durabilidad, materiales/comida
apilables y pickups mixtos. Equipar inserta/retira `WeaponProfile`; romper
emite `WeaponBrokeMessage`; tecla 4 cicla arma y C usa comida. Pickups
graybox: `SpareClub`, `WoodPile` y `Apple` cerca del spawn.

Queda: repetir el checkpoint tras la optimización y revisar el feeling de
día/noche + inventario + bosque + Ranger; después conseguir locomoción normal
compatible para reemplazar los fallbacks UAL2.

Pendiente sin fecha: mapear clips restantes del player (Jump_*, Sword_*,
Hit_Knockback); checkpoint PBR de paleta/luz/niebla y evaluación de AA.

## Pipeline de assets y personaje — estado cerrado

Recomendación investigada (2026-07-17, fuentes en git): Blender → glTF con
custom properties leídas vía `GltfExtras` de primera parte (sin Blenvy, que
está alpha/estancado); RON solo para datos no-espaciales; USD se ignora
(pipelines AAA). El editor oficial de Bevy se construye sobre BSN (0.19
solo código; archivos `.bsn` futuros) — la inversión Blender/glTF migra
limpio. `world/layout.rs` es la costura donde se enchufa.

Assets Quaternius agregados por el usuario (2026-07-19): biblioteca
intencional de **prototipado**, no arte final ni fuente automática de
colisión. Nature trae glTF y Universal Animation trae GLB; Farm Animals y
Medieval Weapons requieren conversión Blender/FBX→glTF. Los licenses
incluidos de MegaKits/Nature/Universal Animation declaran CC0. Antes de
versionar: recuperar license/procedencia de Farm Animals y Medieval Weapons
(no quedó archivo en sus carpetas). Las dos carpetas Universal Animation son
dos bibliotecas intencionales: **se preservan ambas** y el futuro import debe
identificar su catálogo/version por separado, aunque compartan nombres.

También están `Universal Base Characters[Standard]` y `Modular Character
Outfits - Fantasy[Standard]`. El README del outfit exige combinar Ranger solo
con la cabeza del base (el cuerpo completo clippea). El script reproducible
`tools/build_ranger_candidates.py` repara en memoria los URI `_png.png`
erróneos del vendor, corta por pesos `Head`/`neck_01`, une un único rig y
reduce texturas derivadas a 1024 px. Generó y se inspeccionaron visualmente
`assets/game/characters/ranger_{female,male}.glb` (~16 MB cada uno). Ambos
están registrados en `VisualCatalog`; femenino es el default provisional.

`Prototype.glb` está obsoleto por decisión del usuario y ya no tiene clave,
receta ni ruta en `src/`; no borrarlo sin pedido porque sigue siendo un asset
del worktree. Sus rest poses difieren del Ranger en los 65 huesos (máximo
medido 18 cm; además `head` vs `Head`), así que sus clips no se reutilizan.
El script monta las mallas sobre `UAL2_Standard.glb`, cuyo rig coincide, y
hornea sus 43 clips (combate, escudo, tala, slide, etc.) dentro de cada Ranger.
El subset Standard no trae Walk/Sprint normales: locomoción usa provisionalmente
`Idle_No_Loop` y `Walk_Carry_Loop` (acelerado en sprint). En este host Blender
5.2 exporta pero queda colgado al cerrar por PipeWire; usar
`timeout 120s blender -noaudio --background --factory-startup --python
tools/build_ranger_candidates.py` hasta corregir el entorno.

Validación automática tras bosque/personajes: `cargo check`, `cargo build`,
Clippy `--all-targets -D warnings` y 226 tests pasan usando el build-dir
compartido. Smoke X11 del Ranger regenerado estable: Vulkan, 15 tipos de árbol
y 43 clips cargan sin warning/error/panic. El modelo detallado quedó como tier
opt-in (`tree-detail`); el default es el proxy graybox (ver cierre de rendimiento).

`visuals/catalog.rs` introduce el binding entidad→asset sin invertir capas:
la raíz visual combina `VisualOf(owner)` con `AppearanceBinding { key, slot }`;
`VisualCatalog` resuelve scene + adapter de escala/orientación/pivot. La
simulación conserva identidades semánticas (especie de árbol, tipo de arma,
tipo de escudo), nunca `Handle` ni rutas. El catálogo ya posee recetas para el
Ranger femenino/masculino y 15 árboles; el player consume la receta activa y
dejó de derivar su
escala desde `BodyDimensions`. Al integrar equipo, presentación observará el
arma/escudo semántico equipado y creará raíces `MainHand`/`OffHand`; al
integrar árboles, `TreeKind`/estado seleccionarán una raíz `World`. Cambiar
receta, variante o biblioteca no cambia collider, hurtbox ni hitbox.

### Decisión — colisiones e hitboxes para assets finales (2026-07-19)

Las fuentes públicas de Nintendo confirman el uso amplio de física en BotW,
pero no documentan sus hurtboxes exactas; se toma el *feeling*, no una
implementación supuesta. Hoy un único `Collider` cápsula sirve como cuerpo
sólido y receptor de melee/flechas/carga (`GameLayer::Actor`). El visual ya
es separado; su escala/pivot salieron de `BodyDimensions` y viven en la
receta de presentación, sin convertir todavía ningún asset fuente.

Contrato acordado:

1. **Locomotion body:** cápsula simple y estable, elegida por traversal y
   capacidades, no generada desde el mesh. La forma (`standing/crouched`)
   se separará del envelope semántico (pies, cabeza, radio de soporte) que
   consumen ledges/stairs/ladders. Puede variar por arquetipo; un cambio
   cosmético conserva el perfil y no altera `FixedUpdate`.
2. **Hurtboxes:** primitivas sensoras hijas con `owner` + región, sin
   respuesta física. Posturas (stand/sneak/mounted) cambian desde estado de
   simulación, nunca desde el esqueleto renderizado.
3. **Hitboxes:** sweeps de capacidad fija definidos por arma/ataque y fase
   autoritativa. Si una animación exige precisión, Blender exporta sockets o
   curvas horneadas que el loader convierte a datos puros de simulación.
4. **Mundo/assets:** colisión simplificada y semántica (`climbable`, material,
   etc.) en nodos GLTF propios; nunca trimesh visual automático como default.

Migración incremental antes del primer asset final:

1. Separar layers Body/Hurtbox y agregar vínculo hurtbox→Actor; primero la
   raíz puede conservar el volumen actual para migrar sin cambiar feeling.
2. Migrar melee/flecha/carga a resolver dueño/región y deduplicar por Actor.
3. Separar `LocomotionShapeSet` de `BodyEnvelope`; después importar perfiles
   espaciales y traces de ataque authored fuera del hot path.

Auditoría de salud cerrada (2026-07-22): Projectiles usa pool autoritativo y
crea mesh/trails solo en `Update`; ledgers, shapes y workspaces tienen capacidad
preparada fuera del tick; ground/snap excluyen `GameLayer::Actor`. También se
corrigieron overflow/doble-hit de melee, transacciones destructivas de
inventario, foco modal componible, selección determinista de percepción,
alcance de rigs/LOD y orden de feedback. Los módulos grandes de cámara,
player, ataque, movement, attachments, mounts y projectiles quedaron partidos
por responsabilidad; `time_control` es el único dueño de `Time<Virtual>`.

Tests obligatorios: swap visual no cambia simulación; múltiples hurtboxes dan
un solo hit por ataque; self-hit imposible; sensores no bloquean locomoción;
mounted/sneak tienen política explícita; ningún ledger/cache crece en tick.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **Ranger femenino sobredimensionado para graybox:** el watchdog
  (`visuals/budget.rs`) reporta pies 9172 tris, brazos 3636/3000, cuerpo 2962,
  capucha 2136 — cada pieza sobre el presupuesto de 2000. No es cuello hoy (hay
  uno solo) pero es la misma deuda que tenían los árboles: asset descargado que
  finge ser barato. Necesita LOD o proxy cuando el jugador deje de ser único
  (NPCs con el mismo rig) o cuando se busque techo en móvil.

- **Facciones:** `Perceivable` es un bit; reemplazar por facción cuando
  haya hostilidad entre no-jugadores (animales, aliados).
- **Cortar árboles → madera real:** `Inventory`/`ItemKind::Material` ya
  existen; falta la mecánica de tala en sí (el patrón destructible ya
  existe: `PracticeTarget` + `Health` + reacción del dueño en `world/`).
- **Lock-on de cámara** y **escudo/parry**: siguientes piezas de combate.
- **Durabilidad de arco y de la espada montada:** fuera de alcance del
  inventario — ninguna pasa por un `WeaponDurability` equipable
  (`combat/context.rs::effective_weapon` sustituye la espada por
  `MOUNTED_SWORD` sin tocar Inventory; las flechas son un recurso aparte).
- **`combat::motors::attack::ProposeQuery` requiere `WeaponProfile` no
  opcional:** romper el arma a pie también bloquea el combate montado
  hasta re-equipar (quirk aceptado al agregar durabilidad).
- **Respawn no restaura arma:** si el jugador muere desarmado (arma rota)
  sin repuesto en `Inventory` ni un arma cercana en el mundo, respawnea
  con HP completo pero sin `WeaponProfile` — incapaz de atacar cuerpo a
  cuerpo hasta encontrar otra arma. `player/mod.rs::respawn_on_death` no lo
  toca a propósito hoy (el inventario sobrevive a la muerte); decidir si
  el respawn debe garantizar un arma mínima.
- **`InventorySet` y `MountsSet::PostMove` sin orden explícito entre sí:**
  comparten banda (`.after(SyncAttachments).before(ApplyContext)`) sobre
  componentes hoy disjuntos; el primer feature que cruce ambos dominios
  (alforjas de caballo, loot al desmontar) hereda un orden no declarado.
- **Apilado de comida por igualdad exacta de `f32`:** `ItemKind::Food`
  apila por `PartialEq` derivado; una fuente futura que calcule `heal` en
  runtime (en vez de reusar un const) puede fallar el apilado por
  redondeo.
