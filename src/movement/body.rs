//! Shared body dimensions — the single source of truth for the actor capsule.
//!
//! Motors and services used to hardcode these in six different places; any
//! change to the capsule (or a future non-capsule actor) starts here.

/// Capsule radius.
pub const RADIUS: f32 = 0.5;
/// Avian capsule cylinder length while standing (excludes the hemispheres):
/// total height 2.0 ⇒ length = 2.0 − 2·RADIUS.
pub const STAND_CAPSULE_LENGTH: f32 = 1.0;
/// Cylinder length while crouched: design height 1.2 ⇒ 1.2 − 2·RADIUS.
pub const CROUCH_CAPSULE_LENGTH: f32 = 0.2;
/// Half the standing total height (feet-to-center distance while standing).
pub const HALF_HEIGHT: f32 = RADIUS + STAND_CAPSULE_LENGTH / 2.0;
/// Half the crouched total height (feet-to-center distance while crouched).
pub const CROUCH_HALF_HEIGHT: f32 = RADIUS + CROUCH_CAPSULE_LENGTH / 2.0;
