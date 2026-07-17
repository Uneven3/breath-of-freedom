//! SPIKE → REFERENCE EXAMPLE: the movement dispatch shape, proven for N actors.
//!
//! This module is `#[cfg(test)]`-only — it compiles for `cargo test` and never
//! ships. It began as a throwaway spike and is now kept as a **minimal, runnable
//! teaching example** of the per-entity ECS pattern (a newcomer-friendly walkthrough
//! lives in `docs/ARCHITECTURE.md`). It answers one question cheaply, before
//! migrating all ~26 production systems: **does per-entity `Query` dispatch + an
//! in-body state guard + per-entity component state run two actors independently?**
//! (Answer: yes — see the three tests below. Run with `cargo test spike`.)
//!
//! It reimplements a *minimal* slice of the stack — walk / jump / fall — using the
//! real public types (`LocomotionState`, `Intents`, `ProposalBuffer`,
//! `TransitionProposal`, `Priority`, `BodyVelocity`) but trivial transform
//! integration instead of Avian's `move_and_slide` (physics already works; the
//! open question is dispatch shape). The three production single-actor mechanisms
//! are each replaced with their per-entity equivalent:
//!
//! 1. `Single<…, With<Player>>`     → `Query<…, With<Actor>>` iterating all actors.
//! 2. global `in_loco_state` run-if → in-body `if *state != … { continue }` guard.
//! 3. `Local<JumpLocal>` (per-system, shared!) → a per-entity `JumpState` component.

use std::time::Duration;

use bevy::prelude::*;

use super::BodyVelocity;
use super::intents::Intents;
use super::proposal::{Priority, ProposalBuffer, TransitionProposal};
use super::state::LocomotionState;

// --- constants (mirroring the real motors, where relevant) ---
const WALK_SPEED: f32 = 5.0; // jump.rs analog uses move_toward; spike sets velocity directly.
const JUMP_IMPULSE: f32 = 5.5; // jump.rs:16
const COYOTE_TIME: f32 = 0.12; // jump.rs:17
const JUMP_BUFFER_TIME: f32 = 0.12; // jump.rs:18
const GRAVITY: f32 = 9.8; // mod.rs:34

// --- spike-local components ---

/// Marks a movement-capable entity. In the real migration this generalises
/// `Player` (the player would be `Actor` + `Player`).
#[derive(Component)]
struct Actor;

/// Stand-in for `GroundFacts.grounded`, set directly by the test instead of a
/// service (the spike avoids Avian on purpose).
#[derive(Component)]
struct Grounded(bool);

/// Per-entity replacement for jump.rs's `Local<JumpLocal>`. The whole point: this
/// lives on the entity, so two actors cannot clobber each other's jump timers.
#[derive(Component, Default)]
struct JumpState {
    coyote: f32,
    buffer: f32,
    was_on_floor: bool,
    prev_wants: bool,
    needs_release: bool,
}

/// Instrumentation: each `tick` bumps this for the actor it moves, so a test can
/// assert "exactly one motor ticked this actor this frame".
#[derive(Component, Default)]
struct TickCount(u32);

/// Ordered phases — the spike's analog of `MovementSet` (minus SenseWorld; the
/// test sets `Grounded` by hand).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
enum SpikeSet {
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
}

struct SpikePlugin;

impl Plugin for SpikePlugin {
    fn build(&self, app: &mut App) {
        // The systems read `Res<Time>`; the test drives it with `advance_by`.
        app.init_resource::<Time>();
        app.configure_sets(
            FixedUpdate,
            (
                SpikeSet::GatherProposals,
                SpikeSet::Arbitrate,
                SpikeSet::TickActiveMotor,
            )
                .chain(),
        );
        app.add_systems(
            FixedUpdate,
            (walk_propose, fall_propose, jump_propose).in_set(SpikeSet::GatherProposals),
        );
        app.add_systems(FixedUpdate, arbitrate.in_set(SpikeSet::Arbitrate));
        app.add_systems(
            FixedUpdate,
            (walk_tick, fall_tick, jump_tick).in_set(SpikeSet::TickActiveMotor),
        );
    }
}

// --- propose: every motor proposes for every actor, every frame ---

fn walk_propose(mut q: Query<(&Grounded, &mut ProposalBuffer), With<Actor>>) {
    for (ground, mut buffer) in &mut q {
        if ground.0 {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Walk,
                Priority::PlayerRequested,
                0,
                "walk",
            ));
        }
    }
}

