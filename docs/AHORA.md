# Ahora — el trabajo presente

Conversación de trabajo entre sesiones y agentes. Presupuesto: **≤500
líneas**; lo cerrado se borra (queda en git), no se acumula. Léelo antes de
continuar; actualízalo tras cada decisión aceptada, checkpoint jugado o
cambio de foco. Reglas en `ARCHITECTURE.md`, visión en `NORTE.md`. Los sistemas
visuales tienen un doc por tema: `TEXTURES.md`, `BOTWGrass.md`,
`GraphicalTechniques.md`, `BOTWMovements.md`, `CHARACTER_ANIMATION_IK.md`.
El plan para que las leyes se cobren solas (tests de frontera, determinismo,
lints, crates) está en `CRATES.md`.

## Cómo trabajar en este repo

- Validación mínima antes de terminar: `cargo fmt` + `cargo clippy
  --all-targets -- -D warnings` + `cargo test`.
- **Medir en `cargo run` (dev), no en release.** Las deps ya compilan en
  `opt-level 3` en dev, y el cuello es GPU: la diferencia de perfil medida en el
  punto menos dependiente de la vista fue 0,38 ms contra deltas de 4-12 ms.
  Release tarda ~9 min; se reserva para validar el número absoluto antes de dar
  por cumplido un objetivo. Correr la secuencia **dos veces** y quedarse con la
  limpia: la primera a veces trae outliers.
- El feeling se valida jugando (checkpoint, §10): lanzar **`cargo run` a secas**
  en background para el usuario, redirigiendo a un log; al cerrar la sesión,
  leerlo filtrando `error|panic|took|destroyed` antes de reportar. **No forzar
  X11**: la máquina corre Wayland nativo. Verificado el 2026-07-26 — arranca en
  Vulkan/RADV sobre la Polaris 11.
- Debug in-game: **F1 abre el hub** — canales, perillas de render, acciones y la
  secuencia de medición, todo por click. Sobreviven dos teclas: `[`/`]` ciclan
  clips con el navegador abierto, y **P** vuelca el snapshot al log.
- **El log arranca callado (2026-07-26).** Todos los canales de log están en
  `off` por defecto, incluido el sink de consola de `debug`, que antes escribía
  siempre: en un playtest real de dos minutos puso **208 de las 240 líneas** y
  enterró las 28 que hablaban del juego. Para A/B manual se prende `Log: perf
  samples`; para depurar locomoción, `Log: state changes`. El benchmark y el
  flythrough imprimen sus propias tablas, así que dejar todo apagado no cuesta
  ninguna medición. **P** funciona siempre.
- `RUST_LOG` **reemplaza** el filtro de Bevy en vez de sumarse, así que usarlo a
  secas devuelve el spam de `wgpu`/`naga`. Si hace falta:
  `RUST_LOG=wgpu=error,naga=warn,breath_of_freedom=debug`.
- **Antes de bindear una tecla, `grep -rhoE "KeyCode::[A-Za-z0-9]+" src/`.**
  Nadie arbitra colisiones (varias capas leen el hardware directo, hallazgo C2),
  así que una tecla ya usada no da error: da una función que "no hace nada". Al
  2026-07-26 solo quedan libres **F7, F9, F11, F12**.
- Commits a `main`, mensajes convencionales, sin push sin pedido explícito.
- **Compilar cuesta caro, y conviene saber por qué.** El workspace comparte
  `build-dir` (`uneven/.cargo/shared-build`) para compilar bevy una vez entre
  todos los juegos, pero **esa premisa no se cumple**: Cargo cachea por *set de
  features resuelto*, y avian3d activa `bevy/bevy_gizmos`, `bevy_mesh`,
  `bevy_picking` y `bevy_world_serialization`, que los proyectos sin avian no
  piden. Medido el 2026-08-01: **39 variantes de `libbevy_render`**, **136 GB**
  en `deps/`, y **agregar un target nuevo** (un `tests/*.rs`, un bin) recompila
  el árbol de bevy entero para ese target — `tests/architecture.rs` pasó de
  25 min sin llegar a correr un test. Apagar las features default de avian
  (fase 6 de `CRATES.md`) ataca las dos cosas. **Decisión abierta**: seguir
  compartiendo o devolver cada proyecto a su target local, que hace `cargo
  clean` predecible — y nunca `rm -rf` sobre lo que Cargo administra.

