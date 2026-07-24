//! Pure camera presentation state and markers.

use bevy::prelude::*;

#[derive(Component)]
pub struct CameraRig {
    pub current_dip: f32,
    pub smoothed_y: f32,
    /// 0 = orbit camera, 1 = aim camera; eased toward the player's
    /// `CombatState::Aiming`.
    pub aim_blend: f32,
    /// 0 = orientation-driven orbit, 1 = framed on the lock-on target; eased
    /// toward `FacingSource::LockOn`.
    pub lock_blend: f32,
    /// Last yaw toward the lock-on target, held so releasing the lock can ease
    /// back out of the framing instead of snapping.
    pub lock_yaw: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            current_dip: 0.0,
            smoothed_y: f32::NAN,
            aim_blend: 0.0,
            lock_blend: 0.0,
            lock_yaw: 0.0,
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

/// Which behaviour drives the single shared `Camera3d` this frame.
///
/// One camera entity, many behaviours: re-spawning cameras would break the
/// `Single<_, With<Camera3d>>` queries that assume exactly one (day/night sun
/// disc in `world/day_night.rs`, the benchmark park in `perf/sequence.rs`,
/// screen-space juice in `presentation/juice.rs`). The `Camera3d` — with its
/// `DistanceFog` and profile MSAA — persists; only how its `Transform` is
/// driven changes per mode. Future gameplay modes (first-person, a fixed
/// Dota-style boom, a WoW-style orbit) join this enum.
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CameraMode {
    /// The third-person follow camera (gameplay default).
    #[default]
    Orbit,
    /// Detached free-fly camera (debug tool). Freezes the player and releases
    /// the cursor so the F1 hub is operable while flying; the eventual scripted
    /// perf flythrough rides on this.
    Freecam,
}

/// Runtime camera mode plus the freecam's own look angles. A **component on the
/// camera entity** — this is view state that belongs with the camera, alongside
/// `CameraRig`, not a global singleton resource. Kept apart from the player's
/// `ControlOrientation` so flying never steers the character.
#[derive(Component, Default)]
pub struct CameraControl {
    pub mode: CameraMode,
    pub(super) freecam_yaw: f32,
    pub(super) freecam_pitch: f32,
}
