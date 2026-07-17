# Ticket: `mounts-core` — SUPERSEDED / CERRADO

Este ticket queda superseded para trabajo futuro por
`docs/implement-feature/movement-capabilities-and-mount-lifecycle-plan.md`.
El playtest validó la dirección de reutilizar Movement, pero expuso rigidez de
capacidades y errores de ownership/lifecycle. El reemplazo se implementó en
los tickets A–H del plan nuevo; este archivo conserva el alcance histórico y
no se reescribe como especificación del resultado.

**Cierre 2026-07-16:** refactor automatizado completo; checkpoint jugado final
pendiente. La fuente vigente es `docs/architecture/mounts.md`.

## Alcance entregado

- Horse graybox spawn/despawn con F8 y montar/desmontar con E.
- Horse compuesto como `Actor` de Movement: suelo, aire, salto, stamina y
  120 HP; no recibe traversal humanoide.
- Redirect genérico de Movement transfiere planar, sprint y
  salto en el mismo tick y neutraliza al jinete.
- Silla, `ColliderDisabled`, herencia de velocidad, F8 y muerte desmontan de
  forma segura antes del despawn.
- Carga automática sobre 11 m/s: daño y impulso, una vez por `Enemy` hasta
  rearmarse al bajar del umbral.
- Daño con origen, owner persistente e inmunidad hostil completa del horse
  frente al owner.
- Perfiles de espada y arco montados seleccionados por `CombatContext`.

## Validación histórica del prototipo

- Redirección aislada: controles compatibles, neutralización y ningún actor
  no relacionado afectado.
- Composición del horse sin traversal humanoide y carga deduplicada/rearmable.
- Inmunidad de Health frente al dueño, pero no frente a otra fuente.
- Los checks del cierre viven en el plan reemplazante.

## Pendiente

- [ ] Checkpoint jugado: F8, E, sprint/salto, carga contra bokobo,
  espada/arco montados, daño enemigo y muerte del horse.
