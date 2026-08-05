# El editor de mapas — qué autora, qué le falta y cómo se completa

La herramienta de autoría dentro del juego (`src/editor/`, F5). Documento
abierto el **2026-08-05**: describe la técnica que ya usa, analiza lo que no
puede autorar todavía, y propone el código que falta. Incluye la investigación
sobre **BSN** y dónde encaja de verdad.

> **Cómo se usa este documento.** Como los demás planes de dominio: define el
> destino y el orden. Un paso se cierra **autorando un mapa de verdad** con lo
> que ese paso entrega — no con un test. Un editor se juzga usándolo.
>
> **Qué es este documento y qué no.** Es análisis de técnica y propuesta de qué
> código crear. Donde dice "no existe" es para desmentir algo que otro documento
> daba por hecho, no para inventariar el repo.

---

## La ley madre: esta herramienta autora **dato de simulación**

Lo que el editor escribe es **significado**, nunca apariencia. Ni modelos, ni
materiales, ni colores en el archivo de nivel. Presentación lee ese significado
y lo llena con lo que corresponda — el patrón que ya funciona con `TreeKind` →
`VisualCatalog`.

Es la decisión que hace que el archivo de terreno **sea el nivel** en vez de ser
una malla guardada. Todo lo que sigue —incluida la respuesta sobre BSN— se
deduce de acá.

Las tres capas de autoría, en el orden en que se componen:

**relieve → semántica por celda → instancias discretas.**

Primero se le da forma a la colina, después se dice que la colina es roca,
después se ponen las rocas encima. `src/editor/` es la casa de las tres.

---

## Lo que la herramienta hace hoy

| Capa | Con qué | Dónde termina el dato |
|---|---|---|
| **Relieve** | Seis brushes (`Elevar`, `Suavizar`, `Aplanar`, `Rampa`, `Rugosidad`, `Terrazas`), radio con la rueda, fuerza con Shift+rueda | `heights: Vec<f32>` sobre las esquinas de una grilla de 128×128 celdas |
| **Semántica** | Un brush que pinta `TerrainKind` (Soil, Rock, TallGrass, Sand) | `kinds: Vec<TerrainKind>` sobre las celdas, run-length en el archivo |
| **Persistencia** | `Ctrl+S` / `Ctrl+L`, escritura atómica (temporal + `rename`) | Un `.ron` por escena (`assets/game/world/*.ron`) |
| **Deshacer** | Una entrada por trazo, hasta 32, cubriendo las dos capas | Snapshots en memoria (~2,7 MB de historial) |
| **Verificación** | `TerrainDebugView` en el hub F1: Tipo, Escalable, Inflamable, Cortable | — |

Y tres propiedades del diseño que conviene nombrar porque son las que hacen que
esto se pueda ampliar sin reescribirlo:

- **La grilla es dueña de cómo cambia.** Cada brush es un método de `Terrain`; el
  editor sólo decide dónde y cuándo dispara. Un brush nuevo es una fila en
  `BrushKind` más un método — nunca un sistema nuevo.
- **El archivo se remuestrea en espacio de mundo.** Guardado con `CELLS = 128` y
  cargado con otro valor, las alturas se reinterpolan **en metros**, no en
  fracción de grilla. Cambiar la resolución no huerfaniza los niveles.
- **La semántica nunca se interpola.** No hay valor entre "roca" y "pasto": al
  remuestrear se usa el vecino más cercano, y una lista de runs que no suma
  exactamente se **rechaza** en vez de correr el mapa de lugar en silencio.

---

## BSN: qué es hoy, y dónde encaja de verdad

Verificado el 2026-08-05 en las fuentes de `bevy_scene 0.19` instaladas (no en
documentación de internet).

**Lo que sí existe, y es más de lo que este proyecto suponía:**

- `bevy_scene` **es** BSN. El crate entero está construido alrededor de la
  notación: `Scene`, `SceneList`, `ScenePatch`, `ResolvedScene`.
