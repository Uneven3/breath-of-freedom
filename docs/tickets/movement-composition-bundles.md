## Ticket: `movement-composition-bundles`

## Sistema(s)

Movement. Reemplaza el armado manual y frágil del Player por bundles de datos
que expresan el núcleo físico de un actor y las dependencias runtime de cada
capacidad ya migrada.

## Decision de modelo

Los bundles no activan sistemas ni sustituyen los componentes de capacidad.
Son una conveniencia de construcción de entidades: un sistema sigue usando
`Query` sobre `GroundMovement`, `JumpMovement`, etc.

Se distinguen dos niveles:

- `KinematicActorBundle`: datos físicos y de pipeline necesarios para un
  actor cinemático de Movement (cuerpo, facts, estado, velocity, stamina e
  intents).
- Bundles por capacidad: incluyen el componente de tuning y solamente el
  estado runtime privado que sus motores requieren. Por ejemplo,
  `JumpMovementBundle` contiene `JumpMovement`, `JumpPhase` y `JumpLocal`.

Esto deja que un bokobo, caballo o actor remoto componga exactamente sus
capacidades sin repetir ni olvidar los latches/timers internos.

## Alcance (File Touches)

- `src/movement/bundles.rs` (nuevo)
- `src/movement/mod.rs`
- `docs/architecture/movement.md`
- `docs/architecture/rationale/movement-capability-composition.md`
- `docs/tickets/movement-composition-bundles.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No cambia componentes, valores, sensores, estados, arbitraje, schedule ni
  comportamiento de ningún motor.
- No crea bokobos, caballos, IA, red ni nuevos presets de diseño.
- No mete control local (`InputControlledBy`, orientación, cursor) en el
  bundle de Movement: ese contrato pertenece a Input/Brain y es opcional para
  actores controlados por IA o red.

## Definicion de terminado

- [x] El Player se construye con el núcleo físico y bundles de capacidades,
      sin tupla manual de estados de motor.
- [x] Cada bundle por capacidad contiene solo datos del contrato de sus
      propios motores; no fusiona capacidades independientes.
- [x] Una prueba de arquitectura verifica la composición de un actor mínimo
      y de bundles de capacidad sin iniciar sistemas.
- [x] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
      y `git diff --check` pasan.
- [x] El usuario valida el mapa con `cargo run`.

## Estado

Aceptado por checkpoint manual del mapa.
