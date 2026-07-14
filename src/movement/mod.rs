//! Movement plugin — the Broker pipeline.
//!
//! Per-frame flow, expressed as ordered system sets in `FixedUpdate` (pinned to
//! 60 Hz): read intents → sense world → gather proposals → arbitrate → tick
//! active motor. Only the active motor's `tick` system runs each frame, gated
//! by a run condition on `LocomotionState` — this is what enforces "exactly
//! one motor moves the body" structurally, not just by convention. See
//! `docs/architecture/movement.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

pub mod body;
pub mod brain;
pub mod diag;
pub mod facts;
pub mod intents;
pub mod motor_common;
pub mod motors;
pub mod proposal;
pub mod services;
pub mod stamina;
pub mod state;

// SPIKE (throwaway, test-only): multi-actor dispatch proof. See spike.rs header.
#[cfg(test)]
mod spike;

use facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use intents::Intents;
use proposal::ProposalBuffer;
use stamina::Stamina;
use state::LocomotionState;

/// World gravity magnitude (Earth gravity, 9.8 m/s²).
pub const GRAVITY: f32 = 9.8;

/// Marker for the player entity.
#[derive(Component)]
pub struct Player;

/// Generic marker for any movement-capable entity (local player, remote
/// player, AI-controlled actor). Motors dispatch on `Actor`, not `Player` —
/// `Player` narrows to "the local player" for systems (e.g. the camera) that
/// intentionally stay scoped to it.
#[derive(Component)]
pub struct Actor;

/// Our kinematic body velocity — the analog of `CharacterBody3D.velocity`.
/// Kept separate from Avian's `LinearVelocity`: we integrate position ourselves
/// through `move_and_slide`, so the physics engine must not also move us.
#[derive(Component, Default)]
pub struct BodyVelocity(pub Vec3);

/// Ordered phases of the Broker pipeline within `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MovementSet {
    ReadIntents,
    SenseWorld,
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // Pinned to 60 Hz (Bevy defaults to 64 Hz).
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        // Sensor-cast capture for the debug gizmos (no-op until enabled).
        app.init_resource::<diag::CastTrace>();
        app.add_systems(
            FixedUpdate,
            diag::clear_cast_trace
                .after(MovementSet::ReadIntents)
                .before(MovementSet::SenseWorld),
        );

        app.configure_sets(
            FixedUpdate,
            (
                MovementSet::ReadIntents,
                MovementSet::SenseWorld,
                MovementSet::GatherProposals,
                MovementSet::Arbitrate,
                MovementSet::TickActiveMotor,
            )
                .chain(),
        );

        app.add_systems(Startup, spawn_player);

        app.add_systems(
            FixedUpdate,
            brain::read_intents.in_set(MovementSet::ReadIntents),
        );
        app.add_systems(
            FixedUpdate,
            (
                services::ground::ground_service,
                services::ledge::ledge_service,
                services::stairs::stairs_service,
                services::ladder::ladder_service,
                motors::sneak::update_stand_clearance,
            )
                .in_set(MovementSet::SenseWorld),
        );
        app.add_systems(
            FixedUpdate,
            (
                motors::walk::propose,
                motors::fall::propose,
                motors::sprint::propose,
                motors::sneak::propose,
                motors::jump::propose,
                motors::glide::propose,
                motors::climb::propose,
                motors::mantle::propose,
                motors::auto_vault::propose,
                motors::wall_jump::propose,
                motors::edge_leap::propose,
                motors::stairs::propose,
                motors::ladder::propose,
            )
                .in_set(MovementSet::GatherProposals),
        );
        app.add_systems(FixedUpdate, arbitrate.in_set(MovementSet::Arbitrate));
        // Clear climb intent on the relevant transitions, right after the SSoT write.
        app.add_systems(
            FixedUpdate,
            brain::reset_climb_toggle.after(MovementSet::Arbitrate),
        );

        // Tick systems: all 13 register unconditionally now — each self-gates
        // on its own `LocomotionState` guard (see each motor's `tick`), the
        // per-entity replacement for the old global `run_if` gate. This is
        // what lets multiple `Actor` entities run independently.
        app.add_systems(
            FixedUpdate,
            (
                motors::walk::tick,
                motors::fall::tick,
                motors::sprint::tick,
                motors::sneak::tick,
                motors::jump::tick,
                motors::glide::tick,
                motors::climb::tick,
                motors::mantle::tick,
                motors::auto_vault::tick,
                motors::wall_jump::tick,
                motors::edge_leap::tick,
                motors::stairs::tick,
                motors::ladder::tick,
            )
                .in_set(MovementSet::TickActiveMotor),
        );

        // Declarative collider swap for sneak. Runs in FixedUpdate right after
        // the SSoT write so the newly active motor ticks with the correct
        // capsule this same frame (physics never sees a stale collider).
        app.add_systems(
            FixedUpdate,
            motors::sneak::sync_sneak_collider
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );
    }
}