- La macro `bsn!` funciona y está en uso dentro del propio Bevy —
  `bevy_feathers`, los widgets oficiales, se escriben con ella.
- `#[derive(SceneComponent)]` con `#[scene(mi_funcion)]`: **un componente que,
  al insertarse, spawnea toda una escena**. Eso es exactamente el patrón
  `kind → presentación` que este proyecto ya implementa a mano.
- `Template`: parcheo **por campo**, o sea reusar una escena y pisarle sólo el
  ancho de un botón sin repetir el resto de la struct.

**Lo que no existe:**

- **El formato de archivo `.bsn`.** Textual del crate: *"Bevy does not currently
  have support for `.bsn` files, but intends to offer a `.bsn` asset format in
  future releases."* La sintaxis `:"escena.bsn"` y `#[scene("player.bsn")]` están
  preparadas y sin loader.
- **La serialización.** Nada en el crate escribe una `Scene` a texto. Y un
  editor, por definición, **escribe**.

**La conclusión, que es una buena noticia y no una traba:**

> BSN es la forma correcta de **componer entidades**, y no puede ser el archivo
> que el editor guarda. Las dos cosas son verdad a la vez y no compiten, porque
> el archivo de nivel no debería contener presentación de todos modos (la ley
> madre).

El reparto que sale de ahí, y que es el plan:

| Capa | Formato | Por qué |
|---|---|---|
| El nivel en disco | **RON de datos** (`kind`, posición, yaw, escala) | Se escribe desde el juego, se versiona, se lee sin motor, y no menciona un modelo |
| El nivel → entidades | **`bsn!` y `SceneComponent`** | Un `kind` resuelve a una escena entera: malla, collider, hijos, marcadores |
| El graybox de `layout.rs` | **`bsn!`** | Son 656 líneas de tablas declarativas: literalmente un BSN artesanal |

Cuando el formato `.bsn` exista, lo que puede mudarse a disco son **los
catálogos** (qué es una roca), no el mapa (dónde hay rocas). Ese día el editor
no cambia.

**El experimento barato para empezar, ya anotado en `AHORA.md`:** reescribir
`spawn_stair_segment` como un `bsn!` y ver si queda más legible que la función.
Si sí, migrar `layout.rs`. Es reversible, no toca el archivo de nivel y es la
única forma honesta de decidirlo — leyendo las dos versiones.

---

## Qué le falta, en orden de cuánto duele

### 1. La tercera capa no existe: no se pueden poner cosas

Es lo más grande. El editor autora la forma del suelo y su significado, y
**nada de lo que hay encima**. Hoy los objetos del mundo vienen de dos lugares,
ninguno de los cuales es el editor:

- **`src/world/layout.rs`**: 656 líneas de tablas Rust — cajas, escaleras,
  blancos de práctica, el perímetro. Cambiar el mapa es recompilar.
- **`src/world/forest.rs`**: un scatter procedural determinista. El bosque no
  está autorado: está *calculado*.

O sea que un mapa nuevo no se puede hacer sin tocar código, que es exactamente
lo que una herramienta de autoría existe para evitar.

### 2. Se puede pintar `TallGrass` y no aparece pasto

La semántica ya se pinta, se guarda, se recarga y se ve en el diagnóstico. Pero
sus únicos consumidores son el color del terreno y `GroundFacts::surface` (para
el sonido de pisada y las propiedades de traversal). **La vegetación no la
mira**: la pradera es un cuadrado fijo de 25×25 m centrado en una constante
(`MEADOW_CENTER`, `visuals/grass.rs`), y el bosque es el scatter de arriba.

Es la brecha más barata de cerrar y la que más va a cambiar la sensación de que
la herramienta *hace* algo: pintar pasto y que crezca pasto.

### 3. No hay volúmenes ni puntos

Un mapa no es sólo suelo y objetos:

