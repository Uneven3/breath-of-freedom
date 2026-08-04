# Leyes que se cobran solas

Plan para que las leyes de `ARCHITECTURE.md` dejen de depender de que alguien se
acuerde (**≤300 líneas**). **Los crates son la última fase, no la primera**: lo
que se puede cerrar con un test esta semana no espera a un refactor de seis.
Estado: **fases 1-8 cerradas**. Queda borrar este documento cuando su
contenido vivo termine de mudarse a `ARCHITECTURE.md` y `AHORA.md`. Se borra cuando cierre — igual que
`AHORA.md` borra lo cerrado; el detalle de cada fase queda en git.

## El principio

**Si una ley se puede convertir en error de compilación o en un test, se
convierte. La que necesita revisor es la que se incumple** — C2 creció mientras
vivió sólo en prosa; el test ahora obliga a que su lista sólo pueda encoger.

Corolario para este repo: **no se agregan leyes.** 21 es más de lo que se
recuerda al escribir código. Lo que sale de aquí es al revés — leyes que se
funden porque el build ya las cobra.

## Lo cerrado, y con qué mecanismo quedó cobrado

| # | Fase | La cobra ahora |
|---|---|---|
| 1 ✅ | Tests de frontera | `tests/architecture.rs`: hardware fuera de `input` y `unsafe`. **No se borra** (ver abajo) |
| 2 ✅ | Determinismo | Replay headless de 120 ticks por `ActorId`, dentro de `bof_simulation` |
| 3 ✅ | Lints | Los `[lints]` de cada `Cargo.toml`: §8, §9, §12, §13 |
| 4 ✅ | Acceso al terreno | `TerrainAccess` es el único `SystemParam` de lectura |
| 5 ✅ | `bof_domain` | §19: el dato compartido no declara `bevy` ni Avian |
| 6 ✅ | `bof_simulation` | §20: no declara `bevy_render`, `bevy_input` ni `bevy_window` |
| 7 ✅ | Presentación ↔ simulación | Un test: presentación no nombra `bof_simulation` |
| 8 ✅ | Los tipos | `#[require]`: una capacidad sin su estado no existe |

**El test de fase 1 sobrevive a su fecha de vencimiento.** El plan decía que en
fase 6 lo reemplazaría Cargo. Eso valía sólo si `input` cruzaba a simulación; se
quedó en la app (abajo), así que Cargo cobra que *simulación* no lea hardware,
pero dentro de la app no llega nadie. Sus 13 entradas siguen ahí y sólo pueden
encoger.

## El corte en crates

```text
breath-of-freedom (bin)   composición: main.rs, scene, world::layout/spawn, input
   ├── bof_presentation   visuals, camera, presentation, sfx, debug, perf, editor
   ├── bof_simulation     movement, combat, mounts, enemies, player, world,
   │                      projectiles, health, inventory, interaction, time_control
   └── bof_domain         datos puros: tipos, unidades, Intents, estados, facts
```

Nombres según la regla del workspace (paquete prefijado con el slug, import
corto por `[lib] name`):

```toml
bof_domain = { package = "breath_of_freedom_domain", path = "crates/domain" }
```

### Hermanas, no pila

```text
Lineal:     domain ← simulation ← presentation ← app
Hermanas:   domain ← simulation ← app
            domain ← presentation ← app
```

**Decisión del usuario 2026-08-03: hermanas.** §20 dice "presentación solo
READ". Si presentación ve simulación, ve sus funciones y sus sistemas, y "solo
leer" vuelve a ser disciplina. Si sólo ve `domain`, que es dato puro, **leer es
lo único que puede hacer**. El impuesto es real y ya se está pagando: todo lo que
presentación lea tiene que ser dato en `domain`.

### Tres cosas que el reparto original decía mal

Se escribió antes de elegir hermanas y antes de ver el código de cerca:

- **`input` se queda en la app.** Lee hardware *y* maneja el cursor de la
  ventana. Afuera, la simulación no *puede* leer teclado; adentro habría
  arrastrado `bevy_window`. No pierde nada: `ActiveActions`,
  `ControlOrientation` e `IntentAction` ya son domain.
