# Plan: el terreno deja de ser escalable, y la escalada alcanza lo que sí lo es

**Abierto el 2026-08-23.** Plan de migración: temporal por definición, se borra
cuando su última fase cierre y lo vivo se muda a `BOTWMovements.md` (escalada) y
`MAP_EDITOR.md` (autoría de relieve). No fija leyes.

## La decisión que lo origina

**El terreno no tiene que ser escalable.** Lo escalable son objetos: paredes,
rocas, columnas. Decisión del usuario el 2026-08-23, después de mirar el código
de una reimplementación de BotW en Unreal y notar que **nunca escala terreno** —
sólo mallas discretas, que es exactamente lo que este juego ya podía escalar
antes de que se rompiera.

### Por qué, y es matemático

Un heightmap es `y = f(x, z)`: **una altura por celda**. No puede plegarse sobre
sí mismo, así que **no puede representar una vertical ni un saliente, a ninguna
resolución**. Lo que este proyecto llamó "acantilado" siempre fue una rampa cuyo
tramo horizontal es el espaciado de la grilla.

`MAP_EDITOR.md` ya lo tenía escrito el 2026-08-22 —*"una pared siempre tendrá
2,5 m de transición horizontal"*— anotado como techo aceptable. **No era un techo
aceptable: era la señal de que el acantilado no va en el heightmap.**

La práctica de la industria coincide: el heightmap hace las lomas, y los
acantilados, formaciones y entradas de cueva son **mallas colocadas encima**.

### Lo que esto invalida de la sesión del 2026-08-23

Toda la persecución del sensor de escalada sobre terreno esculpido —la normal
saltando 20,6°, `casts=000000` en 1274 de 1286 ticks, el zumbido `Walk`↔`Fall`—
atacaba el síntoma de tratar una rampa empinada como si fuera pared.

**Lo que sí sobrevive y no depende de esta decisión:** el límite de 45°, la
constante `FLOOR_MIN_UP_DOT` unificada y su costura `>`/`<`, la histéresis y el
latch de suelo, el kernel del pincel independiente de la resolución, y
`fall::clamp_against_face` (el rebote hacia arriba pasa en cualquier rampa).

## Fase 1 — El alcance del sensor, proporcional a la inclinación — **HECHA (código), falta jugarla**

**Vale igual con terreno no escalable**, porque una roca o una columna tampoco
son verticales perfectas: se recuestan, y ahí el cast de alcance fijo falla.

**El defecto, ya documentado en el propio código** (`services/ledge.rs`): *"en
una cara inclinada la superficie se aleja `Δaltura / tan(θ)` y el de la cabeza
falla: el umbral efectivo para `head_hit` es ~77° aunque la configuración declare
60"*. Hoy eso está **parcheado** con `leans_back_out_of_reach`, un discriminador
que reemplaza al cast que falla en vez de hacer que no falle.

**La técnica de referencia** (`vitorcantao.com/post/climbing-system`, código en
`github.com/VitorCantao/ZeldaBotwClimbingSystem`, leído entero el 2026-08-23):

```cpp
bool IsFacingSurface(const float Steepness) const {
    constexpr float BaseLength = 80;
    const float SteepnessMultiplier = 1 + (1 - Steepness) * 5;
    return EyeHeightTrace(BaseLength * SteepnessMultiplier);
}
```

`Steepness = dot(normal, normal_aplanada)`: vale 1 en una cara vertical y baja al
recostarse. Multiplicador de 1× a 6×.

**Corrección a la escala que estimaba este plan (2026-08-23, leyendo el código).**
Decía que su base deja 0,6 radios más allá del cuerpo y la nuestra 0,3 — la
mitad. Estaba mal: **ignoraba el radio de la esfera del cast** (`sphere_radius =
0.1`). El cálculo real es 0,65 de viaje + 0,1 de esfera = 0,75 desde el eje,
contra 0,5 de radio de cápsula → **0,5 radios**, casi lo mismo que su 0,6. La
base equivalente sería 0,70 m, no 0,80 — y sobre todo **la base nunca fue el
problema**: lo que nos faltaba era el crecimiento con la inclinación.

Por eso `wall_detection_reach` **no se movió** de 0,65. Esa pregunta la contesta
la perilla en vivo jugando, que es la evidencia que vale.

### Lo que se implementó, que es mejor que copiar la fórmula

El multiplicador lineal `1 + (1 - s) · 5` es una aproximación con una constante
mágica. En vez de eso, `probe_face_overhead` **cambia el origen del cast**: sale
del contacto de la cintura y viaja **contra la normal de la cara**, no desde el
eje del cuerpo en la dirección en que mira el actor.

Ese cambio de origen borra el problema en vez de compensarlo:

- El punto a `Δaltura` sobre el contacto queda exactamente `Δaltura · normal.y`
  afuera del plano. **Un producto, no una tangente**: sin división y sin
  singularidad al acercarse al límite caminable.
- **La guiñada desaparece de la aritmética**, porque el cast ya no viaja en la
  dirección en que mira el actor. Con la fórmula de referencia había que dividir
  por `cos(guiñada)`, y a 30° eso son 15,5% más de alcance.
