//! Movement plugin — the Broker pipeline.
//!
//! Per-frame flow, expressed as ordered system sets in `FixedUpdate` (pinned to
//! 60 Hz): read intents → assign sensing LOD → sense world → gather proposals →
//! arbitrate → tick active motor. The tick phase chains capability-specific
//! systems whose exact queries keep optional data out of the actor core; each
//! system gates on its owned `LocomotionState`, so exactly one moves each body.
//! This is the per-entity contract that lets multiple `Actor`s run independently. See
//! `docs/ARCHITECTURE.md`.

use bevy::prelude::*;

pub mod abilities;
pub mod attachment;
pub(crate) mod attachment_systems;
pub mod body;
pub mod brain;
pub mod bundles;
pub mod constraints;
pub mod control;
pub mod diag;
pub mod facts;
pub mod intents;
pub mod link;
pub mod lod;
pub mod motor_common;
pub mod motors;
pub mod probe;
pub mod probe_data;
pub mod proposal;
pub mod sensing;
pub mod services;
pub mod stamina;
pub mod state;

// SPIKE (throwaway, test-only): multi-actor dispatch proof. See spike.rs header.
#[cfg(test)]
mod spike;

use proposal::ProposalBuffer;
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
    ApplyExternal,
    ReadIntents,
    ControlRedirect,
    SenseWorld,
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
    SyncAttachments,
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
        // Sensing LOD: decide, per actor, whether SenseWorld casts this tick.
        app.init_resource::<lod::SensingLodConfig>();
        app.add_systems(
            FixedUpdate,
            lod::assign_sensing_lod
                .after(MovementSet::ReadIntents)
                .before(MovementSet::SenseWorld),
        );
        // Constraints and impulses requested by other systems (Combat),
        // applied right before motors propose/tick.
        app.add_message::<constraints::LocomotionConstraintMessage>();
        app.add_message::<constraints::BodyImpulseMessage>();
        app.add_message::<link::ActorLinkRequestMessage>();
        app.add_message::<link::ActorLinkResultMessage>();
        app.init_resource::<link::ActorLinkWorkspace>();
        app.add_systems(
            FixedUpdate,
            (
                constraints::apply_locomotion_constraints,
                constraints::apply_body_impulses,
            )
                .after(MovementSet::SenseWorld)
                .before(MovementSet::GatherProposals),
        );

        app.configure_sets(
            FixedUpdate,
            (
                MovementSet::ApplyExternal,
                MovementSet::ReadIntents,
                MovementSet::ControlRedirect,
                MovementSet::SenseWorld,
                MovementSet::GatherProposals,
                MovementSet::Arbitrate,
                MovementSet::TickActiveMotor,
                MovementSet::SyncAttachments,
            )
                .chain(),
        );

        app.add_message::<probe_data::ProbeToggleRequest>();
        app.add_systems(Update, probe::toggle_spawn);

        app.add_systems(
            FixedUpdate,
            (probe::drive_intents, brain::read_intents)
                .chain()
                .in_set(MovementSet::ReadIntents),
        );
        app.add_systems(
            FixedUpdate,
            (
                attachment_systems::apply_actor_link_requests,
                attachment_systems::recover_orphaned_attachments,
                attachment_systems::recover_pending_safe_poses,
            )
                .chain()
                .in_set(MovementSet::ApplyExternal),
        );
        app.add_systems(
            FixedUpdate,
            attachment_systems::redirect_controls.in_set(MovementSet::ControlRedirect),
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
        app.add_systems(
            FixedUpdate,
            motors::jump::pay_accepted_cost
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );
        // Clear climb intent on the relevant transitions, right after the SSoT
        // write and before any motor ticks on it.
        app.add_systems(
            FixedUpdate,
            brain::reset_climb_toggle
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );

        // Tick phase: exact capability queries chained in state order. Each
        // body has one active state and therefore one moving system.
        app.add_systems(
            FixedUpdate,
            (
                motors::stairs::clear_inactive_cache,
                motors::walk::tick_body,
                motors::sprint::tick_body,
                motors::fall::tick_body,
                motors::jump::tick_body,
                motors::auto_vault::tick_body,
                motors::climb::tick_body,
                motors::mantle::tick_body,
                motors::stairs::tick_body,
                motors::ladder::tick_body,
                motors::glide::tick_body,
                motors::sneak::tick_body,
                motors::wall_jump::tick_body,
                motors::edge_leap::tick_body,
            )
                .chain()
                .in_set(MovementSet::TickActiveMotor),
        );
        app.add_systems(
            FixedUpdate,
            attachment_systems::sync_attachments.in_set(MovementSet::SyncAttachments),
        );

        // Declarative crouch-capsule swap (orthogonal to the active state, so it
        // works in Sneak and on Stairs). Runs in FixedUpdate right after the SSoT
        // write so the active motor ticks with the correct capsule this same frame
        // (physics never sees a stale collider).
        app.add_systems(
            FixedUpdate,
            motors::sneak::sync_crouch_collider
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );
    }
}