- **`world::layout` y `world::spawn` se quedan en el binario.** Son composición,
  como `scene`. El binario es la única capa que ve simulación y presentación a la
  vez, así que es el único lugar donde armar collider **y** malla en la misma
  función es legal. Del resto de `world` cruzó todo: heightfield, semántica por
  celda, marcadores authored y el reloj.
- **De `day_night` cruzó el reloj, no la luz.** `advance_time` escribe cada tick
  y presentación sólo lee, así que no podía terminar en el crate equivocado
  aunque hoy nadie de gameplay consulte la hora.

### La costura que se repitió cinco veces

Terreno, player, enemigos, caballo y el reloj estaban todos acoplados a `scene`
por lo mismo: **cuándo** nacen. La respuesta fue siempre igual — simulación
expone *cómo* se construye (una función o un mensaje), la tabla de escenas decide
*cuándo*, y la entidad declara su vida con `SceneScoped`, que la app bindea a
`DespawnOnExit`. Ningún sistema de simulación nombra `AppState`.

Vale como patrón para lo que venga: si algo de simulación quiere consultar el
estado de la app, la pregunta correcta suele ser qué mensaje debería recibir.

## Fase 7 — presentación deja de ver la simulación

**Cerrada el 2026-08-04, sin crear el crate.** El objetivo era que presentación
sólo pudiera leer dato puro; eso ya está: **cero referencias a
`bof_simulation`** en `camera`, `debug`, `visuals`, `presentation`, `sfx` e
`inventory::pickup`, congeladas por `tests/architecture.rs`.

Lo que cruzó a domain, y por qué cada uno es dato y no implementación:

| Tipo | Por qué domain |
|---|---|
| `MovementSet` | contrato de **orden**: `debug` ordena sus trazas contra las fases del broker |
| `Crouched` | `bool` por actor; el visual agacha la cápsula con él |
| `Enemy` | marcador; elige el visual del bokobo |
| `DrawStrength`, `BOW_SOCKET_LOCAL` | carga del arco y el socket donde presentación pone la malla |
| `AIM_PIVOT_HEIGHT`, `AIM_SHOULDER_OFFSET` | la cámara **tiene** que converger con el origen del proyectil |
| `BokoboSpawnRequest` | un pedido, no una implementación |

**La decisión de diseño se resolvió como manda §19.** `visuals::vfx` dibujaba el
arco del swing llamando a `ComboLocal::current_step`, que por eso había tenido
que hacerse `pub`. Ahora el motor publica `SwingFacts { reach, arc_deg }` y el
VFX lee el dato; `current_step` volvió a `pub(crate)`. `ComboLocal` declara
`#[require(SwingFacts)]`, así que el fact no puede faltarle a nadie que ataque,
y se escribe sólo cuando cambia, para que `Changed<SwingFacts>` dispare una vez
por golpe y no una por tick.

### Por qué no se creó `bof_presentation`

Porque el crate no agregaría nada que no esté ya cobrado. Medido antes de
empezar: cero `&mut` de presentación sobre componentes de simulación, y todo lo
que le pide va por mensaje. Con las referencias en cero y un test que las
congela, la ley se cobra igual — y sin pagar el precio de mover siete módulos y
partir `Health`, que **no tenía ningún problema**: vive en domain desde la fase
5, y simulación sólo tiene su plugin y `apply_damage`.

Queda anotado para el día que haga falta: si el binario empieza a ser un lugar
donde presentación y simulación se mezclan de nuevo, el crate es la respuesta
grande y el test avisará primero.

## Fase 8 — los tipos

**8.1 cerrada el 2026-08-04: el estado de motor no se puede olvidar.** Las nueve
capacidades declaran `#[require]` sobre su bookkeeping, así que agregar
`SprintMovement` trae `SprintLock`, `JumpMovement` trae sus dos latches, y así.
Antes lo hacía un bundle por capacidad: una convención que había que recordar.
Olvidarla no daba error — la query no enganchaba y el motor no andaba, en
silencio. Los nueve bundles redundantes se retiraron; sobreviven
`KinematicActorBundle` (identidad, pose, cuerpo físico, perfiles) y
`SneakMovementBundle`, que necesita las dimensiones para sus dos cápsulas.