/// `Arbitrate`: pick the winning proposal, write the SSoT `LocomotionState`, then
/// clear the buffer for next frame. This is the *only* writer of
/// `LocomotionState` (see `docs/architecture/movement.md`).
fn arbitrate(mut q: Query<(&mut LocomotionState, &mut ProposalBuffer), With<Actor>>) {
    for (mut state, mut buffer) in &mut q {
        let winner = buffer.arbitrate(*state);
        if *state != winner {
            *state = winner;
            // (Activated/Deactivated events land with the motors that need them.)
        }
        buffer.clear();
    }
}

fn spawn_player(mut commands: Commands) {
    // The Player is an invisible kinematic collider; the mesh lives on a separate
    // PlayerVisual entity that interpolates toward this body (see `visuals.rs`).
    // Capsule dimensions live in `body` (shared with services and motors).
    commands.spawn((
        Player,
        Actor,
        crate::input::frame::InputControlledBy(crate::input::frame::LOCAL_INPUT_SOURCE),
        crate::input::frame::ControlOrientation::default(),
        Name::new("Player"),
        Transform::from_xyz(0.0, 1.5, 0.0),
        RigidBody::Kinematic,
        Collider::capsule(body::RADIUS, body::STAND_CAPSULE_LENGTH),
        BodyVelocity::default(),
        Intents::default(),
        LocomotionState::default(),
        ProposalBuffer::default(),
        Stamina::default(),
        // Nested tuples keep us under Bevy's 15-element tuple-bundle arity limit.
        (
            BodyContact::default(),
            GroundFacts::default(),
            LedgeFacts::default(),
            StairsFacts::default(),
            LadderFacts::default(),
        ),
        // Per-motor shared phase state (read by both propose and tick systems).
        (
            motors::jump::JumpPhase::default(),
            brain::ClimbInputState::default(),
            crate::input::InputConsumeCursor::default(),
            motors::jump::JumpLocal::default(),
            motors::glide::GlideLocal::default(),
            motors::sprint::SprintLock::default(),
            motors::sneak::Crouched::default(),
            motors::sneak::StandClearance::default(),
            motors::sneak::StandCollider(Collider::capsule(
                body::RADIUS,
                body::STAND_CAPSULE_LENGTH,
            )),
            motors::mantle::MantleState::default(),
            motors::auto_vault::VaultState::default(),
            motors::wall_jump::WallJumpState::default(),
            motors::edge_leap::EdgeLeapState::default(),
        ),
    ));
}

/// Architecture-invariant tests for the `multi-actor-migration` (Query<Actor>)
/// contract — mandatory deliverable, not gated on a *feeling* checkpoint
/// (Constitución §11 Tier-3 exception). Runs the real production `propose`
/// systems and the real (private) `arbitrate` directly on a bare `World`, no
/// Avian — same style as `motors::climb::tests`/`motors::edge_leap::tests`.
/// `tick` correctness under real physics is play-test territory (see the
/// note at the end of this module); these tests only cover the part that can
/// silently bleed across actors: `LocomotionState` and promoted per-entity
/// state (`JumpLocal`, `SprintLock`).
#[cfg(test)]
mod actor_isolation_tests {
    use super::*;
    use crate::movement::facts::GroundFacts;
    use crate::movement::motors::jump::{JumpLocal, JumpPhase};
    use crate::movement::motors::sprint::SprintLock;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn two_actors_resolve_independently_via_real_propose_and_arbitrate() {
        let mut world = World::new();
        let walker = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                LocomotionState::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let faller = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: false,
                    ..default()
                },
                LocomotionState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world
            .run_system_once(motors::walk::propose)
            .expect("walk::propose runs");
        world
            .run_system_once(motors::fall::propose)
            .expect("fall::propose runs");
        world.run_system_once(arbitrate).expect("arbitrate runs");

