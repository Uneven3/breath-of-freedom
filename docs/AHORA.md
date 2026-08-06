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
- **Medir: `BOF_BENCH=<suite> cargo run`**, con la ventana visible y sin nada
  más usando la GPU. **Preguntarle al usuario qué tiene abierto antes de la
  primera corrida**: el 2026-08-06 tres barridos dieron 10,89 / 11,88 / 3,83 ms
  para el mismo pasto con Blender y Firefox arriba. Detalle en "La suite de
  medición".
- **Ver sin jugar: `BOF_SHOT=<suite> cargo run`** deja un PNG en `target/shots/`
  y sale; `BOF_SHOT_POSE="x,y,z:dx,dy,dz"` fuerza un encuadre para reproducir una
  queja; `BOF_SCENE=Pasto cargo run` arranca dentro de una caja sin pasar por el
  menú. **A diferencia de los tiempos, la estadística de píxeles de una captura
  es determinista** y no la contamina la carga de la máquina — pero su piso de
  ruido es **5%**, porque el viento mueve las briznas entre disparos.
- **F7 en el juego captura lo que el usuario está viendo**, numerada, con la pose
  de cámara impresa ya formateada como `BOF_SHOT_POSE`. Es la única forma de que
  un reporte visual suyo llegue sin pasar por mi interpretación.
- **Medir en dev, no en release** — con una duda nueva. Las deps ya compilan en
  `opt-level 3` y la diferencia de perfil medida fue 0,38 ms contra deltas de
  4-12 ms. Pero el 2026-08-06 apareció evidencia de un **techo de CPU** que dev
  podría estar inflando, así que la premisa "el cuello es GPU" dejó de ser
  segura. Release tarda ~9 min y sigue reservado para validar un número absoluto
  — y ahora también para zanjar eso. Correr la secuencia **dos veces** y
  quedarse con la limpia.
- El feeling se valida jugando (checkpoint, §10): lanzar **`cargo run` a secas**
  en background para el usuario, redirigiendo a un log; al cerrar la sesión,
  leerlo filtrando `error|panic|took|destroyed` antes de reportar. **No forzar
  X11**: la máquina corre Wayland nativo. Verificado el 2026-07-26 — arranca en
  Vulkan/RADV sobre la Polaris 11.
- Debug in-game: **F1 abre el hub** — canales, perillas de render, acciones y la
  secuencia de medición, todo por click. Sobreviven dos teclas: `[`/`]` ciclan
  clips con el navegador abierto, y **P** vuelca el snapshot al log.
- **El log arranca callado (2026-07-26).** Todos los canales en `off`: el sink
  de consola escribía siempre y en un playtest de dos minutos puso **208 de 240
  líneas**, enterrando las 28 que hablaban del juego. El benchmark y el
  flythrough imprimen sus propias tablas, así que dejar todo apagado no cuesta
  ninguna medición.
- `RUST_LOG` **reemplaza** el filtro de Bevy en vez de sumarse, así que usarlo a
  secas devuelve el spam de `wgpu`/`naga`. Si hace falta:
  `RUST_LOG=wgpu=error,naga=warn,breath_of_freedom=debug`.
- **Antes de bindear una tecla, `grep -rhoE "KeyCode::[A-Za-z0-9]+" src/`.**
  Nadie arbitra colisiones (hallazgo C2), así que una tecla ya usada no da error:
  da una función que "no hace nada". **F7 se gastó el 2026-08-06** en la captura
  in-game; quedan **F9, F11, F12**.
- Commits a `main`, mensajes convencionales, sin push sin pedido explícito.
- **Compilar cuesta caro y la premisa del `build-dir` compartido no se cumple.**
  Cargo cachea por *set de features resuelto* y avian3d activa features que los
  proyectos sin avian no piden: medido el 2026-08-01, **39 variantes de
  `libbevy_render` y 136 GB** en `deps/`, y agregar un target nuevo recompila el
  árbol de bevy entero para ese target. El 2026-08-04 el disco se llenó y un
  `clean` del workspace lo devolvió a 39 GB. **Decisión abierta**: seguir
  compartiendo o volver a targets locales, que hacen `cargo clean` predecible —
  y nunca `rm -rf` sobre lo que Cargo administra.
- **Para listar las ambigüedades de scheduling por nombre**, activá un rato la
  feature `debug` de `bevy_ecs`: sin ella Bevy imprime placeholders y después de
  `Schedule::initialize` el grafo ya no resuelve nombres. Revertila después —
  crea otra variante en el build compartido.

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

**Pradera** (ver `BOTWGrass.md`): grilla rodante de tres anillos centrada en la
cámara, briznas horneadas en una malla por chunk, cero trabajo por frame. Nació
reemplazando un intento cuya documentación afirmaba "0.0 ms CPU" y "60 FPS
estables" el mismo día en que el medidor marcaba 35-46. **Regla que sale de ahí:
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

## El pasto (2026-08-06) — el detalle vive en `BOTWGrass.md`

**Decisión vigente del usuario:** primero que se vea bien, después se optimiza.
Target **900p30** (`NORTE.md` todavía dice 1080p60 y hay que actualizarlo).

