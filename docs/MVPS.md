# MVPs incrementales — en qué orden se construye lo que los planes proponen

Los documentos de dominio dicen **qué** hay que construir y por qué. Este dice
**en qué orden**, y agrupa el trabajo en incrementos que se pueden jugar y
medir de a uno.

> **Qué es este documento.** Un plan de ingeniería transversal, como fue
> `docs/CRATES.md`. **Temporal por definición**: cada MVP se borra de acá cuando
> cierra, y cuando no quede ninguno el archivo se borra entero. No fija leyes —
> las leyes viven en `ARCHITECTURE.md` y en los documentos dueños de cada tema.
> Si hay contradicción entre esto y el documento dueño, manda el dueño.
>
> **Cómo se usa.** Uno por vez, de arriba hacia abajo. Un MVP no está cerrado
> hasta que se **jugó** y su criterio de aceptación se cumplió; si al jugarlo
> aparece algo mejor que hacer, se reordena la lista y se escribe por qué.
>
> **Qué es un MVP acá.** El incremento más chico que deja el juego
> **medible o mejor**, no una tarea. Varias tareas de documentos distintos
> pueden caer en el mismo MVP si comparten criterio de aceptación.

---

## MVP 0 — Poder medir

**Sin esto, todos los MVP que siguen se justifican con estimaciones.** Es el
único bloque cuyo entregable no cambia nada de lo que se ve, y es el que hace
que los demás sean decidibles en vez de opinables.

**Qué se construye**

1. **Hecho — los diales sueltos entraron al hub.** `GrassDensity` ya es un
   `PerfKnob`, no existe el binding F8 y la secuencia le da warmup, asentamiento
   y chequeo de deriva.
   *(`BOTWGrass.md` Paso 0 · `GraphicalTechniques.md` ley 6.)*
2. **El overdraw publica un número.** Hoy es un mapa de calor aditivo que
   satura alrededor de las 17 capas: responde "¿dónde?" pero no "¿cuánto?" ni
   "¿mejoró?". Es el instrumento principal para un GPU tile-based y es el único
   que no se puede poner en una tabla A/B.
   *(`GraphicalTechniques.md` Fase 1.)*
3. **El juego arranca en un teléfono.** No existe build de Android en el repo:
   ni script, ni receta, ni una sola corrida. Todo lo que los documentos llaman
   "el target" es hoy un razonamiento sobre una arquitectura. No hace falta que
   corra bien — hace falta que **arranque y se pueda medir**.
   *(`GraphicalTechniques.md` Fase 5.)*

**Criterio de aceptación**

Una corrida de la secuencia A/B con densidad de pasto como dial, con su paso de
baseline repetido mostrando la deriva; una lectura de overdraw con cifra
antes/después de un cambio conocido; y una captura del juego corriendo en el
teléfono, con su número de frame, aunque sea malo.

**Qué desbloquea:** todo. En particular, el resultado puede reordenar los MVP
5 y 6: si el pasto resulta fill-bound, la palanca es la densidad del anillo
cercano y no el shader.

---

## MVP 1 — Pintar significa algo

`Soil` es tierra desnuda; `ShortGrass` y `TallGrass` son la cobertura vegetal.
La pradera debe aparecer donde esas celdas se pintan, y el bosque sigue siendo
un scatter procedural hasta su propio corte de autoría.

**Qué se construye**

- `Terrain::contains_kind_in_rect(kind)` en `terrain/query.rs`.
- `visuals::grass` genera sus chunks donde hay `ShortGrass` o `TallGrass`
  pintado, en vez de sobre un campo fijo. El primer corte reconstruye el
  vecindario visible; queda acotarlo a los chunks tocados al repintar.

*(`MAP_EDITOR.md` Paso 1.)*

**Criterio de aceptación**

Pintar una franja de pasto corto/largo cruzando una colina y verla crecer
siguiendo el relieve, sin reiniciar, con el conteo de triángulos del hub
moviéndose con lo pintado.

**Por qué acá:** es el incremento más barato que convierte al editor en una
herramienta que *hace* algo, y es prerrequisito honesto del MVP 6 — no tiene
sentido optimizar una pradera cuyo tamaño y lugar todavía no se pueden autorar.

