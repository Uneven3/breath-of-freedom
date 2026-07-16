# Working Context

This file preserves the active implementation intent across agent sessions and
context compaction. It complements, but does not replace, the scoped work
records in `docs/tickets/`.

## Update Protocol

- Read this file before continuing the current refactor.
- Update it after each accepted design decision, implementation checkpoint, or
  user playtest result.
- Keep tickets as the source of truth for ownership and scope; keep this file
  focused on the cross-session technical handoff.

## Project Roadmap / Phase Gate

The build advances in phases, and the gate between them is explicit:

1. **Phase 1 — Movement (current).** Get locomotion as good as it can be:
   no observable movement bugs on the graybox course, and every gameplay
   `Intent` completable through the ordinary pipeline. The proof that movement
   is "done" is behavioral, not just green tests: a non-player actor (the
   bokobo / `TraversalProbe`) must be able to drive *all* the same intents a
   player can and reach every locomotion state correctly.
2. **Gate → Combat.** Only once (a) no movement bug is outstanding and (b) the
   bokobo AI completes the full intent set correctly do we start Phase 2
   (Combat, `docs/architecture/combat.md`). The shared arbitration core
   (`src/proposal.rs`) is a bet that Combat reuses this shape — expect Combat to
   *validate or reshape* it, not to inherit it unchanged.

**Load-bearing principle for this whole plan:** AI and networked actors move
**only** by writing `Intents`. They never hardcode values into or bypass the
physics/motor pipeline — no direct writes to `Transform`, `BodyVelocity`,
`LocomotionState`, facts, or motor-private state. This is what makes "the AI
uses the same movement the player does" true rather than aspirational, and it
is why the arbiter/motor split exists (see the Invariants section and
`docs/architecture/rationale/multi-actor-dispatch.md`).

## Current Objective

Validate the completed ECS locomotion composition with a second, AI-controlled
actor without changing stable motor behavior. Actors receive persistent
capability/configuration components with per-actor values, while systems
select compatible actors through queries.

The target composition should allow, for example:

- Link and a bokobo to share ground and climb capabilities with different
  tuning.
- A horse to have a faster ground profile but no climb capability.
- Player input, network control, and AI to write the same `Intents` contract.

## Architecture Decisions

- An actor is an entity with data components such as `Health`, `Stamina`,
  `Intents`, capability/configuration components, and exactly one active
  `LocomotionState`.
- Persistent capabilities describe what an actor may do. Active locomotion
  state describes what it is doing now. Do not use an active state as an
  ability flag.
- Systems are always scheduled. A component does not activate a system;
  `Query` filters select the entities for which that system runs.
- Keep the current pipeline: input/control writes `Intents`, sensing writes
  facts, motors propose transitions, the arbiter selects one state, and the
  active motor writes movement.
- Do not replace the central arbiter or make locomotion states concurrent in
  this refactor. That would change the behavioral model rather than merely
  composing actor capabilities.
- Bevy assets are shared data in `Assets<T>` resources; entities normally hold
  handles and presentation components rather than owning asset collections.
- La taxonomia de capacidades y su orden de migracion vive en
  `docs/architecture/rationale/movement-capability-composition.md`. Esa
  rationale es la fuente de verdad para decidir si un motor comparte una
  capacidad o necesita una propia; no usar la cercania entre estados como
  criterio.

## Current Checkpoint

The user accepted the Ground, traversal, air/stairs, airborne-profile,
body-dimensions, and composition-bundles migration batches in `cargo run`.
sensor-profiles migration batches in `cargo run`. No movement migration ticket
is active.

All locomotion tuning and capsule geometry are now per actor. Construction
ergonomics is implemented: `KinematicActorBundle` owns the shared Movement
contract and capability bundles own each motor family's runtime components,
while systems remain driven by individual component queries. The next cut
should be selected from a fresh architecture review; the capability,
construction, body, and sensor-profile foundations are complete.

### Active Ticket

