//! Detached free-fly debug camera.
//!
//! A tool, not a gameplay view: while active it acquires modal input focus
//! (which freezes the player's actions/look and releases the cursor via the
//! existing `input` machinery) so it can fly with a direct keyboard read while
//! the F1 hub stays operable — `ModalInputFocus` is multi-owner, so the hub can
//! hold focus alongside it. Look is hold-RMB, which momentarily grabs the
//! cursor; releasing it hands the pointer back to the hub. The freecam keeps its
//! own yaw/pitch in `CameraControl` so flying never steers the character.

use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

use crate::input::{MOUSE_SENSITIVITY, ModalInputFocusRequest, SetCursorGrab};
use crate::visuals::PlayerVisual;

use super::CameraRig;
use super::data::{CameraControl, CameraMode};

/// Toggles the debug freecam.
const TOGGLE_KEY: KeyCode = KeyCode::F3;
/// Logs the current pose as a paste-ready `flythrough::Waypoint` line.
const CAPTURE_KEY: KeyCode = KeyCode::F4;
/// Metres per second at rest; multiplied while boosting.
const FREECAM_SPEED: f32 = 12.0;
const FREECAM_BOOST: f32 = 4.0;
/// Matches the player-look clamp so the freecam cannot flip over the poles.
const PITCH_LIMIT: f32 = 1.54;

/// F3 flips Orbit↔Freecam. Entering seeds the freecam's angles from the current
/// camera so the view does not jump, acquires modal focus (freezing the player
/// and freeing the cursor), and makes the player model visible for observation;
/// leaving releases focus, which restores the gameplay cursor grab.
pub(super) fn toggle_camera_mode(
    keys: Res<ButtonInput<KeyCode>>,
    rig: Single<(Entity, &Transform, &mut CameraControl), With<CameraRig>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
    mut player_vis: Query<&mut Visibility, With<PlayerVisual>>,
) {
    if !keys.just_pressed(TOGGLE_KEY) {
        return;
    }
    let (rig_entity, rig_transform, mut control) = rig.into_inner();
    match control.mode {
        CameraMode::Orbit => {
            let (yaw, pitch, _) = rig_transform.rotation.to_euler(EulerRot::YXZ);
            control.freecam_yaw = yaw;
            control.freecam_pitch = pitch.clamp(-PITCH_LIMIT, PITCH_LIMIT);
            control.mode = CameraMode::Freecam;
            focus.write(ModalInputFocusRequest::Acquire(rig_entity));
            // Follow no longer runs to un-hide the model, so ensure it is shown.
            if let Ok(mut visibility) = player_vis.single_mut() {
                *visibility = Visibility::Inherited;
            }
        }
        CameraMode::Freecam => {
            control.mode = CameraMode::Orbit;
            focus.write(ModalInputFocusRequest::Release(rig_entity));
        }
    }
}

/// Flies the camera each frame while in freecam mode. Reads the keyboard
/// directly (debug tier, bypassing the intent system, which is frozen anyway),
/// and rotates only while the right mouse button is held.
pub(super) fn fly_freecam(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    time: Res<Time>,
    cam: Single<(&mut Transform, &mut CameraControl), With<CameraRig>>,
    mut grab: MessageWriter<SetCursorGrab>,
) {
    let (mut transform, mut control) = cam.into_inner();

    // Ask Input to grab the cursor only for the duration of a look-drag, so the
    // pointer is free to operate the hub the rest of the time. Input stays the
    // sole writer of the cursor (§7).
    if mouse.just_pressed(MouseButton::Right) {
        grab.write(SetCursorGrab(true));
    }
    if mouse.just_released(MouseButton::Right) {
        grab.write(SetCursorGrab(false));
    }
    if mouse.pressed(MouseButton::Right) && motion.delta != Vec2::ZERO {
        control.freecam_yaw -= motion.delta.x * MOUSE_SENSITIVITY;
        control.freecam_pitch = (control.freecam_pitch - motion.delta.y * MOUSE_SENSITIVITY)
            .clamp(-PITCH_LIMIT, PITCH_LIMIT);
    }

    let rotation =
        Quat::from_rotation_y(control.freecam_yaw) * Quat::from_rotation_x(control.freecam_pitch);
    let forward = rotation * Vec3::NEG_Z;
    let right = rotation * Vec3::X;

    let mut direction = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        direction += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        direction -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        direction += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        direction -= right;
    }
    // Vertical on world up, so ascent/descent stay level regardless of pitch.
    if keys.pressed(KeyCode::Space) {
        direction += Vec3::Y;
    }
    if keys.pressed(KeyCode::ControlLeft) {
        direction -= Vec3::Y;
    }

    let speed = if keys.pressed(KeyCode::ShiftLeft) {
        FREECAM_SPEED * FREECAM_BOOST
    } else {
        FREECAM_SPEED
    };
    if direction != Vec3::ZERO {
        transform.translation += direction.normalize() * speed * time.delta_secs();
    }
    transform.rotation = rotation;
}

