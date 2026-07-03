# Mounts

**Carpeta objetivo:** `src/mounts/`

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `MountIntents` | `mounts/intents.rs` | Propio, aislado de `movement::Intents` (mismo principio que `CombatIntents` en `combat.md`): `move_dir`, `wants_boost` (galope), `wants_takeoff`/`wants_dive` (voladoras). |
| `MountLocomotionState` | `mounts/state.rs` | SSoT propio: `Idle`, `Gallop`, `Sprint`, `Rear`, `TakeOff`, `Fly`, `Dive`, `Land`. Terrestres nunca proponen `Fly`/`Dive`/`TakeOff` — es el motor de esa especie el que no existe, no un chequeo en runtime. |
| `MountProposalBuffer` | `mounts/proposal.rs` | Type alias sobre el núcleo genérico compartido de capacidad fija `proposal::ProposalBuffer<MountLocomotionState, N>` (`src/proposal.rs`) — ver `rationale/proposal-arbitration-core.md`. |
| `MountedOn(Entity)` | `mounts/mod.rs`, en el jinete | Apunta a la entidad-montura. Única relación entre jugador y montura; ver `rationale/mounts-intent-redirect.md`. |
| `MountBody` | `mounts/mod.rs` | Marker de entidad montura simulada por Mounts. Una montura no necesita ser `Actor`; si también participa en Combat/Enemies se agregan markers adicionales explícitos. |
| `Bond` | `mounts/bond.rs` | Vínculo personal jugador-criatura (GDD §8, estilo Avatar) — dato reservado, sin campos definidos todavía. |

## Estados (`MountLocomotionState`) — propuesta, a confirmar

Terrestre: `Idle`, `Gallop`, `Sprint`, `Rear`. Voladora: agrega `TakeOff`,
`Fly`, `Dive`, `Land`. Qué subset tiene cada especie: dato de la especie, no
de la arquitectura.

## Sistemas (comportamiento) — propuesta

Pipeline hermano del de Movement, `SystemSet`s propios (`MountSet`),
encadenados en `FixedUpdate`:

1. **TranslateIntent** — `translate_mount_intents`, `.after(MovementSet::ReadIntents)`:
   lee `Intents` del jinete vía `MountedOn` y escribe `MountIntents` en la
   montura. Único punto donde Mounts lee algo de Movement.
2. **GatherProposals** — motores de montura (`gallop`, `fly`, `dive`, …)
   proponen a `MountProposalBuffer`, análogo a `motors::*::propose`.
3. **Arbitrate** — mismo algoritmo que Movement/Combate (núcleo compartido,
   ver `rationale/proposal-arbitration-core.md`), escribe
   `MountLocomotionState`.
4. **TickActiveMotor** — corre el motor activo de la montura sobre
   `Query<.., With<MountBody>>`, gateado por su propio `MountLocomotionState`.
   El jinete puede ser `Actor`; la montura es un cuerpo de Mounts salvo que
   otro sistema le agregue markers adicionales explícitos.

## Relaciones con otros sistemas

| Relación | Categoría | Mecanismo |
|---|---|---|
| Mounts traduce `Intents` del jinete hacia `MountIntents` de la montura mientras existe `MountedOn` | READ + WRITE-OWN | `translate_mount_intents` vive en Mounts, corre en `MountSet::TranslateIntent`, después de `MovementSet::ReadIntents` y antes de `MountSet::GatherProposals`; `movement::brain` no conoce `MountIntents` — ver `rationale/mounts-intent-redirect.md` |
| Mounts comparte el núcleo de arbitración con Movement/Combate | SHARED-CONTRACT | `MountProposalBuffer` es un type alias sobre `proposal::ProposalBuffer<MountLocomotionState, N>` de capacidad fija — ver `rationale/proposal-arbitration-core.md` |
| Monturas es un pipeline hermano de Movement, no una extensión | ninguna | Su propio `MountBody`/`MountLocomotionState`/buffer/arbitración, no comparte estado locomotor con `movement::` |
| Combate mientras se está montado (¿se puede disparar arco desde el lomo?) | decisión abierta | GDD no lo especifica |

## Decisiones abiertas

- Cuerpo físico del jugador mientras está montado (colisión, parenteado) —
  ver `rationale/mounts-intent-redirect.md`.
- Diseño concreto de criaturas (cuáles, terrestres vs. voladoras) — GDD §13.
- Mecánica de vínculo/doma (`Bond`).
- Desmontar a mitad de una maniobra.
- Si Enemies puede montar: hereda esta misma traducción, leyendo los `Intents`
  que produzca `EnemyBrain`.
