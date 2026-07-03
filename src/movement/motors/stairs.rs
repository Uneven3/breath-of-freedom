//! Stairs motor — per-tread Y-snap traversal of an authored staircase.
//!
//! Avian has no floor-snap-length equivalent; the explicit per-tread Y-snap
//! below is the primary grounding mechanism. Stateless: reads `StairsFacts`
//! each frame.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts, StairsFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity, GRAVITY};

const CAPSULE_HALF_HEIGHT: f32 = 1.0;
const CAPSULE_RADIUS: f32 = 0.5;
const LOOKAHEAD_MARGIN: f32 = 0.1;
const DESCEND_TRAIL: f32 = 0.49;
const INPUT_THRESHOLD_SQ: f32 = 0.01;
const ASCEND_THRESHOLD: f32 = 0.3;
const DESCEND_THRESHOLD: f32 = -0.3;

const ASCEND_SPEED: f32 = 3.5;
const DESCEND_SPEED: f32 = 4.5;
const SPRINT_MULTIPLIER: f32 = 1.7;
const SPRINT_STAMINA_COST_PER_SEC: f32 = 10.0;
const LATERAL_FACTOR: f32 = 0.6;
const ACCELERATION: f32 = 80.0;
const FRICTION: f32 = 60.0;
const SNAP_EPSILON: f32 = 0.08;
const GROUND_TOLERANCE: f32 = 0.15;

pub fn propose(
    mut q: Query<
        (
            &StairsFacts,
            &GroundFacts,
            &LocomotionState,
            &mut ProposalBuffer,
        ),
        With<Actor>,
    >,
) {
    for (stairs, ground, current, mut buffer) in &mut q {
        if !stairs.on_stairs {
            continue;
        }
        // Sticky once active; else require grounded entry (airborne stays in Fall).
        if *current == LocomotionState::Stairs || ground.grounded {
            buffer.0.push(TransitionProposal::new(
                LocomotionState::Stairs,
                Priority::Forced,
                0,
                "stairs",
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
    &'a Intents,
    &'a mut Stamina,
    &'a StairsFacts,
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
        intents,
        mut stamina,
        stairs,
        state,
    ) in &mut q
    {
        if *state != LocomotionState::Stairs {
            continue;
        }

        apply_locomotion_rotation(&mut transform, intents.move_dir, dt, 15.0);

        let horiz_axis = stairs.slope_axis();
        let lateral_axis = Vec3::Y.cross(horiz_axis).normalize_or_zero();
        let world_input = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y);
        let along = world_input.dot(horiz_axis);
        let lateral = world_input.dot(lateral_axis);

        let sprinting = intents.wants_sprint && stamina.current() > 0.0;
        let base_speed = if along >= 0.0 {
            ASCEND_SPEED
        } else {
            DESCEND_SPEED
        };
        let speed = if sprinting {
            base_speed * SPRINT_MULTIPLIER
        } else {
            base_speed
        };
        let target_h = horiz_axis * along * speed + lateral_axis * lateral * speed * LATERAL_FACTOR;

        let has_input = world_input.length_squared() > INPUT_THRESHOLD_SQ;
        let rate = if has_input { ACCELERATION } else { FRICTION };
        let mut v = vel.0;
        v.x = move_toward(v.x, target_h.x, rate * dt);
        v.z = move_toward(v.z, target_h.z, rate * dt);

        if sprinting {
            stamina.drain(SPRINT_STAMINA_COST_PER_SEC * dt);
        }

        // Per-step Y-snap. Sample point leads (ascent) or trails (descent) the body.
        let slope_input = along;
        let look_ahead = if slope_input > ASCEND_THRESHOLD {
            horiz_axis * (CAPSULE_RADIUS + LOOKAHEAD_MARGIN)
        } else if slope_input < DESCEND_THRESHOLD {
            horiz_axis * DESCEND_TRAIL
        } else {
            Vec3::ZERO
        };
        let sample_pos = transform.translation + look_ahead;
        let expected_feet_y = stairs.expected_feet_y(sample_pos);
        let current_feet_y = transform.translation.y - CAPSULE_HALF_HEIGHT;
        let feet_gap = expected_feet_y - current_feet_y;
        let max_snap = stairs.step_rise + GROUND_TOLERANCE;

        if slope_input > ASCEND_THRESHOLD && feet_gap > 0.0 && feet_gap <= max_snap {
            transform.translation.y = expected_feet_y + CAPSULE_HALF_HEIGHT + SNAP_EPSILON;
            v.y = 0.0;
        } else if slope_input < DESCEND_THRESHOLD && feet_gap < 0.0 && feet_gap >= -max_snap {
            transform.translation.y = expected_feet_y + CAPSULE_HALF_HEIGHT + SNAP_EPSILON;
            v.y = 0.0;
        } else if feet_gap < -max_snap {
            v.y -= GRAVITY * dt;
        } else {
            v.y = 0.0;
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
    }
}