/// Authoring half of the flythrough's "capture → constants" flow: F4 logs the
/// current camera pose as a `flythrough::Waypoint { .. }` line, ready to paste
/// into `perf::flythrough::ROUTE`. Fly the freecam, drop a waypoint at each point
/// of interest, paste — the route stays reproducible because it lives in code.
pub(super) fn log_waypoint(
    keys: Res<ButtonInput<KeyCode>>,
    cam: Single<&Transform, With<CameraRig>>,
) {
    if !keys.just_pressed(CAPTURE_KEY) {
        return;
    }
    let p = cam.translation;
    let f = cam.forward().as_vec3();
    info!(
        "[flythrough] Waypoint {{ leg: \"...\", position: Vec3::new({:.2}, {:.2}, {:.2}), \
         facing: Vec3::new({:.2}, {:.2}, {:.2}) }},",
        p.x, p.y, p.z, f.x, f.y, f.z
    );
}

// Default to orbit before the camera exists, so a run condition never blocks on
// a missing singleton. `CameraControl` is a component on the camera entity.
pub(super) fn in_orbit_mode(control: Option<Single<&CameraControl>>) -> bool {
    control.is_none_or(|c| c.mode == CameraMode::Orbit)
}

pub(super) fn in_freecam_mode(control: Option<Single<&CameraControl>>) -> bool {
    control.is_some_and(|c| c.mode == CameraMode::Freecam)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn mode_defaults_to_orbit() {
        assert_eq!(CameraControl::default().mode, CameraMode::Orbit);
    }

    fn press(key: KeyCode) -> ButtonInput<KeyCode> {
        let mut input = ButtonInput::default();
        input.press(key);
        input
    }

    /// The orbit and freecam systems must never both run in one frame, or two
    /// writers would fight over the camera transform.
    #[test]
    fn run_conditions_partition_the_modes() {
        let mut world = World::new();
        let camera = world.spawn(CameraControl::default()).id();
        assert!(world.run_system_once(in_orbit_mode).unwrap());
        assert!(!world.run_system_once(in_freecam_mode).unwrap());

        world.get_mut::<CameraControl>(camera).unwrap().mode = CameraMode::Freecam;
        assert!(!world.run_system_once(in_orbit_mode).unwrap());
        assert!(world.run_system_once(in_freecam_mode).unwrap());
    }

    /// Entering freecam seeds the look angles from the camera (no view jump),
    /// flips the mode, and emits a focus-acquire so the player freezes and the
    /// cursor releases; a second press returns to orbit and releases focus.
    #[test]
    fn toggle_alternates_and_drives_modal_focus() {
        let mut world = World::new();
        world.init_resource::<Messages<ModalInputFocusRequest>>();
        let camera = world
            .spawn((
                CameraRig::default(),
                CameraControl::default(),
                Transform::from_rotation(Quat::from_rotation_y(0.9)),
            ))
            .id();
        world.spawn((PlayerVisual, Visibility::Hidden));

        world.insert_resource(press(TOGGLE_KEY));
        world.run_system_once(toggle_camera_mode).unwrap();

        let control = world.get::<CameraControl>(camera).unwrap();
        assert_eq!(control.mode, CameraMode::Freecam);
        // Seeded from the camera's 0.9 rad yaw, so the view does not snap.
        assert!((control.freecam_yaw - 0.9).abs() < 1e-4);
        let acquired = world
            .resource_mut::<Messages<ModalInputFocusRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(matches!(
            acquired.as_slice(),
            [ModalInputFocusRequest::Acquire(_)]
        ));

        world.run_system_once(toggle_camera_mode).unwrap();
        assert_eq!(
            world.get::<CameraControl>(camera).unwrap().mode,
            CameraMode::Orbit
        );
        let released = world
            .resource_mut::<Messages<ModalInputFocusRequest>>()
            .drain()
            .collect::<Vec<_>>();
        assert!(matches!(
            released.as_slice(),
            [ModalInputFocusRequest::Release(_)]
        ));
    }
}
