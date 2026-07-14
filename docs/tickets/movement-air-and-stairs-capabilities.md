## Ticket: `movement-air-and-stairs-capabilities`

## Sistema(s)

Movement. Migra Jump y Glide a capacidades persistentes independientes y
agrega el perfil de Stairs a `GroundMovement`, sin cambiar transiciones,
sensores, prioridades ni valores Player.

## Decision de capacidad

- `JumpMovement` gobierna Jump: un actor terrestre no tiene por que poder
  saltar.
- `GlideMovement` gobierna Glide: planear es una accion opcional, no una
  propiedad implicita de estar en el aire.
- Stairs no crea `StairMovement`: es una adaptacion de locomocion terrestre a
  geometria authored, no una accion independiente. Su perfil queda en
  `GroundMovement`, permitiendo que un caballo y Link compartan el mecanismo
  con tuning distinto.

## Alcance (File Touches)

- `src/movement/abilities.rs`
- `src/movement/mod.rs`
- `src/movement/motors/jump.rs`
- `src/movement/motors/glide.rs`
- `src/movement/motors/stairs.rs`
- `docs/architecture/movement.md`
- `docs/architecture/rationale/movement-capability-composition.md`
- `docs/tickets/movement-air-and-stairs-capabilities.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No cambia coyote time, buffer, fisica, stamina, triggers de escaleras ni
  geometria authored.
- No crea double jump, planeo con stamina consumible ni nuevas reglas para
  monturas.
- No modifica el arbitraje, `LocomotionState`, sensores ni el schedule.

## Definicion de terminado

- [x] Jump y Glide proponen y ejecutan solo con su capacidad.
- [x] Stairs requiere `GroundMovement` y usa su perfil `stairs`.
- [x] `PLAYER` reproduce los valores actuales exactamente.
- [x] Tests de arquitectura cubren la ausencia de propuesta sin cada
      capacidad.
- [x] fmt, tests, clippy y diff check pasan.
- [x] El usuario valida el batch con `cargo run`.

## Estado

Checkpoint jugable aceptado.