- **Agua.** `BOTWMovements.md` necesita `WaterVolume` para los motores `Swim` y
  `Dive`, y sin editor de volúmenes esos volúmenes nacen hardcodeados.
- **Interiores.** `LIGHTING.md` necesita marcar dónde empieza una cueva.
- **Puntos.** El spawn del jugador vive en `SceneDef` (código). Waypoints,
  puntos de interés y campamentos no tienen dónde vivir.

### 4. El formato no tiene lugar para nada de eso

`TerrainFile` es `{ points, extent, heights, kinds }`. Cualquier capa nueva
necesita un campo nuevo, y ese campo tiene que entrar **sin invalidar los cinco
niveles que ya existen en disco**.

### 5. La cámara de autoría no es de autoría

El brush dispara un raycast desde la cámara de juego, que es orbital y de
tercera persona. Hay freecam (F3), pero es un modo aparte que el editor no
conoce: no hay vista cenital, ni encuadre del mapa entero, ni forma de ver los
320×320 m que se están autorando.

### 6. Cada edición reconstruye el terreno entero

Malla y collider se regeneran completos con cada trazo. A 128 celdas está bien
—y está anotado como decisión consciente en `terrain/mod.rs`— pero es el techo:
subir la resolución o el tamaño del mundo hace que el chunking deje de ser
opcional. También es donde vive la deuda C1 (~130 allocations por tick en
`rebuild_terrain_collider`).

---

## Cómo se completa

### Paso 1: Que la semántica plante lo que dice

**El más barato, y el que hace que pintar signifique algo.**

- **Lógica.** `visuals::grass` deja de leer una constante y pasa a leer el
  terreno: los chunks de pradera se generan donde `TerrainKind::TallGrass` está
  pintado, y su densidad sale de los anillos de `BOTWGrass.md`. Lo mismo abre la
  puerta para que el bosque se siembre por semántica en vez de por scatter fijo.
- **Código a crear:** una consulta `Terrain::cells_of_kind(kind) -> impl
  Iterator<Item = (usize, usize)>` en `terrain/query.rs`, y el cambio en
  `build_field` para recorrerla en vez de `FIELD_CHUNKS`.
- **Cuidado con el costo:** repintar debe reconstruir **sólo los chunks
  tocados**, no la pradera entera; la pradera ya está partida en chunks, así que
  el dato para hacerlo existe.
- **Validación.** Pintar una franja de pasto largo cruzando una colina y verla
  crecer siguiendo el relieve, sin reiniciar. Y el conteo de triángulos del hub
  moviéndose con lo pintado.

### Paso 2: La capa de instancias

**El paso que convierte esto en un editor de mapas.**

- **El dato** (en `bof_domain`, junto a los otros tipos compartidos):
  ```rust
  pub struct Instance {
      pub kind: PropKind,   // qué es, semánticamente
      pub xz: Vec2,         // en metros, espacio de mundo
      pub yaw: f32,
      pub scale: f32,
  }
  ```
  Nada de handles, ni rutas, ni materiales. `PropKind` es un enum del dominio,
  como `TreeKind`.
- **La altura no se guarda.** Se muestrea del terreno al spawnear, igual que hace
  `layout.rs` con su `Anchor::Ground`. Una piedra guardada con `y` absoluto
  queda flotando la primera vez que alguien esculpe debajo — y esculpir debajo es
  el uso normal de esta herramienta.
- **El formato**, extendiendo `TerrainFile` sin romper lo guardado:
  ```rust
  #[serde(default)]
  pub instances: Vec<Instance>,
  ```
  `default` es lo que hace que los cinco niveles actuales sigan cargando. Y a
  diferencia de las alturas, **las instancias no se remuestrean**: ya están en
  metros, así que cambiar `CELLS` no las toca.
- **La herramienta:** una tercera `ToolLayer::Instances`, con colocar (click),
  borrar (click derecho sobre una), rotar y escalar la seleccionada. El
  historial necesita una entrada por acción — y acá el snapshot completo de la
  grilla ya no sirve: conviene un `HistoryStep` enum (`Terrain(snapshot)` /
  `Instances(before, after)`) antes de que el historial mezcle dos costos muy
  distintos.
