## Ticket: `movement-body-dimensions`

## Sistema(s)

Movement. Migra las dimensiones globales de la cápsula del actor a
`BodyDimensions`, un perfil persistente por entidad, para que sensores y
motores funcionen con cuerpos de distinto tamaño.

## Decision de modelo

Las capacidades de locomoción ya controlan qué acciones ejecuta un actor y
con qué tuning. `BodyDimensions` complementa ese modelo con la geometría que
esas acciones necesitan: radio de cápsula y longitudes de pie/agachado.

No es una capacidad ni un estado. Es configuración física de un actor
cinemático. El `Collider` continúa siendo la forma que Avian usa para las
consultas y el movimiento; este perfil da a Movement las medidas semánticas
que no puede recuperar de forma portable de un `Collider` opaco.

## Alcance (File Touches)

- `src/movement/body.rs`
- `src/movement/mod.rs`
- `src/movement/motor_common.rs`
- `src/movement/services/ledge.rs`
- `src/movement/motors/climb.rs`
- `src/movement/motors/wall_jump.rs`
- `src/movement/motors/ladder.rs`
- `src/movement/motors/stairs.rs`
- `src/movement/motors/sneak.rs`
- `src/debug.rs`
- `src/visuals.rs`
- `docs/architecture/movement.md`
- `docs/architecture/rationale/movement-capability-composition.md`
- `docs/tickets/movement-body-dimensions.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No cambia la forma actual del Player ni agrega soporte para cuerpos no
  cápsula.
- No cambia geometría authored, alcance de sensores, prioridades, estados,
  arbitraje ni orden del schedule.
- No crea variantes de bokobo/caballo; deja el contrato listo para ellas.

## Definicion de terminado

- [x] `BodyDimensions::PLAYER` reproduce exactamente la cápsula actual.
- [x] Todo cálculo de Movement que usa radio, altura de pies o cápsula de
      Sneak lee `BodyDimensions` en vez de constantes globales.
- [x] Los servicios y motores que necesitan el perfil lo exigen mediante su
      `Query`; no hay fallback global silencioso.
- [x] Pruebas de arquitectura cubren que un actor sin dimensiones no entra en
      un motor que depende de ellas.
- [x] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
      y `git diff --check` pasan.
- [x] El usuario valida en `cargo run` suelo, pendiente, escaleras, Sneak,
      Climb, Ladder, Mantle y WallJump.

## Estado

Checkpoint jugable aceptado.
