//! Wall-jump motor — launch off a climbed wall in a direction chosen by input.
//!
//! Timer/jumping flags shared between `propose` and `tick` via a
//! `WallJumpState` component.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, LedgeFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const JUMP_UP_IMPULSE: f32 = 7.0;
const STAMINA_COST: f32 = 15.0;
const JUMP_DURATION: f32 = 0.2;
const WALL_CONTACT_PUSH: f32 = 1.0;
const AWAY_UP_BLEND: f32 = 0.4;
const AWAY_LEAP_SPEED: f32 = 3.5;
const AWAY_NORMAL_PUSH: f32 = 4.0;
const LATERAL_SPEED_FRACTION: f32 = 0.8;
const LATERAL_VERTICAL_LIFT: f32 = 0.5;
const LATERAL_NORMAL_RETRACTION: f32 = 0.5;
const BODY_HALF_HEIGHT: f32 = 1.0;
const LEDGE_TOP_OFFSET: f32 = 0.33;
const FORCED_WEIGHT: i32 = 5;

#[derive(Component, Default)]
pub struct WallJumpState {
    is_jumping: bool,
    timer: f32,
    needs_release: bool,
}

pub fn propose(
    mut q: Query<
        (
            &Intents,
            &LocomotionState,
            &Stamina,
            &mut WallJumpState,
            &mut ProposalBuffer,
        ),
        With<Actor>,
    >,
) {
    for (intents, current, stamina, mut state, mut buffer) in &mut q {
        if !intents.wants_jump {
            state.needs_release = false;
        }

        if *current == LocomotionState::Climb && intents.wants_jump && !state.needs_release {
            if !stamina.is_exhausted() {
                // Arm the jump for this activation.
                state.needs_release = true;
                state.is_jumping = true;
                state.timer = JUMP_DURATION;
                buffer.0.push(TransitionProposal::new(
                    LocomotionState::WallJump,
                    Priority::Forced,
                    FORCED_WEIGHT,
                    "wall_jump",
                ));
                continue;
            }
        }

        if *current == LocomotionState::WallJump && state.is_jumping {
            buffer.0.push(TransitionProposal::new(
                LocomotionState::WallJump,
                Priority::Forced,
                FORCED_WEIGHT,
                "wall_jump",
            ));
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a mut BodyContact,
    &'a mut WallJumpState,
    &'a Intents,
    &'a mut Stamina,
    &'a LedgeFacts,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (
        entity,
        collider,
        mut transform,
        mut vel,
        mut contact,
        mut state,
        intents,
        mut stamina,
        ledge,
        loco_state,
    ) in &mut q
    {
        if *loco_state != LocomotionState::WallJump {
            continue;
        }

        let mut v = vel.0;

        // First tick: apply the launch impulse (timer freshly set to JUMP_DURATION).
        if state.timer == JUMP_DURATION {
            let mut normal = ledge.climb_normal;
            if normal == Vec3::ZERO {
                normal = if contact.on_wall {
                    -contact.wall_normal
                } else {
                    transform.rotation * Vec3::Z
                };
            }
            let right_dir = Vec3::Y.cross(normal).normalize_or_zero();

            if intents.is_climbing_up() {
                v = Vec3::Y * JUMP_UP_IMPULSE - normal * WALL_CONTACT_PUSH;
            } else if intents.is_climbing_down() {
                let away = (normal + Vec3::Y * AWAY_UP_BLEND).normalize_or_zero();
                v = away * AWAY_LEAP_SPEED + normal * AWAY_NORMAL_PUSH;
            } else if intents.is_climbing_left() {
                v = -right_dir * (JUMP_UP_IMPULSE * LATERAL_SPEED_FRACTION);
                v.y = LATERAL_VERTICAL_LIFT;
                v -= normal * LATERAL_NORMAL_RETRACTION;
            } else if intents.is_climbing_right() {
                v = right_dir * (JUMP_UP_IMPULSE * LATERAL_SPEED_FRACTION);
                v.y = LATERAL_VERTICAL_LIFT;
                v -= normal * LATERAL_NORMAL_RETRACTION;
            } else {
                let away = (normal + Vec3::Y * AWAY_UP_BLEND).normalize_or_zero();
                v = away * AWAY_LEAP_SPEED + normal * AWAY_NORMAL_PUSH;
            }
            stamina.drain(STAMINA_COST);
        }

        state.timer -= dt;

        // Soft ceiling clip (same as climb).
        if ledge.lip_height > 0.0 && v.y > 0.0 {
            let feet_y = transform.translation.y - BODY_HALF_HEIGHT;
            let max_y = feet_y + ledge.lip_height - LEDGE_TOP_OFFSET;
            if transform.translation.y >= max_y {
                transform.translation.y = max_y;
                v.y = 0.0;
                state.is_jumping = false;
            }
        }

        vel.0 = body_move_and_slide(
            &mas,
            entity,
            collider,
            &mut transform,
            v,
            time.delta(),
            &mut contact,
        );

        if state.timer <= 0.0 {
            state.is_jumping = false;
        }
    }
}
