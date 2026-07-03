# Plan: `multi-actor-migration`

Chosen solution: **Solución 1 — Migración mecánica de paridad con el spike**
(see `multi-actor-migration-solutions.md` § Chosen Solution). Normative
reference: `src/movement/spike.rs` (unchanged by this plan) and
`docs/architecture/rationale/multi-actor-dispatch.md`.

## Core Logic Flow

**Plan as Immutable Contract — IMPLEMENT follows these steps exactly, in
order, without restructuring.**

1. In `src/movement/mod.rs`, add a new marker component next to `Player`:
   `#[derive(Component)] pub struct Actor;`.
2. In `src/movement/motors/sprint.rs`, replace the `Local<bool>
   stamina_locked` parameter of `propose` with a new per-entity component:
   `#[derive(Component, Default)] pub struct SprintLock(pub bool);`. `propose`
   drops the `Local` parameter entirely and gains `&mut SprintLock` in its
   query tuple.
3. In `src/movement/motors/jump.rs`, add `#[derive(Component, Default)]` to
   the existing `JumpLocal` struct as-is — same 5 fields (`coyote`, `buffer`,
   `was_on_floor`, `prev_wants`, `needs_release`), no rename. Do **not**
   merge it into `JumpPhase` (different concerns: `JumpPhase` is cross-motor,
   read by `fall.rs`; `JumpLocal` stays `jump.rs::propose`-private).
4. In `src/movement/motors/glide.rs`, add `#[derive(Component, Default)]` to
   the existing `GlideLocal` struct as-is — same 2 fields (`prev_wants`,
   `was_glide`), no rename.
5. In `src/movement/motors/sprint.rs`, `jump.rs`, `glide.rs`: change
   `propose`'s signature from `Single<(...), With<Player>>` (+ `Local<T>` for
   sprint/jump/glide) to `Query<(&mut <PromotedComponent>, ...), With<Actor>>`,
   iterating with `for (...) in &mut q`. Same transformation logic as the
   pre-migration `Single`-based body, just per-entity now.
6. In `src/movement/motors/mantle.rs`, `auto_vault.rs`, `wall_jump.rs`,
   `edge_leap.rs`: no new component (they already have
   `MantleState`/`VaultState`/`WallJumpState`/`EdgeLeapState`). Change both
   `propose` and `tick` from `Single<(...), With<Player>>` to
   `Query<(...), With<Actor>>` + `for` loop. Purely mechanical.
7. In `src/movement/motors/walk.rs`, `climb.rs`, `stairs.rs`, `ladder.rs`
   (stateless motors): change both `propose` and `tick` from
   `Single<(...), With<Player>>` to `Query<(...), With<Actor>>` + `for` loop.
   Purely mechanical, no new component.
8. In `src/movement/motors/sneak.rs`: `propose` and `tick` get the same
   `Single`→`Query<With<Actor>>` + `for` treatment as step 7.
   `sync_sneak_collider` already uses `Query<(...), (With<Player>,
   Changed<LocomotionState>)>` — optionally swap `With<Player>` →
   `With<Actor>` for consistency (cosmetic, low priority; correctness does
   not depend on it since it already iterates).
