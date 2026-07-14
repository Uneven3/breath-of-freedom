## Ticket: `movement-sensor-profiles`

## Sistema(s)

Movement sensing. Convierte los alcances, alturas y umbrales que todavía son
constantes globales de GroundService y LedgeService en perfiles físicos por
actor.

## Decision de modelo

`GroundSensing` y `LedgeSensing` son perfiles de percepción física, no
capacidades de gameplay. No conceden Walk, Climb, Mantle, Vault ni WallJump;
solamente describen cómo un cuerpo consulta el mundo para publicar
`GroundFacts` y `LedgeFacts`.

- `GroundSensing` pertenece al núcleo de todo actor cinemático sujeto a suelo:
  distancia de probe y umbral de ascenso.
- `LedgeSensing` es opcional e independiente de las capacidades. Es necesario
  para que un actor produzca facts de pared/borde; Climb, LedgeTraversal y
  WallJump continúan decidiendo por sus propios componentes y facts.
- Stairs y Ladder quedan fuera: sus triggers authored ya viven por entidad de
  mundo y los motores reciben las dimensiones físicas del actor.

Los perfiles `PLAYER` reproducen exactamente los valores existentes. No se
derivan nuevas alturas desde `BodyDimensions` en este corte: eso cambiaría la
geometría de sensores validada. El perfil deja preparada esa variación para
actores futuros sin alterar el Player.

## Alcance (File Touches)

- `src/movement/sensing.rs` (nuevo)
- `src/movement/services/ground.rs`
- `src/movement/services/ledge.rs`
- `src/movement/bundles.rs`
- `src/movement/mod.rs`
- `docs/architecture/movement.md`
- `docs/architecture/rationale/movement-capability-composition.md`
- `docs/tickets/movement-sensor-profiles.md`
- `docs/WORKING-CONTEXT.md`

## Fuera de alcance

- No cambia la geometría efectiva, valores Player, facts, estados, arbitraje
  ni orden de systems.
- No mezcla los perfiles de sensor con `GroundMovement`, `ClimbMovement` o
  `LedgeTraversal`.
- No rediseña la interpretación de Ledge para cuerpos grandes/pequeños ni
  crea nuevas entidades de prueba jugables.

## Definicion de terminado

- [ ] GroundService y LedgeService solo procesan actores con su perfil de
      sensor correspondiente.
- [ ] Todos los números que gobiernan esos casts y clasificaciones proceden
      del perfil por actor; `PLAYER` conserva los valores actuales.
- [ ] `KinematicActorBundle` instala `GroundSensing`; el Player recibe
      `LedgeSensing::PLAYER` sin acoplarlo a una capacidad.
- [ ] Tests de arquitectura y de perfiles cubren ausencia de sensor y
      preservación de valores Player.
- [x] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
      y `git diff --check` pasan.
- [x] El usuario valida el mapa con `cargo run`.

## Estado

Aceptado por checkpoint manual del mapa.
