# Rationale: dispatch multi-actor (`Actor` genérico)

Monturas, Enemies y Multiplayer necesitan N actores corriendo el mismo
pipeline (Brain → Intents → Broker → Motors → Body) simultáneamente e
independientemente. El contrato objetivo es un marker genérico `Actor`:
`Player`, `Enemy`, jugadores remotos y otros cuerpos controlables son
especializaciones de ese actor.

## Decisión

El pipeline de simulación no debe depender de `Single<..., With<Player>>`.
Los sistemas de Movement y Combat operan sobre `Query<..., With<Actor>>`,
iterando todos los actores relevantes.

La fase `TickActiveMotor` es **un único sistema dispatcher**
(`motors::tick_active_motor`): una sola pasada sobre
`Query<MotorTick, With<Actor>>` que despacha cada actor con un `match`
exhaustivo sobre su `LocomotionState` al `tick_body` del motor dueño.
`MotorTick` (`#[derive(QueryData)]`) es la unión de las filas de todos los
motores: el contrato de `KinematicActorBundle` como campos requeridos, y
capacidades/estado-por-motor como `Option` — cada `tick_body` hace
early-return si su capacidad no está (situación que el arbitraje ya
previene: un motor solo propone para actores con su capacidad).

Todo estado temporal que pueda variar por actor vive en componentes por
entidad, no en `Local<T>` compartido por el sistema (`JumpLocal`,
`GlideLocal`, `SprintLock`, `StairsLocal`, …).

## Por qué el dispatcher (y no 13 sistemas auto-filtrados)

La versión anterior registraba los 13 `tick` como sistemas separados, cada
uno con el guard `if *state != LocomotionState::X { continue }`. Eso
funcionaba, pero el invariante "exactamente un motor mueve cada cuerpo por
frame" era una **convención** que cada motor debía sostener, verificada por
revisión humana. Además, los 13 sistemas conflictúan entre sí para el
scheduler (todos con `&mut Transform` sobre `With<Actor>`): se serializaban
igual, iterando 13 veces a todos los actores para saltarse casi todos.

Con el dispatcher:

- El invariante lo garantiza el **compilador**: un `match` exhaustivo no
  puede olvidar un estado ni ejecutar dos motores; agregar un
  `LocomotionState` nuevo es un error de compilación hasta escribir su brazo.
- Una sola iteración por frame sobre los actores, en lugar de 13 pasadas.
- El sistema único queda listo para `par_iter_mut` si el conteo de actores
  alguna vez lo justifica (hoy no).

Los `propose` siguen siendo 13 sistemas independientes a propósito: ahí la
gracia del Broker es que cada motor decida por sí mismo, y el orden de
ejecución no importa porque el arbitraje resuelve por `(Priority, weight)`
(ver los tests `*_regardless_of_propose_order`).

## Por qué

- **Enemies:** `EnemyBrain` necesita escribir `Intents`/`CombatIntents` en
  entidades que no son el jugador local.
- **Multiplayer:** un actor remoto es un `Actor` cuyo `InputSource` viene de
  red; los mismos Brains genéricos traducen `ActiveActions` a
  `Intents`/`CombatIntents`.
- **Monturas:** el pipeline de Mounts es separado, pero jinetes no-jugador
  heredan el mismo modelo de actor genérico.

Ver `docs/architecture/movement.md` y `docs/ARCHITECTURE-MAP.md`
(categoría `BLOCKING-PREREQUISITE`).
