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

/// Encuadre inicial del World Lab. Abrir a la altura de un jugador dejaba la
/// cámara **dentro** del terreno (2026-08-22); la distancia se deriva de estos
/// dos para que no puedan desincronizarse (`camera::authoring_pose`).
pub(super) const AUTHORING_HEIGHT: f32 = 45.0;
pub(super) const AUTHORING_PITCH: f32 = -0.7;

impl CameraControl {
    /// Los ángulos van sembrados —no en cero— porque `fly_freecam` reconstruye
    /// la rotación desde ellos: en cero, el primer movimiento del mouse tiraría
    /// el encuadre inicial.
    pub(super) fn authoring() -> Self {
        Self {
            mode: CameraMode::Freecam,
            freecam_yaw: 0.0,
            freecam_pitch: AUTHORING_PITCH,
        }
    }
}

/// Radianes por píxel en la freecam. Aparte de la del jugador y ajustable en
/// caliente con `Ctrl+rueda`: el número cómodo se calibra (`MAP_EDITOR.md`).
#[derive(Resource, Debug, Clone, Copy)]
pub struct LookSensitivity(pub f32);

impl LookSensitivity {
    pub(super) const SLOWEST: f32 = 0.0001;
    pub(super) const FASTEST: f32 = 0.006;
    pub(super) const MULTIPLIER_PER_NOTCH: f32 = 1.15;
}

/// Calibrado jugando el 2026-08-22: ~15.700 px por vuelta.
impl Default for LookSensitivity {
    fn default() -> Self {
        Self(0.0004)
    }
}

#[cfg(test)]
mod sensitivity_tests {
    use super::*;

    /// Es la diferencia entre mirar y apuntar, y el motivo de existir aparte.
    #[test]
    fn authoring_look_is_gentler_than_the_players() {
        assert!(LookSensitivity::default().0 < crate::input::MOUSE_SENSITIVITY);
    }

    /// Multiplicativo: subir y bajar lo mismo tiene que volver al inicio.
    #[test]
    fn calibrating_up_and_back_down_returns_to_the_start() {
        let start = LookSensitivity::default().0;
        let up = start * LookSensitivity::MULTIPLIER_PER_NOTCH.powf(3.0);
        let back = up * LookSensitivity::MULTIPLIER_PER_NOTCH.powf(-3.0);
        assert!((back - start).abs() < 1.0e-6, "{start} → {up} → {back}");
    }

    #[test]
    fn the_default_sits_inside_its_own_range() {
        let default = LookSensitivity::default().0;
        assert!(default > LookSensitivity::SLOWEST && default < LookSensitivity::FASTEST);
    }
}
