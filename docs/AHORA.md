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
  queja; `BOF_KNOBS="grass-view=5,msaa=1"` fija cualquier perilla del hub desde
  el arranque; `BOF_SCENE=Pasto cargo run` arranca dentro de una caja sin pasar
  por el menú. **A diferencia de los tiempos, la estadística de píxeles de una
  captura es determinista** y no la contamina la carga de la máquina — pero su
  piso de ruido es **5%**, porque el viento mueve las briznas entre disparos.
- **Siempre `cargo run`, nunca `./target/debug/...` a secas.** Bevy busca
  `assets/` junto al ejecutable, así que el binario directo arranca **sin un solo
  shader** y todo lo demás sigue funcionando: el 2026-08-07 una corrida así sacó
  una foto de puro cielo mientras el inventario reportaba 691.200 triángulos de
  pradera al 95% del cuadro. Ahora la foto y la tabla avisan si algún asset falló
  — pero el aviso existe porque el caso ocurrió.
- **Contar píxeles: lo hace la misma corrida** (`BOF_KNOBS=grass-view=6`, la
  vista `medir`). El informe sale en el log junto a la foto: cobertura total, el
  reparto por anillo y el **perfil por distancia** en metros. Reemplaza los
  perfiles por detección de bordes, que saturan con densidad alta. **Omite el
  perfil y dice por qué** cuando la fila de pantalla no se puede convertir en
  distancia — cámara sin perspectiva, `render-scale` puesto, o más de 20 cm de
  relieve bajo la línea de vista. Hasta el 2026-08-08 esto era
  `tools/shot_stats.py`, que decodificaba el PNG por su cuenta; el port a Rust se
  verificó dando el mismo conteo píxel a píxel sobre la misma captura.
- **Antes de explicar una diferencia entre dos configuraciones, sacar la misma
  captura dos veces.** Cuesta tres minutos y el 2026-08-07 fue lo único que
  destapó un bug que apagaba un nivel entero de la pradera con resultado distinto
  en cada corrida: cada foto suelta parecía una configuración con su explicación
  plausible. La estadística de píxeles es determinista salvo el viento (piso 5%),
  así que **una diferencia grande entre dos corridas idénticas es un bug, no
  ruido**.
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
- **El `build-dir` compartido no cumple su premisa.** Cargo cachea por set de
  features resuelto y avian3d activa las que otros proyectos no piden: medido el
  2026-08-01, **39 variantes de `libbevy_render` y 136 GB**. Decisión abierta;
  nunca `rm -rf` sobre lo que Cargo administra.
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

**Pradera** (ver `BOTWGrass.md`): grilla rodante de cuatro niveles centrada en la
cámara, briznas horneadas en una malla por chunk. *"Cero trabajo por frame"* fue
una afirmación de este documento hasta que se midió: hornear un chunk cuesta
**5,5 ms de media y hasta 9,5**. Nació reemplazando un intento cuya documentación
afirmaba "0.0 ms CPU" y "60 FPS estables" el mismo día en que el medidor marcaba
35-46. **Regla que sale de ahí: ningún número entra sin salir del medidor.**

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

## El pasto — el detalle vive en `BOTWGrass.md`

**Norte (2026-08-07):** el feeling de BOTW en low-poly, y **el móvil dejó de ser
un veto** — ver `NORTE.md`. Primero que se vea bien; el profiler y la adaptación
al target vienen después. Target de imagen **900p30**.

> *"Cuando el pasto se vea bien por sí solo, todo lo demás va a caber bien, lo
> cual no es cierto al revés."* — y el contraejemplo que zanja la discusión de
> presupuesto: **Flower (PS3, 2009)** llena la pantalla con una fracción de este
> hardware.

**Resuelto y jugado:** el parpadeo (era z-fighting, no MSAA); la altura; la
paleta derivada del suelo; el LOD de brizna; la **brizna de dos triángulos con
arista horizontal** (misma cobertura, menos geometría, y ahora puede arquearse);
la **carta opaca** en el nivel lejano (ocho veces menos y pinta más); y el **LOD
por tamaño en pantalla** en vez de por radio autorado.

**Abierto, en orden de cuánto sabemos:**

1. **Los anillos son el problema de fondo**, identificado por el usuario tras
   tres sesiones y confirmado midiendo. Un nivel decidía cuatro cosas; tres ya se
   separaron y falta la **semilla**, que sigue incluyendo el nivel — por eso
   cruzar una frontera reemplaza briznas en vez de agregarlas. La reescritura
   anidada se implementó, se midió y se revirtió: cuesta 2,9× y deja agujeros,
   porque el modelo de densidad está mal **en la forma**. Antes de reintentarla
   hay que medir la curva de cobertura contra densidad a varias distancias.