- **El alcance queda acotado sin inventar un tope**: sólo se sondean caras no
  caminables, o sea `normal.y < 0,707`, así que el alcance nunca pasa de
  `Δaltura · 0,707 + holgura` ≈ 0,77 m. La versión con la fórmula de referencia
  llegaba a 1,57 m, y sin acotar la guiñada a 26 m.

**Y lo que arregla que el parche no arreglaba:** el parche nunca comprobaba que
hubiera superficie a la altura de la cabeza. Una roca puntiaguda de 1 m —muy
empinada para caminarla, con el tope no pisable, o sea tampoco bordillo— se
declaraba escalable sin pared donde agarrarse. Ahora se pregunta de verdad.

**Lo que NO se tocó, y es deliberado:** `has_head_hit` se queda con el cast de
perfil crudo. `climb.rs` lo usa como veto de ápice (`near_apex`), que es otra
pregunta; ensancharlo ahí soltaría la escalada justo al coronar.

**Verificación:** falta jugarlo. En el CSV, el **bit 6** de `climb_cast_hits`
dice que la cara se encontró *sólo* con el sondeo — es decir, exactamente los
ticks donde esta fase hizo la diferencia. El canal de casts lo muestra con
etiqueta propia, `ledge_face_overhead`.

## Fase 2 — Los límites del heightmap — **HECHA (código), falta jugarla**

**La premisa de esta fase era falsa, y se corrigió el 2026-08-23.** Todo lo que
sigue debajo del título viejo está conservado como registro, pero la regla que
proponía —*"ninguna diferencia entre vecinos sobre 0,5 m"*— **no** es la que
quedó. Lo que la tumbó:

- **Vault y mantle existen.** `LedgeSensing::PLAYER` pasa caras de 0,3 a 1,4 m
  saltando y hasta 2,5 m mantleando. Una cara empinada corta no es geometría
  muerta. La travesía está limitada por **altura de cara**, no por pendiente, así
  que escribir la regla como un ángulo era escribirla en la unidad equivocada.
- **Habría destruido el pincel de Terrazas**, que existe justamente para hacer
  contrahuellas empinadas con huellas planas — la forma de las mesetas de BOTW.

**La regla que quedó:** ningún tramo empinado continuo puede acumular más de
`MAX_UNWALKABLE_RISE_METRES` (2,5 m, atado por `const _` al alcance del mantle).
Una huella caminable en el medio corta el tramo. La mide
`Terrain::steepest_run`, en `world/terrain/traversable.rs`.

**Medido el 2026-08-23**, diez segundos a fuerza máxima: `raise_area` acumula
1,53 m a radio 6 m (se anda) y 21,37 m a radio 40 m (pared); `carve_area` llega a
45 m a cualquier radio; una ladera terraceada queda en 1,98 m. `sandbox.ron`
tiene un tramo de **68,83 m**, que es el cañón.

**Y el editor avisa en vez de clampear**, contra lo que este plan recomendaba.
Las dos razones son medidas: al radio por defecto el pincel **ya cumple**
(1,53 < 2,5), y un pase automático no puede distinguir la pared que sobra de la
que se autoró a propósito, porque **el heightmap no guarda qué pincel hizo qué**.
Un clamp global sobre `sandbox.ron` movería 30.545 puntos con una bajada media de
10,1 m — o sea borraría el cañón que este mismo plan acaba de bendecir como telón
de fondo.

### Lo que queda abierto, y es de diseño

**No hay dónde marcar "el jugador no va acá".** `TerrainKind` tiene
`Soil/ShortGrass/Rock/TallGrass/Sand` y ninguno dice telón de fondo. Sin esa
marca, ninguna medición puede excluir el cañón legítimo, y por lo tanto ninguna
reparación automática es segura. Es el agujero real que hoy tapa un doc-comment.

---

## Fase 2 (versión original, conservada como registro)

Con el terreno no escalable, **una cara de terreno sobre el límite caminable es
geometría muerta**: no se camina y no se escala. El jugador queda sin nada que
hacer ahí. Eso convierte el límite en una **regla de autoría**, no sólo en un
umbral de runtime.

### La aritmética, con los números de hoy

Espaciado `s = WORLD_SIZE / CELLS = 320 / 640 = 0,5 m`. El ángulo entre dos
muestras vecinas es `atan(Δh / s)`:

| ángulo | Δh máximo entre puntos vecinos, a 0,5 m |
| --- | --- |
| 45° (límite caminable) | **0,50 m** |
| 60° | 0,87 m |
| 81° (el cañón medido) | 3,15 m |

**La regla de autoría cae sola: ninguna diferencia de altura entre puntos vecinos
debería pasar de `s · tan(WALKABLE_LIMIT_DEG)`.** A 0,5 m de celda y 45°, eso es
medio metro.

La banda de alturas (`MIN_HEIGHT = -60`, `MAX_HEIGHT = 120`) no cambia: no es un
límite de diseño sino una guarda.

### Lo que hay que decidir, y no está decidido

