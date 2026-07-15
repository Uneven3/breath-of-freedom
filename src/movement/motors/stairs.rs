//! Stairs motor — per-tread Y-snap traversal of an authored staircase.
//!
//! Avian has no floor-snap-length equivalent; the explicit per-tread Y-snap
//! below is the primary grounding mechanism. Stateless: reads `StairsFacts`
//! each frame.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::GroundMovement;
use crate::movement::body::BodyDimensions;
use crate::movement::facts::{BodyContact, GroundFacts, StairsFacts};
use crate::movement::intents::{GaitIntent, Intents};
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::motors::sneak::Crouched;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity, GRAVITY};

const LOOKAHEAD_MARGIN: f32 = 0.1;
const DESCEND_TRAIL: f32 = 0.49;
const INPUT_THRESHOLD_SQ: f32 = 0.01;
const ASCEND_THRESHOLD: f32 = 0.3;
const DESCEND_THRESHOLD: f32 = -0.3;

use bevy::platform::collections::HashMap;

const SNAP_EPSILON: f32 = 0.08;
const GROUND_TOLERANCE: f32 = 0.15;

/// Frames to keep proposing Stairs after `on_stairs` goes false.
/// Prevents oscillation at trigger-volume edges.  12 frames ≈ 200 ms at 60 Hz.
const STAIRS_EXIT_GRACE: u32 = 12;

type ProposeQuery<'a> = (
    Entity,
    &'a StairsFacts,
    &'a GroundFacts,
    &'a LocomotionState,
    &'a mut ProposalBuffer,
);

