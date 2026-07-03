# Combat

**Carpeta objetivo:** `src/combat/`

## Datos (Components/Messages/Resources) — propuesta

| Tipo | Dónde | Qué es |
|---|---|---|
| `CombatIntents` | `combat/intent.rs` | Propio, nombre distinguible del `Intents` de Movement (evita confusión entre agentes que no comparten memoria de sesión): `wants_attack`, `wants_parry`, `wants_aim`. |
| `CombatState` | `combat/state.rs` | Enum SSoT propio de Combate — ver Estados. |
| `CombatProposalBuffer` | `combat/proposal.rs` | Type alias sobre el núcleo genérico compartido de capacidad fija `proposal::ProposalBuffer<CombatState, N>` (`src/proposal.rs`) — ver `rationale/proposal-arbitration-core.md`. |
| `LocomotionConstraintMessage` | `combat/messages.rs` (o similar) | Combate lo emite con `MessageWriter`; Movement lo interpreta como restricción semántica (`ForbidSprint`, `Interrupt(Stagger)`, etc.) y decide un estado locomotor físicamente válido. Combate nunca escribe el `ProposalBuffer` de Movement directamente. |
| `health::DamageRequestMessage` (no propio) | ver `health.md` | Combate calcula el monto final (incluye bonus de sigilo) y lo emite hacia el `target`; Health valida/aplica, Combate no conoce el pool de vida. |
| `projectiles::SpawnProjectileMessage` (no propio) | ver `projectiles.md` | Combate lo emite al soltar el arco; Projectiles construye y simula la flecha. |
| `inventory::ApplyDurabilityLossMessage` (no propio) | ver `inventory.md` | Combate lo emite al conectar un golpe con el arma equipada; Inventory valida y descuenta `Durability`, Combate no conoce el pool. |

## Estados (`CombatState`) — propuesta, a confirmar

`Idle`, `Windup`, `Active` (hitbox activo), `Recovery`, `Parrying`, `Aiming`
(arco tensado), `Staggered` (te pegaron).

Abierto: ¿falta un estado de bloqueo con escudo? ¿`Aiming` necesita
sub-estados (tensando vs. sostenido) para la cámara lenta del arco?

## Sistemas (comportamiento) — propuesta

Pipeline hermano de Movement, `SystemSet`s propios (`CombatSet`),
encadenados en `FixedUpdate`:

0. **ReadIntents** — un Brain propio de Combate escribe `CombatIntents`,
   mismo slot conceptual que `movement::brain::read_intents`. Lee
   `input::ActiveActions` para el `InputSource` enlazado por
   `input::InputControlledBy` en el actor (`input.md`) y traduce
   `Attack`/`Parry`/`Aim` a
   `wants_attack`/`wants_parry`/`wants_aim`. Los gatillos usan un
   `input::InputConsumeCursor` propio de Combate/actor; Combate no lee
   `ButtonInput<KeyCode>`, no sabe qué esquema de hardware está activo y no
   muta el snapshot global de input.
1. **GatherProposals** — motores de combate (`attack`, `parry`, `aim`, …)
   proponen a `CombatProposalBuffer`. `idle::propose` empuja
   `CombatState::Idle` a `Priority::Default` cada frame — análogo al
   `fall::propose` de Movement — así el buffer nunca llega vacío a
   `arbitrate(current)`.
2. **Arbitrate** — núcleo compartido (`rationale/proposal-arbitration-core.md`),
   escribe `CombatState`.
3. **TickActiveMotor** — corre el motor activo sobre `Query<.., With<Actor>>`,
   gateado por su propio `CombatState`.
4. **EmitConstraints** — tras `Arbitrate`, traduce ciertos `CombatState` a
   `LocomotionConstraintMessage` hacia Movement: **`Windup`, `Active`,
   `Recovery`, `Parrying`, `Aiming` → `ForbidSprint`** (cualquier estado de
   compromiso activo con una acción, no solo `Aiming` — ver nota abajo),
   `Staggered` → `Interrupt`. **`Sneak` nunca se restringe** — sigilo +
   ataque es el combo de bonus de daño que ya define el GDD (§7), no un
   conflicto a evitar.

   Por qué se generalizó más allá de `Aiming`: `sprint::propose` en
   Movement no exige `move_dir != 0` para proponer `Sprint` (alcanza con
   `grounded && wants_sprint`), así que sostener el modifier de esprintar
   mientras se ataca cuerpo a cuerpo (no solo apuntando el arco) puede
   ganarle a `Walk` en arbitración aunque el jugador esté quieto atacando —
   `Windup`/`Active`/`Recovery`/`Parrying` necesitan la misma restricción
   que `Aiming` ya tenía, o `LocomotionState` puede terminar en `Sprint`
   mientras `CombatState` está `Active`, una combinación sin sentido físico
   que además puede disparar animación/SFX de correr en pleno golpe.