## Limpieza pre-push (2026-08-02)

Se retiraron el elfo, sus fuentes sin licencia y la herramienta de reskin. El
player volvió a graybox: una cápsula procedural construida con el mismo
`BodyDimensions` y el mismo estado `Crouched` que el collider. No hay rig,
modelo ni controlador de animación activos; ese dominio se retoma cuando exista
un personaje propio o con procedencia compatible.

Los otros bloqueantes de la auditoría también quedaron resueltos: benchmark y
flythrough rechazan arrancar con una vista semántica activa; el loader de
texturas reintenta sólo ante `AssetEvent`; `material_report` lee el material
real; y `ARCHITECTURE.md`/`NORTE.md` declaran la excepción PBR del terreno.
Queda como deuda, no bloqueo: unificar paleta/IDs Rust↔WGSL y dividir
`terrain_material.rs` (§1, §16). La suite completa cerró en verde: 389 tests
unitarios y 2 de arquitectura, además de `fmt`, `check` y `clippy -D warnings`.
Tras reiniciar, el juego abrió y cerró limpio sobre Vulkan/Polaris; el usuario
confirmó que la cápsula se vio bien en juego. Checkpoint cerrado.

## Estado (2026-08-03)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/jump/
glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con percepción
gradual (melee + arquero), health/muerte/respawn, horse, espada con combos, arco
de dos fases con carga Bannerlord, cápsula graybox como player, mundo 320×320
con bosque y audio de pasos por superficie. La validación automatizada de esta
limpieza está verde y la cápsula quedó validada en juego el 2026-08-02.

**CRATES fases 4-5 cerradas.** `TerrainAccess` concentra toda lectura de terreno.
En el checkpoint post-refactor del 2026-08-03 `sandbox.ron` cargó, se esculpió y
guardó repetidamente, y la ventana cerró sin error/panic. Se eligió el grafo de
crates **hermanas**: simulation y presentation sólo comparten `bof_domain`.
El primer corte ya existe y mueve allí contratos de input, Movement, Combat,
Health, Inventory, Mounts, projectiles, debug/perf y assets. `build.rs` y el
manifiesto authored también pertenecen a domain. Ese crate declara sólo
`bevy_ecs`/`bevy_math`/`bevy_reflect`: `cargo tree` no contiene render ni Avian.
Validación verde: 390 tests unitarios + 3 de arquitectura y Clippy estricto.

**Pradera** (ver `BOTWGrass.md`): 45 briznas/m², 28.125 briznas de 2 tris
horneadas en una malla por chunk — 25 entidades, cero trabajo por frame. Medido
en la caja `Pasto`: **5,78 ms de frame, 4,16 de GPU, 11 draws**. Reemplaza un
intento de matojos por entidad cuya documentación afirmaba "0.0 ms CPU" y "60 FPS
estables" el mismo día en que el medidor marcaba 35-46 FPS. **Regla que sale de
ahí: ningún número entra a estos documentos sin salir del medidor.**

Auditorías: arquitectura (2026-07-17, 4/4 corregidos), calidad (2026-07-24) y
código (`AUDITORIA_CODIGO_2026-07-25.md`); de esta última siguen abiertas **C1**
(`Vec<Vec<f32>>` por tick esculpido) y **C2** — ambas en Deudas anotadas.

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
puede leer**. Esa fase existe desde que el player nacía 6,6 m bajo tierra — y un
heightfield es una superficie de una cara, así que desde abajo la caída no
termina nunca.

## La herramienta de mapas

**Autora dato de simulación, no presentación.** En el archivo de nivel no hay un
modelo, ni un material, ni un color: solo significado. Presentación lo lee y
decide cómo se ve (el patrón que ya funciona con `TreeKind` → `VisualCatalog`).
Orden de las capas de autoría, que es también el orden en que se construyen:
**relieve (hecho) → semántica (hecha) → instancias (siguiente)**.

Ubicación: dato en `world/terrain.rs`, malla en `visuals/terrain.rs`, autoría en
`editor/` (`brush` + `paint` + `history` + `persist` + `hud`).

### Relieve

El relieve nace de un heightfield que es la fuente única de verdad. De esa grilla
se derivan el `Collider::heightfield` y una malla flat-shaded, sincronizadas por
change detection. La navegación **no se hornea**: emerge de la forma + la física.