2. **El borde de un nivel es un cuadrado** (Chebyshev, cuantizado a chunks). Se
   hizo visible al derivar las densidades. Misma causa raíz: el LOD horneado.
3. **La salida de fondo:** que la brizna deje de ser geometría y pase a ser un
   registro — `MeshTag` + `ShaderStorageBuffer` + instancing, que Bevy soporta de
   fábrica conservando `ExtendedMaterial`.
4. **Con el veto levantado**, la carta con **alfa recortado** es el próximo
   experimento: le daría silueta de briznas en vez del borde superior plano.
5. **El horizonte no se llena.** Sin tocar.

**Tres lecciones de método que no son sobre pasto:**
## La suite de medición (2026-08-06)

**Correr un barrido ya no requiere jugar.** `BOF_BENCH=<suite> cargo run` entra
a la caja de la suite, se para en su mirador, mide, escribe la tabla y cierra.
Tres suites en `perf/suite.rs` como **dato**: `grass`, `general`, `shadows`;
agregar una es una variante y una tabla, el motor no se toca. Los porqués están
en los doc-comments de `suite.rs` y `auto.rs`.

Reglas que los tests cobran sobre toda suite: empieza y termina en el baseline
(la diferencia es la deriva), cada paso mueve **un** eje, y el baseline es la
configuración que se envía.

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
  El reporte lo detecta y avisa que hay que leer `d-gpu`.

**El reporte declara qué hay en cuadro (2026-08-07).** Cada corrida abre con el
reparto por sistema —`pradera=15%/92% bosque=15%/0% terreno=1%/5%`, mallas y
triángulos— y avisa cuando el tema que la suite dice medir cae por debajo del
10% en cualquiera de las dos. Un mirador es una afirmación, y hasta ahora
ninguna corrida podía desmentirla: el test que había verificaba que la *escena*
declarara bosque, que es otra cosa y siempre pasaba.

**Y lo primero que dijo cierra un pendiente viejo con mejor diagnóstico.**
Ocultar el bosque valía 0,34 ms y se leyó como "el mirador no mira al bosque".
El número real: desde ahí el bosque es el **0% de los triángulos** porque la
pradera se lleva el **92%**. El mirador no está mal apuntado — la suite general
mide un frame que es casi todo pasto. Medir el bosque pide su propia caja.

**Ruido de Bevy, no nuestro:** cada corrida imprime ~270 líneas
`bevy_render::slab_allocator: Use-after-free`, al despawnear muchas mallas de
golpe. No rompe la corrida ni los números; ensucia el log y no está investigado.

## Crates: cerrado (2026-08-04), y lo que quedó vivo de ahí

Las ocho fases están en `git log -- docs/CRATES.md` y las leyes en
`ARCHITECTURE.md`. Lo que sigue informando decisiones y no se deduce del código:

- **Tres decisiones que se apartaron del plan.** `input` no cruzó a simulación:
  dejándolo afuera, simulación **no puede** leer teclado en vez de acordarse de
  no hacerlo. `layout`/`spawn` tampoco: son composición. El reloj sí cruzó, la
  luz no (§20).
- **Los newtypes de unidades se midieron y se descartaron**, y
  **`bof_presentation` no se creó a propósito**: con las referencias en cero y la
  ley congelada en un test, no agregaba nada.
- **Al leer un log, ojo:** una escena sin heightmap arranca plana **y en
  silencio** — la ausencia de línea no es ausencia de escena.

Siguiente en esa línea: instancias discretas, cerrar el ciclo semántico, y jugar
graybox sobre relieve + tipografía.

## Rendimiento: lo que sigue informando decisiones

**Y desde el 2026-08-07 va después del feeling** (`NORTE.md`): el profiler y la
adaptación al target se construyen cuando la imagen esté lograda. El peor punto
del bosque pasó de ~72 ms a nunca bajar de 60 (2026-07-21); el detalle está en
git y quedaron los principios:

- **El graybox tiene que ser honesto sobre el costo.** Los árboles Quaternius
  fingían ser baratos; se reemplazaron por proxies procedurales instanciados.
- **El costo es propiedad de la representación, no de la identidad.** `TreeKind`
  resuelve a dos tiers en `VisualCatalog`; impostores e instancing se enchufan
  ahí sin tocar simulación.