1. **¿El pincel impide pasarse, o sólo avisa?** Impedirlo hace el editor incapaz
   de producir terreno muerto, y es lo más fuerte. Avisar deja autorar decoración
   inalcanzable a propósito. **Sin decidir.**
2. **`carve_area` (el pincel Acantilado) probablemente sobra.** Se agregó el
   2026-08-22 con un único propósito: *"que el jugador pudiera escalar algo que el
   editor supiera construir"*. Si el terreno no se escala, ese propósito
   desapareció. **Sin decidir, y es lo que resuelve el test rojo** (ver abajo).
3. **¿Qué hace el relieve que ya está esculpido?** `sandbox.ron` tiene caras de
   hasta 81°. O se suaviza, o queda como decoración con la que no se interactúa.

### El test rojo que espera esta decisión

`world::terrain::tests::only_the_cliff_brush_carves_a_climbable_wall` **falla
hoy, a propósito**: diez segundos de `raise_area` sostenido llegan a **61,88°**,
donde a 2,5 m de celda medían 22°. No es un bug del pincel: `steepest_face_degrees`
mide entre vértices vecinos con el espaciado real, así que la grilla gruesa
promediaba el gradiente y lo subestimaba. **El pincel de elevar siempre hizo caras
así; la grilla no podía mostrarlas.**

Con el terreno no escalable ese número cambia de significado otra vez: 61,88° ya
no es "una pared que el pincel de elevar no debería hacer", es **geometría muerta
que el pincel de elevar no debería poder hacer**. El test hay que reescribirlo
contra la regla de la Fase 2, y su mensaje —que sugiere borrar `carve_area`— pasa
a ser correcto por una razón distinta de la que dice.

## Fase 3 — Lo escalable pasa a ser objeto — **HECHA (código), falta jugarla**

**El suelo lleva `NonClimbable` desde el 2026-08-23** (`spawn_terrain`), con test
diferencial. Es el cambio que cierra la decisión: el terreno dejó de ser una
respuesta posible para el sensor de escalada.

**Lo escalable son peñascos** (`src/world/crags.rs`): tres piezas —Roca, Pared,
Acantilado— con collider **trimesh de la misma malla que se dibuja**, activadas
por la capacidad `crags` en Traversal, Terreno y Mundo.

**Generados, no importados, y la razón importa.** El pack KayKit trae `.gltf` y
el pipeline sólo indexa `.glb` bajo `authored/`, con paleta de materiales propia:
meterlo es una tarea de pipeline, no de escalada. Pero además una malla de
catálogo **puede no tener ni una cara reclinada**, y ésa es justo la que el
sensor falla. Generándolas, el desorden es un parámetro y se garantiza que esté.

Decisiones de forma, todas al servicio de probar el sensor:
- **Normales planas**, una por triángulo. Con normales suavizadas la superficie
  miente y el sensor se comporta mejor de lo que es.
- **Trimesh y no casco convexo**: un casco alisaría las concavidades, que son
  media prueba.
- **Elipsoide abollada con tres octavas de lóbulos**, no ruido por vértice — el
  ruido suelto se ve como sal y pimienta; los lóbulos dan bultos del tamaño de
  un cuerpo, que es la escala a la que se escala.

Lo que **no** se hizo, y sigue abierto: las instancias del editor siguen sin
collider (`src/visuals/instances.rs` es sólo visual, y §20 impide que
presentación arme colliders — el dueño sería `src/world/`). Los peñascos son
mobiliario de prueba en una tabla, no props colocables. Eso es el paso siguiente
**una vez que la escalada esté validada jugando**.

---

## Fase 3 (versión original, conservada como registro)

**No empezar hasta que la Fase 1 esté jugada**, porque hasta entonces no sabemos
si el sensor engancha bien sobre props.

Lo que ya existe y no hay que construir:
- **Capa de instancias en el editor** para props `.glb` (2026-08-14).
- **`NonClimbable`** como marcador por entidad: la máscara autorada por objeto ya
  funciona, y la usan las escaleras y el perímetro.
- El pack `KayKit_Forest_Nature_Pack` en `assets/`, **sin trackear todavía**.

Lo que falta, y es el trabajo real de esta fase:
- **Las instancias no tienen collider.** `src/visuals/instances.rs` es sólo
  visual. Sin collider no hay nada que escalar ni contra qué chocar.
- Decidir la forma del collider por prop (convex hull, trimesh, primitiva
  aproximada) y quién la autora — probablemente el pipeline de assets, no el
  editor de mapas.

## Lo que este plan NO hace

- **No mete auto-climb.** Diferido por el usuario hasta que el toggle funcione
  bien: *"siempre esconde bugs a no ser que con el toggle funcione bien"*.
- **No copia el promedio de normales de la referencia sin filtrar.** Él promedia
  todos los hits porque sus superficies escalables son objetos separados del
  suelo; sobre terreno eso mezcla la normal de la pared con la del piso.
- **No revierte `CELLS = 640`.** Su justificación —poder autorar una repisa de
  vault de 0,3–1,4 m— no depende de que el terreno sea escalable.
- **No toca la deriva al caminar** (7,1° de desvío, 85,6% del avance), que sigue
  abierta y sin causa confirmada.