Seis pinceles con `1..6`, elegidos para **hacer mundos que valga la pena
caminar** — formar, calmar, tener dónde pararse, **conectar** dos niveles,
ensuciar lo que se ve a CAD, y escalonar una ladera para que sea un lugar:

`1` Elevar · `2` Suavizar · `3` Aplanar · `4` Rampa · `5` Rugosidad · `6` Terrazas

- **MMB suaviza siempre**, sin cambiar de modo: es el borrador de esculpir.
- Rueda = radio; **Shift+rueda o `[`/`]` = fuerza**. **Ctrl+Z/Y deshacen por
  trazo**, no por frame.
- `Terrain` es dueño del **cómo** cambia la grilla (un método por pincel sobre
  `brush_stroke`, que toma un *segmento* — un círculo es una cápsula de largo
  cero); `editor/` solo decide **dónde y cuándo**. Un séptimo pincel es un método
  + una fila, nunca un sistema nuevo.
- **La suavidad se arregló en la causa, no en la constante.** Aplicar
  `delta * falloff` a los mismos puntos cada frame *integra la curva de falloff
  en una carpa con pico*; ahora el trazo se relaja a sí mismo mientras sube
  (`RELAX_PER_METRE`). Un test lo fija.

Formato **RON** (decisión del usuario para todo el workspace). Resolución y
extent viajan dentro y `apply_ron` **remuestrea en espacio de mundo** si
difieren, así cambiar `CELLS` o `WORLD_SIZE` no huerfaniza los niveles.

### Semántica por celda (2026-07-26, jugada y validada)

- **`world/terrain_kind.rs`**: `TerrainKind {Soil, Rock, TallGrass, Sand}` y una
  **tabla** `KINDS` de la que salen las propiedades (`surface`, `flammable`,
  `cuttable`). Decisión del usuario: se pinta *un significado*, no atributos
  sueltos — así una celda no puede ser piedra inflamable, y cambiar qué significa
  "pasto largo" es una fila, no repintar los niveles. **Agua no entra acá**: una
  laguna necesita altura de superficie, no es propiedad de una celda.
- **Dos canales en una grilla**: alturas en los `129×129` **vértices** (se
  interpolan), kinds en las `128×128` **celdas** (no se interpolan). `paint_area`
  no tiene falloff ni rate — no hay medio camino entre roca y arena — así que
  pintar es **idempotente** y el borde de un parche es la grilla de 2,5 m.
- **Capas de autoría** (`ToolLayer {Relief, Meaning}`, **F6** cambia; Tab no,
  que ya abre el inventario). Undo es **una sola pila** para las dos capas:
  Ctrl+Z es "lo último que hice", no "lo último en el modo en que estoy".
- **Persistencia**: el canal semántico va **run-length** en el mismo `.ron`
  (`kinds: [(16384, Soil)]` en un nivel virgen). `#[serde(default)]` mantiene
  cargables los niveles viejos. El remuestreo es **vecino más cercano** — copiar
  el camino bilineal habría inventado kinds que nadie pintó. Una lista de runs
  que no suma se **rechaza**: correría el mapa entero de lugar en silencio.
  Medido: 47 runs = **585 bytes**, contra 105.700 de las alturas.
- **Simulación**: `GroundFacts::surface` sale de `kind_at(punto de contacto)` en
  el terreno, y del `Surface` autorado en cajas y escalones. Al terreno se le
  **quitó** el componente `Surface`: uno solo no puede describir 320 m.
- **Presentación**: el color de vértice codifica capa + propiedades y un único
  `ExtendedMaterial` muestrea cuatro PNG desde un `texture_2d_array`. F1 ofrece
  `Arte/Tipo/Escalable/Inflamable/Cortable`; su arquitectura sigue bloqueada por
  los puntos de auditoría arriba.
- **Pintar no toca la física**: `Changed<Terrain>` no distingue canales, así que
  `Terrain` lleva `relief_revision` y el rebuild del collider sale temprano si
  las alturas no se movieron.

Validado jugando (2026-07-26): pintado, `surface=Dirt→Stone→Dirt` bajo los pies,
undo, y guardado en disco. **Falta** la relectura en vivo (F10 → menú → volver).

