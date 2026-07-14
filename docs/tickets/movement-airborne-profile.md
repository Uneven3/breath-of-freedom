## Ticket: `movement-airborne-profile`

## Sistema(s)

Movement. Migra el tuning actualmente global de `Fall` a un perfil persistente
por actor, sin cambiar la fisica, el arbitraje ni los valores del Player.

## Decision de modelo

`AirborneMovement` no es un permiso de gameplay llamado `CanFall`. Es la
configuracion base de locomocion aerea para un `Actor` sujeto a gravedad:
velocidad de control en el aire, aceleracion, gravedad de ascenso/caida,
salto corto, giro y recuperacion de stamina.

Esto permite que Link, una criatura o una montura compartan el motor `Fall`
con valores distintos. Una entidad que no sea un cuerpo de personaje sujeto a
esta fisica puede omitir el componente y no sera seleccionada por `Fall`.

## Alcance (File Touches)

- `src/movement/abilities.rs`
- `src/movement/mod.rs`
- `src/movement/motors/fall.rs`
- `docs/architecture/movement.md`
- `docs/architecture/rationale/movement-capability-composition.md`
- `docs/tickets/movement-airborne-profile.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No cambia la formula de gravedad, velocidad, recuperacion, prioridades,
  `LocomotionState`, sensores, arbitraje ni orden del schedule.
- No convierte `AirborneMovement` en una dependencia implicita de Jump,
  Glide, Climb, Ladder o WallJump; la composicion valida se documenta, pero
  este corte no altera reglas de entrada de otros motores.
- No disena vuelo, double jump, natacion ni la variante Metroid de WallJump.

## Definicion de terminado

- [x] `AirborneMovement` contiene todo tuning configurable que hoy pertenece
      al motor `Fall`.
- [x] `Fall` propone y ejecuta solo para actores con ese perfil.
- [x] `AirborneMovement::PLAYER` conserva exactamente el comportamiento
      actual.
- [x] Existe una prueba de arquitectura para la ausencia de propuesta sin el
      perfil.
- [x] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
      y `git diff --check` pasan.
- [x] El usuario valida con `cargo run` caida, salto corto, salida de borde y
      planeo antes de abrir otro ticket.

## Estado

Checkpoint jugable aceptado.