Para declararlo hubo que bajar trece tipos de bookkeeping a
`bof_domain::movement::motor_state`: la capacidad vive en domain y no puede
apuntar hacia arriba. Sus campos son `pub` porque no protegen ningún invariante
— son latches y contadores. La excepción es `KinematicArc`, que sí tiene uno
(`elapsed <= duration`) y bajó con sus campos privados y su API intacta.

Van 15 usos de `#[require]`.

### 8.2 — newtypes de unidades: **medido y descartado**

El plan decía que los 141 `f32` crudos eran el mayor retorno de tipado, contra
2 newtypes. El número asustaba y era engañoso. Medido el 2026-08-04:

**En todo `movement` hay 4 funciones con dos o más parámetros `f32`** — el único
lugar donde intercambiar dos valores compila y produce un bug callado. Y de esas
cuatro, dos toman parámetros de la **misma** unidad (`move_toward(from, to,
delta)`, `stair(base_x, center_x)`), donde un tipo de unidad no distingue nada.
Quedan dos casos reales: `KinematicArc::step(dt, arc_height)` y
`apply_locomotion_rotation(…, dt, speed)`.

Los 141 campos no son 141 oportunidades de error: viven dentro de structs con
nombre (`GroundMovement { max_forward_speed, acceleration, … }`), los lee su
propio motor y nunca cruzan una frontera donde puedan confundirse. Envolverlos
exigiría implementar aritmética en cada newtype o salpicar `.0` por cada
fórmula, y todo lo que toque `Vec3`/`Transform` vuelve a `f32` igual, porque
glam no tiene unidades.

Es el mismo criterio con el que el medidor decide una técnica de render: **dice
cuándo vale la pena, no se aplica siempre**. Acá dijo que no. Si algún día
aparece un bug de unidades real, se tipa esa unidad y no las seis.

## La frontera se prueba, no se declara

Cargo prohíbe la dependencia inversa, pero no prueba que la simulación siga
siendo la misma sin pantalla. Eso sólo se prueba corriéndola. El smoke headless
ya no levanta una esfera con gravedad: levanta `SimulationPlugin` entero, sin
ventana ni GPU, en segundos.

La fase 2 fijó el snapshot: `Transform`/`BodyVelocity`/`LocomotionState` de cada
`ActorId` durante 120 ticks. En fase 7 se corre el mismo contrato con y sin
presentación para probar que la capa sólo lee.

**Los crates se testean por paquete.** Meter el binario en la misma invocación
que el crate unifica las features de Avian y el smoke deja de ser headless
(panickea). `cargo test`, `cargo test -p breath_of_freedom_simulation` y
`cargo test -p breath_of_freedom_domain`, por separado.

## Qué pasa con las leyes al terminar

Este plan no agrega leyes: las funde. Al cerrar cada fase, su ley se reescribe
como una línea que **nombra su mecanismo**, y el espacio liberado paga el techo
de `ARCHITECTURE.md` (200/200, al límite). Ya se hizo con §4, §14, §19 y §20.

Ese techo debería cobrarse solo: un test que sume los cuatro documentos core y
falle sobre 1000 líneas. Hoy es honor-system.

## Criterios de aceptación

| Fase | Verde cuando |
|---|---|
| 6 ✅ | Sin `Mesh`/`StandardMaterial`/`bevy_input` en `crates/simulation/src`; smoke headless levanta el juego entero; checkpoint jugado. |
| 7 ✅ | Cero referencias a `bof_simulation` desde presentación, congeladas por test; checkpoint jugado. |
| 8 ✅ | Spawnear una capacidad sin su bookkeeping es imposible; los newtypes de unidades quedaron descartados por medición; checkpoint jugado. |