---

## MVP 2 — Se pueden poner cosas

El paso que convierte esto en un editor de mapas. Hoy los objetos del mundo
viven en `world/layout.rs` (656 líneas de tablas Rust) y en un scatter
determinista: un mapa nuevo no se puede hacer sin recompilar.

**Primer corte construido, 2026-08-14 — alcance reducido a propósito:**
colocar, borrar y persistir. Rotar/escalar interactivo y deshacer/rehacer
quedan explícitamente para después de jugar este corte — separar el historial
en un `HistoryStep` enum es trabajo real y no vale la pena pagarlo antes de
saber si el resto sirve.

- `Instance { kind: PropKind, xz, yaw, scale }` y `PropKind` (6 variantes de
  pasto, hoy) en `bof_domain::props` — sin handles, sin rutas, sin materiales.
  La altura **no se guarda**: se muestrea del terreno al spawnear. Requirió
  sumarle `serde` a `bof_domain` (§17, con OK explícito) y el feature
  `serialize` de `bevy_math` (`Vec2` no serializa sin él).
- `#[serde(default)] instances: Vec<Instance>` en `TerrainFile`, y un
  `instances_revision: u32` en `Terrain` (mismo patrón que `relief_revision`)
  para que la presentación no reconstruya todo en cada trazo de escultura.
  No se remuestrean: ya están en metros — probado en las dos ramas de
  `apply_ron` (copia directa y remuestreo), no sólo en una.
- `ToolLayer::Instances`, tercera capa del editor F5/F6. **Interacción de
  pincel, no de click**: LMB arrastrado esparce dentro del círculo (con
  separación mínima entre instancias), RMB arrastrado borra todo lo que cae
  adentro — mismo radio compartido que Relieve/Semántica, para que la
  herramienta se sienta como una sola cosa en vez de tres reglas distintas.
  Deliberadamente sin deshacer: el HUD lo dice en la capa Instancias en vez de
  mostrar un contador de deshacer/rehacer que no le pertenece.
- `PropKind` → escena: **primer uso real de `bsn!`/`SceneComponent`** del
  proyecto (`src/visuals/instances.rs`). Sin tier de proxy ni máquina de
  estados de swap — a diferencia de `forest.rs`, que sí la tiene y que el
  usuario identificó como parte de por qué dos semanas de pelea con el pasto
  procedural no llegaron a buen puerto. La sincronización con `Terrain` es
  incremental en el caso caliente (esparcir sólo agrega la cola nueva, sin
  reconstruir lo ya colocado); un borrado sí reconstruye todo, porque
  `Vec::retain` no dice qué filas sacó.

*(`MAP_EDITOR.md` Paso 2.)*

**Criterio de aceptación — sin jugar todavía**

Colocar props de pasto, guardar, cerrar el juego, volver a abrirlo y
encontrarlos donde estaban. Después esculpir debajo y verlos seguir el suelo.
Y el que de verdad importa acá: si una pradera hecha de instancias `.glb` se
ve mejor que el meadow procedural — la pregunta que motivó todo este MVP.

---

## MVP 3 — Que un golpe no cree nada

El MVP más barato de la lista y el único cuyo efecto se lee sin cronómetro.

**Qué se construye**

- Mesh y material del hit burst pasan a un recurso creado una vez, en vez de un
  `meshes.add` + `materials.add` **por impacto**.
- La chispa deja de ser una icosfera de **720 triángulos** —el default de
  `Sphere` en Bevy— y pasa a un icosaedro sin subdividir (20 tris) o un quad.
  Son bolitas de 7 cm que viven 0,22 s: hoy ocho de ellas son 5.760 triángulos
  por golpe.
- El arco de barrido usa un pool indexado por `(reach, arc_deg)` en vez de
  crear malla y material por swing.
- `VfxBudget`: el recurso que cuenta entidades transitorias vivas y descarta la
  más lejana al llenarse. Está escrito como ley desde hace tiempo y no existe.

