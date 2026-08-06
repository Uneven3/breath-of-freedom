# Ahora — el trabajo presente

Conversación de trabajo entre sesiones y agentes. Presupuesto: **≤500
líneas**; lo cerrado se borra (queda en git), no se acumula. Léelo antes de
continuar; actualízalo tras cada decisión aceptada, checkpoint jugado o
cambio de foco. Reglas en `ARCHITECTURE.md`, visión en `NORTE.md`. Los sistemas
visuales tienen un doc por tema: `TEXTURES.md`, `BOTWGrass.md`,
`GraphicalTechniques.md`, `BOTWMovements.md`, `CHARACTER_ANIMATION_IK.md`.
El plan que llevó a los tres crates cerró el 2026-08-04 y su documento se
borró: lo vivo quedó en `ARCHITECTURE.md` (las capas y qué cobra cada frontera)
y el detalle de las ocho fases en `git log -- docs/CRATES.md`.

## Cómo trabajar en este repo

- Validación mínima antes de terminar: `cargo fmt` + `cargo clippy
  --all-targets -- -D warnings` + `cargo test`.
- **Los crates se testean por paquete, no con `--workspace`.** Meter el binario
  en la misma invocación unifica las features de Avian, y el smoke headless de
  `bof_simulation` panickea con esa resolución. La corrida
  buena son tres: `cargo test`, `cargo test -p breath_of_freedom_simulation` y
  `cargo test -p breath_of_freedom_domain`. Con `--workspace` todo lo demás pasa
  y sólo cae ese smoke: si cae otro, es real.
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
  Basta con `src/`: `crates/` no tiene ni una ocurrencia y no puede tenerla
  (simulación no declara `bevy_input`), verificado el 2026-08-04.
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
  25 min sin llegar a correr un test. Avian ya usa `default-features = false`
  en `bof_simulation`, que ataca la bifurcación por el lado de las features.
  **El 2026-08-04 el disco se llenó y hubo que hacer `clean` de todo el
  workspace**: el árbol volvió a **39 GB** desde los 136 GB. O sea que la
  estrategia no sólo bifurca — acumula hasta obligar a tirar todo y recompilar,
  que es el costo que la decisión de abajo tiene que pesar. **Decisión
  abierta**: seguir compartiendo o devolver cada proyecto a su target local,
  que hace `cargo clean` predecible — y nunca `rm -rf` sobre lo que Cargo
  administra.
- **Para listar las ambigüedades de scheduling por nombre**, activá un rato la
  feature `debug` de `bevy_ecs` en `crates/simulation/Cargo.toml`: sin ella
  Bevy imprime `<Enable the debug feature to see the name>`, y después de
  `Schedule::initialize` el grafo ya no puede resolver nombres por ninguna otra
  vía (los sistemas se mudan al ejecutable). El reporte sale por `LogPlugin` +
  `ambiguity_detection: LogLevel::Warn`. Acordate de revertir la feature: crea
  otra variante en el build compartido.

## Estado (2026-08-04)

Jugable y validado: locomoción completa multi-actor (walk/sprint/sneak/jump/
glide/climb/ladder/mantle/vault/wall-jump/stairs), enemigos con percepción
gradual (melee + arquero), health/muerte/respawn, horse, espada con combos, arco
de dos fases con carga Bannerlord, cápsula graybox como player, mundo 320×320
con bosque y audio de pasos por superficie. El player es graybox a propósito: no
hay rig ni controlador de animación hasta que exista un personaje propio o con
procedencia compatible.

**CRATES fase 6 cerrada: el juego entero corre sin pantalla.** Tres crates
hermanas (`bof_domain`, `bof_simulation`, y el binario), 18.910 LOC de simulación
contra 14.952 de presentación y composición. `main` son diez plugins más
`SimulationPlugin`. El smoke headless levanta el juego completo sin ventana ni
GPU. Suite: 86 tests del binario + 4 de arquitectura, 258 de simulation, 50 de
domain, Clippy estricto en todo. Detalle.

**Pradera** (ver `BOTWGrass.md` y la sección del pasto más abajo): grilla
rodante de tres anillos centrada en la cámara, briznas de 2 tris horneadas en
una malla por chunk, cero trabajo por frame. Nació reemplazando un intento de
matojos por entidad cuya documentación afirmaba "0.0 ms CPU" y "60 FPS estables"
el mismo día en que el medidor marcaba 35-46 FPS. **Regla que sale de ahí:
ningún número entra a estos documentos sin salir del medidor.**

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