fn fall_propose(mut q: Query<&mut ProposalBuffer, With<Actor>>) {
    // Fall is the floor (Default priority): it wins only when nothing else proposes,
    // mirroring `ProposalBuffer::arbitrate`'s empty-buffer default.
    for mut buffer in &mut q {
        let _ = buffer.push(TransitionProposal::new(
            LocomotionState::Fall,
            Priority::Default,
            0,
            "fall",
        ));
    }
}

/// Faithful port of `jump.rs:30-74`, but keyed on the per-entity `JumpState`.
fn jump_propose(
    time: Res<Time>,
    mut q: Query<
        (
            &Grounded,
            &Intents,
            &LocomotionState,
            &mut JumpState,
            &mut ProposalBuffer,
        ),
        With<Actor>,
    >,
) {
    let dt = time.delta_secs();
    for (ground, intents, current, mut s, mut buffer) in &mut q {
        let on_floor = ground.0;

        if !intents.jump.held {
            s.needs_release = false;
        }

        if s.was_on_floor && !on_floor && *current != LocomotionState::Jump {
            s.coyote = COYOTE_TIME;
        } else if !on_floor {
            s.coyote = (s.coyote - dt).max(0.0);
        }
        s.was_on_floor = on_floor;

        if intents.jump.held && !s.prev_wants {
            s.buffer = JUMP_BUFFER_TIME;
        } else if s.buffer > 0.0 {
            s.buffer = (s.buffer - dt).max(0.0);
        }
        s.prev_wants = intents.jump.held;

        let can_jump = on_floor || s.coyote > 0.0;
        let wants = (intents.jump.held || s.buffer > 0.0) && !s.needs_release;

        if can_jump && wants {
            s.coyote = 0.0;
            s.buffer = 0.0;
            s.needs_release = true;
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Jump,
                Priority::Forced,
                0,
                "jump",
            ));
        }
    }
}

// --- arbitrate: per-entity, no `Single` ---

fn arbitrate(mut q: Query<(&mut LocomotionState, &mut ProposalBuffer), With<Actor>>) {
    for (mut state, mut buffer) in &mut q {
        let winner = buffer.arbitrate(*state);
        if *state != winner {
            *state = winner;
        }
        buffer.clear();
    }
}

// --- tick: one system per motor, each self-gates on the actor's state ---
// This is the structural replacement for the global `in_loco_state` run condition.

fn walk_tick(
    time: Res<Time>,
    mut q: Query<
        (
            &LocomotionState,
            &Intents,
            &mut BodyVelocity,
            &mut Transform,
            &mut TickCount,
        ),
        With<Actor>,
    >,
) {
    let dt = time.delta_secs();
    for (state, intents, mut vel, mut transform, mut ticks) in &mut q {
        if *state != LocomotionState::Walk {
            continue;
        }
        vel.0.x = intents.planar.direction.x * WALK_SPEED;
        vel.0.z = intents.planar.direction.y * WALK_SPEED;
        vel.0.y = 0.0;
        transform.translation += vel.0 * dt;
        ticks.0 += 1;
    }
}

fn fall_tick(
    time: Res<Time>,
    mut q: Query<
        (
            &LocomotionState,
            &mut BodyVelocity,
            &mut Transform,
            &mut TickCount,
        ),
        With<Actor>,
    >,
) {
    let dt = time.delta_secs();
    for (state, mut vel, mut transform, mut ticks) in &mut q {
        if *state != LocomotionState::Fall {
            continue;
        }
        vel.0.y -= GRAVITY * dt;
        transform.translation += vel.0 * dt;
        ticks.0 += 1;
    }
}