#[cfg(test)]
mod control_tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn redirect_transfers_only_supported_controls_and_neutralizes_controller() {
        let mut world = World::new();
        let controller = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                control::ControlRedirect {
                    controlled: Entity::PLACEHOLDER,
                    mask: control::ControlMask::MOUNT,
                },
                intents::Intents {
                    planar: intents::PlanarMoveIntent {
                        direction: Vec2::X,
                        strength: 1.0,
                    },
                    wants_sneak: true,
                    jump: intents::JumpIntent {
                        held: true,
                        pressed: true,
                    },
                    climb: intents::ClimbIntent {
                        requested: true,
                        ..default()
                    },
                    ..default()
                },
            ))
            .id();
        let controlled = world.spawn((Actor, intents::Intents::default())).id();
        world
            .entity_mut(controller)
            .get_mut::<control::ControlRedirect>()
            .unwrap()
            .controlled = controlled;
        let unrelated = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                intents::Intents {
                    wants_sprint: true,
                    ..default()
                },
            ))
            .id();
        world
            .run_system_once(attachment_systems::redirect_controls)
            .unwrap();

        let target = world.entity(controlled).get::<intents::Intents>().unwrap();
        assert_eq!(target.planar.direction, Vec2::X);
        assert!(!target.wants_sneak);
        assert!(target.jump.held);
        assert!(!target.climb.requested);
        assert_eq!(
            world
                .entity(controller)
                .get::<intents::Intents>()
                .unwrap()
                .planar
                .direction,
            Vec2::ZERO
        );
        assert!(
            world
                .entity(unrelated)
                .get::<intents::Intents>()
                .unwrap()
                .wants_sprint
        );
    }
}

/// `Arbitrate`: pick the winning proposal, write the SSoT `LocomotionState`, then
/// clear the buffer for next frame. This is the *only* writer of
/// `LocomotionState` (see `docs/ARCHITECTURE.md`).
type ArbitrationQuery<'a> = (&'a mut LocomotionState, &'a mut ProposalBuffer);