- **La presentación:** `PropKind` → escena, y este es el lugar natural del
  primer `SceneComponent` del proyecto.
- **Validación.** Colocar veinte rocas, guardar, salir del juego, volver y
  encontrarlas donde estaban. Después esculpir debajo y verlas seguir el suelo.

### Paso 3: `layout.rs` a `bsn!`, si el experimento lo justifica

- **Lógica.** Reescribir `spawn_stair_segment` con `bsn!` y comparar legibilidad.
  Si gana, migrar el resto: `layout.rs` es un BSN artesanal y el motor ya trae
  el de verdad.
- **Lo que no cambia:** el archivo de nivel. Esto es composición de entidades,
  del lado de presentación.
- **Validación.** Las cajas de traversal se ven y se comportan idénticas, y el
  archivo queda más corto de leer.

### Paso 4: Volúmenes

- **Lógica.** Una cuarta capa: `Volume { kind: VolumeKind, min: Vec3, max: Vec3 }`
  con `VolumeKind::{Water, Interior, Trigger}`. AABB primero: es lo que
  `WaterFacts` necesita y lo que se puede dibujar y arrastrar con dos clicks.
- **Desbloquea** los motores `Swim`/`Dive` de `BOTWMovements.md` y los interiores
  de `LIGHTING.md`, que hoy no tienen de dónde sacar sus datos.
- **Validación.** Cavar un lago con el brush de relieve, marcar el volumen de
  agua encima y nadar en él.

### Paso 5: Cámara de autoría

- **Lógica.** Que F5 pueda encuadrar el mapa: una vista orbital alta con el
  cursor proyectado al terreno. La freecam ya existe; falta que el editor la
  gobierne en vez de ser un modo paralelo.
- **Se hace cuando duela**, y va a doler exactamente cuando el Paso 2 permita
  poblar el mapa entero.

---

## Decisiones ya tomadas que conviene no deshacer

Cada una costó una sesión y ninguna es obvia leyendo el código:

1. **La escritura es atómica** (temporal + `rename`). El archivo *es* el nivel:
   un corte a mitad de escritura no puede dejar una grilla truncada.
2. **El remuestreo va por espacio de mundo**, con el `extent` que el archivo
   trae. Estirar un nivel de 320 m sobre un mundo de 640 duplicaría cada
   pendiente en silencio: se vería bien y se caminaría mal.
3. **Un archivo con un `NaN` se rechaza.** `parry` construye con él un
   heightfield que se traga lo que se le pare encima, sin error en ningún lado.
4. **F6 cambia de capa, no Tab.** Tab ya abría el inventario, que lee el teclado
   crudo: una tecla hacía las dos cosas y el panel silenciaba el brush. El
   problema de fondo sigue abierto (nadie arbitra colisiones de teclas, hallazgo
   C2) y la próxima se va a encontrar igual: probando.
5. **El botón derecho no es una goma en la capa semántica.** No hay "sin tipo" al
   que borrar: toda celda es de algo, así que limpiar es pintar `Soil`.
6. **Repintar del mismo tipo no es una acción.** El historial sólo recibe entrada
   si alguna celda cambió de verdad.

---

## Fuera de alcance (a propósito)

- **Escribir `.bsn` desde el editor.** No existe serializador, y aunque
  existiera, el nivel no debe contener presentación.
- **Un editor fuera del juego.** La herramienta vive dentro del motor porque
  autorar contra la física y la cámara reales es la mitad de su valor.
- **Chunking del terreno.** Deferido hasta que la resolución o el tamaño del
  mundo lo exijan; hoy el rebuild completo es más simple y alcanza.
- **Undo infinito.** 32 pasos, ~2,7 MB. Subirlo es cambiar una constante el día
  que moleste.