*(`PARTICLES.md` Pasos 1 y 2, leyes 3 y 6.)*

**Criterio de aceptación**

Golpear veinte veces seguidas con el hub F1 abierto: `mats` y `tris` quedan
planos, cuando hoy suben con cada impacto. Y una pelea con seis enemigos sin
que el frame se despegue.

---

## MVP 4 — El suelo se paga como corresponde

El terreno es la superficie de mayor cobertura de pantalla del juego, y hoy se
muestrea de la peor forma posible.

**Qué se construye**

1. **`basis-universal` habilitado** en el `Cargo.toml` de este juego
   (`bevy = { workspace = true, features = ["basis-universal"] }`). Sin esa
   feature, Bevy **no transcodifica** KTX2 universal, que es exactamente lo que
   el plan de texturas da por hecho. Verificado con `cargo tree`: hoy están
   `ktx2` y `zstd_rust`, no `basis-universal`.
2. **Mips en el array del terreno.** `array_image` construye la imagen con un
   solo nivel aunque el sampler pida filtrado de mip y anisotropía 16×. Sin
   mips, la minificación lee memoria al azar: en un tiler eso es bandwidth puro
   y es invisible en escritorio.
3. **Anisotropía a lo que se note.** 16× es el valor más caro posible y se
   eligió sin medir. A/B entre 16×, 4× y 1× **después** de que existan mips.
4. **`assets/textures/SOURCES.ron`** con una fila por archivo (autor, origen,
   licencia SPDX) y un test que falle si hay un PNG sin fila. Hay diez PNG en el
   repo sin procedencia declarada, en un proyecto GPL.

*(`TEXTURES.md` Pasos 0, 4 y 5.)*

**Criterio de aceptación**

El array entero bajo el tope de 2 MB, con mips, indistinguible del PNG a
distancia de juego; una lectura de overdraw antes/después (si los mips valen
algo, se ve en el suelo); y el test de procedencia en verde.

---

## MVP 5 — Las sombras dejan de tener un escalón

Las sombras son la palanca más cara ya medida del proyecto: llevar las hojas a
no castear y el mapa a 1024 px las bajó de ~70% del frame a 2,74 ms.

**Qué se construye**

- **Cerrar la ventana crepuscular.** Sol y luna deciden por separado si
  castean, y en el cruce los dos superan el umbral: ~1,3 minutos reales de
  **cascadas dobles** en cada crepúsculo, dos veces por día de juego, justo
  cuando el sol rasante produce los volúmenes más grandes. El corte pasa a ser
  comparativo: sólo el astro dominante castea.
  **Hecho en código y cubierto por barrido minuto a minuto (2026-08-10); falta
  checkpoint jugado.**
- **El caso del relámpago, de una vez.** Un destello de 50.000 lux cruza el
  umbral y enciende las cascadas por un frame — el frame que tiene que ser
  instantáneo. El destello va en una luz que no castea.
- **La matriz de sombras, corrida en el teléfono** (necesita MVP 0): mapa,
  distancia, alcance de casters y cascadas. Los defaults actuales se eligieron
  midiendo en una GPU de escritorio.

*(`LIGHTING.md` ley 2, Fases 1 y 4.)*

**Criterio de aceptación**

Un test que barra la hora del cruce y afirme que nunca hay dos direccionales
casteando; el frame del crepúsculo sin escalón; y una tabla de sombras con
números del target.

---

## MVP 6 — El pasto se paga solo

**Adelantado por decisión explícita del 2026-08-07/10:** se priorizó lograr el
feeling antes de terminar la instrumentación móvil del MVP 0. La base ya está
activa: `ExtendedMaterial`, registros de 16 B, terreno teñido y tres anillos por
tamaño en pantalla. No cierra mientras el borde/anillo siga visible.

**Qué se construye, en este orden**

1. Medir una baseline repetible con la implementación activa.
2. Perturbar por posición de mundo la distancia de transición para romper los
   círculos de LOD sin mover las briznas entre niveles.
3. Sólo si lo anterior no basta, sesgar el LOD por posición en pantalla.
4. Resolver el borde lejano con otra técnica; aumentar otra vez el alcance no
   cuenta como solución.