Ubicación tras el corte en crates: dato en
`crates/simulation/src/world/terrain.rs`, malla en `src/visuals/terrain.rs`,
autoría en `src/editor/` (`brush` + `paint` + `history` + `persist` + `hud`), y
qué archivo carga cada escena en `src/scene/`.

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

## El pasto (2026-08-06) — jugado, arreglado, y por primera vez medido

**Sigue vigente la decisión del usuario:** primero que se vea bien, después se
optimiza. `BOTWGrass.md` manda medir antes de cada paso; esa regla está
suspendida para la parte estética. Target **900p30** (`NORTE.md` todavía dice
1080p60 y hay que actualizarlo — es decisión del usuario, no se tocó).

### Lo que se arregló hoy, y qué falta validar jugando

De los cinco problemas reportados el 2026-08-05:

1. **Briznas muy chicas** → 0,26-0,52 m pasaron a **0,45-0,90 m**, con el lean
   escalado a la par. *Jugado y aceptado: "está mejor la altura y el feeling".*
2. **Densidad insuficiente** (pedido en el mismo playtest) → los tres anillos
   subieron a **56 / 28 / 10 briznas por m²**. Sin jugar.
3. **Punta cuadrada** → taper 0,35 → **0,18**, muesca 0,82 → **0,72**. Sin
   comentario del usuario todavía.
4. **"Veo crecer el pasto muy cerca"** → **dos intentos, y el primero estuvo
   mal diagnosticado.** La banda de crecimiento hacía dos trabajos con una sola
   constante: cuánto tarda *una* brizna en crecer, y en cuántos metros se
   reparten los umbrales *entre* briznas. Acortarla de 8 a 3 m alejó el
   fenómeno y lo hizo **más** brusco — el usuario lo volvió a reportar como "muy
   notorio". Ahora son dos constantes: `GROWTH_RAMP_M` = 1 m (una brizna) y
   `GROWTH_SPREAD_M` = 6 m (la dispersión). Lo que se ve pasa de una ola que
   avanza con el jugador a un campo que ralea con la distancia. Sin jugar.
5. **Falta abundancia en el horizonte** → sin hacer. El anillo externo subió de
   6 a 10/m², que ayuda pero no es el arreglo.
6. **El parpadeo** → **resuelto y confirmado jugando**, y no era ninguna de las
   dos cosas que veníamos suponiendo. Era **z-fighting**: al encogerse, la brizna
   colapsaba hacia la altura del suelo y quedaba como un cuadrilátero plano
   coplanar con el terreno, agitado por el viento. Ahora colapsa 18 cm **bajo**
   tierra (`GROWTH_SINK_M`) y de paso brota del suelo, que es el efecto que el
   doc pedía. La hipótesis de MSAA sostenida dos días era falsa; lo que la
   descartó fue la descripción del usuario, *"unos pastos que parecen pegados en
   el piso"*. La perilla `msaa` queda igual, ahora con su costo medido: entre
   1,81 y 3,17 ms de GPU.

### La medición, que es lo primero que este sistema tiene

Caja Pasto, altura de ojo, **tres corridas** cuyos números no coinciden: el costo
del pasto salió 3,77 / 2,56 / 2,36 ms de GPU. La deriva *dentro* de cada corrida
fue de 0,2 ms, así que la dispersión es externa — el usuario tenía **Blender
abierto**, que compite por CPU y GPU. **Regla que sale de ahí: cerrar lo que
compita por la GPU antes de medir**, porque el encabezado de contexto del
reporte no puede declarar lo que no ve.

Lo que aguanta las tres: la pradera es **entre el 45% y el 62% de la GPU** de su
caja, y **es fill-bound** — en las tres, bajar la resolución a la mitad ahorra
más que apagar el pasto entero, con la misma geometría. El alcance ahorra menos
que la densidad. Tabla completa en `BOTWGrass.md`.

Eso reordena las prioridades: **el conteo de triángulos no es lo que cuesta el
frame en esta máquina**, y el cambio a 900p30 golpea la palanca correcta. La
salvedad de siempre sigue en pie — un tiler cobra el vértice en bandwidth aunque
no pinte un píxel, y eso no se mide acá.