`docs/tickets/traversal-probe.md` is active. It adds a visible,
AI-controlled `TraversalProbe` to the graybox course. Its controller writes
only its own `Intents` in `MovementSet::ReadIntents`; it never writes
`LocomotionState`, facts, motor-private state, `Transform`, or body velocity.
The current checkpoint is a continuous climb scenario: approach the climbable
wall, request `Climb` after the observed wall fact, then ascend and hold below
the lip without requesting `Jump` or `Mantle`. It passes only after the
ordinary sensor/proposal/arbiter pipeline reaches those observed conditions.

The probe is deliberately not an Enemy or a Debug command. It is an
integration consumer of Movement's existing multi-actor contract, and its
name avoids third-party game IP.

### Implemented

- `src/movement/abilities.rs` defines:
  - `GroundLocomotion`, a pure per-actor tuning profile.
  - `GroundMovement`, the persistent ground capability with `walk`, `sprint`,
    and `sneak` profiles.
  - `GroundMovement::PLAYER`, preserving all prior values exactly:
    Walk `(5.0, 20.0, 25.0, 15.0, +15.0)`, Sprint
    `(10.0, 25.0, 35.0, 15.0, -10.0)`, Sneak
    `(2.5, 15.0, 20.0, 10.0, +5.0)`, in the order max speed,
    acceleration, friction, rotation speed, stamina/sec.
- `GroundMovement` additionally owns the `StairsLocomotion` profile.
- `BodyDimensions` owns capsule radius plus standing/crouched cylinder
  lengths. Ledge, Stairs, Ladder, Climb/WallJump lip clipping, Sneak, debug
  gizmos, and Player presentation use its derived heights or colliders rather
  than global dimensions.
- `JumpMovement`, `GlideMovement`, `ClimbMovement`, `LadderMovement`,
  `LedgeTraversal`, and `WallJumpMovement` are implemented with their Player
  values preserved exactly.
- Every migrated motor filters its proposal and tick query by the component
  that owns its tuning. `Stairs` uses `GroundMovement`.
- `KinematicActorBundle` constructs the actor's physical/pipeline contract;
  capability bundles construct `GroundMovement` with Sprint/Sneak state,
  `JumpMovement` with its phase/timers, `GlideMovement` with press memory,
  `LedgeTraversal` with Mantle/AutoVault state, and `WallJumpMovement` with
  WallJump/EdgeLeap state. They are construction helpers only, not runtime
  capability gates.
- `GroundSensing` and `LedgeSensing` are the active physical sensor-profile
  migration: Ground belongs to the kinematic core, while Ledge remains an
  optional producer of facts independent from gameplay abilities.
- The player currently receives all existing locomotion profiles. The
  actor-isolation tests cover absence-of-capability proposal contracts.

### Changed Files

- `src/movement/abilities.rs` (new)
- `src/movement/bundles.rs` (new)
- `src/movement/mod.rs`
- `src/movement/motor_common.rs`
- `src/movement/motors/walk.rs`
- `src/movement/motors/sprint.rs`, `src/movement/motors/sneak.rs`, and
  `src/movement/motors/stairs.rs`
- `src/movement/motors/jump.rs`, `src/movement/motors/glide.rs`,
  `src/movement/motors/climb.rs`, `src/movement/motors/ladder.rs`,
  `src/movement/motors/mantle.rs`, `src/movement/motors/auto_vault.rs`,
  `src/movement/motors/wall_jump.rs`, and `src/movement/motors/edge_leap.rs`
- `src/movement/body.rs`, `src/movement/services/ledge.rs`, and
  `src/debug.rs`
- `docs/architecture/movement.md`
- `docs/tickets/movement-ground-ability.md`
- `docs/tickets/movement-ground-modes.md`

### Validation Already Run

Before this handoff, the implementation passed:

```text
cargo fmt
cargo test                 # 73 passed
cargo clippy --all-targets -- -D warnings
git diff --check
```

## Accepted Checkpoints

- Walk: flat ground, ramp, and stairs were accepted before the Sprint/Sneak
  slice.