        assert_eq!(
            *world.entity(walker).get::<LocomotionState>().unwrap(),
            LocomotionState::Walk
        );
        assert_eq!(
            *world.entity(faller).get::<LocomotionState>().unwrap(),
            LocomotionState::Fall
        );
        assert!(
            world
                .entity(walker)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none()
        );
        assert!(
            world
                .entity(faller)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none()
        );
    }

    #[test]
    fn jump_local_state_does_not_bleed_between_actors() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let jumper = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    wants_jump: true,
                    ..default()
                },
                LocomotionState::default(),
                JumpPhase::default(),
                JumpLocal::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let neutral = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents::default(),
                LocomotionState::default(),
                JumpPhase::default(),
                JumpLocal::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world
            .run_system_once(motors::jump::propose)
            .expect("jump::propose runs");

        let jumper_proposals: Vec<_> = world
            .entity(jumper)
            .get::<ProposalBuffer>()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(jumper_proposals.len(), 1);
        assert_eq!(jumper_proposals[0].target_state, LocomotionState::Jump);
        assert!(
            world
                .entity(jumper)
                .get::<JumpLocal>()
                .unwrap()
                .needs_release
        );

        assert!(
            world
                .entity(neutral)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none(),
            "neutral actor's jump must not fire from the jumper's input"
        );
        let neutral_local = world.entity(neutral).get::<JumpLocal>().unwrap();
        assert!(
            !neutral_local.needs_release,
            "neutral actor's JumpLocal must stay untouched"
        );
        assert_eq!(
            neutral_local.buffer, 0.0,
            "neutral actor's jump buffer must stay clear"
        );
    }

    #[test]
    fn sprint_stamina_lock_is_per_actor() {
        let mut world = World::new();
        let mut exhausted_stamina = Stamina::default();
        exhausted_stamina.drain(1_000.0);

        let exhausted = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                StairsFacts::default(),
                LedgeFacts::default(),
                Intents {
                    wants_sprint: true,
                    ..default()
                },
                exhausted_stamina,
                SprintLock::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let full = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                StairsFacts::default(),
                LedgeFacts::default(),
                Intents {
                    wants_sprint: true,
                    ..default()
                },
                Stamina::default(),
                SprintLock::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world
            .run_system_once(motors::sprint::propose)
            .expect("sprint::propose runs");

        assert!(world.entity(exhausted).get::<SprintLock>().unwrap().0);
        assert!(
            world
                .entity(exhausted)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none()
        );

        assert!(!world.entity(full).get::<SprintLock>().unwrap().0);
        let full_proposals: Vec<_> = world
            .entity(full)
            .get::<ProposalBuffer>()
            .unwrap()
            .iter()
            .collect();
        assert_eq!(full_proposals.len(), 1);
        assert_eq!(full_proposals[0].target_state, LocomotionState::Sprint);
    }

    #[test]
    fn jump_beats_stairs_regardless_of_propose_order() {
        // Both used to propose (Forced, 0); the winner then depended on system
        // execution order, which Bevy does not guarantee. Jump now carries a
        // higher Forced weight, so the outcome is fixed either way.
        for jump_first in [true, false] {
            let mut world = World::new();
            world.init_resource::<Time>();
            let e = world
                .spawn((
                    Actor,
                    GroundFacts {
                        grounded: true,
                        ..default()
                    },
                    StairsFacts {
                        on_stairs: true,
                        ..default()
                    },
                    Intents {
                        wants_jump: true,
                        ..default()
                    },
                    LocomotionState::Stairs,
                    JumpPhase::default(),
                    JumpLocal::default(),
                    ProposalBuffer::default(),
                ))
                .id();

            if jump_first {
                world.run_system_once(motors::jump::propose).unwrap();
                world.run_system_once(motors::stairs::propose).unwrap();
            } else {
                world.run_system_once(motors::stairs::propose).unwrap();
                world.run_system_once(motors::jump::propose).unwrap();
            }
            world.run_system_once(arbitrate).unwrap();

            assert_eq!(
                *world.entity(e).get::<LocomotionState>().unwrap(),
                LocomotionState::Jump,
                "jump_first = {jump_first}"
            );
        }
    }

    // `tick` correctness under real physics (does a `tick`'s in-body guard
    // actually stop it from running for the wrong actor?) is NOT covered
    // here. A headless `App` with Avian's real `PhysicsPlugins` was
    // attempted and abandoned: `MoveAndSlide` pulls in several of Avian's
    // internal sub-plugins (collider-tree diagnostics, spatial-query
    // diagnostics, mesh-collider caching via `Assets<Mesh>`) that are only
    // fully wired up by `DefaultPlugins`, not `MinimalPlugins` — each missing
    // piece surfaced as its own "Resource does not exist" panic one at a
    // time, with no indication of how many more remained. This is the same
    // boundary `motors::climb::tests`/`motors::edge_leap::tests` already
    // draw ("the tick assertions need a physics world and are covered by
    // play-testing") — the user is the tester for feel/physics-in-motion,
    // not this test suite.
}
