## Ticket: `movement-ground-modes`

## Sistema(s)

Movement. Extiende la capacidad persistente `GroundMovement` para que sus
perfiles por actor gobiernen Sprint y Sneak, sin cambiar la maquina de estados
ni las reglas de esos motores.

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md`
2. `docs/ARCHITECTURE-MAP.md` (fila Movement)
3. `docs/architecture/movement.md`
4. `docs/architecture/rationale/multi-actor-dispatch.md`
5. `docs/WORKING-CONTEXT.md`

## Acoplamiento

Movement es Tight con Enemies y Multiplayer por el contrato `Actor` /
`Intents`, pero este ticket no cambia esos tipos ni el arbitraje central.

## Alcance (File Touches)

- `src/movement/abilities.rs`
- `src/movement/motor_common.rs`
- `src/movement/motors/sprint.rs`
- `src/movement/motors/sneak.rs`
- `src/movement/mod.rs` (fixtures de tests)
- `src/movement/services/ground.rs` (referencia interna)
- `docs/architecture/movement.md`
- `docs/tickets/movement-ground-ability.md`
- `docs/tickets/movement-ground-modes.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No crea perfiles para Climb, Ladder, Glide, Stairs ni otros motores.
- No cambia `LocomotionState`, propuesta, arbitraje, prioridades ni orden del
  schedule.
- No introduce actores nuevos, IA, red ni monturas.
- No cambia los valores del Player ni su feeling validado.

## Definicion de terminado

- [x] `GroundMovement` contiene perfiles por actor para Walk, Sprint y Sneak.
- [x] Sprint y Sneak solo proponen y ejecutan para actores con
      `GroundMovement`.
- [x] Los perfiles Player conservan exactamente los valores anteriores.
- [x] Tests de contrato cubren ausencia de propuesta de Sprint/Sneak sin la
      capacidad; los tests de aislamiento declaran la capacidad requerida.
- [x] `cargo fmt` y `cargo test` pasan.
- [x] El usuario valido en `cargo run` Walk/Sprint/Sneak, stamina, rampa y
      escaleras antes de continuar la migracion.

## Estado

Checkpoint aceptado. La siguiente capacidad requiere un ticket nuevo y una
pasada de diseno; no ampliar este ticket con Climb u otros motores.