**Sin zanjar:** en la única corrida con frame utilizable, apagar el pasto bajó
la GPU 2,56 ms y el frame sólo 0,31, lo que apunta a un techo de CPU de ~7,4 ms.
Pero fue con Blender abierto y en build dev. Zanjarlo pide una corrida en
release con la máquina limpia.

**Costo declarado que subió hoy:** 250.800 → 489.200 triángulos por vista
(`perf::budget::MEADOW_VIEW_TRIANGLES`), casi cinco veces el presupuesto móvil.
62.700 son el anillo interior de 8 a 10 m; el resto, las tres densidades. Se
pagó a sabiendas bajo la regla vigente.

## La suite de medición (2026-08-06)

**Correr un barrido ya no requiere jugar.** `BOF_BENCH=<suite> cargo run` entra
a la caja de la suite, se para en su mirador, mide, escribe la tabla y cierra el
proceso. Tres suites, en `perf/suite.rs` como **dato**: `grass`, `general`,
`shadows`. Agregar una es una variante y una tabla — el motor (`sequence.rs`) no
se toca. También hay un botón por suite en el hub F1.

Reglas que los tests cobran sobre toda suite, presente y futura: empieza y
termina en el baseline (la diferencia es la deriva), cada paso mueve **un** eje,
y el baseline es la configuración que se envía.

**Tres trampas de esta máquina, encontradas corriendo:**

- **Cerrar Blender (o cualquier cosa que use la GPU) antes de medir.** Con él
  abierto, tres corridas de la misma configuración dieron costos del pasto entre
  2,36 y 3,77 ms, con derivas internas de 0,2. Lo externo no aparece en la
  tabla.

- **La ventana tiene que estar visible.** En Wayland nativo el compositor no
  manda frame callbacks a una superficie oculta y el juego entero se duerme —
  1,9 s de CPU en 105 de reloj, sin medir nada. Para correr en segundo plano:
  `WINIT_UNIX_BACKEND=x11 WAYLAND_DISPLAY= BOF_BENCH=grass cargo run`. **Sólo
  para medir**; el juego se juega en Wayland nativo.
- **El frame suele quedar clavado por la presentación** (16,67 ms en todos los
  pasos) mientras la GPU varía de 2,15 a 8,17. El reporte lo detecta solo y
  avisa que hay que leer `d-gpu` y descartar `d-frame`. El criterio no nombra
  ningún refresh: compara cuánto se movió el frame contra cuánto la GPU.

El reporte abre declarando **suite, pregunta, escena, perfil, ventana, MSAA,
render scale, densidad y alcance del pasto, sombras y mirador**, porque una
tabla de milisegundos sin contexto no se compara con otra de la semana que
viene — el 2026-08-06 hubo que deducir de un delta en qué escena había corrido
una tabla.

**Perillas nuevas:** `msaa` (1/4/2 muestras) y `grass-reach` (100/75/50% del
alcance de los anillos), más un paso de **0 briznas/m²** que convierte "cuánto
cuesta el pasto" de extrapolación en resta.

**Pendiente que la suite destapó:** ocultar el bosque entero desde el mirador
canónico vale 0,34 ms, lo que sugiere que **el mirador "del bosque" no mira al
bosque**. Autorearlo con F4 sigue sin hacerse y ahora tiene evidencia.

**Ruido de Bevy, no nuestro:** cada corrida imprime ~260 líneas
`bevy_render::slab_allocator: Use-after-free`. Aparecen al despawnear muchas
mallas de golpe (cambiar densidad o alcance re-hornea la grilla entera). No
rompe la corrida ni los números; ensucia el log y no está investigado.

## Crates: cerrado (2026-08-04), y lo que quedó vivo de ahí

Las ocho fases están en `git log -- docs/CRATES.md` y las leyes en
`ARCHITECTURE.md`. Lo que sigue informando decisiones y no se deduce del código:

- **Tres decisiones que se apartaron del plan.** `input` no cruzó a simulación:
  maneja hardware *y* cursor, y dejándolo afuera simulación no declara
  `bevy_input` ni `bevy_window` y no **puede** leer teclado, en vez de acordarse
  de no hacerlo. `layout`/`spawn` tampoco: son composición, y el binario es la
  única capa que ve simulación y presentación a la vez. El reloj sí cruzó, la
  luz no — `advance_time` escribe cada tick y presentación sólo lee (§20).