fn arbitrate(mut q: Query<ArbitrationQuery, attachment::LocomotionActorFilter>) {
    for (mut state, mut buffer) in &mut q {
        let winner = buffer.arbitrate(*state);
        if *state != winner {
            *state = winner;
            // (Activated/Deactivated events land with the motors that need them.)
        }
        buffer.clear();
    }
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
    use crate::movement::abilities::{AirborneMovement, GroundMovement, JumpMovement};
    use crate::movement::facts::{GroundFacts, LedgeFacts, StairsFacts};
    use crate::movement::intents::Intents;
    use crate::movement::motors::jump::{JumpLocal, JumpPhase};
    use crate::movement::motors::sneak::{Crouched, StandClearance};
    use crate::movement::motors::sprint::SprintLock;
    use crate::movement::stamina::Stamina;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn two_actors_resolve_independently_via_real_propose_and_arbitrate() {
        let mut world = World::new();
        let walker = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundMovement::PLAYER,
                JumpMovement::PLAYER,
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
                crate::movement::attachment::LocomotionEnabled,
                AirborneMovement::PLAYER,
                JumpMovement::PLAYER,
                GroundMovement::PLAYER,
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
                crate::movement::attachment::LocomotionEnabled,
                GroundMovement::PLAYER,
                JumpMovement::PLAYER,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        held: true,
                        ..default()
                    },
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
                crate::movement::attachment::LocomotionEnabled,
                JumpMovement::PLAYER,
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
                crate::movement::attachment::LocomotionEnabled,
                crate::movement::abilities::SprintMovement::PLAYER,
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
                Crouched::default(),
                StandClearance::default(),
                SprintLock::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let full = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                crate::movement::abilities::SprintMovement::PLAYER,
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
                Crouched::default(),
                StandClearance::default(),
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
                    crate::movement::attachment::LocomotionEnabled,
                    GroundMovement::PLAYER,
                    JumpMovement::PLAYER,
                    GroundFacts {
                        grounded: true,
                        ..default()
                    },
                    StairsFacts {
                        on_stairs: true,
                        ..default()
                    },
                    Intents {
                        jump: crate::movement::intents::JumpIntent {
                            held: true,
                            ..default()
                        },
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

    #[test]
    fn edge_leap_beats_mantle_regardless_of_propose_order() {
        // From Climb at a corner (mantle edge above, open lateral edge beside),
        // jump + lateral input makes both motors propose Forced. EdgeLeap's
        // heavier weight must win either way: the deliberate lateral input
        // outranks Mantle's bare jump-at-lip trigger (see `proposal::weight`).
        for edge_leap_first in [true, false] {
            let mut world = World::new();
            let e = world
                .spawn((
                    Actor,
                    crate::movement::attachment::LocomotionEnabled,
                    abilities::LedgeTraversal::PLAYER,
                    abilities::WallJumpMovement::PLAYER,
                    Intents {
                        jump: crate::movement::intents::JumpIntent {
                            pressed: true,
                            held: true,
                        },
                        climb: crate::movement::intents::ClimbIntent {
                            lateral: crate::movement::intents::ClimbLateralIntent::Left,
                            ..default()
                        },
                        ..default()
                    },
                    LocomotionState::Climb,
                    LedgeFacts {
                        is_at_mantle_edge: true,
                        lip_height: 1.5,
                        has_wall_left: false,
                        mantle_target_position: Some(Vec3::new(0.0, 3.0, 0.0)),
                        ..default()
                    },
                    Stamina::default(),
                    motors::mantle::MantleState::default(),
                    motors::edge_leap::EdgeLeapState::default(),
                    ProposalBuffer::default(),
                ))
                .id();

            if edge_leap_first {
                world.run_system_once(motors::edge_leap::propose).unwrap();
                world.run_system_once(motors::mantle::propose).unwrap();
            } else {
                world.run_system_once(motors::mantle::propose).unwrap();
                world.run_system_once(motors::edge_leap::propose).unwrap();
            }
            world.run_system_once(arbitrate).unwrap();

            assert_eq!(
                *world.entity(e).get::<LocomotionState>().unwrap(),
                LocomotionState::EdgeLeap,
                "edge_leap_first = {edge_leap_first}"
            );
        }
    }

    #[test]
    fn air_and_stairs_motors_require_their_capabilities() {
        let mut world = World::new();
        world.init_resource::<Time>();

        let jumper = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        held: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Walk,
                JumpPhase::default(),
                JumpLocal::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let glider = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundFacts::default(),
                LedgeFacts::default(),
                Intents {
                    glide: crate::movement::intents::GlideIntent::Requested,
                    ..default()
                },
                LocomotionState::Fall,
                motors::glide::GlideLocal::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let stair_walker = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                StairsFacts {
                    on_stairs: true,
                    ..default()
                },
                LocomotionState::Walk,
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(motors::jump::propose).unwrap();
        world.run_system_once(motors::glide::propose).unwrap();
        world.run_system_once(motors::stairs::propose).unwrap();

        for entity in [jumper, glider, stair_walker] {
            assert!(
                world
                    .entity(entity)
                    .get::<ProposalBuffer>()
                    .unwrap()
                    .iter()
                    .next()
                    .is_none()
            );
        }
    }

    #[test]
    fn fall_motor_requires_an_airborne_profile() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                GroundFacts::default(),
                LocomotionState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(motors::fall::propose).unwrap();

        assert!(
            world
                .entity(entity)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none(),
            "an actor without AirborneMovement must not propose Fall"
        );
    }
}