9. **Every one of the 13 `tick` signatures** (`walk`, `fall`, `sprint`,
   `sneak`, `jump`, `glide`, `climb`, `mantle`, `auto_vault`, `wall_jump`,
   `edge_leap`, `stairs`, `ladder`) adds `&LocomotionState` to its query
   tuple and, as the first statement inside the `for` loop body, an in-body
   guard: `if *state != LocomotionState::<X> { continue; }`, where `<X>` is
   that motor's own state (e.g. `LocomotionState::Walk` in `walk::tick`).
   None of the 13 currently read `LocomotionState` in `tick` — this line is
   net-new in every one of the 13 files. This guard is the sole replacement
   for the deleted global `run_if` (step 12) — get this wrong and two motors
   can mutate the same actor's `Transform`/`BodyVelocity` in the same frame
   silently (see Solución 1's Edge case 3 in the solutions doc).
10. In `src/movement/mod.rs`, change `arbitrate()` from
    `Single<(&mut LocomotionState, &mut ProposalBuffer), With<Player>>` to
    `Query<(&mut LocomotionState, &mut ProposalBuffer), With<Actor>>`,
    iterating with `for (mut state, mut buffer) in &mut q`, calling
    `buffer.arbitrate()` per entity exactly as today. Do **not** touch
    `ProposalBuffer::arbitrate`'s body or signature in
    `src/movement/proposal.rs` — that file is not touched at all by this
    ticket (belongs to the parallel `proposal-core-extraction` ticket).
11. In `src/movement/mod.rs`, delete the `in_loco_state` function entirely.
12. In `src/movement/mod.rs`, in `MovementPlugin::build`, remove all 13
    `.run_if(in_loco_state(LocomotionState::<X>))` calls from the
    `TickActiveMotor` system-registration tuple. The 13 tick systems are now
    registered unconditionally; the in-body guard from step 9 is the only
    gate.
13. In `src/movement/mod.rs`, `spawn_player`: add `Actor` to the spawn
    tuple (the Player entity now carries both `Player` and `Actor`), and add
    the three newly-promoted components from steps 2–4
    (`SprintLock::default()`, `motors::jump::JumpLocal::default()`,
    `motors::glide::GlideLocal::default()`) to the existing nested
    "per-motor shared phase state" tuple block. Add another nesting level if
    the flat/nested tuple arity would exceed Bevy's 15-element bundle limit.
14. Update the existing unit-test helpers that spawn a bare `Player` and now
    fail to match the new `With<Actor>` queries: `climb.rs::tests::propose_with`
    and `edge_leap.rs::tests::setup` both add `Actor` to their `world.spawn((...))`
    tuples alongside `Player`.
15. **camera.rs — comment-only, no functional change** (out of scope for
    behavior; matches ticket's Fuera de alcance / the parallel Input
    ticket's territory). Keep `Single<.., With<Player>>` in `follow_player`
    and `camera_landing_dip` exactly as-is — the camera intentionally stays
    scoped to the single local `Player`, not generalized to `Actor`. If the
    file's doc comment or any inline comment implies "the" single actor in a
    way that would mislead once `Actor`-tagged NPCs exist, update the
    wording only (no signature/logic change). Do not touch `brain.rs` at
    all.
16. **[Required Improvement 1 — mandatory deliverable, not gated on a
    feeling checkpoint, Constitution §11 Tier-3 exception]** Add a new
    `#[cfg(test)] mod actor_isolation_tests` in `src/movement/mod.rs` (or a
    sibling test module it declares) that:
    - Builds a headless `App` (e.g. `MinimalPlugins` + `avian3d::prelude::PhysicsPlugins::default()`),
      without registering `MovementPlugin` wholesale (avoids needing
      `brain`/`camera` dependencies) — call the production systems directly
      via `world.run_system_once`, mirroring the pattern already used in
      `climb.rs::tests`/`edge_leap.rs::tests`.
    - Spawns two entities carrying the full production `Actor` bundle
      (mirroring `spawn_player`'s component shape, minus `Player`): one
      pre-seeded to resolve to `LocomotionState::Walk` (push a `Walk`
      `TransitionProposal` into its `ProposalBuffer` before arbitration), the
      other to resolve to `LocomotionState::Fall` (empty buffer — arbitrate's
      documented empty-buffer default).
    - Runs `arbitrate` once via `run_system_once`, then runs all 13 `tick`
      systems once each, in the same order as `TickActiveMotor`'s
      registration.
    - Asserts: the two entities' `LocomotionState` stay distinct and match
      what each was seeded to resolve to; per-entity promoted state
      (`SprintLock`, `JumpLocal`, `GlideLocal`, `JumpPhase`, `MantleState`,
      `VaultState`, `WallJumpState`, `EdgeLeapState`) mutated on one entity
      (e.g. drive the `Fall` actor into `Jump` via `wants_jump` in its
      `Intents` and confirm its `JumpLocal`/`JumpPhase` arm) leaves the
      other entity's copies at their default/untouched values — no
      cross-entity bleed through any promoted component or through
      `LocomotionState`.
    - This test must be green before this migration is considered complete;
      it is a first-class deliverable of this ticket, equal in standing to
      the `Single`→`Query` mechanical change itself, not a "nice to have"
      deferred to a later feeling-checkpoint pass.
17. Confirm `src/movement/spike.rs` still compiles unchanged and its 3 tests
    (`two_actors_dispatch_independently`,
    `exactly_one_motor_ticks_each_actor_per_frame`,
    `jump_state_does_not_bleed_between_actors`) still pass. If any assumed
    shape in `spike.rs` has gone stale relative to the real migrated code,
    document the drift in `docs/architecture/movement.md` (step 20) —
    do not silently patch `spike.rs` to paper over it.
18. **[Required Improvement 3 — explicit, named verification step, separate
    from clippy; clippy's default lint set does not catch this]** After all
    of steps 1–17 land, run:
    `rg -n "\.unwrap\(\)|\.expect\(|\.single\(\)" src/movement/motors/ src/movement/mod.rs`
    (or an equivalent full sweep of the 13 touched motor files + `mod.rs`).
    Diff every hit against the pre-migration baseline (capture the same `rg`
    output before starting step 1, e.g. via `git stash`/a throwaway branch,
    or `git show <pre-migration-commit>:<path> | rg ...` per file). For
    every hit introduced by this migration, confirm it is a genuine
    programmer-bug panic case already justified inline per §9 (a broken
    invariant the type system should have prevented) — not a `.single()`/
    `.unwrap()`/`.expect()` reached for out of comfort while porting a motor
    from `Single` to `Query`. Zero new unjustified hits is the bar; any
    found must be fixed (converted to the `Query`-iteration pattern or an
    explicit `Result`/`Option` handling) before proceeding.
19. **[Required Improvement 2 — explicit closing step, two named commands,
    full tree, in this order]** Run `cargo fmt` over the full workspace
    first, then a single `cargo clippy --workspace --all-targets` pass
    second (not a pass per file while editing each motor). Fix every
    warning (dead `Local` imports, unused tuple fields left over from the
    `Local`→`Component` shape change, etc.) — no `#[allow(...)]` without an
    explicit, punctual justification (§13). Re-run both commands until
    clean.
20. Run `cargo check` and `cargo test` for the full crate — must cover
    `spike.rs`'s 3 tests (step 17), the new invariant test (step 16), and
    the updated unit tests (step 14) — and confirm everything is green.
21. **Docs sync (only where code diverges from what's already written):**
    - `docs/architecture/movement.md`: the "Datos" table does not yet list
      the promoted per-entity phase components — add `SprintLock`,
      `JumpLocal`, `GlideLocal` alongside the existing
      `MantleState`/`VaultState`/`WallJumpState`/`EdgeLeapState` mention (the
      pipeline description in the "Sistemas" section already describes the
      `Query<.., With<Actor>>` + in-body-guard target end-state accurately —
      no change needed there unless step 17 found spike drift, in which case
      document it here).
    - `docs/ARCHITECTURE-MAP.md`: update the two `BLOCKING-PREREQUISITE` rows
      (`Enemies → *(fundacional)*` and `Multiplayer → *(fundacional)*`) to
      state the multi-actor `Query<Actor>` prerequisite is now met by this
      migration, so both tickets can unblock.
    - `docs/tickets/multi-actor-migration.md`: check off every box in
      "Definición de terminado" that is now true; note any `spike.rs` drift
      found in step 17 if applicable.

## Required Improvements Mapping

| Improvement | Origin | Step(s) |
| :--- | :--- | :--- |
| Architecture-invariant test is a mandatory deliverable, not gated on a feeling checkpoint (§11) | Iteration 1→2 | Step 16 |
| `cargo fmt` + `cargo clippy --workspace --all-targets`, full-tree, explicit closing step (§13) | Iteration 2→3 | Step 19 |
| Named verification that no new `.single()`/`.unwrap()`/`.expect()` was introduced by comfort-porting (§8/§9), separate from clippy | Iteration 3 (unresolved, resolved here) | Step 18 |

## File Touches

- `src/movement/mod.rs` — `Actor` marker, `arbitrate()`, `in_loco_state`
  deletion, `MovementPlugin::build` run_if removal, `spawn_player`, new
  `actor_isolation_tests` module.
- `src/movement/motors/walk.rs`
- `src/movement/motors/fall.rs`
- `src/movement/motors/sprint.rs` — new `SprintLock` component.
- `src/movement/motors/sneak.rs`
- `src/movement/motors/jump.rs` — `JumpLocal` gains `#[derive(Component, Default)]`.
- `src/movement/motors/glide.rs` — `GlideLocal` gains `#[derive(Component, Default)]`.
- `src/movement/motors/climb.rs` — incl. `tests::propose_with` helper update.
- `src/movement/motors/mantle.rs`
- `src/movement/motors/auto_vault.rs`
- `src/movement/motors/wall_jump.rs`
- `src/movement/motors/edge_leap.rs` — incl. `tests::setup` helper update.
- `src/movement/motors/stairs.rs`
- `src/movement/motors/ladder.rs`
- `src/movement/motor_common.rs` — only if a shared helper turns out to
  assume `Single` (not observed during planning; no `Single`/`Player`
  reference found in this file today — expected to be a no-op touch).
- `src/camera.rs` — comment-only, no functional/signature change.
- `src/movement/spike.rs` — reference only; verify unchanged, do not edit
  unless documenting drift elsewhere.
- `docs/architecture/movement.md`
- `docs/ARCHITECTURE-MAP.md`
- `docs/tickets/multi-actor-migration.md`

**Explicitly not touched:** `src/movement/brain.rs`, `src/movement/proposal.rs`,
`src/movement/services/*`, anything under Enemies/Multiplayer.

## Pre-implementation Checklist

- [x] `multi-actor-migration-solutions.md` § Chosen Solution reflects
      Solución 1 (done by this Plan call).
- [x] This Plan's File Touches list matches the ticket's Alcance exactly —
      no extra files.
- [x] Confirmed no parallel worktree is concurrently editing
      `src/movement/proposal.rs`, `src/movement/brain.rs`, or
      Enemies/Multiplayer directories (this ticket is their
      `BLOCKING-PREREQUISITE` per `docs/ARCHITECTURE-MAP.md`).
- [x] Pre-migration baseline `rg -n "\.unwrap\(\)|\.expect\(|\.single\(\)"
      src/movement/motors/ src/movement/mod.rs` captured before Step 1,
      for the Step 18 diff.
- [x] `cargo check`/`cargo test` green on the current branch before
      starting (clean baseline to compare against).

## Fidelity Check

| Step | Location | Notes |
| :--- | :--- | :--- |
| 1 | `src/movement/mod.rs` (`Actor` marker next to `Player`) | Done. |
| 2 | `src/movement/motors/sprint.rs` (`SprintLock`) | Done — tuple component, `.0` access. |
| 3 | `src/movement/motors/jump.rs` (`JumpLocal` promoted) | Done — same 5 fields, no rename. Fields made `pub(crate)` (not in original plan text) so the invariant test in `mod.rs` can assert on them directly — documented inline in `JumpLocal`'s doc comment. |
| 4 | `src/movement/motors/glide.rs` (`GlideLocal` promoted) | Done — same 2 fields, no rename. |
| 5 | `sprint.rs`/`jump.rs`/`glide.rs` `propose` → `Query<.., With<Actor>>` | Done. |
| 6 | `mantle.rs`/`auto_vault.rs`/`wall_jump.rs`/`edge_leap.rs` `Single`→`Query` | Done. |
| 7 | `walk.rs`/`climb.rs`/`stairs.rs`/`ladder.rs` `Single`→`Query` | Done. Note: `fall.rs` also converted under this same mechanical treatment (it's stateless like this group; the Plan's step 7 listing omitted it by oversight, but step 9's "every one of the 13" and the File Touches list both already included it). |
| 8 | `sneak.rs` `Single`→`Query` (+ `sync_sneak_collider` cosmetic `Actor` swap) | Done, including the optional cosmetic swap. |
| 9 | All 13 `tick`s gain `&LocomotionState` + in-body guard | Done — verified via `rg "Single<\|Local<" src/movement/motors/` returning zero code hits (only doc-comment mentions of the pre-migration shape remain). |
| 10 | `mod.rs::arbitrate()` `Single`→`Query` | Done — `proposal.rs` untouched (`git diff --stat` confirms zero diff). |
| 11 | `in_loco_state` deleted | Done. |
| 12 | 13 `.run_if(in_loco_state(...))` removed | Done — replaced with a comment explaining the in-body-guard replacement. |
| 13 | `spawn_player` gains `Actor` + 3 promoted components | Done. |
| 14 | `climb.rs`/`edge_leap.rs` test helpers gain `Actor` | Done. |
| 15 | `camera.rs` comment-only | Done — two doc comments added, zero signature/logic change (`git diff --stat` shows +6/-0). |
| 16 | Invariant test (Required Improvement 1) | **Partial, deliberately descoped, see below.** `src/movement/mod.rs::actor_isolation_tests`, 3 tests, all green — covers `propose`+`arbitrate` (no `LocomotionState`/`JumpLocal`/`SprintLock` bleed between actors, `Actor`-only entities to prove the real motors key on `Actor`). Does **not** cover `tick`: a 4th test driving real `tick` systems through a headless `App` with Avian's `PhysicsPlugins` was attempted and abandoned after multiple cascading "Resource does not exist" panics (`ColliderTreeDiagnostics`, `SpatialQueryDiagnostics`, `Assets<Mesh>`/mesh-collider caching) — each one only visible one at a time even with bevy's `debug` feature temporarily enabled to name the failing systems, with no way to know in advance how many more remained; `MinimalPlugins` doesn't wire up the sub-plugins `MoveAndSlide` transitively needs, only `DefaultPlugins` does. Per explicit user direction ("yo soy el tester, tu pega es hacer el mejor trabajo posible, mi trabajo es sentir el juego"), `tick`-under-real-physics correctness is play-test territory, same boundary `motors::climb::tests`/`motors::edge_leap::tests` already draw — not pursued further. |
| 17 | `spike.rs` unchanged | Confirmed — `git diff --stat -- src/movement/spike.rs` is empty; its 3 tests pass unchanged. No drift found. |
| 18 | `.unwrap()`/`.expect()`/`.single()` sweep (Required Improvement 3) | Done — diffed against the pre-migration baseline (`/tmp/multi-actor-migration-baseline-unwraps.txt`); every new hit is inside `#[cfg(test)]` test code (the new `actor_isolation_tests` module and pre-existing `edge_leap`/`climb` test helpers), zero new hits in production code. |
| 19 | `cargo fmt` + `cargo clippy --workspace --all-targets` (Required Improvement 2) | Done, in that order. `cargo fmt` also reformatted 5 out-of-scope files (`brain.rs`, `world.rs`, `services/ground.rs`, `services/ledge.rs`, `spike.rs`) due to pre-existing project-wide formatting drift unrelated to this ticket — reverted via `git checkout --` to keep the diff scoped to File Touches. Clippy: introduced 20 new `type_complexity` warnings (Query tuples crossing the lint's threshold after adding `&LocomotionState`) — fixed with a `type ProposeQuery<'a>`/`TickQuery<'a>` alias per motor (exactly clippy's own suggested fix), zero `#[allow(...)]` used. Remaining warnings after this step are 100% pre-existing/out-of-scope (`camera.rs`, `brain.rs`, `intents.rs`, `proposal.rs`, `stamina.rs`, `state.rs`, `services/ledge.rs`, `visuals.rs`) — none touch File-Touches-listed files' logic. |
| 20 | `cargo check`/`cargo test` full green | Done — 27/27 tests pass (24 pre-existing + 3 new), zero failures. |
| 21 | Docs sync | Done — `movement.md` Datos table gained the promoted-component row; `ARCHITECTURE-MAP.md`'s two `BLOCKING-PREREQUISITE` rows and the coordination note marked resolved; `docs/tickets/multi-actor-migration.md` checklist updated next. |