### Lección de la diagonal de la celda (2026-07-26, arreglado)

`parry3d` triangula el heightfield por la **anti-diagonal** y nuestra malla usaba
la principal: **0,33 m** de desvío en el peor punto y **36% de las muestras** a
más de 1 cm, invisible en piso plano y por eso sobrevivió. El porqué está en el
doc de `Terrain::to_collider`; el detalle, en git. Lo que se queda es **por qué
la suite no lo agarró**: comparaba 6 puntos a mano (con relieve suave las dos
diagonales casi coinciden) y **nadie enfrentaba la malla que se ve contra la
superficie que se camina**. Ahora se barren 3600 muestras con paso de 2,51 m
contra celdas de 2,5 m, y se comparan **centroides** de triángulo — los vértices
no sirven, ahí las triangulaciones coinciden por construcción.

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

## Dónde se retoma (2026-08-04)

1. **CRATES fase 6.3:** trasladar `inventory` y `projectiles` a
   `bof_simulation`, compilable y verde, sin rediseñarlos.
2. **CRATES fase 7 — `bof_presentation` + app:** hermana de simulation; depende
   sólo de `bof_domain`, y el binario compone ambas.
3. **Instancias discretas**: la tercera capa de autoría. Colocar/mover/borrar
   "acá hay una roca" y guardarlo en el archivo: filas `kind` + posición + yaw +
   escala; presentación resuelve el modelo por catálogo.
4. **Cerrar el ciclo de la capa semántica**: entrar, salir al menú y volver, para
   ver el parche releído del disco.
5. **Jugar lo que quedó sin validar**: el graybox asentado sobre relieve
   (esculpir en `Traversal`, guardar, reentrar) y la tipografía de la UI.

## Rendimiento: lo que sigue informando decisiones

El peor punto del bosque pasó de ~72 ms a nunca bajar de 60 (2026-07-21). El
detalle está en git; los principios que quedaron:

- **El graybox tiene que ser honesto sobre el costo.** Los árboles Quaternius
  fingían ser baratos y daban un número falso; se reemplazaron por **proxies
  procedurales** instanciados, con el modelo detallado como tier opt-in.
- **El costo es propiedad de la representación, no de la identidad.** `TreeKind`
  resuelve a dos tiers en `VisualCatalog`; impostores e instancing se enchufan
  ahí sin tocar simulación.
- **Ceguera medida:** el total `gpu:` suma solo spans registrados; los pases de
  sombra usan `info_span!`, no el grabador. "El gpu medido no cambió" **no**
  implica "no es GPU" — indujo un diagnóstico equivocado una vez. **Y corta al
  revés:** invocar esa ceguera para culpar a las sombras fue el segundo
  diagnóstico equivocado. Lo que zanjó la duda fue *quitar la escena*: con 3
  draws en pantalla no hay dónde esconderse.
- **El medidor dice *cuándo* una técnica vale la pena; no se aplican todas
  siempre.** Eso es cargo-culting y frena al dev, no al juego.
- Último perfil móvil medido: **37,3k tris, 62 draws, 53 mats → "medio", por
  materiales.** De ahí sale la ley 1 de `TEXTURES.md`.

### Presupuesto de polígonos como contrato (2026-07-25)

**Conteos sí, milisegundos no.** Los tris/draws/materiales son *dato*:
deterministas, testeables, pueden romper el build. Los tiempos son *medición*: un
test de ms falla por ruido, se ignora y muere. Carriles separados.

- `build.rs` cuenta triángulos por LOD al importar cada GLB; el presupuesto vive
  en `schema.rs::lod0_triangle_budget` y **falla el build nombrando el asset** —
  no un `warn!` que se lee después, que es como pasaron unos pies de 9172 tris.
- Tests de escena (`perf/budget.rs::static_cost`) suman lo que cada fila de
  `SCENES` declara contra el presupuesto móvil. Es lo que el contador de runtime
  no puede ser: él grada lo que la cámara ve, así que una escena pasada de
  presupuesto puede leer "bien" desde un rincón donde casi todo está culleado.
- El terreno son **32768 tris fijos** en toda escena — un tercio del presupuesto
  móvil antes de poner nada encima. Subir `CELLS` es una decisión de presupuesto.