- **Los newtypes de unidades se midieron y se descartaron:** en todo `movement`
  hay 4 funciones con dos o más `f32` y en dos de ellas comparten unidad. El
  riesgo que justificaba envolver 141 campos no existe.
- **`bof_presentation` no se creó a propósito:** con las referencias a
  simulación en cero y la ley congelada en un test, el crate no agregaba nada
  que no estuviera ya cobrado.
- **Al leer un log, ojo:** sólo `sandbox.ron` existe en disco, y `spawn_terrain`
  únicamente loguea cuando encuentra archivo, así que una escena sin heightmap
  arranca plana **y en silencio** — la ausencia de línea no es ausencia de
  escena.

Siguiente en esa línea: instancias discretas, cerrar el ciclo semántico, y jugar
graybox sobre relieve + tipografía.

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
- **Fill antes que geometría (2026-08-06, medido en las tres suites).** En la
  caja Pasto bajar la resolución a la mitad ahorra más GPU que apagar la pradera
  entera; en el Mundo, 2,55 ms de 4,19. Todo lo que se midió en esta máquina
  apunta al mismo lado: el frame se va en píxeles pintados, no en vértices
  transformados. Vale para elegir en qué orden atacar cualquier sistema visual
  nuevo — y con la salvedad de que el target es un tiler, donde el vértice
  también se paga.

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
- Un draw call exacto **no** es testeable sin cámara; lo testeable es la cota
  superior. (El hueco de la pradera fuera de la suma se cerró el 2026-08-04
  haciendo `meadow_triangles` visible; qué destapó, más abajo.)
- **Pero el conteo es guardrail, no objetivo.** El target es un Android
  tile-based (`NORTE.md`), donde manda fill-rate/overdraw y bandwidth. Los
  conteos siguen porque son testeables; pasarlos no prueba que corra en el
  teléfono. El dial de overdraw del hub existe desde siempre y nunca se usó.

### Cámara y flythrough

Un solo `Camera3d`; los modos son comportamientos gateados por `CameraMode` **en
la entidad cámara**, no entidades distintas — re-spawnear rompería los
`Single<With<Camera3d>>`. **Orbit** (gameplay) y **Freecam** (F3; **F4** loguea la
pose como `Waypoint` pegable). El flythrough acumula frame/gpu/tris/draws/mats
**por tramo**, así correr la misma ruta hoy y en un mes compara peras con peras.

Pendiente: **autorear la ruta canónica real** con F4 — hoy los tramos se llaman
`spawn→clearing` en cajas que no tienen ni claro ni bosque, y una regresión de
~1 ms de GPU pasó dos días inadvertida. **Lo mismo vale para el mirador del
bosque**, y ahora hay evidencia: la suite `general` mide que ocultar el bosque
entero desde ahí vale 0,34 ms, o sea que desde ese punto casi no se ve bosque. Diferido salvo que el profiler lo pida:
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
un BSN artesanal. Reverificado en `bevy_scene 0.19` el 2026-08-05: `bsn!`,
`SceneComponent` y el parcheo por campo **sí existen y se usan dentro de Bevy**;
el formato de archivo `.bsn` **no**, y nada serializa una `Scene`. Por eso BSN
no puede ser el archivo que escribe la herramienta de mapas, y sí es la forma
correcta de resolver `kind` → entidades.

El análisis completo y el plan viven en `docs/MAP_EDITOR.md`. La prueba barata
para decidir cuándo migrar `layout.rs`: reescribir `spawn_stair_segment` como un
`bsn!` y ver si queda más legible que la función.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **Paleta/IDs Rust↔WGSL sin unificar, y `terrain_material.rs` sin dividir**
  (§1, §16). Viene de la auditoría del 2026-08-02; no bloquea, pero cada color
  nuevo hay que escribirlo en dos lados que nadie obliga a coincidir.
- **`InventorySet` y `MountsSet::PostMove` ya comparten crate.** Su orden mutuo
  seguía sin declarar cuando estaban separados; ahora que ambos viven en
  `SimulationPlugin`, declararlo es una línea al lado de las otras cuatro.