- **Ceguera medida:** el total `gpu:` suma solo spans registrados y los pases de
  sombra usan `info_span!`. "El gpu medido no cambió" **no** implica "no es GPU".
  Lo que zanjó la duda fue *quitar la escena*.
- **El medidor dice *cuándo* una técnica vale la pena**; aplicarlas todas siempre
  es cargo-culting y frena al dev, no al juego.
- Último perfil móvil medido: **37,3k tris, 62 draws, 53 mats → "medio", por
  materiales.** De ahí sale la ley 1 de `TEXTURES.md`.
- **Fill antes que geometría (2026-08-06, medido en las tres suites).** En la
  caja Pasto bajar la resolución a la mitad ahorra más GPU que apagar la pradera
  entera: el frame se va en píxeles pintados, no en vértices transformados. Vale
  para elegir en qué orden atacar cualquier sistema visual nuevo.

### Presupuesto de polígonos como contrato (2026-07-25)

**Conteos sí, milisegundos no.** Los tris/draws/materiales son *dato*:
deterministas, testeables. Los tiempos son *medición*: un test de ms falla por
ruido, se ignora y muere. Carriles separados.

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
  triángulos y draws, que es el número que cada ajuste del pasto necesitaba y no
  existía.
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
pose como `Waypoint` pegable). El flythrough acumula frame/gpu/tris/draws/mats
**por tramo**, así correr la misma ruta hoy y en un mes compara peras con peras.

Pendiente: **autorear la ruta canónica real** con F4 — hoy los tramos se llaman
`spawn→clearing` en cajas que no tienen ni claro ni bosque, y una regresión de
~1 ms de GPU pasó dos días inadvertida. El nombre de un tramo es una afirmación
que nada verifica, igual que lo era el mirador del bosque antes de que el
reporte midiera qué hay en cuadro.

Diferido: impostores, streaming por chunks y **occlusion culling** — el de Bevy
es experimental vía meshlets, y la Polaris 11 del dev no los soporta.

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

`world/layout.rs` **es** un BSN artesanal. Reverificado en `bevy_scene 0.19` el
2026-08-05: `bsn!`, `SceneComponent` y el parcheo por campo existen y se usan
dentro de Bevy; el formato de archivo `.bsn` **no**, y nada serializa una
`Scene`. Por eso BSN no puede ser el archivo de nivel, y sí la forma correcta de
resolver `kind` → entidades. Plan en `MAP_EDITOR.md`; la prueba barata para
decidir cuándo migrar es reescribir `spawn_stair_segment` como `bsn!` y ver si
queda más legible.

## Deudas anotadas (pagar cuando el gameplay las pida)

- **Paleta/IDs Rust↔WGSL sin unificar, y `terrain_material.rs` sin dividir**
  (§1, §16). Viene de la auditoría del 2026-08-02; no bloquea, pero cada color
  nuevo hay que escribirlo en dos lados que nadie obliga a coincidir.
- **`InventorySet` y `MountsSet::PostMove` comparten crate** y su orden mutuo
  sigue sin declarar: es una línea al lado de las otras cuatro.
- **C1 — allocation en `FixedUpdate`:** `rebuild_terrain_collider` arma un
  `Vec<Vec<f32>>` que Avian vuelve a aplanar, ~130 allocations por tick
  esculpido. La vía barata exige `parry` como dep directa.
- **C2 — hardware leído fuera de `input`,** en **13 archivos** (eran 15). El
  test impide que la lista crezca; falta el dueño único que traduzca bindings a
  acciones tipadas.
- **113 pares de sistemas ambiguos en `FixedUpdate`**, auditados uno por uno y
  congelados en `scheduling_audit::FIXED_UPDATE_AMBIGUITIES`. Asustan menos de lo
  que parecen: 78 son los `propose` compartiendo el `ProposalBuffer` (la
  arbitración por `(Priority, weight)` los neutraliza) y 25 son el collider del
  terreno contra los cuerpos. De las 10 restantes, cuatro quedaron ordenadas.
  **Para listarlas por nombre hay que activar la feature `debug` de `bevy_ecs`**
  — sin ella Bevy imprime placeholders y después de `initialize` ya no hay vía.
- **La pradera cuesta ~690.000 triángulos por vista** (medido en cuadro, no
  declarado), y es el **92% de los triángulos del frame** en el Mundo. En esta
  máquina eso no es lo que cuesta —es fill-bound—, pero la deuda sigue declarada
  porque el target es un tiler. **La palanca a tocar primero es el overdraw**, y
  ahora hay dos hallazgos concretos para atacarlo: el dial del hub sigue sin
  usarse, y el primer plano está plantado por cuatro anillos a la vez.
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
