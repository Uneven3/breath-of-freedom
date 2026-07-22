//! Pure camera presentation state and markers.

use bevy::prelude::*;

#[derive(Component)]
pub struct CameraRig {
    pub current_dip: f32,
    pub smoothed_y: f32,
    /// 0 = orbit camera, 1 = aim camera; eased toward the player's
    /// `CombatState::Aiming`.
    pub aim_blend: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            current_dip: 0.0,
            smoothed_y: f32::NAN,
            aim_blend: 0.0,
        }
    }
}

/// Trauma-based screen shake, decayed on real time so hitstop does not stall
/// presentation feedback.
#[derive(Resource, Default)]
pub struct CameraShake {
    pub(super) trauma: f32,
}

impl CameraShake {
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }
}

#[derive(Component)]
pub struct Crosshair;

#[derive(Component)]
pub struct CrosshairRing;