- **C1 — allocation en `FixedUpdate`:** `rebuild_terrain_collider` arma un
  `Vec<Vec<f32>>` que Avian vuelve a aplanar, ~130 allocations por tick
  esculpido. La vía barata exige `parry` como dep directa.
- **C2 — hardware leído fuera de `input`,** en **13 archivos** (eran 15 el
  2026-08-01: la lista ya sólo encoge). Ya se cobraron `Tab` y el navegador de
  animación. El test de fase 1 ya impide que la lista crezca; falta el dueño
  único que traduzca bindings a acciones tipadas.
- **113 pares de sistemas ambiguos en `FixedUpdate`**, auditados uno por uno el
  2026-08-04 y congelados en `scheduling_audit::FIXED_UPDATE_AMBIGUITIES`. El
  número asusta más de lo que debe: **78 son los trece `propose` compartiendo el
  `ProposalBuffer`**, que la arbitración por `(Priority, weight)` ya neutraliza
  —hay un test que prohíbe los empates—, y **25 son `rebuild_terrain_collider`
  contra los cuerpos**, que comparten el tipo `Collider` pero no las entidades.
  De las 10 restantes, cuatro tenían consecuencia observable y quedaron
  ordenadas. **Para volver a listarlas por nombre hay que activar la feature
  `debug` de `bevy_ecs` un rato**: sin ella Bevy imprime placeholders, y después
  de `initialize` el grafo ya no puede resolver nombres por ninguna otra vía.
- **La pradera cuesta 489.200 triángulos por vista, casi 5× el presupuesto
  móvil entero** — y desde el 2026-08-06 sabemos que en esta máquina *eso no es
  lo que cuesta el frame*: es fill-bound, no vertex-bound (`BOTWGrass.md`). La
  deuda sigue declarada porque el target es un tiler y ahí el vértice se paga en
  bandwidth aunque no pinte, pero la palanca a tocar primero es el overdraw. El
  histórico de por qué existe:
- **La pradera costaba 52% del presupuesto del Mundo sobre el 0,6% de su área.**
  Cerrar el hueco del presupuesto (2026-08-04: `meadow_triangles` era privado y
  no la sumaba nadie) destapó que la escena Mundo declara **106.918 triángulos
  contra 100.000** — coincide con lo que el medidor de runtime venía gritando.
  El desglose: pradera 56.250, terreno 32.768, bosque 17.900. La pradera cubre
  625 m² de 320×320, así que **no escala**: la forma "campo horneado de tamaño
  fijo" no llega al mapa y afinarla no la va a hacer llegar. El exceso está
  declarado con número en `perf::budget::WORLD_SCENE_OVERSHOOT` y el test falla
  si crece. `BOTWGrass.md` se reescribió el 2026-08-04 contra el target real
  (Android tile-based) y la salida es una **grilla rodante centrada en la
  cámara** con densidad horneada por anillo, precedida por dos pasos que no
  cambian la imagen: enchufar el `ExtendedMaterial` que lleva meses registrado y
  sin usar, y **dejar el vértice en sólo la posición** (normal, color y uv son
  derivables; la uv sale de `vertex_index & 3` porque la brizna son 4 vértices en
  orden fijo) — 48 B a 12 B, imagen idéntica.
- **`GroundFacts.surface` se publica y nadie la consume.** El sensor la
  resuelve por punto de contacto y el HUD la muestra, pero ningún motor la usa:
  correr sobre arena, roca o pasto largo da exactamente el mismo movimiento.
  Encontrado el 2026-08-04 investigando la montura. Es el enganche natural de
  "la lluvia moja y afecta el agarre" (`NORTE.md`) y de la tracción por
  superficie; la tabla `KINDS` ya tiene dónde colgar el dato. Toca el feeling de
  locomoción, así que no entra sin checkpoint jugado.
- **El HUD de locomoción miente mientras montás.** Al montar, el player pierde
  `LocomotionEnabled` y los servicios dejan de actualizar sus facts, pero la
  sección sigue mostrando los últimos valores a pie **sin marcarlos como
  congelados**. El caballo ya tiene los suyos (`grounded`/`surface` en la
  sección Mount, 2026-08-04); falta que la del player diga que está en pausa en
  vez de mostrar un dato viejo como si fuera de ahora.
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
