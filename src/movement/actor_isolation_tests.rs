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
