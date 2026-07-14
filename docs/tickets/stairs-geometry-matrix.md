# Ticket: `stairs-geometry-matrix`

## Sistema(s)

Movement / World. World aporta cursos graybox authored; Movement mantiene el
contrato de recorrido de escaleras sin acoplarse a Ladder, Climb o Ground.

## Problema

El stair actual valida solo escalones rectos, uniformes, de 0.5 m de huella.
Sus offsets de snap son constantes y el trigger es un AABB global, por lo que
no representa bien peldaños largos/cortos ni una escalera curva de castillo.

## Contrato

- `Stairs` describe un tramo recto uniforme y un volumen oriented-box; una
  escalera curva se compone de tramos de un peldaño, sin añadir un estado nuevo
  ni una dependencia hacia otros motores.
- `StairsService` publica la geometría del tramo que contiene al actor.
- El motor limita su muestra de subida/bajada a la profundidad authored del
  tramo para no saltar más de una contrahuella en peldaños cortos; conserva la
  gravedad cuando la caída excede una contrahuella.
- Los cursos graybox incluyen huella larga con salida a pendiente, huella corta
  y arco segmentado. Son escenarios de play-test, no reglas de gameplay.

## Alcance (File Touches)

- `src/world.rs`
- `src/movement/facts.rs`
- `src/movement/services/stairs.rs`
- `src/movement/motors/stairs.rs`
- `docs/architecture/movement.md`
- `docs/tickets/stairs-geometry-matrix.md`

## Fuera de alcance

- Escaleras móviles, mallas importadas, peldaños no uniformes dentro del mismo
  tramo, animación, sonido y cambio de reglas de Sprint/Walk.

## Verificación

- [ ] Los tres cursos graybox se recorren sin cambio de estado espurio.
- [ ] Transición desde el stair largo a la pendiente no deja un labio ni caída.
- [x] El servicio elige el trigger de escalera más cercano dentro de un
      solape; no depende del orden de query.
- [x] `cargo fmt`, `cargo test`, `cargo clippy --all-targets -- -D warnings`
      y `git diff --check` pasan.