**Lo que el día enseñó, y es más que lo que arregló:** siete arreglos distintos
apuntaron al mismo síntoma —*dónde y cuándo se ve la transición*— y ninguno lo
resolvió. El veredicto que ordena todo lo demás:

> *"Cuando el pasto se vea bien por sí solo, todo lo demás va a caber bien, lo
> cual no es cierto al revés."*

Y el contraejemplo que zanja la discusión de presupuesto: **Flower (PS3, 2009)**
llena la pantalla con una fracción de este hardware.

**Resuelto y confirmado jugando:** el parpadeo (era z-fighting, no MSAA: la
brizna colapsaba coplanar con el terreno y ahora se hunde 18 cm); la altura de
las briznas; la paleta, derivada del suelo donde se paran y no elegida a ojo
—*"la textura del suelo y del pasto se ven más uniformes"*—; y el LOD de brizna
de 3/2/1 triángulos por anillo, que salió gratis: *"se siente igual que antes"*.

**Abierto, en orden de cuánto sabemos:**

1. **El crecimiento se nota al avanzar.** Medido por primera vez el 2026-08-06
   desde una vista cenital: la densidad es plana hasta 10 m, cae **28% entre 10
   y 16 m**, y vuelve a aplanarse. Esa banda viaja con la cámara, así que el
   suelo que estaba ralo a 16 m se ve engordar al llegar a 10. Con la cámara 4 m
   detrás, son 5-6 m delante del player. Los dos parámetros que tocamos —alcance
   y dispersión— **mueven la banda pero no la borran**, y eso explica los cuatro
   intentos fallidos. La salida sin probar es la derivación del propio
   documento: sembrar con la caída `1/d` incorporada en vez de plantar plano y
   recortar al borde.
2. **Al inclinar la cámara el campo se ralea.** Las briznas no desaparecen: se
   acortan en pantalla. El detalle cae al 78% donde el ángulo predice 76% — pero
   *detalle de borde es un proxy*, y el usuario duda con razón. Zanjarlo pide
   contar píxeles de silueta, no gradiente. Sólo lo arregla que la brizna deje de
   ser un plano vertical.
3. **El horizonte no se llena.** Sin tocar.
4. **La tabla de anillos del código está 5× por encima de la derivada**, y el
   disparador que `BOTWGrass.md` dejó armado —*"si el barrido confirma
   fill-bound, bajar el anillo 0 es la palanca más barata"*— ya se cumplió sin
   que nadie actuara.

**Dos lecciones de método que no son sobre pasto:**

- **Un mensaje de éxito que no se distingue de un fracaso no es un mensaje de
  éxito.** `"packed 4 canonical PNGs"` se imprimía igual con el arte cargado que
  con el fallback de 1×1, y por eso el terreno mostró el fallback durante meses.
- **Un número no sirve si el objetivo contra el que mide se eligió a ojo.** Dos
  veces ese día medí con tres decimales la distancia hasta un blanco supuesto.

## La suite de medición (2026-08-06)

**Correr un barrido ya no requiere jugar.** `BOF_BENCH=<suite> cargo run` entra
a la caja de la suite, se para en su mirador, mide, escribe la tabla y cierra el
proceso. Tres suites, en `perf/suite.rs` como **dato**: `grass`, `general`,
`shadows`. Agregar una es una variante y una tabla — el motor (`sequence.rs`) no
se toca. También hay un botón por suite en el hub F1. Los porqués están en los
doc-comments de `suite.rs` y `auto.rs`; acá sólo lo que no se deduce del código.

Reglas que los tests cobran sobre toda suite, presente y futura: empieza y
termina en el baseline (la diferencia es la deriva), cada paso mueve **un** eje,
y el baseline es la configuración que se envía. El reporte abre declarando
suite, pregunta, escena, perfil, ventana, MSAA, render scale, densidad y alcance
del pasto, sombras y mirador.

**Perillas nuevas:** `msaa` y `grass-reach`, más un paso de **0 briznas/m²** que
convierte "cuánto cuesta el pasto" de extrapolación en resta.

**Tres trampas de esta máquina, encontradas corriendo:**

- **Cerrar Blender (o cualquier cosa que use la GPU) antes de medir.** Con él
  abierto, tres corridas de la misma configuración dieron costos del pasto entre
  2,36 y 3,77 ms, con derivas internas de 0,2. Lo externo no aparece en la tabla.
- **La ventana tiene que estar visible.** En Wayland nativo el compositor no
  manda frame callbacks a una superficie oculta y el juego entero se duerme —
  1,9 s de CPU en 105 de reloj, sin medir nada. Para correr en segundo plano:
  `WINIT_UNIX_BACKEND=x11 WAYLAND_DISPLAY= BOF_BENCH=grass cargo run`. **Sólo
  para medir**; el juego se juega en Wayland nativo.
- **El frame suele quedar clavado por la presentación** mientras la GPU varía.
  El reporte lo detecta y avisa que hay que leer `d-gpu`. El criterio compara
  los dos recorridos entre sí y no cada uno contra un umbral — la primera
  versión usaba umbrales sueltos y dejó pasar una corrida con la GPU al 203% y
  el frame al 6,3%.

