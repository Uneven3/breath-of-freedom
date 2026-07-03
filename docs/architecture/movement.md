# Movement

**Carpeta objetivo:** `src/movement/`

## Datos (Components/Messages/Resources)

| Tipo | Dónde | Qué es |
|---|---|---|
| `Intents` | `intents.rs` | Snapshot de input del frame (move_dir, wish_dir, wants_jump, wants_sprint, wants_sneak, wants_climb, wants_mantle, wants_vault, wants_glide). Solo el Brain lo escribe. |
| `LocomotionState` | `state.rs` | Enum SSoT del modo activo. Solo `arbitrate` lo escribe. |
| `ProposalBuffer` | `proposal.rs` | Type alias sobre el núcleo genérico compartido de capacidad fija `proposal::ProposalBuffer<LocomotionState, N>` (`src/proposal.rs`), drenado por `arbitrate(current)`. Ver `rationale/proposal-arbitration-core.md`. |
| `TransitionProposal` | `proposal.rs` | `{ target_state, category: Priority, override_weight, source_id }`. |
| `BodyVelocity` | `mod.rs` | Velocidad del cuerpo cinemático (análogo a `CharacterBody3D.velocity`). Solo el motor activo la escribe. |
| `Stamina` | `stamina.rs` | Pool de esfuerzo. Solo sus propios métodos `drain`/`recover` la mutan. |
| `BodyContact`, `GroundFacts`, `LedgeFacts`, `StairsFacts`, `LadderFacts` | `facts.rs` | Hechos del mundo que los servicios calculan y los motores leen. |
| `ClimbInputState` | `brain.rs` | Estado del toggle de escalar por actor/controlador (tecla `1`, no un hold). No es `Resource` global, porque el contrato multi-actor requiere input independiente por actor. |
| `JumpPhase`, `JumpLocal`, `GlideLocal`, `SprintLock` | `motors/jump.rs`, `motors/glide.rs`, `motors/sprint.rs` | Estado propio de cada motor (timers, latches), por-actor. Eran `Local<T>` de sistema antes de `multi-actor-migration` — promovidos a componente para no compartir estado entre actores. |
| `MantleState`, `VaultState`, `WallJumpState`, `EdgeLeapState` | `motors/mantle.rs`, `motors/auto_vault.rs`, `motors/wall_jump.rs`, `motors/edge_leap.rs` | Máquina de fase compartida entre `propose` y `tick` de cada motor, por-actor desde su diseño original (`Local` no se puede compartir entre dos sistemas). |

`brain::read_intents` no lee `ButtonInput<KeyCode>` — lee
`input::ActiveActions` para el `InputSource` enlazado por
`input::InputControlledBy` en el actor (ver `input.md`) y arma
`move_dir`/`wish_dir` a partir de
`MoveForward/Back/Left/Right`, y `wants_jump`/`wants_sprint`/etc. a partir
de `Jump`/`Sprint`/etc. Los gatillos discretos usan un
`input::InputConsumeCursor` propio de Movement/actor; Movement no muta el
snapshot global de input. Qué entrada física dispara cada `IntentAction` es
una tabla rebindeable que Movement no conoce — el mismo código sirve sin
cambios para `Gamepad`/`KeyboardOnly`/`KeyboardMouse`.

## Estados (`LocomotionState`)

`Idle`, `Walk`, `Sprint`, `Fall`, `Jump`, `AutoVault`, `Climb`, `Mantle`,
`Stairs`, `Ladder`, `Glide`, `Sneak`, `WallJump`, `EdgeLeap`. Default: `Fall`.

## Sistemas (comportamiento)

Pipeline en `FixedUpdate` a 60Hz, `SystemSet`s encadenados (`MovementSet`):

1. **ReadIntents** — brains de hardware, IA o red escriben `Intents` por actor.
2. **SenseWorld** — 4 servicios (`ground`, `ledge`, `stairs`, `ladder`) escriben los `*Facts`.
3. **GatherProposals** — los 13 motores corren su `propose` cada frame, sin
   condición — cada uno decide si empuja un `TransitionProposal` al buffer.
4. **Arbitrate** — `arbitrate()` (`mod.rs`) elige el ganador por
   `(category, override_weight)` y escribe `LocomotionState`.
5. **TickActiveMotor** — los motores corren sobre `Query<.., With<Actor>>` y
   se auto-filtran con un guard interno por entidad
   (`if *state != LocomotionState::X { continue }`) en vez de un `run_if`
   global — esto es lo que permite simulación concurrente e independiente de
   jugadores locales, remotos, enemigos y otros cuerpos controlables. Ver
   `rationale/multi-actor-dispatch.md`.

`motors::sneak::sync_sneak_collider` corre en `Update` (swap de collider,
declarativo sobre `Changed<LocomotionState>`). Es presentación/forma física
derivada; no decide locomoción.

## Relaciones con otros sistemas

Movement expone datos de simulación para lectura read-only y mensajes de
interrupción; otros sistemas no mutan su estado interno directamente.

**Punto de extensión:** mensajes semánticos como
`LocomotionConstraintMessage::ForbidSprint` o
`LocomotionConstraintMessage::Interrupt` permiten que sistemas como Combate o
Salud pidan restricciones/interrupciones sin escribir directamente el estado
de Movement. Movement valida esos pedidos contra sus facts físicos y decide
el `LocomotionState` final — ver `docs/architecture/combat.md` § Relaciones.

**Monturas** (`docs/architecture/mounts.md`) no modifica Movement ni obliga a
`brain::read_intents` a conocer `MountIntents`. El input montado se resuelve
con un sistema de traducción propio de Mounts que lee `Intents` del jinete y
escribe `MountIntents` en la montura — ver
`rationale/mounts-intent-redirect.md`.

**Enemies** y **Multiplayer** no modifican Movement — reutilizan el mismo
`Intents`/`LocomotionState` con un Brain distinto (IA o red), pero ambos
requieren el contrato multi-actor descrito abajo.

## Decisiones abiertas

- Interpolación visual >60Hz (Avian transform interpolation).
- Cámara: colisión de spring-arm y landing dip.
- Regla de sprint en escaleras/muros escalables.
- Nadar/Bucear y Snowboard (GDD §9) se diseñan como motores nuevos de este
  mismo plugin — ver `swim.md`, `snowboard.md` y
  `rationale/traversal-extensions-in-movement.md`.
- **Contrato multi-actor:** `Query<.., With<Actor>>` + guard interno por
  entidad — implementado (ticket `multi-actor-migration`), ya no es un
  prerequisito pendiente de Enemies y Multiplayer. Ver
  `rationale/multi-actor-dispatch.md` y `docs/ARCHITECTURE-MAP.md`.