Qué dispara `Staggered`: un sistema de Combate escucha
`health::DamageAppliedMessage`/`DeathMessage` dirigidos a su propio actor (no
a quien lo emitió) y, si
corresponde, empuja una propuesta `Staggered` a `CombatProposalBuffer` con
prioridad alta — Health nunca elige estados de Combate, solo notifica que
algo pasó (`rationale/health-ownership-boundary.md`).

## Relaciones con otros sistemas

| Relación | Quién lee a quién | Mecanismo |
|---|---|---|
| Sigilo: golpe conectado mientras `Movement::LocomotionState == Sneak` → bonus de daño | Combate lee Movement | Query read-only; el monto final se emite como `health::DamageRequestMessage` (`health.md`) |
| Flecha del arco: `CombatState::Active` tras `Aiming` | Combate → Projectiles | Combate emite `projectiles::SpawnProjectileMessage`; Projectiles simula el vuelo y aplica el daño al impactar (`projectiles.md`) |
| Estar comprometido con una acción (`Windup`/`Active`/`Recovery`/`Parrying`/`Aiming`) impide Sprint | Combate → Movement | Combate emite `LocomotionConstraintMessage::ForbidSprint(Entity)`. Movement decide si corresponde `Walk`, `Idle`, `Fall`, `Swim`, etc. según sus facts y estado físico — nunca `Sprint` mientras dure la restricción |
| Recibir un golpe (`Staggered`) interrumpe el motor activo de Movement (ej. cancela un Mantle a medio camino) | Combate → Movement, forzado | Combate emite `LocomotionConstraintMessage::Interrupt(Entity, InterruptKind::Stagger)`. Movement convierte esa interrupción en una propuesta válida para su dominio |
| Golpe conectado con el arma equipada descuenta su durabilidad | Combate → Inventory | Combate emite `inventory::ApplyDurabilityLossMessage`; Inventory decide si el arma se rompe (`WeaponBrokenMessage`) — ver `inventory.md` |

Ninguna relación implica que Combate o Movement compartan un `Intents` o un
enum de estado — cada uno tiene el suyo. La regla es aislar por sistema, no
por instancia compartida; ver `rationale/when-not-broker-pattern.md` para el
criterio de cuándo sí corresponde copiar el patrón Brain/Intents/Broker.

**Timing de `LocomotionConstraintMessage` (decidido):** el sistema de Combate que
detecta el stagger y emite el mensaje **no** se ordena explícitamente antes de
`MovementSet::GatherProposals` — Movement lo consume recién en su propio
`GatherProposals` del frame siguiente. Es 1 frame de latencia (~16ms a 60Hz),
imperceptible para una interrupción de este tipo, y evita que el plugin de
Combate necesite conocer el ordenamiento interno de los `SystemSet`s de
Movement (mantiene los dos plugins desacoplados en el schedule). Movement es
el único sistema que elige el `LocomotionState` final.

## Decisiones abiertas

- Confirmar la lista de estados de arriba.
- Arco: mecánica exacta de apuntado (cámara lenta, primera persona) — GDD §8.
- Durabilidad de armas: **no es `Health`** (ver `health.md` § Relaciones) —
  vive en Inventory/Equipment; el mensaje de descuento ya está decidido
  arriba, falta confirmar cuánta durabilidad descuenta cada tipo de golpe.
- Variedad de armas cuerpo a cuerpo (pocos tipos, GDD §8): modelar
  cómo peso/velocidad/alcance afectan `Windup`/`Recovery`.
- IA enemiga que "lee" al jugador (GDD §7) — **confirmado**: `Enemies`
  (`docs/architecture/enemies.md`) reusa este mismo `CombatIntents`/
  `CombatState` con un `EnemyBrain` propio y requiere el contrato multi-actor
  (`rationale/multi-actor-dispatch.md`).
