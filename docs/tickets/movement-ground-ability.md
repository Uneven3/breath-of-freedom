## Ticket: `movement-ground-ability`

## Estado

Checkpoint aceptado. El Player conserva el comportamiento validado de Walk;
Sprint y Sneak se migran en el ticket sucesor `movement-ground-modes`.

## Sistema(s)

Movement. Introduce la primera capacidad/configuracion persistente por actor
sin cambiar ningun contrato con otro plugin.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md`
2. `docs/ARCHITECTURE-MAP.md` (fila Movement)
3. `docs/architecture/movement.md`
4. `docs/architecture/rationale/multi-actor-dispatch.md`

## Acoplamiento

Movement es Tight con Enemies y Multiplayer por el contrato `Actor` /
`Intents`, pero este ticket no cambia ninguno de esos tipos. No toca otros
sistemas.

## Alcance (File Touches)

- `src/movement/abilities.rs`
- `src/movement/mod.rs`
- `src/movement/motor_common.rs`
- `src/movement/motors/walk.rs`
- `src/movement/motors/sprint.rs`, `src/movement/motors/sneak.rs`
- `docs/architecture/movement.md`
- `docs/tickets/movement-ground-ability.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No migra Sprint ni Sneak a configuracion por entidad.
- No agrega criaturas, monturas, IA ni capacidades de Climb/Glide.
- No cambia `LocomotionState`, propuesta, arbitraje ni el orden del schedule.

## Definicion de terminado

- [x] `GroundMovement` representa capacidad y tuning de Walk por actor.
- [x] Walk solo propone y ejecuta para actores con `GroundMovement`.
- [x] El Player recibe los valores anteriores de Walk sin cambio de feeling.
- [x] Un test prueba que un `Actor` sin capacidad no propone Walk.
- [x] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings` y
      `git diff --check` pasan.

## Notas para el agente que lo toma

Este fue el primer checkpoint de una migracion incremental hacia locomocion
componible.