- Ground modes: Walk, Sprint stamina drain/lock/recovery, Sneak ceiling
  clearance, ramp, and stairs were accepted by the user after the current
  slice.
- Traversal: Climb, Ladder, Mantle, AutoVault, WallJump, and EdgeLeap were
  accepted together after the user-requested mechanical migration batch.
- Air and stairs: Jump, Glide, and the Stairs profile of `GroundMovement` were
  accepted together.
- Airborne profile: Fall, released short jump, ledge exit, and leaving Glide
  were accepted after `AirborneMovement` replaced global Fall tuning.
- Body dimensions: all existing traversal remained stable after capsule
  geometry became persistent actor data.
- Composition bundles: the full map remained stable after Player construction
  moved into core and capability bundles.
- Sensor profiles: the full map remained stable after Ground/Ledge cast
  parameters moved from globals into per-actor physical data.

## Next Step After Confirmation

**Phase gate passed (2026-07-15):** movement validated (probe F6 completes
climb→mantle→turn→jump→glide) and the bokobo (F7) drives all intents with a
full senses model (vision/hearing/damage-aggro `Awareness`). Current focus:
**Combat MVP** — design in `docs/architecture/combat.md` (BotW feel,
per-weapon combo chains as data, phases in `CombatState`, step in
`ComboLocal`; see `rationale/combat-combo-chains.md`). Scope decided by the
user (2026-07-15): one sword, shield (guard/parry), bow with normal arrows,
camera lock-on, HP (Health system), swing VFX placeholder. **No flurry
rush / time dilation.**

**Implemented (2026-07-15, awaiting the played feeling checkpoint):**
`combat-scaffolding` + `combat-melee-combo` — full CombatSet pipeline after
Movement's, graybox 3-step sword (left click), hitbox sweep masked to
`GameLayer::Actor`, damage as log cue (until `health-core`),
`DirectThreatMessage` aggros the struck bokobo, swing-arc VFX, `combat:`
line in the HUD, `ForbidSprint` constraint consumed by `sprint::propose`
(message owned by `movement/constraints.rs`). See both tickets for the
handoff detail.

**Game feel layer (`combat-game-feel`, implemented 2026-07-15):**
`presentation/juice.rs` consumes `combat::HitImpactMessage` — hit flash,
procedural white burst, floating damage text (gold on crits), knockback via
`movement::BodyImpulseMessage`, jump/land jelly on every actor visual
(`visuals::VisualOf`), 90 ms hitstop on criticals via
`Time<Virtual>::relative_speed(0)`, and player-received feedback (screen
flash + `camera::CameraShake` trauma) wired but dormant until enemies attack.
**Next tickets:** `health-core` → `enemies-combat` → `combat-defense` →
`combat-bow` → `camera-lock-on`.

## Invariants To Preserve

- `LocomotionState` remains exclusive per actor.
- `Intents` remains the control boundary shared by player, AI, and future
  networking. AI/remote actors write **only** `Intents`; they never write
  `Transform`, `BodyVelocity`, `LocomotionState`, facts, or motor-private
  state directly. Bypassing the pipeline for a non-player actor is a bug, not a
  shortcut.
- Facts/sensors remain separate from motor execution.
- Only the active motor writes an actor's movement in a tick. The
  `motors::tick_active_motor` dispatcher enforces this with an exhaustive
  `match` on `LocomotionState` (one `tick_body` arm per state, checked by the
  compiler); the flat-ground motors share
  `motor_common::ground_locomotion_step`. The `arbitration_matrix` tests
  (`src/movement/proposal.rs`) pin that every state has exactly one owning
  motor and that no two co-proposing motors tie.
- Existing schedule ordering and the transition arbiter remain intact.
- Do not revert unrelated working-tree changes.

## Repository State

- Stable traversal work was previously committed as `f5b8700`
  (`feat: stabilize traversal locomotion`) and pushed by the user.
- The accepted locomotion-capability migration slices are intentionally
  uncommitted and should be committed together only when the user requests it.
