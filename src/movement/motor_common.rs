//! Shared motor helpers.
//!
//! Every motor uses these (`apply_locomotion_rotation`, `move_toward`) plus the
//! `move_and_slide` call every motor makes. The slide is Avian's `MoveAndSlide`
//! system param; we wrap it so each motor calls one function — the only place
//! the body is moved (see `docs/architecture/movement.md`).

use avian3d::prelude::*;
use bevy::prelude::*;
use std::time::Duration;

use super::facts::BodyContact;

/// A surface counts as floor if its normal is within this dot of straight up.
/// `cos(60°) = 0.5` matches `GroundService`'s `max_slope_angle_deg = 60`.
/// Shared with `services::ground` (the downward grounded probe) as the single
/// 60° slope source of truth.
pub const FLOOR_MIN_UP_DOT: f32 = 0.5;
/// A surface counts as wall if it is nearly vertical (`|n.y|` small).
const WALL_MAX_UP_DOT: f32 = 0.2;

/// Step `from` toward `to` by at most `delta`.
pub fn move_toward(from: f32, to: f32, delta: f32) -> f32 {
    if (to - from).abs() <= delta {
        to
    } else {
        from + (to - from).signum() * delta
    }
}

/// Slerp the body's yaw toward its planar move direction. Movement is planar,
/// so we rotate purely about Y (cheaper and stable vs a full `looking_to`).
pub fn apply_locomotion_rotation(transform: &mut Transform, move_dir: Vec2, dt: f32, speed: f32) {
    if move_dir.length_squared() <= 0.01 {
        return;
    }
    let dir = Vec3::new(move_dir.x, 0.0, move_dir.y).normalize_or_zero();
    // Yaw so that the body's forward (-Z) points along `dir`: solving R_y(θ)·(-Z) = dir.
    let yaw = (-dir.x).atan2(-dir.z);
    let target = Quat::from_rotation_y(yaw);
    let t = (speed * dt).clamp(0.0, 1.0);
    transform.rotation = transform.rotation.slerp(target, t);
}

/// Move the kinematic body along `velocity` for one fixed step, sliding along
/// colliders. Updates `transform.translation`, classifies floor/wall contacts into
/// `contact`, and returns the post-slide ("projected") velocity to store for next
/// frame.
pub fn body_move_and_slide(
    mas: &MoveAndSlide,
    entity: Entity,
    collider: &Collider,
    transform: &mut Transform,
    velocity: Vec3,
    delta: Duration,
    contact: &mut BodyContact,
) -> Vec3 {
    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let mut grounded = false;
    let mut floor_normal = Vec3::ZERO;
    let mut on_wall = false;
    let mut wall_normal = Vec3::ZERO;

    let out = mas.move_and_slide(
        collider,
        transform.translation,
        transform.rotation,
        velocity,
        delta,
        &MoveAndSlideConfig::default(),
        &filter,
        |hit| {
            let n: Vec3 = Vec3::from(*hit.normal);
            if n.y > FLOOR_MIN_UP_DOT {
                grounded = true;
                floor_normal = n;
            } else if n.y.abs() < WALL_MAX_UP_DOT {
                on_wall = true;
                wall_normal = n;
            }
            MoveAndSlideHitResponse::Accept
        },
    );

    transform.translation = out.position;
    contact.grounded = grounded;
    contact.floor_normal = floor_normal;
    contact.on_wall = on_wall;
    contact.wall_normal = wall_normal;
    out.projected_velocity
}