- **Hueco conocido:** la pradera no entra en la suma (`GRASS_TIERS` es privado en
  `visuals/grass.rs`). Lo cierra hacerla `pub(crate)`. Un draw call exacto **no**
  es testeable sin cámara; lo testeable es la cota superior.

### Cámara y flythrough

Un solo `Camera3d`; los modos son comportamientos gateados por `CameraMode` **en
la entidad cámara**, no entidades distintas — re-spawnear rompería los
`Single<With<Camera3d>>`. **Orbit** (gameplay) y **Freecam** (F3; **F4** loguea la
pose como `Waypoint` pegable). El flythrough acumula frame/gpu/tris/draws/mats
**por tramo**, así correr la misma ruta hoy y en un mes compara peras con peras.

Pendiente: **autorear la ruta canónica real** con F4 — hoy los tramos se llaman
`spawn→clearing` en cajas que no tienen ni claro ni bosque, y una regresión de
~1 ms de GPU pasó dos días inadvertida. Diferido salvo que el profiler lo pida:
impostores, streaming por chunks, y **occlusion culling** — el de Bevy es
experimental vía meshlets, no mobile-friendly.

## Pipeline authored de assets

El contrato permanente Blender→GLB→Bevy vive en `ASSET_PIPELINE.md`; el de
texturas en `TEXTURES.md`. Scanner estricto limitado a `assets/game/authored/`,
manifiesto build-time como única autoridad espacial, carga visual con fallback y
swap atómico.

Primera vertical: `tree_pine_a`, arte propio low-poly con LOD0/1/2, `UCY_Trunk`,
tags y socket. Falta el checkpoint jugado antes de retirar Quaternius `Pine_1`.

**Contrato futuro de animación:** `schema.rs::PLAYER_CLIP_CONTRACT` y `build.rs`
siguen rechazando un GLB authored incompleto, pero no existe rig/resolvedor
runtime mientras el player sea la cápsula graybox. La dirección de blending e
IK vive en sus docs de dominio y se implementa sólo con un personaje compatible.

**Facing unificado**: `FacingSource { Free, Look, LockOn(Entity) }` con
`resolve_facing` como dueño único tras `TickActiveMotor`, así los motores no
pelean por rotar el cuerpo. Encima: lock-on, intención facing-relativa con
`StrafeDir` (circle-strafe real), y arco que auto-apunta al objetivo lockeado.

### Colisiones e hitboxes para assets finales (decisión 2026-07-19)

Se toma el *feeling* de BotW, no una implementación supuesta: las fuentes
públicas no documentan sus hurtboxes. El contrato para cuando lleguen los assets
finales:

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

A raíz de "¿no estamos rehaciendo BSN?": **sí, en parte** — `world/layout.rs` es
un BSN artesanal. Pero verificado en las fuentes de `bevy_scene 0.19`: el formato
`.bsn` **no existe** (los docs del crate dicen "not currently shipped" y el único
test que carga uno usa un loader falso) y el crate **no sabe serializar** un
`Scene`. O sea que BSN **no puede ser el archivo que escribe la herramienta de
mapas**; sirve del lado de presentación (`kind` → componentes).

Plan: no migrar en caliente; la API se va a mover. La prueba barata para decidir
cuándo: reescribir `spawn_stair_segment` como un `bsn!` y ver si queda más
legible que la función. Si sí, migrar `layout.rs` entero.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **C1 — allocation en `FixedUpdate`:** `rebuild_terrain_collider` arma un
  `Vec<Vec<f32>>` que Avian vuelve a aplanar, ~130 allocations por tick
  esculpido. La vía barata exige `parry` como dep directa.
- **C2 — hardware leído fuera de `input`,** en **13 archivos** (eran 15 el
  2026-08-01: la lista ya sólo encoge). Ya se cobraron `Tab` y el navegador de
  animación. El test de fase 1 ya impide que la lista crezca; falta el dueño
  único que traduzca bindings a acciones tipadas.
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
- **`InventorySet` y `MountsSet::PostMove` sin orden explícito entre sí:**
  comparten banda sobre componentes hoy disjuntos; el primer feature que cruce
  ambos dominios (alforjas, loot al desmontar) hereda un orden no declarado.
- **Apilado de comida por igualdad exacta de `f32`:** una fuente futura que
  calcule `heal` en runtime puede fallar el apilado por redondeo.