pub fn propose(
    mut q: Query<ProposeQuery, (With<Actor>, With<GroundMovement>)>,
    mut grace: Local<HashMap<Entity, u32>>,
) {
    for (entity, stairs, ground, current, mut buffer) in &mut q {
        // Hysteresis: refresh grace while on stairs; count down after leaving.
        if stairs.on_stairs {
            grace.insert(entity, STAIRS_EXIT_GRACE);
        } else {
            let remaining = grace.get(&entity).copied().unwrap_or(0);
            if remaining == 0 {
                continue;
            }
            grace.insert(entity, remaining - 1);
        }
        // Sticky once active; else require grounded entry (airborne stays in Fall).
        if *current == LocomotionState::Stairs || ground.grounded {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Stairs,
                Priority::Forced,
                weight::STAIRS,
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
    &'a GroundMovement,
    &'a BodyDimensions,
    Option<&'a Crouched>,
    &'a LocomotionState,
);

pub fn tick(
    mut q: Query<TickQuery, (With<Actor>, With<GroundMovement>)>,
    mas: MoveAndSlide,
    time: Res<Time>,
    mut cached_facts: Local<HashMap<Entity, StairsFacts>>,
) {
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
        movement,
        body,
        crouched,
        state,
    ) in &mut q
    {
        if *state != LocomotionState::Stairs {
            cached_facts.remove(&entity);
            continue;
        }

        // Grace period: on_stairs can go false between treads because the
        // trigger AABB misses the gap.  During the grace window the propose
        // system keeps state==Stairs, but StairsFacts is zeroed by the
        // service.
        // Cache the last seen valid StairsFacts so we can keep using the correct
        // stair geometry for Y-snapping and movement math instead of zeroing out.
        let effective_stairs = if stairs.on_stairs {
            cached_facts.insert(entity, stairs.clone());
            stairs
        } else if let Some(cached) = cached_facts.get(&entity) {
            cached
        } else {
            stairs
        };

        let profile = movement.stairs;
        // Crouch is orthogonal to the Stairs state (see `sync_crouch_collider`):
        // the capsule may already be crouched here, so the tread math must use the
        // matching half-height, and the gait may ask for a slower crouched pace.
        let is_crouched = crouched.is_some_and(|c| c.0);
        let half_height = if is_crouched {
            body.crouched_half_height()
        } else {
            body.standing_half_height()
        };
        apply_locomotion_rotation(
            &mut transform,
            intents.planar.direction,
            dt,
            profile.rotation_speed,
        );

        let horiz_axis = stair_axis(effective_stairs);
        let lateral_axis = Vec3::Y.cross(horiz_axis).normalize_or_zero();
        let world_input = Vec3::new(intents.planar.direction.x, 0.0, intents.planar.direction.y);
        let along = world_input.dot(horiz_axis);
        let lateral = world_input.dot(lateral_axis);

        let sprinting = intents.gait == GaitIntent::Sprint && stamina.current() > 0.0;
        let base_speed = if along >= 0.0 {
            profile.ascend_speed
        } else {
            profile.descend_speed
        };
        let speed = if sprinting {
            base_speed * profile.sprint_multiplier
        } else if is_crouched {
            base_speed * profile.sneak_multiplier
        } else {
            base_speed
        };
        let target_h =
            horiz_axis * along * speed + lateral_axis * lateral * speed * profile.lateral_factor;

        let has_input = world_input.length_squared() > INPUT_THRESHOLD_SQ;
        let rate = if has_input {
            profile.acceleration
        } else {
            profile.friction
        };
        let mut v = vel.0;
        v.x = move_toward(v.x, target_h.x, rate * dt);
        v.z = move_toward(v.z, target_h.z, rate * dt);

        if sprinting {
            stamina.drain(profile.sprint_stamina_cost_per_sec * dt);
        } else if is_crouched && has_input {
            // Sneak drains stamina when moving on stairs (at half of sprint stamina rate)
            stamina.drain(profile.sprint_stamina_cost_per_sec * 0.5 * dt);
        } else {
            // Recover stamina when standing still or walking normally on stairs
            stamina.recover(movement.walk.stamina_per_sec * dt);
        }

        // Per-step Y-snap. Sample point leads (ascent) or trails (descent) the body.
        let slope_input = along;
        let look_ahead = if slope_input > ASCEND_THRESHOLD {
            horiz_axis * (body.radius + LOOKAHEAD_MARGIN).min(effective_stairs.step_depth)
        } else if slope_input < DESCEND_THRESHOLD {
            horiz_axis * DESCEND_TRAIL.min(effective_stairs.step_depth)
        } else {
            Vec3::ZERO
        };
        let sample_pos = transform.translation + look_ahead;
        let expected_feet_y = expected_feet_y(effective_stairs, sample_pos);
        let current_feet_y = transform.translation.y - half_height;
        let feet_gap = expected_feet_y - current_feet_y;
        let max_snap = effective_stairs.step_rise + GROUND_TOLERANCE;

        // Snap up to the next tread while ascending, down to the previous one
        // while descending; free-fall past a whole-step drop; hold Y otherwise.
        let ascending_snap =
            slope_input > ASCEND_THRESHOLD && feet_gap > 0.0 && feet_gap <= max_snap;
        let descending_snap =
            slope_input < DESCEND_THRESHOLD && feet_gap < 0.0 && feet_gap >= -max_snap;
        if ascending_snap || descending_snap {
            transform.translation.y = expected_feet_y + half_height + SNAP_EPSILON;
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

fn stair_axis(stairs: &StairsFacts) -> Vec3 {
    let d = stairs.top - stairs.base;
    Vec3::new(d.x, 0.0, d.z).normalize_or_zero()
}

pub(crate) fn expected_feet_y(stairs: &StairsFacts, world_pos: Vec3) -> f32 {
    let distance = (world_pos - stairs.base).dot(stair_axis(stairs));
    if distance <= 0.0 {
        return stairs.base.y;
    }
    let total_run = stairs.step_count as f32 * stairs.step_depth;
    if distance >= total_run {
        return stairs.base.y + stairs.step_count as f32 * stairs.step_rise;
    }
    let index = (distance / stairs.step_depth).floor() as i32;
    stairs.base.y + (index + 1) as f32 * stairs.step_rise
}
