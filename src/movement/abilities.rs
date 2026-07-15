//! Persistent movement abilities and their per-actor tuning.

use bevy::prelude::*;

/// Tuning shared by locomotion modes that move on the ground plane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GroundLocomotion {
    pub max_speed: f32,
    pub acceleration: f32,
    pub friction: f32,
    pub rotation_speed: f32,
    /// Stamina change per second: positive recovers, negative drains.
    pub stamina_per_sec: f32,
}

/// Enables ground locomotion and configures its Walk, Sprint, and Sneak
/// profiles for one actor.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GroundMovement {
    pub walk: GroundLocomotion,
    pub sprint: GroundLocomotion,
    pub sneak: GroundLocomotion,
    pub stairs: StairsLocomotion,
}

/// Tuning for traversal along authored stair geometry.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StairsLocomotion {
    pub ascend_speed: f32,
    pub descend_speed: f32,
    pub sprint_multiplier: f32,
    pub sneak_multiplier: f32,
    pub sprint_stamina_cost_per_sec: f32,
    pub lateral_factor: f32,
    pub acceleration: f32,
    pub friction: f32,
    pub rotation_speed: f32,
}

impl GroundMovement {
    pub const PLAYER: Self = Self {
        walk: GroundLocomotion {
            max_speed: 5.0,
            acceleration: 20.0,
            friction: 25.0,
            rotation_speed: 15.0,
            stamina_per_sec: 15.0,
        },
        sprint: GroundLocomotion {
            max_speed: 10.0,
            acceleration: 25.0,
            friction: 35.0,
            rotation_speed: 15.0,
            stamina_per_sec: -10.0,
        },
        sneak: GroundLocomotion {
            max_speed: 2.5,
            acceleration: 15.0,
            friction: 20.0,
            rotation_speed: 10.0,
            stamina_per_sec: 5.0,
        },
        stairs: StairsLocomotion {
            ascend_speed: 3.5,
            descend_speed: 4.5,
            sprint_multiplier: 1.7,
            // Half speed while crouched, matching Sneak's 2.5/5.0 flat-ground ratio.
            sneak_multiplier: 0.5,
            sprint_stamina_cost_per_sec: 10.0,
            lateral_factor: 0.6,
            acceleration: 80.0,
            friction: 60.0,
            rotation_speed: 15.0,
        },
    };
}

/// Base airborne locomotion profile for an actor subject to gravity.
///
/// Unlike `JumpMovement` or `GlideMovement`, this does not grant a discrete
/// player action. It configures the `Fall` fallback that takes over whenever
/// no stronger locomotion state applies.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct AirborneMovement {
    pub max_speed: f32,
    pub acceleration: f32,
    pub rise_gravity_multiplier: f32,
    pub fall_gravity_multiplier: f32,
    pub jump_cut_velocity: f32,
    pub rotation_speed: f32,
    pub stamina_recover_per_sec: f32,
    pub stamina_recovery_factor: f32,
}

impl AirborneMovement {
    pub const PLAYER: Self = Self {
        max_speed: 5.0,
        acceleration: 5.0,
        rise_gravity_multiplier: 1.3,
        fall_gravity_multiplier: 2.5,
        jump_cut_velocity: 2.0,
        rotation_speed: 15.0,
        stamina_recover_per_sec: 15.0,
        stamina_recovery_factor: 0.25,
    };
}

/// Enables a basic jump and configures its impulse, grace windows, and turn speed.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct JumpMovement {
    pub impulse: f32,
    pub coyote_time: f32,
    pub buffer_time: f32,
    pub rotation_speed: f32,
}

impl JumpMovement {
    pub const PLAYER: Self = Self {
        impulse: 5.5,
        coyote_time: 0.12,
        buffer_time: 0.12,
        rotation_speed: 15.0,
    };
}

/// Enables gliding and configures its air-control and stamina recovery profile.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GlideMovement {
    pub fall_speed: f32,
    pub gravity_multiplier: f32,
    pub max_speed: f32,
    pub acceleration: f32,
    pub rotation_speed: f32,
    pub stamina_recover_per_sec: f32,
    pub stamina_recovery_factor: f32,
}

impl GlideMovement {
    pub const PLAYER: Self = Self {
        fall_speed: 1.5,
        gravity_multiplier: 0.25,
        max_speed: 6.0,
        acceleration: 4.0,
        rotation_speed: 15.0,
        stamina_recover_per_sec: 8.0,
        stamina_recovery_factor: 0.25,
    };
}

/// Enables wall climbing and configures its movement profile for one actor.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct ClimbMovement {
    pub speed: f32,
    pub stamina_cost_per_sec: f32,
    pub wall_approach_speed: f32,
}

impl ClimbMovement {
    pub const PLAYER: Self = Self {
        speed: 2.5,
        stamina_cost_per_sec: 5.0,
        wall_approach_speed: 0.5,
    };
}

/// Enables authored ladder traversal and configures its vertical speed.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LadderMovement {
    pub speed: f32,
}

impl LadderMovement {
    pub const PLAYER: Self = Self { speed: 2.5 };
}

/// Tuning for the kinematic mantle traversal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MantleTraversal {
    pub vertical_speed: f32,
    pub forward_speed: f32,
    pub arc_height: f32,
}

/// Tuning for the kinematic auto-vault traversal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct VaultTraversal {
    pub speed: f32,
    pub arc_height: f32,
}

/// Enables traversal over ledges and configures Mantle and AutoVault.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LedgeTraversal {
    pub mantle: MantleTraversal,
    pub vault: VaultTraversal,
}

impl LedgeTraversal {
    pub const PLAYER: Self = Self {
        mantle: MantleTraversal {
            vertical_speed: 4.0,
            forward_speed: 3.0,
            arc_height: 0.25,
        },
        vault: VaultTraversal {
            speed: 5.0,
            arc_height: 0.4,
        },
    };
}

/// Tuning for a jump launched from a wall or ladder.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WallJumpTraversal {
    pub jump_up_impulse: f32,
    pub stamina_cost: f32,
    pub duration: f32,
    pub wall_contact_push: f32,
    pub away_up_blend: f32,
    pub away_leap_speed: f32,
    pub away_normal_push: f32,
    pub lateral_speed_fraction: f32,
    pub lateral_vertical_lift: f32,
    pub lateral_normal_retraction: f32,
}

/// Tuning for a leap from the lateral edge of a climbable wall.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EdgeLeapTraversal {
    pub away_impulse: f32,
    pub vertical_boost: f32,
    pub stamina_cost: f32,
    pub duration: f32,
    pub wall_push_speed: f32,
}

/// Enables wall-launched traversal and configures WallJump and EdgeLeap.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct WallJumpMovement {
    pub wall_jump: WallJumpTraversal,
    pub edge_leap: EdgeLeapTraversal,
}

impl WallJumpMovement {
    pub const PLAYER: Self = Self {
        wall_jump: WallJumpTraversal {
            jump_up_impulse: 7.0,
            stamina_cost: 15.0,
            duration: 0.2,
            wall_contact_push: 1.0,
            away_up_blend: 0.4,
            away_leap_speed: 3.5,
            away_normal_push: 4.0,
            lateral_speed_fraction: 0.8,
            lateral_vertical_lift: 0.5,
            lateral_normal_retraction: 0.5,
        },
        edge_leap: EdgeLeapTraversal {
            away_impulse: 8.0,
            vertical_boost: 2.0,
            stamina_cost: 10.0,
            duration: 0.3,
            wall_push_speed: 2.0,
        },
    };
}
