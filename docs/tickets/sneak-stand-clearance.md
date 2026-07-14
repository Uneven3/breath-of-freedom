# Ticket: `sneak-stand-clearance`

## Sistema(s)

Movement / Sneak. El cambio está confinado al motor Sneak y a sus datos por
actor; no modifica los motores de Climb, Mantle ni el arbitraje genérico.

## Problema

Al soltar Sneak bajo una superficie baja, el estado volvía a locomoción de pie
y reemplazaba la cápsula sin comprobar si cabía. Eso permite expandirse dentro
de geometría y deja a la física decidir una corrección no determinista.

## Implementación

- `StandCollider` mantiene la cápsula de pie precalculada en el actor: no se
  construye geometría en `FixedUpdate`.
- `update_stand_clearance` corre en `MovementSet::SenseWorld` y consulta la
  cápsula de pie con los pies anclados mediante
  `SpatialQuery::shape_intersections_callback`, que se detiene en el primer
  bloqueo y no asigna memoria.
- `propose` conserva Sneak mientras `Crouched && !StandClearance`, incluso si
  el botón ya fue liberado. Al desaparecer el techo, el arbitraje vuelve a
  locomoción normal y `sync_sneak_collider` restaura la cápsula.

## Verificación

- [x] El chequeo se ejecuta antes de `GatherProposals`.
- [x] No hay asignaciones ni construcción de collider en el hot path.
- [x] Las transformaciones de crouch y stand mantienen los pies anclados.
- [x] `cargo fmt`, `cargo test` y `cargo clippy --all-targets -- -D warnings`
      pasan.
