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

## Estado (2026-07-24)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/
jump/glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con
percepción gradual (melee + arquero), health/muerte/respawn, horse (montar
F8/E, carga con sweep, inmunidad de dueño), espada con combos, arco de dos
fases con carga Bannerlord, pradera densa de hierba estilo BOTW (Pasos 1-9
completados: Macro-Chunking ~49 baldosas en 0.0ms CPU, AABB +1.5m, 45 matojos/chunk,
muestreo topológico CPU de pendientes con filtrado >45°, arquitectura de doble hijo
pre-instanciado 3D/2D hasta 48m con billboarding Y-axis y mascarilla alpha en FoliageCard a 60 FPS estables),
maniquí UAL1 como player, mundo 320×320 con bosque, audio de pasos por superficie.

Auditoría adversarial de arquitectura (2026-07-17): 4 hallazgos reales, 4
corregidos el mismo día (input a PreUpdate, patrón CapacityPending
eliminado, `Perceivable`, test del veto ForbidSprint).

Audit de calidad (2026-07-24): sin errores ECS, panics en runtime ni
referencias huérfanas; la única redundancia real (tres `despawn_orphaned_*_visual`
idénticas) unificada en un sistema genérico sobre `VisualOf`, acotado a las
familias graybox con un `Or` para no tocar catálogo ni flechas pooled. Capas
datos/simulación/presentación limpias (§20). 333 tests.

## Escenas: cajas contenidas + mundo (2026-07-25)

Motivo (decisión del usuario): **para hacer bien la herramienta hay que poder
empezar una escena desde 0.** Todo el mundo nacía en `Startup` y vivía para
siempre, así que esculpir era esculpir *debajo* de un bosque que `main.rs`
spawneaba igual, y un `terrain.json` nunca era "el nivel".

La línea que ahora existe (`src/scene/`, `AppState { MainMenu, Scene(SceneId) }`):

- **Infraestructura**, vive todo el proceso: cámara, paneles de UI, pool de
  flechas, assets de animación, y el foco + HUD del editor.
- **Contenido de escena**, nace y muere con ella: terreno, cielo, sol/luna,
  layout graybox, bosque, pradera, player. Marcado con `DespawnOnExit`, así que
  salir del estado lo borra sin un sistema de limpieza que mantener sincronizado.

**Las escenas son dato** (`scene::SCENES`): una fila con etiqueta, **su propio
heightmap** y qué piezas contiene. El menú se genera de la tabla, así que no
puede desviarse de lo que carga.

Aclaración del usuario (2026-07-25): la mayoría **no son áreas del juego, son
cajas de prueba** — el mundo más chico que permite juzgar *una* cosa mientras la
construís, sin que el resto del juego discuta. "Estoy haciendo pasto, quiero
probarlo aislado antes de meterlo al principal". Las cajas van y vienen con el
trabajo: agregar una es una variante + una fila; borrarla cuando la feature
aterriza son las mismas dos líneas al revés.

Por eso `Contents` tiene **un flag por sistema visible** (curso, escaleras,
dianas, pickups, bosque, pradera, enemigos, caballo), no por área: "solo el
pasto" tiene que ser expresable o la caja no aísla nada.

| escena | heightmap | piezas |
|---|---|---|
| Traversal | `traversal.ron` | curso + escaleras |
| Combate | `combat.ron` | dianas, pickups, bokobos |
| Pasto | `grass.ron` | solo la pradera |
| Terreno | `sandbox.ron` | ninguna — lienzo limpio y caso de medición |
| Mundo | `world.ron` | todas |

Cada escena tiene su heightmap, así esculpir una caja no toca otra. **El editor
no es una escena**: F5 esculpe donde estés y `Ctrl+S` escribe en el archivo de
esa escena. `setup_world` quedó partido en `setup_sky` (todas) +
`setup_course`/`setup_stairs`/`setup_targets`/`setup_pickups`/`setup_forest`,
cada uno gateado por su flag; enemigos y caballo entran por el mismo mensaje que
ya usaba el hub F1, no por un segundo camino de spawn. **F10** vuelve al menú —
no Escape, que ya es de `cursor_control`. Tres tests fijan la tabla: la caja de
pasto no puede ganar una segunda pieza, el mundo las junta todas, y ninguna
escena comparte archivo de terreno con otra.

Dos agujeros que el refactor abría, cerrados: salir de una escena **apaga el
modo esculpir** (su dueño de foco modal es infraestructura, habría quedado
congelando el input de la escena siguiente) y **vacía el historial** (todas las
grillas miden igual, así que un Ctrl+Z habría pegado el suelo de otra escena).