*(`BOTWGrass.md`, Fases 0 a 2. Ahí están las cuentas.)*

**Criterio de aceptación**

Cada paso se cierra jugando la caja `Pasto` y midiendo con la secuencia A/B. No
debe verse el anillo donde cambia el LOD ni el borde del campo.

---

## MVP 7 — El personaje se mueve como un personaje

Hoy el juego no reproduce ninguna animación, y la cápsula del jugador es un
poste de un metro de diámetro.

**Qué se construye**

1. **La cápsula correcta, como paso propio.** `BodyDimensions::PLAYER` pasa de
   radio 0.5 a 0.35. No es cosmético: el radio decide por dónde cabe, cómo se
   pega a la pared al escalar y cuánto sobresale al hacer mantle, y hay umbrales
   de `AutoVault`, `EdgeLeap` y `Climb` afinados contra el valor viejo sin
   saberlo. Va **antes** de afinar animaciones contra la escala, o se afina dos
   veces.
2. **Un clip se reproduce.** `src/visuals/animation.rs` con `AnimationRole`,
   `ROLE_TABLE` (cadenas de fallback), `CharacterAnimations` y el resolutor que
   aplica el rol al `AnimationPlayer` con crossfade. El contrato del que se
   cuelga —`PLAYER_CLIP_CONTRACT` y el guardrail de `build.rs`— ya existe y no
   tiene consumidor.
3. **Sin foot-sliding:** `k_speed_node = V_real / V_autorada`, protegido bajo
   0,05 m/s.

*(`BOTWMovements.md` secciones 3 y 5 · `CHARACTER_ANIMATION_IK.md` Pasos 0 y 2.)*

**Criterio de aceptación**

Rejugar la caja `Traversal` completa después del cambio de cápsula —escalar,
mantle, vault, salto de borde— y que ninguno se sienta distinto de como estaba
afinado. Después: el personaje camina y su animación camina con él.

---

## MVP 8 — El mundo suena y termina en algún lado

**Qué se construye**

1. **El primer sonido de verdad.** Un `.ogg` propio para `Step` sobre `Grass`,
   reproducido con `AudioPlayer`. Hoy `play_audio_cues` imprime por log y no hay
   un solo archivo de audio en el repo. Este paso descubre lo que no se sabe:
   latencia, volumen relativo, si el ritmo de 2 m se siente bien al correr.
   Después, la tabla `SurfaceKind → Handle<AudioSource>`, la cota espacial de
   25 m y el presupuesto de voces.
2. **Decidir qué es la niebla.** `FOG_MAX_ALPHA = 0.3` es un techo duro sobre la
   mezcla: a cualquier distancia el terreno lejano sigue siendo 70% él mismo, o
   sea que la niebla **no puede** cerrar el horizonte, aunque dos documentos lo
   prometían. Se elige: o el alfa llega a 1.0 y `start`/`end` se ajustan para
   que el juego cercano siga limpio, o se acepta que es atmósfera y no LOD y se
   borra la promesa.

*(`AUDIO.md` Fase 2 · `LIGHTING.md` Fase 3 · `TEXTURES.md` Paso 11.)*

**Criterio de aceptación**

Caminar por terrenos distintos y escuchar el cambio, con una lectura de CPU con
y sin audio. Y, desde el punto más alto que se pueda esculpir, o el terreno se
funde con el cielo o está escrito que no es el objetivo.

---

## Lo que deliberadamente no está en esta lista

- **Billboards, cartas de grupo y shell texturing** para el pasto: descartados
  con cuentas en `BOTWGrass.md`, no por costo de implementación.
- **Vertex pulling**: descartado por aritmética, con un disparador falsable
  escrito para reabrirlo.
- **Volumetric fog, SSAO, toon shading, cubemap de cielo**: cada uno tiene su
  motivo escrito en el documento dueño.
- **Todo lo que dependa de arte final.** Estos MVP están ordenados para que
  ninguno quede bloqueado esperando un asset: los que tocan arte usan lo que ya
  hay, o placeholders propios.