fn jump_tick(
    time: Res<Time>,
    mut q: Query<
        (
            &LocomotionState,
            &mut BodyVelocity,
            &mut Transform,
            &mut TickCount,
        ),
        With<Actor>,
    >,
) {
    let dt = time.delta_secs();
    for (state, mut vel, mut transform, mut ticks) in &mut q {
        if *state != LocomotionState::Jump {
            continue;
        }
        vel.0.y = JUMP_IMPULSE; // impulse, matching jump.rs:97
        transform.translation += vel.0 * dt;
        ticks.0 += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    /// Build a headless app with the spike plugin. No `DefaultPlugins`, no Avian —
    /// we drive `Time` and the `FixedUpdate` schedule by hand for determinism.
    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(SpikePlugin);
        app
    }

    /// Advance the manual clock and run the pipeline `n` fixed steps.
    fn step(app: &mut App, n: usize) {
        for _ in 0..n {
            app.world_mut()
                .resource_mut::<Time>()
                .advance_by(Duration::from_secs_f32(DT));
            app.world_mut().run_schedule(FixedUpdate);
        }
    }

    fn spawn_actor(app: &mut App, grounded: bool, intents: Intents) -> Entity {
        app.world_mut()
            .spawn((
                Actor,
                Grounded(grounded),
                intents,
                LocomotionState::default(), // Fall
                BodyVelocity::default(),
                ProposalBuffer::default(),
                JumpState::default(),
                TickCount::default(),
                Transform::default(),
            ))
            .id()
    }

    fn state(app: &App, e: Entity) -> LocomotionState {
        *app.world().get::<LocomotionState>(e).unwrap()
    }
    fn pos(app: &App, e: Entity) -> Vec3 {
        app.world().get::<Transform>(e).unwrap().translation
    }

    /// 1. Two actors dispatch independently: a grounded actor walks (and moves along
    ///    its own intent) while an airborne actor falls — neither moved by the other's
    ///    motor. Proves `Single`→`Query` + the in-body state guard.
    #[test]
    fn two_actors_dispatch_independently() {
        let mut app = app();
        let walker = spawn_actor(
            &mut app,
            true,
            Intents {
                planar: crate::movement::intents::PlanarMoveIntent {
                    direction: Vec2::new(1.0, 0.0),
                    strength: 1.0,
                },
                ..default()
            },
        );
        let faller = spawn_actor(&mut app, false, Intents::default());

        step(&mut app, 10);

        // Walker: resolved to Walk, slid +X, stayed level.
        assert_eq!(state(&app, walker), LocomotionState::Walk);
        assert!(pos(&app, walker).x > 0.1, "walker should move +X");
        assert!(pos(&app, walker).y.abs() < 1e-4, "walker stays level");

        // Faller: resolved to Fall, dropped −Y, no lateral drift.
        assert_eq!(state(&app, faller), LocomotionState::Fall);
        assert!(pos(&app, faller).y < -0.1, "faller should drop");
        assert!(
            pos(&app, faller).x.abs() < 1e-4,
            "faller has no lateral motion"
        );
    }

    /// 2. Exactly one motor ticks each actor per frame, even though all three tick
    ///    systems run (no run condition). Proves P2 survives the global→per-entity move.
    #[test]
    fn exactly_one_motor_ticks_each_actor_per_frame() {
        let mut app = app();
        let walker = spawn_actor(&mut app, true, Intents::default());
        let faller = spawn_actor(&mut app, false, Intents::default());

        let n = 5;
        step(&mut app, n);

        assert_eq!(app.world().get::<TickCount>(walker).unwrap().0, n as u32);
        assert_eq!(app.world().get::<TickCount>(faller).unwrap().0, n as u32);
    }

    /// 3. No cross-actor state bleed: actor A jumps (its own `JumpState` arms) while
    ///    actor B holds neutral and never jumps — B's jump buffer stays untouched.
    ///    This is the assertion the production `Local<JumpLocal>` (per-system, shared)
    ///    would fail; the per-entity component is what makes it pass.
    #[test]
    fn jump_state_does_not_bleed_between_actors() {
        let mut app = app();
        let jumper = spawn_actor(
            &mut app,
            true,
            Intents {
                jump: crate::movement::intents::JumpIntent {
                    held: true,
                    ..default()
                },
                ..default()
            },
        );
        let neutral = spawn_actor(&mut app, true, Intents::default());

        step(&mut app, 1);

        // A took the jump this frame (Forced > Walk's PlayerRequested).
        assert_eq!(state(&app, jumper), LocomotionState::Jump);
        assert!(app.world().get::<JumpState>(jumper).unwrap().needs_release);

        // B is unaffected: still walking, jump buffer never armed.
        assert_eq!(state(&app, neutral), LocomotionState::Walk);
        let b = app.world().get::<JumpState>(neutral).unwrap();
        assert_eq!(b.buffer, 0.0, "neutral actor's jump buffer must stay clear");
        assert!(!b.needs_release, "neutral actor must not be jump-locked");
    }
}
