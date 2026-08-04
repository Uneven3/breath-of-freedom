# Leyes que se cobran solas

Plan para que las leyes de `ARCHITECTURE.md` dejen de depender de que alguien se
acuerde (**≤300 líneas**). **Los crates son la última fase, no la primera**: lo
que se puede cerrar con un test esta semana no espera a un refactor de seis.
Estado: **fases 1-7 cerradas; queda la 8**. Se borra cuando cierre — igual que
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
| 8 | Los tipos | El estado inválido deja de ser representable |

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

Los crates ordenan *quién ve a quién*; esto ataca el error de runtime que podría
ser error de compilación. Es aditivo y se paga por módulo.

| # | Trabajo | Dolor que evita |
|---|---|---|
| 8.1 | `#[require]` en las nueve capacidades (`SprintMovement` → `SprintLock`, etc.) | Spawnear un actor a medias: ninguna query engancha y el actor no hace nada, **sin un solo error** |
| 8.2 | Newtypes de unidades en `movement` (141 `f32` crudos, 2 newtypes) | Pasar metros donde van metros/segundo, o un radio donde va una altura: compila y produce un movimiento *casi* bien, que se atribuye al tuning |

**8.1 depende de bajar trece tipos de bookkeeping a domain** (`SprintLock`,
`JumpLocal`, `JumpPhase`, `GlideLocal`, `StairsLocal`, `StairsGrace`,
`MantleState`, `VaultState`, `WallJumpState`, `EdgeLeapState`, `SneakLock`,
`Crouched`, `StandClearance`). Son datos puros por actor, pero con campos
privados que los motores manipulan: mudarlos obliga a abrirlos. Se abren los que
no protegen ningún invariante (`SprintLock(bool)` no tiene nada que proteger);
los que sí tienen lógica real, como `MantleState` con su `KinematicArc`, se
quedan y su capacidad no lleva `require`.

**8.2 va con checkpoint jugado**: toca el feeling de locomoción.

**Dónde parar.** Los 51 `bool` que podrían ser enums **no** entran: un `bool`
con nombre claro y un solo escritor no es deuda. Se convierte el que cause un
bug real. Perseguir los `pub` que quedan por §4 tampoco: es pulido de superficie
que no previene ningún bug de gameplay.

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
| 8 | Spawnear una capacidad sin su bookkeeping es imposible; las unidades de `movement` no se pueden intercambiar; checkpoint jugado. |
