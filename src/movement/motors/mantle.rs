//! Mantle motor — kinematic pull-up over a ledge.
//!
//! Its phase machine is shared between `propose` (sticky check) and `tick`
//! (the run), so it must be a **component** (`MantleState`) — `Local` can't be
//! shared across two systems. Movement is a position lerp
//! (`motor_common::KinematicArc`, shared with AutoVault), not velocity, so
//! `tick` writes `Transform` directly.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::LedgeTraversal;
use crate::movement::facts::{BodyContact, LedgeFacts};
use crate::movement::intents::{ClimbVerticalIntent, Intents, TraversalActionIntent};
use crate::movement::motor_common::{KinematicArc, body_move_and_slide};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const PRIORITY_WEIGHT: u32 = 10;
const MIN_SPEED: f32 = 0.01;
const MIN_DURATION: f32 = 0.08;
const TALL_ENOUGH_LIP: f32 = 1.2;

/// Shared phase state for the mantle.
#[derive(Component, Default)]
pub struct MantleState {
    pub(crate) arc: KinematicArc,
    needs_release: bool,
}

type ProposeQuery<'a> = (
    &'a Intents,
    &'a LocomotionState,
    &'a LedgeFacts,
    &'a mut MantleState,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, (With<Actor>, With<LedgeTraversal>)>) {
    for (intents, current, ledge, mut state, mut buffer) in &mut q {
        if intents.traversal != TraversalActionIntent::Mantle {
            state.needs_release = false;
        }

        // Sticky: once running, keep MANTLE until tick() finishes.
        if *current == LocomotionState::Mantle && state.arc.running {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Mantle,
                Priority::Forced,
                PRIORITY_WEIGHT,
                "mantle",
            ));
            continue;
        }

        if state.needs_release {
            continue;
        }

        let from_climb = *current == LocomotionState::Climb;
        let from_walljump = *current == LocomotionState::WallJump;
        let from_ladder = *current == LocomotionState::Ladder;
        if !(from_climb || from_walljump || from_ladder) {
            continue;
        }

        if ledge.is_at_mantle_edge && ledge.lip_height >= TALL_ENOUGH_LIP {
            let requesting = intents.traversal == TraversalActionIntent::Mantle;
            let jump_at_lip =
                (from_climb || from_ladder) && (intents.jump.held || intents.jump.pressed);
            if requesting
                || jump_at_lip
                || (from_walljump && intents.climb.vertical == ClimbVerticalIntent::Up)
            {
                let _ = buffer.push(TransitionProposal::new(
                    LocomotionState::Mantle,
                    Priority::Forced,
                    PRIORITY_WEIGHT,
                    "mantle",
                ));
            }
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a mut BodyContact,
    &'a mut MantleState,
    &'a LedgeFacts,
    &'a LedgeTraversal,
    &'a LocomotionState,
);

pub fn tick(
    mut q: Query<TickQuery, (With<Actor>, With<LedgeTraversal>)>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (
        entity,
        collider,
        mut transform,
        mut vel,
        mut contact,
        mut state,
        ledge,
        movement,
        loco_state,
    ) in &mut q
    {
        if *loco_state != LocomotionState::Mantle {
            continue;
        }

        // First active frame: begin the mantle. No valid target — hold still
        // for this frame; mantle will drop next frame.
        if state.arc.running || begin_mantle(&mut state, transform.translation, ledge, movement) {
            transform.translation = state.arc.step(dt, movement.mantle.arc_height);
        }

        vel.0 = Vec3::ZERO;
        body_move_and_slide(
            &mas,
            entity,
            collider,
            &mut transform,
            Vec3::ZERO,
            time.delta(),
            &mut contact,
        );
    }
}

