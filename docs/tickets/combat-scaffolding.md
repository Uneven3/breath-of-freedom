# Ticket: `combat-scaffolding` — IMPLEMENTADO (pendiente checkpoint jugado)

## Sistema(s)

Combat (nuevo plugin `src/combat/`), con un toque quirúrgico en Movement
(consumo de constraints) e Input (acción `Attack`).

## Lectura obligatoria, en este orden

1. `docs/CONSTITUTION.md` — completo.
2. `docs/architecture/combat.md` (el plan que este ticket ejecuta).
3. `docs/architecture/rationale/combat-combo-chains.md`.
4. `docs/architecture/rationale/proposal-arbitration-core.md`.

## Qué se construyó

- **Pipeline hermano de Movement**: `CombatSet` (ReadIntents →
  GatherProposals → Arbitrate → TickActiveMotor → EmitConstraints)
  encadenado en `FixedUpdate` **después** de `MovementSet::TickActiveMotor`
  (los sweeps leen transforms post-movimiento del mismo tick).
- `CombatState` (`Idle/Windup/Active/Recovery` por ahora — los demás llegan
  con sus fases), con `ALL` + guard de exhaustividad (patrón
  `LocomotionState`) y `commits_the_body()`.
- `CombatIntents { attack }` — `wants_guard/parry/aim` llegan cuando un
  motor los lea, no antes.
- `CombatProposalBuffer`: **segundo consumidor real del núcleo compartido**
  (`src/proposal.rs`) — la apuesta de `proposal-arbitration-core.md` quedó
  validada sin cambios en el núcleo. Pesos en `combat/proposal.rs::weight`
  con const-asserts de orden.
- Brain de hardware (`combat/brain.rs`) con `CombatInputCursor` (newtype:
  Movement ya posee un `InputConsumeCursor` en el mismo actor y dos
  consumidores no pueden compartir cursor sin robarse flancos).
- Dispatcher exhaustivo `combat::motors::tick_active_motor` desde el día
  uno; `idle::propose` como fallback Default.
- **Constraints**: `LocomotionConstraintMessage` vive en
  `movement/constraints.rs` (**el receptor es dueño del contrato** — se
  corrigió respecto del plan original que lo ponía en combat/messages.rs,
  para ser consistente con `DirectThreatMessage`/`DamageRequestMessage`).
  `apply_locomotion_constraints` deriva `LocomotionConstraintFacts` por
  actor antes de GatherProposals; los facts expiran por silencio;
  `sprint::propose` se abstiene bajo `forbid_sprint`.
- Input: `IntentAction::Attack` (mouse izquierdo, held + trigger), gateado
  por `PointerCaptured` para que el click que recaptura el cursor no golpee.

## Fuera de alcance (respetado)

Armas/daño (→ `combat-melee-combo`), Guarding/Parrying/Aiming/Staggered,
`Interrupt` de constraints, `CombatIntents` para IA.

## Definición de terminado

- [x] fmt/clippy/test limpios; dispatcher exhaustivo.
- [x] Invariantes §11: constraint no-bleed entre actores y expiración por
      silencio; flanco de attack consumido una sola vez; arbitraje decae a
      Idle por silencio; emit solo desde estados comprometidos.
- [x] `combat.md` actualizado donde el código divergió del plan (ubicación
      del mensaje de constraints).