## Herramienta de terreno — relieve (en construcción, 2026-07-24)

El mundo pasa de piso plano a **datos**: el relieve nace de un heightfield
(grilla de alturas) que es la fuente única de verdad, en `world` (§datos-en-el-
mundo). De esa grilla se derivan dos representaciones: el `Collider::heightfield`
(simulación, barato) y una malla flat-shaded (presentación), sincronizadas por
change detection. La navegación (caminar/galopar/trepar/saltar/**morir por
caída**) **no se hornea**: emerge de la forma + la física existente. Solo lo
**no geométrico** se pintará como dato después (superficie, pasto largo,
quemable/cortable) — esa es la *próxima* herramienta, no esta.

Ubicación acordada: dato en `world/terrain.rs`, malla en `visuals/terrain.rs`,
y el editor de esculpir en un módulo `editor/` propio (casa de toda autoría
futura: relieve → semántica → instancias).

Rebanadas **[todas HECHAS]**: `Terrain` + heightfield collider + malla
flat-shaded reemplazando el box `Floor`; modo esculpir en `editor/` (F5, pick por
raycast, gizmo de anillo); suavizado y radio por rueda; y **persistencia**: el
archivo **es** el nivel (`Ctrl+S` escribe, `Ctrl+L` recarga, `setup_terrain` lo
carga al entrar).

Formato **RON** (decisión del usuario 2026-07-25: RON para todo el workspace;
`ron = "0.12"` en las deps compartidas — los otros proyectos siguen en JSON,
migrarlos es tarea aparte). Pretty salvo los arrays, para que la cabecera se lea
sin que las 16k alturas ocupen 16k líneas. Resolución y extent viajan dentro y
`apply_ron` **remuestrea bilineal** si difieren, así cambiar `CELLS` mañana no
huerfaniza los niveles de hoy; un test fija el formato en disco contra un archivo
escrito a mano, para que tocar el serializador no los huerfanice tampoco.

### Set de pinceles (2026-07-25, sin jugar todavía)

Decisión: la herramienta no es para mover tierra, es para **hacer mundos que
valga la pena caminar**. De ahí el set — formar, calmar, tener dónde pararse,
**conectar** dos niveles para que sean transitables, ensuciar lo que se ve a
CAD, y escalonar una ladera para que sea un lugar. Seis pinceles con `1..6`:

`1` Elevar · `2` Suavizar · `3` Aplanar · `4` Rampa · `5` Rugosidad · `6` Terrazas

- **MMB suaviza siempre**, sin cambiar de modo: es el borrador de esculpir.
- Rueda = radio; **Shift+rueda o `[`/`]` = fuerza**. **Ctrl+Z/Y deshacen por
  trazo**, no por frame — un arrastre de dos segundos es una sola cosa que
  hiciste (32 trazos, ~2 MB; más barato que la contabilidad de un diff).
- Rampa se arrastra: ancla al presionar y tiende pendiente recta hasta el cursor.
  Aplanar nivela al alto del ancla. Rugosidad usa value noise a 12 m sobre el
  mismo hash del bosque — determinista por posición, sin crate de ruido nuevo.

**La suavidad se arregló en la causa, no en la constante.** El pincel agresivo
no era `RAISE_RATE`: aplicar `delta * falloff` a los mismos puntos cada frame
*integra la curva de falloff en una carpa con pico*. Ahora el trazo se relaja a
sí mismo mientras sube (`RELAX_PER_METRE`), que es lo que mantiene la cúpula
redonda mientras crece. Hay un test que lo fija: 120 aplicaciones deben quedar
más redondas que el mismo trazo sin relajación.

Arquitectura: `Terrain` es dueño del **cómo** cambia la grilla (un método por
pincel sobre `brush_stroke`, que toma un *segmento* — un círculo es una cápsula
de largo cero, y por eso rampa comparte traversal con los radiales); `editor/`
solo decide **dónde y cuándo**. Un séptimo pincel es un método + una fila en
`BrushKind`, nunca un sistema nuevo. `editor/` = `brush`+`history`+`persist`+`hud`.

Medido jugando en la escena limpia (2026-07-25, 5 min, exit 0, sin panics):
**esculpir cuesta, el relieve no.** Con 3 draws / 46.5k tris en pantalla: editor
apagado sobre terreno ya esculpido = **59-61 FPS clavados**; editor encendido y
pintando = 36-55; gpu constante ~4.4 ms en ambos. Esto **corrige** el
diagnóstico del 2026-07-24 (se había apuntado a las sombras del relieve). El
costo es CPU en el camino pincel→malla, y con la escena vacía no hay dónde
esconderse. Dos culpables **[ARREGLADOS]**: `sync_terrain_visual` minteaba un
asset de 32768 tris cada frame editado (ahora escribe sobre el existente con
`get_mut`), y `brush_stroke` recorría los 16641 puntos para tocar ~100 (ahora
solo la ventana alcanzable, `Terrain::window`, con dos tests que fijan que
acotar no recorta el radio ni se sale en las esquinas).

**[PENDIENTE]** `rebuild_terrain_collider` rehace el heightfield de parry entero
cada tick editado (~130 allocations: `Vec<Vec<f32>>` que Avian vuelve a aplanar).
Medir si se nota antes de tocarlo; la vía barata exige `parry` como dep directa.
Lo mismo el trabajo por frame de trazo, que sigue siendo el grueso: 3 MB de
atributos de malla + un `clone()` de 66 KB de la grilla por aplicación (dos en
`raise_area`, que relaja mientras sube). Sin medir todavía si alcanza para 60.

### Errores de la herramienta corregidos (2026-07-25, sin jugar todavía)

Auditados a pedido del usuario, `visuals`/pasto excluido (lo lleva otro agente).

- **El pincel mordía a través de la UI y de la freecam.** El puntero es
  compartido (hub e inventario se operan por click, la freecam gira con RMB) y el
  pincel leía `ButtonInput` crudo: un click en el hub cavaba un cráter detrás del
  panel. Ahora `ModalInputFocus` responde *de quién* es el click (`owns_pointer`
  / `is_held_by`): con un panel abierto el pincel calla y el gizmo se esconde;
  **con la freecam sí se esculpe** (decisión del usuario: autorear una montaña no
  debe exigir caminar hasta ella), pero ahí el RMB mira y no baja. Perder el
  puntero **cierra** el trazo.
- **Ctrl+Z a mitad de arrastre** archivaba de vuelta el estado deshecho al soltar
  el botón; se ignora mientras hay trazo en curso (`is_stroking`).
- **`apply_ron` ignoraba el `extent` del archivo**: un nivel de 160 m en un mundo
  de 320 m se estiraba en silencio, partiendo sus pendientes al medio. El
  remuestreo pasa ahora **por espacio de mundo**; además rechaza NaN/∞ (un NaN
  llega a parry como vértice y rompe todo contacto sin un solo error) y clampea
  al guard band.
- **`Ctrl+S` ahora es atómico** (`.ron.tmp` + rename): el archivo *es* el nivel y
  un corte a mitad de escritura se llevaba la sesión que describía. Y un `Ctrl+L`
  fallido ya no deja un paso de undo espurio.

Fuera de alcance por pedido: `visuals/grass.rs` incumple `clippy -D warnings`
(`radius` sin usar) y sus presets de radio no se aplican — `spawn_grass_density`
usa un cuadrado fijo de 48 m. **La build no cierra bajo §13 hasta que se
arregle.**

**Caerse al vacío al cargar un nivel (2026-07-25, encontrado jugando):** el
terreno guardado tenía 8.10 m sobre el punto de spawn y `PLAYER_SPAWN` era la
constante `(0, 1.5, 0)` de cuando el piso era plano — el player nacía **6.6 m
bajo tierra**, y un heightfield es una superficie de una cara: desde abajo no
frena, así que la caída no termina nunca. Arreglado en la causa: se autorea
**solo el XZ** (`PLAYER_SPAWN_XZ`) y la altura sale de `Terrain::height_at`
(bilineal); igual para el respawn por muerte. Como el player necesita que el
suelo ya exista, `scene::SceneBuild { Ground, Actors }` ordena las fases de
`OnEnter` — cualquier cosa que se pare sobre el suelo va en `Actors`.

**[DEUDA] El mismo problema tiene el graybox de `Playing`:** cajas, escaleras,
pickups y dianas llevan `y` autorada de cuando el piso era plano. Sobre relieve
quedarán enterrados o flotando. No se toca hasta que haga falta jugar `Playing`
sobre terreno esculpido.

Sin verificar todavía: **el pincel se siente suave** (solo lo sabe quien juega),
la **carga del nivel al arrancar** (ya hay `terrain.json` guardado con 1116
vértices movidos, hasta 9.91 m), y el **ida y vuelta entre escenas**, que es
donde saldría una entidad sin marcar con `DespawnOnExit`.

Aprendido jugando (2026-07-25):
- **Resolución** subida 64→128 celdas (2.5m/vértice): con 64 el pincel cubría
  1-2 vértices y salían carpas puntiagudas. `TerrainVisual` ahora 32768 tris.
- **Collider "a veces no colisionaba" era timing, no polígonos:** avian
  sincroniza en `FixedPostUpdate`, el rebuild estaba en `Update` (≥1 frame de
  retraso). Movido a `FixedUpdate` → colisiona en el mismo tick.
- **Entrar al suelo: resuelto, y la hipótesis previa era falsa.** Se había
  culpado al límite de 60° del `ground_service`; la captura del usuario lo
  desmintió — `grounded=ON` y `slope_ok=ON` **con el cuerpo dentro**. Lo real:
  nada sacaba al cuerpo una vez adentro (el probe encuentra superficie ahí mismo
  y lo reporta cómodamente apoyado). `lift_actors_out_of_terrain` corre tras los
  motores y lo sube si quedó bajo `height_at`; lee la forma del collider vigente,
  así que vale de pie o agachado. 3 tests.
- **[NO ES BUG] "El pasto no suena":** el audio de pasos es un `debug!` stopgap y
  `debug` está apagado en `cargo run`. Falta cargar `.ogg` y reproducirlo en el
  cue `Step` (`sfx/mod.rs::play_audio_cues`).
- **Flotar sobre el relieve (introducido y corregido el 2026-07-25):**
  `height_at` interpolaba **bilineal**, pero el collider y la malla son dos
  triángulos planos por celda; la superficie bilineal se abomba por encima de esa
  triangulación dentro del quad (medio metro en celdas de 2.5 m con relieve real),
  así que la depenetración levantaba el cuerpo hasta una superficie que no existe.
  Ahora muestrea el **triángulo** (la diagonal `(row,col)→(row+1,col+1)` coincide
  entre malla y parry, verificado). 2 tests.

Stop-line (fuera, diferido a propósito): capas semánticas, chunks/LOD/streaming,
malla adaptativa, texture splatting, cuevas/instancias, **generación** procedural
del mundo (el pincel de rugosidad es autoría manual, no generación), y el tuning
de wall-climb para pendientes orgánicas (tarea de *movimiento*, aparte). Cuevas =
mallas colocadas como instancias, no heightfield.

## Tipografía de la UI (2026-07-25, sin jugar todavía)

**La fuente por defecto de Bevy tiene 95 glifos** (`FiraMono-subset.ttf`): ASCII
y nada más, ni `ó`. Todo acento y todo `·`, `—`, `→` del HUD en español salía
como caja vacía. Bevy 0.19 tampoco cae a fuentes del sistema salvo que se active
la feature `system_font_discovery`, que no sirve para distribuir.

- **Cuerpo: Fira Sans** (OFL, 404 KB), *compilada* en el binario y escrita sobre
  el handle de fuente por defecto en `ThemePlugin::build` — así todo `TextFont`
  del proyecto la usa sin tocar un solo call site, y no existe un primer frame
  sin fuente. Hermana de la FiraMono que Bevy traía: la UI no cambia de carácter.
- **Iconos: Symbols Nerd Font** (MIT) y **emoji a color: Noto Color Emoji** (OFL),
  como assets (2.4 + 10.7 MB, fuera del binario). Se usan por *nombre de familia*
  en un span propio: `theme::icon_font()` / `theme::emoji_font()`. Bevy sí
  rasteriza emoji a color (swash `ColorBitmap` → atlas RGBA), y por eso ignoran
  `TextColor`. Nerd Font ≠ emoji: son iconos monocromos del área de uso privado.
- Tres tests: cobertura del cuerpo contra los caracteres que la UI escribe, los
  nombres de familia contra los archivos, y que Parley registra ambas familias
  (un nombre mal escrito falla **en silencio**, cayendo a otra tipografía).

## Cierre de rendimiento (2026-07-21): 13 → 60 FPS estables

El peor punto del bosque pasó de ~72 ms a nunca bajar de 60, con vsync. El
detalle está en git; lo que sigue informando decisiones:

- **La decisión de raíz: el graybox tenía que ser honesto sobre el costo.** Los
  árboles Quaternius fingían ser baratos y daban un número falso; se
  reemplazaron por **proxies procedurales** instanciados, con el modelo detallado
  como tier opt-in (`tree-detail`). Lo demás fue presupuestar: troncos `OPAQUE`
  (early-Z sobre el 70% del bosque), sombras 2048→1024, hojas sin sombra.
- **El costo es propiedad de la representación, no de la identidad.** `TreeKind`
  resuelve a dos tiers en `VisualCatalog`; impostores e instancing se enchufan
  ahí sin tocar simulación. El **watchdog de polígonos** (`visuals/budget.rs`)
  cuenta tris al cargar y avisa: así delató al Ranger (pies: 9172 tris).
- **La atmósfera parte del pipeline estándar**: PBR mate + `DistanceFog` lineal
  (45→240 m, ≤30%, sigue el cielo). Toon y outline descartados — no es la
  dirección visual, y el outline chocaba con el MSAA del perfil móvil.

## Referencia de rendimiento (cerrado 2026-07-21)

- **Máquina de destino:** AMD Polaris 11 (RX 460/560), 2 GB VRAM, 2016 — low-end
  real. El costo escala con lo que se **ve**, no con el tamaño del mundo (Bevy
  hace frustum culling), mientras la distancia de dibujo esté acotada.
- **Herramientas** (`src/perf/`, hub F1): split CPU/GPU, ~11 perillas A/B,
  secuencia automática (precalienta pipelines, dos vantages, invalida al
  moverse). Cascadas se fijan al arrancar (`BOF_CASCADES=1..4`): cambiarlas en
  vivo panica la contabilidad de visibilidad de Bevy.
- **Ceguera medida:** el total `gpu:` suma solo spans registrados; los pases de
  sombra usan `info_span!`, no el grabador. "El gpu medido no cambió" **no**
  implica "no es GPU" — indujo un diagnóstico equivocado una vez. Lo no
  instrumentado se mide por A/B. **Corolario aprendido el 2026-07-25:** la
  precaución también corta al revés — invocar esa ceguera para culpar a las
  sombras fue el segundo diagnóstico equivocado. Lo que zanjó la duda fue
  *quitar la escena*: con 3 draws en pantalla no hay dónde esconderse.
- **Pendientes de rendimiento** (no urgentes, hay margen): comprimir texturas
  del bosque a BCn/KTX2 (~88 MB RGBA8 hoy); LOD/impostores cuando la densidad
  suba; streaming por chunks para el mundo grande: la costura ya existe en
  `world/layout.rs`.

## Suite de rendimiento (2026-07-23)

Principio: **el medidor dice *cuándo* una técnica vale la pena; no se aplican
todas siempre** (eso es cargo-culting y frena al dev, no al juego). Piso
objetivo: **móvil gama media ~2021**; arte propio en Blender (ver NORTE).

Instrumentación cerrada (2026-07-22): FPS/frame-time y GPU por passes
(`gpu_pass_costs`, sombras fuera), watchdog de tris, cull por distancia, 12
perillas A/B, la sección `scene` del debug, vistas `wireframe`/`overdraw`, y
presupuestos móviles con `BOF_PROFILE=mobile`. Último perfil móvil medido:
**37.3k tris, 62 draws, 53 mats → "medio", por materiales.**

### Presupuesto de polígonos como contrato (2026-07-25)

**Conteos sí, milisegundos no.** Los tris/draws/materiales son *dato* (dependen
de lo que la escena declara): deterministas, testeables, pueden romper el build.
Los tiempos son *medición* (dependen de GPU, driver y de qué más corría): un test
de ms falla por ruido, se ignora y muere. Van por carriles separados.

- `build.rs` **cuenta triángulos por LOD** al importar cada GLB y los emite en el
  manifiesto (`GeneratedAsset::triangles`). El presupuesto por categoría vive en
  `schema.rs::lod0_triangle_budget` (SoT compartida, como el contrato de clips) y
  **falla el build** al exportar, nombrando el asset — no un `warn!` que se lee
  después, que es como pasaron unos pies de 9172 tris. También rechaza un LOD que
  cueste *más* que el anterior (igual sí: un card son 2 tris en todos).
- Tests de escena (`perf/budget.rs::static_cost`): suman lo que cada fila de
  `SCENES` declara y lo enfrentan al presupuesto móvil. Es lo que el contador de
  runtime no puede ser: él grada lo que la cámara ve, así que una escena pasada
  de presupuesto puede leer "bien" desde un rincón donde casi todo está culleado.
- Hoy: props de pasto 12 tris (card 2, flor 24), `tree_pine_a` 100. El terreno
  son **32768 tris fijos** en toda escena — un tercio del presupuesto móvil antes
  de poner nada encima; subir `CELLS` es una decisión de presupuesto.
- **Hueco conocido:** la pradera no entra en la suma (su tier vive en
  `GRASS_TIERS`, privado en `visuals/grass.rs`, del otro agente). Lo cierra hacer
  esa constante `pub(crate)`. Un draw call exacto **no** es testeable sin cámara
  (un draw es un par malla/material que sobrevivió al culling): lo testeable es
  la cota superior.

### Cámara y flythrough (2026-07-23)

Un solo `Camera3d`; los modos son comportamientos gateados por `CameraMode` **en
la entidad cámara**, no entidades distintas — re-spawnear rompería los
`Single<With<Camera3d>>`. **Orbit** (gameplay) y **Freecam** (F3: vuela libre,
adquiere foco modal multi-dueño, **F4** loguea la pose como `Waypoint` pegable).

**Flythrough por tramos** (`perf/flythrough.rs`, 4 tests): ruta como constantes
autoreada con F4, lap de warmup + lap medido que acumula frame/gpu/tris/draws/
mats **por tramo**, tabla clasificada con el presupuesto móvil. Correr la misma
ruta hoy y en un mes compara peras con peras. La ruta real sigue sin autorear.

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

Hecho, probado y con rendimiento cerrado (2026-07-21). Lo que sigue informando
decisiones (el detalle de implementación quedó en git):

- **Ciclo día/noche por transición** (`world/day_night.rs`): luna direccional
  propia y ambiente azul nocturno, para no perder volumen ni navegación. 5 tests.
- **Inventario con UI en capa propia** (`presentation/inventory_ui/`):
  presentación **solo lee** y emite mensajes por slot que `InventoryPlugin`
  valida en `FixedUpdate` — el patrón a copiar para cualquier UI que actúe.
- **Mundo 320×320 + bosque** (`world/forest.rs`, `visuals/forest.rs`): 179
  árboles deterministas, clearing de 42 m, camino N/S libre. `TreeKind` vive en
  mundo y presentación lo resuelve por `VisualCatalog` — la separación que
  después permitió cambiar a proxies procedurales sin tocar simulación.

Queda: revisar el feeling de día/noche + inventario + bosque + maniquí; después
modelar un personaje propio low-poly que herede el rig UAL1/UAL2.

Contrato de animación con SoT única (`schema.rs::PLAYER_CLIP_CONTRACT`,
compartida por `build.rs` y el resolvedor). Runtime: `AnimationRole`+`ROLE_TABLE`
resuelven `AN_<Rol>` → alias vendor → fallback, con `debug!` nombrando el rol sin
clip propio. Compile-time: un GLB con `bof_animset="player"` falla el build si le
falta un clip `required`. El placeholder fusiona UAL1+UAL2 (85 clips): locomoción
de UAL1, climb/slide/ninja de UAL2 — los 13 roles ligan a clip real. Roles
planeados en el contrato (swim/dive, eje direccional aim+lock-on) esperan motor.

Facing unificado (roadmap 3): `FacingSource { Free, Look, LockOn(Entity) }`
(`movement/facing.rs`) + `resolve_facing` tras `TickActiveMotor`, dueño único del
facing desacoplado (fija el yaw al objetivo, sobrescribe el giro del motor →
encara limpio; climb/ladder mantienen facing de pared).
- **3b Lock-on** (`player/lock_on.rs`): toggle `IntentAction::LockOn` (middle-mouse
  o `T`), adquiere el enemigo más centrado al crosshair (rango 30 m, cono ~60°),
  rompe por despawn o >40 m.
- **Intención facing-relativa explícita** (`intents.planar.local` + `StrafeDir`,
  en `brain.rs`): con facing desacoplado, el stick se lee en el marco del objetivo
  — "izquierda" es un strafe explícito, y el movimiento es circle-strafe relativo
  al objetivo. En `Free` siempre es forward. Visible en debug (`strafe=`).
- **3c Cámara lock-on** (`camera/mod.rs`, `CameraRig::lock_blend`): encuadra hacia
  el objetivo con blend suave al entrar/salir.
- **Animación direccional** (`animation.rs::directional_role`): `StrafeDir` elige
  `AN_Walk/Run/SneakStrafeL/R|Bwd`; sin clips en el placeholder cae a la base
  (walk), listo para cuando existan. El back-pedal reproduce el clip base en
  reversa (`strafe_playback_speed`) para no moonwalkear; L/R esperan clips.

**Arco + lock-on** (`combat/motors/aim.rs`): estando lockeado, el disparo usa una
orientación efectiva que apunta al objetivo (`lock_aim_orientation`), no el mouse
—que la cámara ya desacopló—, así el arco auto-apunta al enemigo lockeado.

El facing lockeado es un **giro suave** (`resolve_facing` slerp a
`DECOUPLED_TURN_RATE`): los motores no rotan el cuerpo cuando el facing está
desacoplado (`MotorCore.facing` + `faces_movement`), así `resolve_facing` es el
único dueño y no hay pelea. `Free` intacto.

Falta: **clips de strafe** propios; motores swim/dive; clips de combate. Bug
conocido resuelto: teleport por caída (era realimentación de `body_yaw`). La
preview de aim mientras cargás (no el disparo) aún puede no seguir al objetivo.

## Pipeline authored de assets — trabajo activo (2026-07-23)

El contrato permanente Blender→GLB→Bevy vive en `ASSET_PIPELINE.md`. Decisiones
aprobadas para esta implementación: scanner estricto limitado a
`assets/game/authored/`; `gltf` + `serde_json` directos (§17); materiales
importados y graybox resueltos a una paleta de handles compartidos; manifiesto
build-time como única autoridad espacial; carga visual con fallback y swap
atómico.

Primera vertical implementada: `tree_pine_a`, arte propio low-poly con LOD0/1/2,
`UCY_Trunk`, tags y socket. Reemplaza sólo `TreeKind::Pine1`; el collider
authored conserva el radio/alto validados del graybox. Falta el checkpoint jugado
antes de retirar físicamente Quaternius `Pine_1`.

Decisión del usuario: el Ranger fue retirado por su costo poligonal. El player
es ahora el maniquí neutro (`AppearanceKey::PLAYER_MANNEQUIN`): mesh+rig de UAL1
referenciado directo desde vendor, sin paso Blender.
**UAL1** = locomoción neutra (Walk/Jog/Sprint/Crouch/Jump);
**UAL2** = acciones (sword/farm/climb/ninja). El player **fusiona ambas**
(`animation_sources`), catálogos separados que comparten rig. `Prototype.glb`
(obsoleto) se borró al migrar la animación del maniquí.

### Decisión — colisiones e hitboxes para assets finales (2026-07-19)

Las fuentes públicas de Nintendo confirman el uso amplio de física en BotW,
pero no documentan sus hurtboxes exactas; se toma el *feeling*, no una
implementación supuesta. Hoy un único `Collider` cápsula sirve como cuerpo
sólido y receptor de melee/flechas/carga (`GameLayer::Actor`). El visual ya
es separado; su escala/pivot salieron de `BodyDimensions` y viven en la
receta de presentación, sin convertir todavía ningún asset fuente.

Contrato acordado:

1. **Locomotion body:** cápsula simple elegida por traversal y capacidades, no
   generada desde el mesh. La forma (`standing/crouched`) se separará del
   envelope semántico (pies, cabeza, radio de soporte) que consumen
   ledges/stairs/ladders. Un cambio cosmético no altera `FixedUpdate`.
2. **Hurtboxes:** primitivas sensoras hijas con `owner` + región, sin respuesta
   física. Las posturas cambian desde simulación, nunca desde el esqueleto.
3. **Hitboxes:** sweeps de capacidad fija definidos por arma/ataque y fase
   autoritativa. Si una animación exige precisión, Blender exporta sockets o
   curvas horneadas que el loader convierte a datos puros de simulación.
4. **Mundo/assets:** colisión simplificada y semántica (`climbable`, material,
   etc.) en nodos GLTF propios; nunca trimesh visual automático como default.

Migración incremental antes del primer asset final: separar layers
Body/Hurtbox con vínculo hurtbox→Actor; migrar melee/flecha/carga a resolver
dueño/región y deduplicar por Actor; separar `LocomotionShapeSet` de
`BodyEnvelope`.

Auditoría de salud cerrada (2026-07-22): pool autoritativo de proyectiles,
capacidad preparada fuera del tick, ground/snap excluyen `GameLayer::Actor`, y
correcciones de melee/inventario/percepción. Los módulos grandes quedaron
partidos por responsabilidad; `time_control` es único dueño de `Time<Virtual>`.

Tests obligatorios: swap visual no cambia simulación; múltiples hurtboxes dan
un solo hit por ataque; self-hit imposible; sensores no bloquean locomoción;
mounted/sneak tienen política explícita; ningún ledger/cache crece en tick.

## Costura superficie→sonido (checkpoint jugado 2026-07-24)

Primera costura para *recibir* assets mientras se autora en Blender: el juego
sabe qué superficie se pisa y emite el sonido correcto, sin que la simulación
dependa del audio (§20). Cuatro capas, la flecha siempre baja hacia datos:

- **Datos** (`asset_pipeline/schema.rs`): `SurfaceKind {Grass, Stone, Wood}` +
  `surface_from_material()`, la forma tipada del `material_kind` de Blender.
- **Mundo** (`world::Surface`): cada box/tread lo recibe desde su `material_key`.
- **Simulación** (`movement::GroundFacts::surface`): el probe lo lee del
  `hit.entity`, gratis. La sim lo registra, nunca ramifica en él.
- **Presentación** (`sfx`): `emit_step_cues` emite un paso por `STRIDE_LEN` (2 m)
  con suelo; `play_audio_cues` mapea superficie→sonido, y el mapeo vive solo acá.

Validado jugándolo: 92 pasos, cadencia correcta, exit 0.

Pendiente: **audio real** (hoy es un `debug!`) y **timing por foot-plant** (el
acumulador de zancada es un stopgap hasta que la animación emita eventos de
pisada — roadmap paso 3).

## Migrar a BSN en vez de seguir construyendo herramientas propias (§21)

Decisión del usuario (2026-07-25), a raíz de "¿no estamos rehaciendo BSN?".
**Sí, en parte.** `bevy_scene` 0.19 trae BSN de verdad: macro `bsn!`, `bsn_list!`,
derive `SceneComponent`, parches por campo y assets por string. Resuelve
composición de escenas, overrides granulares y handles automáticos.

Qué se solapa y qué no:

- **Es un BSN artesanal:** `world/layout.rs` — las tablas `BOXES`/`STAIRS`/
  `PICKUPS` más `spawn_box`/`spawn_oriented_box`/`spawn_stair_segment` son
  literalmente "describí un objeto una vez y spawnealo donde haga falta". El
  propio doc del archivo ya decía que era la costura de un futuro loader.
  **Candidato #1.**
- **Zona gris:** `scene::Contents` elige entre *sistemas Rust*; con BSN elegiría
  entre *escenas declaradas*. La tabla no desaparece, cambia de contenido.
- **Sigue siendo nuestro:** `AppState`/`States` y el ciclo de vida
  (`DespawnOnExit`), el contenido **procedural** (bosque por hash, pradera), y
  el heightfield — una matriz de 16k floats no es una jerarquía de entidades.

Plan: **no migrar en caliente.** Primero terminar la herramienta de terreno; BSN
recién aterriza en 0.19 y su API se va a mover, así que migrar 400 líneas ahora
se paga dos veces. La prueba barata para decidir cuándo: reescribir
`spawn_stair_segment` como un `bsn!` y ver si queda más legible que la función.
Si sí, migrar `layout.rs` entero; si no, el andamiaje todavía gana.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **Player sin personaje propio:** el maniquí neutro UAL1 (~13.7k tris, 2
  materiales; las esferas `M_Joints` pesan 8012 tris, más que el cuerpo) es un
  placeholder. Falta modelar un personaje low-poly propio que herede el rig
  UAL1/UAL2 y lo sustituya; el Ranger quedó descartado por costo poligonal (pies
  9172 tris) y ya fue retirado.

- **Facciones:** `Perceivable` es un bit; reemplazar por facción cuando
  haya hostilidad entre no-jugadores (animales, aliados).
- **Escalar a N enemigos = dato, no código** (audit 2026-07-24: la
  arquitectura pasa el test — capacidad + Intents + dato, no código-por-tipo).
  El spawn hoy es hardcodeado (`spawn_bokobos` crea 2 entidades con nombre).
  Antes de poblar el mundo, dos costuras gemelas *andamiaje-graybox→dato*:
  (a) **roster como tabla de arquetipos** (set de capacidades + Brain + stats +
  `AppearanceKey`) — patrón-hermano: la tabla `world/layout.rs`;
  (b) **visuales de enemigo al `VisualCatalog`** (hoy: cápsula hardcodeada en
  `visuals/enemy.rs`) — patrón-hermano ya probado: el catálogo de árboles (~15
  variantes por dato con un solo `visuals/forest.rs`). Con ambas, agregar un
  enemigo = filas de dato, cero módulos nuevos. La señal de que se está
  torciendo: tentación de copiar un cuarto `visuals/enemyN.rs`.
- **Monturas voladoras = un motor, no una clase:** reutilizan todo el montado
  (ActorLink); lo nuevo es un motor `Fly` que suspende el contrato de suelo —
  primo directo de swim/dive (roadmap paso 3). Verificado: nada fuera del core
  de movimiento ramifica el gameplay sobre `grounded`, así que es aditivo
  (nuevo `LocomotionState` + su motor, §2).
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
