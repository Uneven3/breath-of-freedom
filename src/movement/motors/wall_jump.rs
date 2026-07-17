//! Wall-jump motor — launch off a climbed wall in a direction chosen by input.
//!
//! Timer/jumping flags shared between `propose` and `tick` via a
//! `WallJumpState` component.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::WallJumpMovement;
use crate::movement::facts::LedgeFacts;
use crate::movement::intents::{ClimbLateralIntent, ClimbVerticalIntent, Intents};
use crate::movement::motor_common::{body_move_and_slide, clip_below_ledge_lip, launch_normal};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

#[derive(Component, Default)]
pub struct WallJumpState {
    is_jumping: bool,
    timer: f32,
    needs_release: bool,
    /// Armed by `propose`, consumed by `tick`'s first active frame (the launch
    /// impulse). An explicit flag, not a `timer == JUMP_DURATION` float
    /// comparison — the timer value can't reliably identify "first tick".
    launch_pending: bool,
}

type ProposeQuery<'a> = (
    &'a Intents,
    &'a LocomotionState,
    &'a Stamina,
    &'a WallJumpMovement,
    &'a mut WallJumpState,
    &'a mut ProposalBuffer,
);

type ProposeFilter = (
    crate::movement::attachment::LocomotionActorFilter,
    With<WallJumpMovement>,
);

pub fn propose(mut q: Query<ProposeQuery, ProposeFilter>) {
    for (intents, current, stamina, movement, mut state, mut buffer) in &mut q {
        let jump_requested = intents.jump.held || intents.jump.pressed;
        if !intents.jump.held {
            state.needs_release = false;
        }

        if (*current == LocomotionState::Climb || *current == LocomotionState::Ladder)
            && jump_requested
            && !state.needs_release
            && !stamina.is_exhausted()
        {
            // Arm the jump for this activation.
            state.needs_release = true;
            state.is_jumping = true;
            state.timer = movement.wall_jump.duration;
            state.launch_pending = true;
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::WallJump,
                Priority::Forced,
                weight::WALL_JUMP,
                "wall_jump",
            ));
            continue;
        }

        if *current == LocomotionState::WallJump && state.is_jumping {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::WallJump,
                Priority::Forced,
                weight::WALL_JUMP,
                "wall_jump",
            ));
        }
    }
}

type TickQuery<'a> = (
    MotorCore,
    &'a WallJumpMovement,
    &'a LedgeFacts,
    &'a mut WallJumpState,
    Option<&'a mut Stamina>,
);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, ledge, mut state, mut stamina) in &mut actors {
        if *row.state != LocomotionState::WallJump {
            continue;
        }
        let dt = time.delta_secs();

        let mut v = row.velocity.0;

        // First tick: apply the launch impulse. Vertical intent wins over
        // lateral; a neutral stick leaps away from the wall.
        if state.launch_pending {
            state.launch_pending = false;
            let profile = movement.wall_jump;
            let normal = launch_normal(ledge.climb_normal, &row.contact, &row.transform);
            let right_dir = Vec3::Y.cross(normal).normalize_or_zero();
            let away_leap = || {
                let away = (normal + Vec3::Y * profile.away_up_blend).normalize_or_zero();
                away * profile.away_leap_speed + normal * profile.away_normal_push
            };
            let lateral_leap = |side_dir: Vec3| {
                let mut v = side_dir * (profile.jump_up_impulse * profile.lateral_speed_fraction);
                v.y = profile.lateral_vertical_lift;
                v - normal * profile.lateral_normal_retraction
            };

            v = match (row.intents.climb.vertical, row.intents.climb.lateral) {
                (ClimbVerticalIntent::Up, _) => {
                    Vec3::Y * profile.jump_up_impulse - normal * profile.wall_contact_push
                }
                (ClimbVerticalIntent::Down, _) => away_leap(),
                (ClimbVerticalIntent::Neutral, ClimbLateralIntent::Left) => {
                    lateral_leap(-right_dir)
                }
                (ClimbVerticalIntent::Neutral, ClimbLateralIntent::Right) => {
                    lateral_leap(right_dir)
                }
                (ClimbVerticalIntent::Neutral, ClimbLateralIntent::Neutral) => away_leap(),
            };
            if let Some(stamina) = stamina.as_deref_mut() {
                stamina.drain(profile.stamina_cost);
            }
        }

        state.timer -= dt;

        // Soft ceiling clip (shared with climb); pinning at the lip ends the jump.
        if clip_below_ledge_lip(&mut row.transform, &mut v, ledge.lip_height, *row.body, dt) {
            state.is_jumping = false;
        }

        row.velocity.0 = body_move_and_slide(
            &mas,
            row.entity,
            row.collider,
            &mut row.transform,
            v,
            time.delta(),
            &mut row.contact,
        );

        if state.timer <= 0.0 {
            state.is_jumping = false;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::Actor;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn neutral_jump_press_from_climb_proposes_backward_wall_jump() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                WallJumpMovement::PLAYER,
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        pressed: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Climb,
                Stamina::default(),
                WallJumpState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        let proposals = world.entity(entity).get::<ProposalBuffer>().unwrap();
        assert!(
            proposals
                .iter()
                .any(|proposal| proposal.target_state == LocomotionState::WallJump)
        );
    }

    #[test]
    fn neutral_jump_press_from_ladder_proposes_backward_wall_jump() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                WallJumpMovement::PLAYER,
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        pressed: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Ladder,
                Stamina::default(),
                WallJumpState::default(),
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
                .any(|proposal| proposal.target_state == LocomotionState::WallJump)
        );
    }

    #[test]
    fn actor_without_wall_jump_movement_cannot_propose_wall_jump() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                Intents {
                    jump: crate::movement::intents::JumpIntent {
                        pressed: true,
                        ..default()
                    },
                    ..default()
                },
                LocomotionState::Climb,
                Stamina::default(),
                WallJumpState::default(),
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
