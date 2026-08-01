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

## Bloqueo pre-push del merge animación + terreno (2026-07-30)

**No hacer push.** Decisión del usuario: corregir mañana; esta auditoría queda
sin commit. `fmt --check`, `clippy -D warnings` y los **394 tests** pasan, pero
eso no valida ownership, lifecycle, procedencia ni feeling. El playtest mostró
"muchos errores" de animación sin `error`/`panic` en el log.

### Bloqueantes para mañana

1. **Una sola autoridad de animación.** `set_active_nodes` maneja
   `AnimationPlayer` directamente mientras el mismo rig conserva
   `AnimationTransitions`; Bevy prohíbe mezclar ambos porque su `main_animation`
   queda desincronizada. Elegir un `AnimationGraph` con blend coherente o un
   controlador propio completo, nunca el híbrido actual.
2. **Estado por rig, no por sistema.** `blend_nodes: Local<Vec<_>>` se comparte
   entre todos los `PlayerAnimationRig` y sobrevive a despawns/cambios de escena.
   Volverlo componente puro del rig y fijar tests de dos rigs, despawn/recreación,
   entrada/salida del blend y navegador de clips después del blend.
3. **Licencia antes de distribuir.** `art/blender/char/char_elf.blend` deriva de
   `Elf_MaleCharacter-Free`, cuya fuente no trae licencia. Obtener una compatible
   y registrar procedencia bajo `art/vendor/`, o retirar el derivado de los
   commits que se publicarían. Sin esto no puede entrar a un repo GPL público.
4. **Diagnóstico fuera de medición.** `TerrainDebugView` persiste, pero benchmark
   y flythrough sólo apagan wireframe/overdraw: hoy pueden medir el shader
   semántico unlit. Suspender/restaurar la vista por mensaje o invalidar la
   medición mientras cualquier diagnóstico visual esté activo.
5. **Reparar las fuentes de verdad.** `ARCHITECTURE.md` todavía declara shaders
   custom sólo opt-in; decidir y escribir la excepción del material único de
   terreno o revertirla. Quitar de `BOTWMovements.md` el `[x]` y estado fechado:
   los docs de dominio describen el objetivo; el estado vivo queda acá.

### Deuda del terreno a cerrar con esa corrección

- El loader deja `dirty=true` ante PNG ausente/inválido y reintenta cada frame;
  usar estados Pending/Ready/Failed y reintentar sólo ante `AssetEvent`.
- Paleta y números de modo están triplicados en catálogo Rust, leyenda y WGSL;
  pasar la paleta canónica al shader por uniform.
- `material_report` imprime roughness/metallic hardcodeados en vez de leer el
  `TerrainMaterial` real.
- Separar responsabilidades: `animation.rs` llegó a 972 líneas y
  `terrain_material.rs` a 409 (§1, §16).

### Estado real del elfo y del worktree

El elfo **nunca apareció en Bevy**: el juego siguió usando el maniquí UAL1.
`char_elf.blend` sólo fue revisado con renders headless y no se exportó a
`assets/game/authored/`. El proceso de worktrees sigue sin forma de compilar
antes del merge por el workspace compartido y nombre de paquete duplicado; no
volver a integrar una rama sin resolver ese gate.

## Estado (2026-07-26)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/jump/
glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con percepción
gradual (melee + arquero), health/muerte/respawn, horse, espada con combos, arco
de dos fases con carga Bannerlord, maniquí UAL1 como player, mundo 320×320 con
bosque, audio de pasos por superficie. **394 tests**; `fmt` + `clippy -D
warnings` en verde.

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

### La diagonal de la celda no es nuestra (2026-07-26, ARREGLADO)

Encontrado jugando: "hay una colisión que no es igual al mesh del terreno, está
donde el terreno es muy dispar". Los cuatro vértices de una celda solo son
coplanares en terreno parejo; donde el relieve se retuerce, las dos diagonales
posibles describen **dos superficies distintas**, y manda el collider. `parry3d`
corta por la **anti-diagonal**; nuestra malla y `height_at` usaban la principal.
Medido antes del arreglo: **0,33 m en el peor punto, 36% de las muestras
desalineadas más de 1 cm**. En piso plano costaba cero, y por eso sobrevivió.

El porqué quedó en el doc de `Terrain::to_collider`. Lo que hay que recordar es
**por qué la suite no lo agarró**:

- El test comparaba **6 puntos a mano**; con relieve suave las dos diagonales
  casi coinciden. Ahora barre 3600 muestras con paso de 2,51 m contra celdas de
  2,5 m, para que caigan por dentro de los quads.
- **Nadie comparaba la malla que se ve contra la superficie que se camina.**
  `world` chequeaba su grilla contra el collider y `visuals` construía una malla
  de la misma grilla, pero ese par nunca se enfrentaba. Test nuevo sobre
  **centroides** de triángulo (los vértices no sirven: ahí las triangulaciones
  coinciden por construcción). Verificado que atrapa la regresión.
- Un tercer test tenía la diagonal vieja horneada en su número esperado; ahora la
  saca **del collider** en vez de asumirla.

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

## Dónde se retoma (2026-07-30)

1. **Cerrar los cinco bloqueantes pre-push** de esta página, comenzando por
   licencia y autoridad única de animación. Repetir checkpoint jugado; sólo
   entonces considerar commit/push.
2. **Instancias discretas**: la tercera capa de autoría. Colocar/mover/borrar
   "acá hay una roca" y guardarlo en el archivo: filas `kind` + posición + yaw +
   escala; presentación resuelve el modelo por catálogo.
3. **Cerrar el ciclo de la capa semántica**: entrar, salir al menú y volver, para
   ver el parche releído del disco.
4. **Jugar lo que quedó sin validar**: el graybox asentado sobre relieve
   (esculpir en `Traversal`, guardar, reentrar) y la tipografía de la UI.
5. **Cerrar C2 y el determinismo** con los tests de `CRATES.md` (fases 1-2): no
   dependen del refactor de crates y frenan dos deudas que hoy crecen.

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

**Contrato de animación** con SoT única (`schema.rs::PLAYER_CLIP_CONTRACT`,
compartida por `build.rs` y el resolvedor): `AnimationRole`+`ROLE_TABLE` resuelven
`AN_<Rol>` → alias vendor → fallback, y un GLB con `bof_animset="player"` falla el
build si le falta un clip `required`. El player es el maniquí neutro que **fusiona
UAL1** (locomoción) **y UAL2** (acciones), catálogos separados que comparten rig.
Falta: clips de strafe propios, motores swim/dive, clips de combate, y un
personaje low-poly propio que herede el rig (el maniquí pesa ~13,7k tris, con las
esferas `M_Joints` en 8012 — más que el cuerpo).

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
- **C2 — hardware leído fuera de `input`,** en **15 archivos** (eran 14 el
  2026-07-25: la deuda crece). Ya se cobró una víctima (la tecla `Tab`). Quiere
  un dueño único que traduzca bindings a acciones tipadas, y un test que prohíba
  `ButtonInput` fuera de `src/input/` — ese test es la fase 1 de `CRATES.md` y
  se puede escribir sin mover un módulo.
- **Determinismo roto en el spread del arco (2026-08-01).**
  `combat/motors/aim.rs:288` siembra su LCG con `time.elapsed_secs_f64()` y
  `shooter.to_bits()` — el ID de entidad depende del orden de spawn, así que dos
  máquinas en co-op no ponen la flecha en el mismo sitio. Primera divergencia
  del pilar 5. Plan en `CRATES.md` fase 2.
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
