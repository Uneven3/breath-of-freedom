# Soluciones: `multi-actor-migration`

Iteración 3 (Previous critique: Reviewer Phase A, Iteración 2 — RECOMMEND WITH
REQUIRED IMPROVEMENTS sobre Solución 1). Alcance, acoplamiento y "Definición de
terminado" según `docs/tickets/multi-actor-migration.md`; patrón normativo ya
validado en `src/movement/spike.rs` y decisión ya tomada en
`docs/architecture/rationale/multi-actor-dispatch.md`. Las 3 soluciones abajo
comparten el objetivo (`Single<Player>` → `Query<Actor>` en los 13 motores +
`arbitrate`, sin `run_if` global) pero difieren en **cómo se filtra, por
entidad, qué motor corre** y **dónde vive el estado antes `Local<T>`** — los
dos puntos de diseño reales que el ticket deja abiertos más allá de "seguir el
spike".

**Cambios de esta iteración (Solución 1 únicamente):** la crítica previa
(Iteración 2) señaló que §13 cita `cargo fmt` y `cargo clippy` como una
unidad ("cargo fmt y cargo clippy se corren antes de dar por terminada una
tarea"), pero solo `cargo clippy` había sido promovido a paso explícito y
nombrado del Enfoque — `cargo fmt` seguía implícito. El paso 2 del Enfoque de
Solución 1 ahora nombra ambos comandos explícitamente, en el mismo nivel de
obligatoriedad. Soluciones 2 y 3 no cambian — quedan como puntos de
comparación, no elegidas.

---

## Solución 1 — Migración mecánica de paridad con el spike (recomendada)

### Enfoque

Aplicar exactamente la transformación ya validada en `spike.rs`, sin desviarse:
`Actor` como marker nuevo (`Player` lo lleva además); los 13 `propose`/`tick` y
`arbitrate()` pasan de `Single<.., With<Player>>` a
`Query<.., With<Actor>>` con un `for` y un guard interno
`if *state != LocomotionState::X { continue }` en cada `tick`. Los tres
`Local<T>` reales (`sprint.rs::stamina_locked`, `jump.rs::JumpLocal`,
`glide.rs::GlideLocal`) se promueven a componentes por entidad
(`SprintLock`, `JumpState`, `GlideState`), con el mismo patrón que ya usan
`MantleState`/`VaultState`/`WallJumpState`/`EdgeLeapState`. `in_loco_state` y
los 13 `.run_if(...)` en `MovementPlugin::build` se eliminan.

Esta migración se considera terminada solo cuando se completan, además de la
transformación mecánica anterior, estos dos pasos — ambos entregables
obligatorios de esta solución, no riesgos opcionales a "vigilar" más adelante:

1. **Test de invariante de arquitectura (obligatorio, no gateado por
   checkpoint de feeling):** escribir y dejar en verde, como parte del mismo
   trabajo de migración, un test de integración que arme dos actores
   (`Query<Actor>`) con `LocomotionState` distinto (p. ej. uno en `Walk`, otro
   en `Fall`) y verifique que ningún `Local<T>` promovido ni ningún
   `LocomotionState` cruza de una entidad a otra tras correr `arbitrate()` y
   los 13 `tick`. Por §11 (excepción Tier 3: invariantes de arquitectura,
   seguridad y contratos multi-actor se testean desde el diseño), este test
   no espera a que el *feeling* jugable llegue a checkpoint — es un
   entregable de esta migración en sí, al mismo nivel que el cambio de
   `Single` a `Query`. La migración no se da por completa sin él.
2. **`cargo fmt` y `cargo clippy` de árbol completo (obligatorio, paso final
   explícito, dos comandos nombrados):** una vez editados los 13 motores +
   `mod.rs`, correr primero `cargo fmt` sobre el árbol completo y a
   continuación una sola pasada de `cargo clippy --workspace --all-targets`
   (o el equivalente que cubra todo el crate en una sola invocación) — no una
   pasada por archivo mientras se edita cada motor. `cargo fmt` normaliza el
   formato de los 13 archivos tocados antes de que clippy los audite, para
   que ningún diff de formato quede mezclado con el diff funcional de la
   migración. El objetivo de la pasada de clippy es atrapar imports `Local`
   que quedaron muertos y campos de tupla que el cambio de forma dejó sin uso
   en cualquiera de los 13 archivos, algo que una pasada por archivo aislado
   no detecta de forma confiable porque cada motor se edita antes de que el
   resto del árbol también haya cambiado. Este paso cierra la migración; no
   se da por terminada sin `cargo fmt` aplicado y clippy limpio en el árbol
   completo.

### Clausulas de CONSTITUTION.md en riesgo

- **§9** — `Single<..>.into_inner()`/`q.single()` de hoy puede *panickear* si
  la cardinalidad no es 1; el cambio a `Query` + iteración **reduce** ese
  riesgo (nada que mitigar activamente, pero verificar que ningún motor nuevo
  reintroduzca un `.single()`/`unwrap()` por comodidad al portar código).
- **§11 (excepción Tier 3)** — cubierta por el paso 1 del Enfoque: el test de
  "dos actores no comparten `LocomotionState` ni `Local` promovido" es un
  entregable obligatorio de esta migración, exento de esperar checkpoint de
  *feeling* por la excepción explícita de §11. Sin ese test la solución
  incumple la Definición de terminado aunque el código compile — no queda
  como riesgo a vigilar, queda como paso que se ejecuta.
- **§13** — cubierta por el paso 2 del Enfoque: la cláusula cita `cargo fmt` y
  `cargo clippy` como una unidad, y ambos son ahora pasos nombrados y
  explícitos, no uno implícito y otro no. Tocar 13 archivos + `mod.rs` en una
  sola pasada es superficie grande tanto para desvíos de formato como para
  warnings de `clippy` (imports muertos de `Local`, tuplas con campos ya no
  usados tras el cambio de forma); `cargo fmt` de árbol completo corre
  primero, y la pasada de `cargo clippy` de árbol completo corre a
  continuación, ambos al final de la migración (no por archivo mientras se
  edita) — son los pasos que cierran esta cláusula, no una intención a
  futuro.
- **§16** — cada `tick` gana una línea de guard y el `Query` crece un campo
  (`&LocomotionState`); ninguno de los 13 motores se acerca a ~300 líneas por
  esto, pero es la señal a vigilar si algún motor ya estaba cerca del límite.

### Tradeoffs

- **Pro:** cero diseño nuevo — el patrón ya está probado por 3 tests en
  `spike.rs` y ya está documentado como el target en `movement.md` y
  `rationale/multi-actor-dispatch.md`; el diff es predecible y revisable
  motor por motor.
- **Pro:** el guard interno es local y explícito — cualquiera que abra un
  motor ve la condición de activación en la primera línea del `tick`, sin
  saltar a otro archivo.
- **Con:** con `in_loco_state` eliminado, los 13 `tick` corren **todos, todos
  los frames, para todos los actores** — el guard es la única barrera. Un
  guard olvidado o mal escrito en una de las 13 ediciones ya no falla ruidoso
  (el sistema simplemente no corría antes); ahora corre silenciosamente para
  el actor equivocado. El blast radius de un solo error de copy-paste es
  mayor que en el diseño actual.
- **Con:** duplica el mismo patrón de guard 13 veces (una por motor) — puro
  boilerplate, sin abstracción compartida que lo fuerce a ser consistente.

### Edge cases

1. Un `Actor` futuro (Enemies) spawneado sin uno de los componentes
   promovidos (p. ej. sin `JumpState`) — el `Query` de `jump::propose` lo
   excluye en silencio (no hay error de compilación ni panic): ese actor
   nunca podrá saltar, y nada lo señala salvo jugarlo. El ticket ya pide
   documentar el shape que "cualquier Actor futuro necesitará replicar";
   esto es la razón concreta.
2. Dos actores cambian de estado en el mismo tick fijo (uno aterriza y pide
   salto, otro entra en `Glide`) — verificar que `arbitrate()` iterando por
   entidad no cruce el buffer de proposals de uno con el `LocomotionState`
   del otro (cada `ProposalBuffer`/`LocomotionState` es de la entidad, pero
   es la clase de bug que un guard copy-pasteado mal sí podría introducir).
3. Si algún motor olvida el guard (`if *state != X { continue }`) al portar
   uno de los 13, ese `tick` corre para *cualquier* estado en *cualquier*
   actor — dos motores mutando `Transform`/`BodyVelocity` del mismo actor en
   el mismo frame, silenciosamente. Es exactamente el caso que el test
   obligatorio del paso 1 del Enfoque (uno `Walk`, uno `Fall`) está diseñado
   para atrapar — este edge case es la justificación de por qué ese test no
   es opcional.

---

## Solución 2 — Dispatch por marker de estado activo (filtrado a nivel de archetype)

### Enfoque

En vez de un guard de datos leído dentro del `tick`, `arbitrate()` gestiona un
marker component por `LocomotionState` (p. ej. `ActiveWalk`, `ActiveFall`,
`ActiveJump`, ... 13 markers unitarios) e inserta/remueve exactamente uno por
actor cuando el estado cambia (vía `Commands`, solo en la transición, no cada
frame). Cada `tick` filtra
`Query<(...), (With<Actor>, With<ActiveX>)>` — el motor **no puede** correr
para el actor equivocado porque ese actor ni siquiera aparece en el `Query`;
no hay rama `if` que alguien pueda olvidar. Los `Local<T>` se promueven a
componentes igual que en la Solución 1 (ese punto no cambia).

### Clausulas de CONSTITUTION.md en riesgo

- **§18** — insertar/remover un marker component dispara un archetype move en
  Bevy; en un actor que oscila de estado seguido (aterrizar/saltar/aterrizar),
  esto ocurre varias veces por segundo dentro de `FixedUpdate` a 60Hz — hay
  que confirmar que el costo no viola "sin allocations en el hot path"
  (el archetype move no es una allocation de sistema en el sentido de `Vec`,
  pero mueve memoria de la entidad entre tablas; requiere justificar que es
  aceptable, no asumirlo).
- **§2** — `arbitrate()` pasa a ser el único lugar que conoce la relación
  completa `LocomotionState ↔ marker`; agregar un 14° motor a futuro exige
  editar esa función central para sumar el add/remove del nuevo marker (un
  registro más largo que la Solución 1, donde el 14° motor solo agrega su
  propio guard sin tocar `arbitrate`).
- **§16** — 13 structs marker nuevas (aunque unitarias) más la lógica de
  diffing en `arbitrate()` es más código nuevo total que la Solución 1 para
  resolver el mismo problema.
- Riesgo de proceso (no una cláusula, pero bloqueante): la "Definición de
  terminado" del ticket pide literalmente el guard `if *state != X {
  continue }`. Esta solución cumple el objetivo de fondo (nada de `run_if`
  global, dispatch independiente por actor) pero no la letra — requeriría
  renegociar esa línea del ticket antes de adoptarse.

### Tradeoffs

- **Pro:** elimina por construcción la clase de bug de la Solución 1 (guard
  olvidado/mal escrito) — el filtrado es a nivel de tipo/archetype, no de
  rama en runtime.
- **Pro:** cada `tick` ya no necesita leer `&LocomotionState` en su tupla —
  la query en sí *es* la condición de activación, más declarativo.
- **Con:** introduce un mecanismo nuevo (markers + diffing de transición en
  `arbitrate`) en vez de reusar el que ya está probado en `spike.rs` — más
  superficie de diseño para revisar, y el spike deja de ser una prueba fiel
  1:1 del código real (contradice la nota del ticket: "si algo del spike
  quedó desactualizado, es señal de alerta a documentar").
- **Con:** `Commands` es diferido — el add/remove del marker no está
  garantizado visible para `TickActiveMotor` en el mismo frame sin verificar
  el punto exacto de aplicación del schedule, mientras que la Solución 1 lee
  `LocomotionState` recién escrito de forma síncrona, sin ese riesgo.

### Edge cases

1. Un actor recién spawneado (antes de que `arbitrate` corra por primera
   vez) no tiene ningún marker todavía — los 13 `tick` lo ignoran ese primer
   frame por completo. `spawn_player`/un futuro `spawn_enemy` tendría que
   insertar el marker inicial (`ActiveFall`, dado que `LocomotionState`
   default es `Fall`) explícitamente, o el actor queda "invisible" a todo
   motor un frame.
2. Si el `Commands` del cambio de marker no se aplica antes de
   `MovementSet::TickActiveMotor` en el mismo frame (depende del punto de
   flush del schedule), el actor tickea con el marker del frame anterior —
   un frame de lag respecto al `LocomotionState` recién arbitrado, divergencia
   de comportamiento vs. Solución 1 que debe verificarse con un test
   explícito, no asumirse.
3. Un bug de oscilación en `propose` (flip-flop entre dos estados en frames
   consecutivos) provoca dos archetype moves por frame en vez de un simple
   `if` barato — bajo esa condición adversarial el costo por frame crece de
   forma no trivial, justo el escenario que §18 pide vigilar.

---

## Solución 3 — Abstracción compartida de dispatch (`SystemParam`/helper genérico)

### Enfoque

Mantener el mecanismo de la Solución 1 (guard interno leyendo
`&LocomotionState`, sin markers nuevos) pero extraer el patrón repetido
("`Query<With<Actor>>` + iterar + comparar estado + `continue`") a una
abstracción compartida en `motor_common.rs` — un `SystemParam` custom o un
helper genérico que los 13 motores reusan en vez de reescribir el mismo `if`
trece veces. El objetivo es que un guard olvidado sea *imposible de escribir
mal* porque el punto de guard vive en un solo lugar, no en 13 copias.

### Clausulas de CONSTITUTION.md en riesgo

- **§4** — una abstracción genérica que intenta cubrir los 13 motores (cuyas
  tuplas de componentes son todas distintas: 4 campos en unos, 7 en otros)
  arriesga terminar exponiendo una interfaz ancha "por si acaso" en vez de
  chica y específica — el riesgo central de esta solución (ver Tradeoffs:
  la abstracción real puede no unificar los 13 casos).
- **§1** — si el helper termina absorbiendo también parte de la lógica de
  cada motor (para "ahorrar" líneas) mezcla la responsabilidad de dispatch
  con la responsabilidad de cada motor — debe quedar estrictamente en
  "iterar + filtrar", nada más.
- **§15** — un `SystemParam` genérico con bounds no triviales necesita
  comentarios para explicar *por qué* existe la abstracción (no es obvio por
  el nombre), lo cual empuja contra "comentarios se evitan por defecto salvo
  necesidad genuina" — aquí sí sería necesaria, pero es una cláusula a vigilar.
- **§9** — si el helper genérico oculta un `.unwrap()`/`.expect()` interno
  "para simplificar" el tipo de retorno, reintroduce un panic path que la
  Solución 1 no tiene.

### Tradeoffs

- **Pro:** si la abstracción efectivamente unifica los 13 casos, un guard
  nunca más se escribe a mano — elimina la clase de bug de la Solución 1 sin
  el costo estructural (archetype moves, markers nuevos) de la Solución 2.
- **Pro:** no se aparta de la letra de la Definición de terminado (sigue
  siendo "guard interno por entidad", solo que factorizado).
- **Con:** las tuplas de componentes de los 13 motores son heterogéneas
  (`sprint.rs` lee `Stamina`, `jump.rs` lee `JumpState`+`GroundFacts`,
  `mantle.rs` lee `LedgeFacts`, etc.) — generalizar sobre eso en Rust sin
  macros es difícil, y con macros se pierden mensajes de error legibles del
  compilador; hay riesgo real de que la abstracción termine cubriendo solo
  6-7 motores "fáciles" y el resto vuelva a `Query` cruda, dejando el código
  en dos estilos a la vez (peor que elegir uno solo desde el principio).
- **Con:** cuesta más tiempo de diseño upfront que la Solución 1 para un
  beneficio que solo se paga si de verdad se evita un bug de guard — con 13
  sitios y un test de invariante ya exigido por el ticket, ese bug ya está
  cubierto por otra vía (los tests), reduciendo el valor marginal real de la
  abstracción.

### Edge cases

1. Un motor con una condición de activación *compuesta* (no solo
   `*state == X`, sino además "y no está `needs_release`", como
   `mantle.rs`/`jump.rs`) no encaja limpio en un helper genérico pensado para
   "un estado, un guard" — termina necesitando un escape hatch que reintroduce
   el `if` manual de todos modos, para justo los motores con más estado.
2. Un contribuidor que agrega un 14° motor más adelante (fuera de este
   ticket) tiene que entender los bounds genéricos del helper/`SystemParam`
   antes de poder escribir un guard de una línea — más fricción que copiar
   el patrón explícito de la Solución 1, que es "leer cualquiera de los 13
   motores existentes y repetir".
3. Si se implementa como `SystemParam` derivado, las restricciones del propio
   derive de Bevy (un solo `Query` mutable por slot, lifetimes) pueden chocar
   con motores que necesitan leer y mutar varios componentes distintos a la
   vez — forzando a pelear con el borrow checker/macro por una ganancia de
   boilerplate, no por una necesidad funcional real.

---

## Recomendación del Builder

Solución 1. Es la única que no introduce mecanismo nuevo — reusa,
literalmente, el patrón ya validado por los 3 tests de `spike.rs` y ya
documentado como target en `movement.md`/`rationale/multi-actor-dispatch.md`.
Las Soluciones 2 y 3 atacan un riesgo real (un guard de copy-paste olvidado
en una de 13 ediciones) pero lo hacen agregando superficie de diseño nueva
(markers + diffing, o una abstracción genérica) cuyo costo — medido en
cláusulas de CONSTITUTION en juego (§18 y §2 en la Solución 2; §4 y §1 en la
Solución 3) — supera el beneficio, dado que el ticket ya exige un test de
invariante de arquitectura que cubre exactamente ese riesgo (dos actores no
comparten `LocomotionState` ni estado `Local` promovido). Ese test, no una
abstracción nueva, es la defensa correcta contra la clase de bug que
Soluciones 2 y 3 intentan prevenir estructuralmente.

## Chosen Solution

**Solución 1 — Migración mecánica de paridad con el spike.**

Elegida sin modificaciones de diseño adicionales sobre la Iteración 3: es la
única de las tres que no introduce mecanismo nuevo (markers de archetype de
la Solución 2, o abstracción genérica de la Solución 3) — reusa, literalmente,
el patrón ya validado por los 3 tests de `spike.rs` y ya documentado como
target en `movement.md`/`rationale/multi-actor-dispatch.md`. El riesgo real
que Soluciones 2 y 3 atacaban (un guard de copy-paste olvidado en una de 13
ediciones) queda cubierto por el test de invariante de arquitectura que el
ticket ya exige como entregable obligatorio (Constitución §11, excepción
Tier 3) — no por una abstracción nueva que agregaría superficie de diseño
(§18/§2 en la Solución 2; §4/§1 en la Solución 3) sin necesidad.

Los tres "Required Improvements" señalados por la crítica del Reviewer a lo
largo de las iteraciones quedan resueltos en `multi-actor-migration-plan.md`
como pasos numerados y explícitos del Core Logic Flow, no como notas
implícitas:

1. El test de invariante de arquitectura ("dos actores no comparten
   `LocomotionState` ni `Local` promovido") es el Paso 15 del Plan —
   entregable obligatorio, no gateado por checkpoint de *feeling* (§11).
2. `cargo fmt` y `cargo clippy --workspace --all-targets` de árbol completo
   son el Paso 18 del Plan — dos comandos nombrados, en ese orden, una sola
   pasada al final de la migración (§13).
3. La verificación de que ningún `.single()`/`.unwrap()`/`.expect()` nuevo se
   coló al portar los 13 motores es el Paso 17 del Plan — un paso separado y
   nombrado con el comando `rg` exacto a correr y diff contra baseline, no
   folded dentro del paso de clippy (§8/§9).