fn begin_mantle(
    state: &mut MantleState,
    pos: Vec3,
    ledge: &LedgeFacts,
    movement: &LedgeTraversal,
) -> bool {
    let Some(target) = ledge.mantle_target_position else {
        return false;
    };
    state.needs_release = true;

    let vertical = (target.y - pos.y).abs();
    let horizontal = Vec2::new(pos.x, pos.z).distance(Vec2::new(target.x, target.z));
    let v_dur = vertical / movement.mantle.vertical_speed.max(MIN_SPEED);
    let h_dur = horizontal / movement.mantle.forward_speed.max(MIN_SPEED);
    state
        .arc
        .begin(pos, target, v_dur.max(h_dur).max(MIN_DURATION));
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn ladder_mantle_requires_explicit_request() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                LedgeTraversal::PLAYER,
                crate::movement::abilities::WallJumpMovement::PLAYER,
                Intents {
                    traversal: crate::movement::intents::TraversalActionIntent::Mantle,
                    ..default()
                },
                LocomotionState::Ladder,
                LedgeFacts {
                    is_at_mantle_edge: true,
                    lip_height: TALL_ENOUGH_LIP,
                    mantle_target_position: Some(Vec3::new(0.0, 3.0, 0.0)),
                    ..default()
                },
                MantleState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        let proposals = world.entity(entity).get::<ProposalBuffer>().unwrap();
        assert!(
            proposals
                .iter()
                .any(|proposal| proposal.target_state == LocomotionState::Mantle)
        );
    }

    #[test]
    fn climbing_up_a_ladder_does_not_auto_mantle() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                LedgeTraversal::PLAYER,
                crate::movement::abilities::WallJumpMovement::PLAYER,
                Intents {
                    climb: crate::movement::intents::ClimbIntent {
                        vertical: ClimbVerticalIntent::Up,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Ladder,
                LedgeFacts {
                    is_at_mantle_edge: true,
                    lip_height: TALL_ENOUGH_LIP,
                    mantle_target_position: Some(Vec3::new(0.0, 3.0, 0.0)),
                    ..default()
                },
                MantleState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(entity)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .all(|proposal| proposal.target_state != LocomotionState::Mantle)
        );
    }

    #[test]
    fn jump_at_a_climb_lip_proposes_mantle() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                LedgeTraversal::PLAYER,
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        pressed: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Climb,
                crate::movement::abilities::WallJumpMovement::PLAYER,
                LedgeFacts {
                    is_at_mantle_edge: true,
                    lip_height: TALL_ENOUGH_LIP,
                    mantle_target_position: Some(Vec3::new(0.0, 3.0, 0.0)),
                    ..default()
                },
                MantleState::default(),
                crate::movement::motors::wall_jump::WallJumpState::default(),
                crate::movement::stamina::Stamina::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();
        world
            .run_system_once(crate::movement::motors::wall_jump::propose)
            .unwrap();

        let proposals = world.entity(entity).get::<ProposalBuffer>().unwrap();
        assert!(
            proposals
                .iter()
                .any(|proposal| proposal.target_state == LocomotionState::WallJump)
        );
        assert_eq!(
            proposals.arbitrate(LocomotionState::Climb),
            LocomotionState::Mantle
        );
    }

    #[test]
    fn jump_at_a_ladder_lip_proposes_mantle_over_wall_jump() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                LedgeTraversal::PLAYER,
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        pressed: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Ladder,
                crate::movement::abilities::WallJumpMovement::PLAYER,
                LedgeFacts {
                    is_at_mantle_edge: true,
                    lip_height: TALL_ENOUGH_LIP,
                    mantle_target_position: Some(Vec3::new(0.0, 3.0, 0.0)),
                    ..default()
                },
                MantleState::default(),
                crate::movement::motors::wall_jump::WallJumpState::default(),
                crate::movement::stamina::Stamina::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();
        world
            .run_system_once(crate::movement::motors::wall_jump::propose)
            .unwrap();

        assert_eq!(
            world
                .entity(entity)
                .get::<ProposalBuffer>()
                .unwrap()
                .arbitrate(LocomotionState::Ladder),
            LocomotionState::Mantle
        );
    }

    #[test]
    fn actor_without_ledge_traversal_cannot_propose_mantle() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                Intents {
                    traversal: crate::movement::intents::TraversalActionIntent::Mantle,
                    ..default()
                },
                LocomotionState::Climb,
                LedgeFacts {
                    is_at_mantle_edge: true,
                    lip_height: TALL_ENOUGH_LIP,
                    ..default()
                },
                MantleState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

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