**Pendiente que la suite destapó:** ocultar el bosque entero desde el mirador
canónico vale 0,34 ms, lo que sugiere que **el mirador "del bosque" no mira al
bosque**. Autorearlo con F4 sigue sin hacerse y ahora tiene evidencia.

**Ruido de Bevy, no nuestro:** cada corrida imprime ~270 líneas
`bevy_render::slab_allocator: Use-after-free`, al despawnear muchas mallas de
golpe. No rompe la corrida ni los números; ensucia el log y no está investigado.

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
  fingían ser baratos; se reemplazaron por proxies procedurales instanciados.
- **El costo es propiedad de la representación, no de la identidad.** `TreeKind`
  resuelve a dos tiers en `VisualCatalog`; impostores e instancing se enchufan
  ahí sin tocar simulación.
- **Ceguera medida:** el total `gpu:` suma solo spans registrados y los pases de
  sombra usan `info_span!`. "El gpu medido no cambió" **no** implica "no es GPU"
  — e invocar esa ceguera para culpar a las sombras fue el error simétrico. Lo
  que zanjó la duda fue *quitar la escena*.
- **El medidor dice *cuándo* una técnica vale la pena; no se aplican todas
  siempre.** Eso es cargo-culting y frena al dev, no al juego.
- Último perfil móvil medido: **37,3k tris, 62 draws, 53 mats → "medio", por
  materiales.** De ahí sale la ley 1 de `TEXTURES.md`.
- **Fill antes que geometría (2026-08-06, medido en las tres suites).** En la
  caja Pasto bajar la resolución a la mitad ahorra más GPU que apagar la pradera
  entera. El frame se va en píxeles pintados, no en vértices transformados —
  vale para elegir en qué orden atacar cualquier sistema visual nuevo, con la
  salvedad de que el target es un tiler, donde el vértice también se paga.

### Presupuesto de polígonos como contrato (2026-07-25)

**Conteos sí, milisegundos no.** Los tris/draws/materiales son *dato*:
deterministas, testeables, pueden romper el build. Los tiempos son *medición*: un
test de ms falla por ruido, se ignora y muere. Carriles separados.

- `build.rs` cuenta triángulos por LOD al importar cada GLB; el presupuesto vive
  en `schema.rs::lod0_triangle_budget` y **falla el build nombrando el asset** —
  no un `warn!` que se lee después, que es como pasaron unos pies de 9172 tris.
- Tests de escena (`perf/budget.rs::static_cost`) suman lo que cada fila de
  `SCENES` declara. Es lo que el contador de runtime no puede ser: él grada lo
  que la cámara ve, así que una escena pasada puede leer "bien" desde un rincón.
  **Y el de runtime tuvo su propio agujero hasta el 2026-08-06**: consultaba
  mallas por tipo de material y `GrassMaterial` era un tercero, así que nunca
  contó la pradera. Ahora cuenta por `Mesh3d` y una ley de arquitectura exige
  que todo `MaterialPlugin` registrado aparezca ahí.
- El terreno son **32768 tris fijos** en toda escena — un tercio del presupuesto
  móvil antes de poner nada encima. Subir `CELLS` es una decisión de presupuesto.
- Un draw call exacto **no** es testeable sin cámara; lo testeable es la cota.
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
rechazan un GLB authored incompleto, pero no hay rig runtime mientras el player
sea la cápsula graybox. Blending e IK viven en sus docs de dominio.

**Facing unificado**: `FacingSource { Free, Look, LockOn(Entity) }` con
`resolve_facing` como dueño único tras `TickActiveMotor`, así los motores no
pelean por rotar el cuerpo. Encima: lock-on, intención facing-relativa con
`StrafeDir` (circle-strafe real), y arco que auto-apunta al objetivo lockeado.

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
- **113 pares de sistemas ambiguos en `FixedUpdate`**, auditados uno por uno y
  congelados en `scheduling_audit::FIXED_UPDATE_AMBIGUITIES`. Asustan menos de lo
  que parecen: 78 son los `propose` compartiendo el `ProposalBuffer` (la
  arbitración por `(Priority, weight)` los neutraliza) y 25 son el collider del
  terreno contra los cuerpos. De las 10 restantes, cuatro quedaron ordenadas.
  **Para listarlas por nombre hay que activar la feature `debug` de `bevy_ecs`**
  — sin ella Bevy imprime placeholders y después de `initialize` ya no hay vía.
- **La pradera cuesta 600.000 triángulos por vista, seis veces el presupuesto
  móvil entero.** En esta máquina *eso no es lo que cuesta el frame* —es
  fill-bound—, pero la deuda sigue declarada porque el target es un tiler y ahí
  el vértice se paga en bandwidth aunque no pinte. **La palanca a tocar primero
  es el overdraw**, y el dial del hub nunca se usó. El anillo interior de 16 m es
  la primera candidata a revertir: costó +73% y no compró nada verificable.
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
