# Ticket: `traversal-probe`

## Sistema(s)

Movement. El probe es un controlador de integracion del curso gris: consume el
contrato multi-actor de Movement sin introducir el sistema Enemies todavia.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/ARCHITECTURE-MAP.md` — fila `Movement`.
3. `docs/COUPLING-MAP.md` — Movement con Enemies/Debug/Input.
4. `docs/architecture/movement.md`.
5. `docs/architecture/rationale/multi-actor-dispatch.md`.
6. `docs/architecture/rationale/movement-capability-composition.md`.

## Acoplamiento

- Movement comparte el contrato `Intents` con el futuro sistema Enemies
  (Tight). El probe usa la forma existente sin cambiarla y no introduce tipos
  de Enemies.
- Debug es Loose y sigue siendo solo lectura. El probe no se implementa como
  un comando o sistema de Debug porque escribe `Intents`.
- Input no participa: el probe no lleva `InputControlledBy`; su brain es el
  unico writer de los intents de esa entidad.

## Alcance (File Touches)

- `src/movement/mod.rs`
- `src/movement/probe.rs` y `src/movement/probe_data.rs`
- `src/visuals.rs`
- `src/world.rs` (solo geometria graybox de contencion del curso)
- `docs/architecture/movement.md`
- `docs/WORKING-CONTEXT.md`
- `docs/tickets/traversal-probe.md`

## Fuera de alcance

- No agrega Enemies, pathfinding, combate, navegacion general ni networking.
- No cambia motores, servicios, hechos, arbitraje, prioridades ni el formato
  de `Intents`.
- No usa estados forzados para simular una maniobra: el probe nunca escribe
  `LocomotionState`.
- No convierte Debug en writer de simulacion.

## Checkpoint actual: escalada continua

El primer escenario evita deliberadamente resets y teletransportes. El probe
aparece frente a la muralla escalable gris y debe completar esta secuencia por
el pipeline normal: avanzar hasta que `LedgeService` publique `can_climb`,
solicitar `Climb`, ascender con input vertical y quedarse bajo el borde. No
emite `Jump` ni `Mantle`; por tanto, alcanzar el borde no debe iniciar
`Mantle`.

`IntentAction::MoveForward` sigue siendo una accion agnostica del controlador.
El brain la traduce a `ClimbVerticalIntent::Up` cuando publica `Intents`; los
motores de pared/ladder consumen esos ejes semanticos nombrados y nunca
interpretan input crudo como direccion de climb.

## Decision de contrato: intenciones semanticas de locomocion

Antes de ampliar el curso, `Intents` se normaliza como el contrato comun entre
input, IA, red y cinematica. Las fuentes expresan acciones de control como
`IntentAction::MoveForward`; el brain/controlador las traduce a estos datos de
gameplay, que son los unicos que consumen los motores:

| Dominio | Tipo | Semantica |
|---|---|---|
| Movimiento planar | `PlanarMoveIntent` | Direccion en mundo y magnitud deseadas. |
| Gait | `GaitIntent::{Walk,Sprint,Sneak}` | Modo de marcha mutuamente exclusivo. |
| Salto | `JumpIntent { held, pressed }` | Sostenido y flanco de presion. |
| Climb | `ClimbIntent` | Solicitud y ejes nombrados vertical/lateral. |
| Ladder | `LadderIntent::{Up,Down,Hold}` | Direccion vertical restringida de la escalera. |
| Mantle / Vault | `TraversalActionIntent` | Solicitud manual discreta mutuamente exclusiva. |
| Glide | `GlideIntent::{Inactive,Requested}` | Solicitud sostenida de planeo. |

Los motores no leen `raw_input`, acciones de hardware ni escriben
`LocomotionState`. Un controlador no modifica `Transform` o `BodyVelocity`
durante locomocion ordinaria. `Swim`, monturas e interaccion no se incluyen
hasta que existan sus propios motores y contratos.

- [ ] Spawn frente a la muralla escalable, sin reescrituras posteriores de
      `Transform` ni `BodyVelocity` por el brain.
- [ ] `ApproachWall` escribe solo avance; `AttachClimb` solo solicita climb;
      `AscendClimb` sube mientras el estado real sea `Climb`.
- [ ] El escenario se completa solo al sostener `Climb` en el borde y nunca
      por una transición a `Mantle`.
- [ ] El usuario valida el escenario con `cargo run`.

## Definicion de terminado del curso completo

- [ ] Se spawnea un segundo `Actor` visible, sin `Player` ni
      `InputControlledBy`, con el conjunto completo de capacidades de Link y
      tuning propio de criatura humanoide.
- [ ] Su brain corre en `MovementSet::ReadIntents` y es el unico writer de
      sus `Intents`; el brain local conserva el control exclusivo del Player.
- [ ] Tras validar el checkpoint de escalada, el script recorre fases de Walk, Sprint, Sneak, Jump, Glide, Stairs,
      AutoVault, Climb, Mantle, WallJump, EdgeLeap y Ladder. Cada fase solo se
      completa cuando el `LocomotionState` real observado corresponde a la
      maniobra solicitada.
- [ ] Las estaciones posteriores se conectan con navegacion continua o se
      documentan como escenarios independientes; nunca se usan teletransportes
      como sustituto de locomocion.
- [ ] Test de invariante: el brain del probe no modifica los `Intents` del
      Player y no puede completar una fase sin que el arbitraje haya elegido
      el estado pedido.
- [ ] El usuario valida en `cargo run` que el Player conserva su comportamiento
      y que el probe hace visibles las fases del curso.
- [ ] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
      y `git diff --check` pasan.
- [ ] Sin `unsafe`, dependencias nuevas, ni `unwrap()`/`expect()` fuera de
      tests.

## Notas para el agente que lo toma

El nombre de juego no debe tomar IP ajena. En UI/logs usar `TraversalProbe` o
`Criatura de prueba`, aunque el pedido original lo llamara "bokobo". Es una
prueba del limite controlador -> Intents, no un enemigo de produccion.
