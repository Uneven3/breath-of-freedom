//! Player Brain — the only place hardware input enters the simulation.
//!
//! Hardware input lives only in a Brain, which writes the shared `Intents`
//! (see `docs/architecture/movement.md`). We read Bevy's native
//! `ButtonInput<KeyCode>` (centralised here, so a dedicated input-mapping crate
//! buys us little for a single brain) and rotate WASD into camera space.

use bevy::prelude::*;

use super::Player;
use super::intents::Intents;
use super::state::LocomotionState;
use crate::camera::CameraRig;

const WISH_DIR_THRESHOLD: f32 = 0.5;

/// Climb is a toggle (key `1`), not a hold — matches `ClimbToggleComponent`.
#[derive(Resource, Default)]
pub struct ClimbToggle(pub bool);

/// `MovementSet::ReadIntents`: hardware → `Intents` on the player.
pub fn read_intents(
    keys: Res<ButtonInput<KeyCode>>,
    mut climb_toggle: ResMut<ClimbToggle>,
    camera: Single<&CameraRig>,
    mut intents: Single<&mut Intents, With<Player>>,
) {
    // Toggle climb on key-down edge.
    if keys.just_pressed(KeyCode::Digit1) {
        climb_toggle.0 = !climb_toggle.0;
    }

    // Raw WASD combined into one vector:
    // x = right−left, y = back−forward (so forward is negative Y).
    let mut input = Vec2::ZERO;
    if keys.pressed(KeyCode::KeyD) {
        input.x += 1.0;
    }
    if keys.pressed(KeyCode::KeyA) {
        input.x -= 1.0;
    }
    if keys.pressed(KeyCode::KeyS) {
        input.y += 1.0;
    }
    if keys.pressed(KeyCode::KeyW) {
        input.y -= 1.0;
    }
    if input.length_squared() > 1.0 {
        input = input.normalize();
    }

    let i = &mut **intents;
    *i = Intents::default();
    i.raw_input = input;
    i.input_strength = input.length();

    // Discrete wish_dir (semantic intent), thresholded.
    i.wish_dir.x = if input.x > WISH_DIR_THRESHOLD {
        1
    } else if input.x < -WISH_DIR_THRESHOLD {
        -1
    } else {
        0
    };
    i.wish_dir.y = if input.y < -WISH_DIR_THRESHOLD {
        1 // forward
    } else if input.y > WISH_DIR_THRESHOLD {
        -1
    } else {
        0
    };

    // Camera-relative world move direction (XZ).
    if input != Vec2::ZERO {
        let yaw_rot = Quat::from_rotation_y(camera.yaw);
        let mut forward = yaw_rot * Vec3::NEG_Z;
        let mut right = yaw_rot * Vec3::X;
        forward.y = 0.0;
        right.y = 0.0;
        forward = forward.normalize_or_zero();
        right = right.normalize_or_zero();
        // input.y is negative when moving forward.
        let world = (right * input.x - forward * input.y).normalize_or_zero();
        i.move_dir = Vec2::new(world.x, world.z);
    } else {
        i.move_dir = input;
    }

    // Action triggers.
    if keys.pressed(KeyCode::Space) {
        i.wants_jump = true;
        i.wants_glide = true;
    }
    if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        i.wants_sprint = true;
    }
    i.wants_climb = climb_toggle.0;
    if keys.pressed(KeyCode::Digit2) {
        i.wants_mantle = true;
    }
    if keys.pressed(KeyCode::Digit3) {
        i.wants_vault = true;
    }
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        i.wants_sneak = true;
    }
}

/// Clear the climb toggle after moves that should drop climb intent.
/// Keyed on `Changed<LocomotionState>` so it fires exactly on a transition.
pub fn reset_climb_toggle(
    mut toggle: ResMut<ClimbToggle>,
    q: Query<(&LocomotionState, &Intents), (With<Player>, Changed<LocomotionState>)>,
) {
    let Ok((state, intents)) = q.single() else {
        return;
    };
    match *state {
        LocomotionState::Mantle | LocomotionState::AutoVault | LocomotionState::EdgeLeap => {
            toggle.0 = false;
        }
        LocomotionState::WallJump => {
            // Keep climb intent only when wall-jumping to re-stick (up/left/right).
            let sticking = intents.is_climbing_up()
                || intents.is_climbing_left()
                || intents.is_climbing_right();
            if !sticking {
                toggle.0 = false;
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    //! Covers the toggle-reset rules. `Changed<LocomotionState>` fires on the
    //! first run after spawn (the freshly added component counts as changed).
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// Start with climb toggled ON, enter `state` with the given input, return the
    /// resulting toggle value.
    fn toggle_after(state: LocomotionState, intents: Intents) -> bool {
        let mut world = World::new();
        world.insert_resource(ClimbToggle(true));
        world.spawn((Player, state, intents));
        world
            .run_system_once(reset_climb_toggle)
            .expect("reset runs");
        world.resource::<ClimbToggle>().0
    }

    #[test]
    fn resets_on_mantle_vault_edgeleap() {
        assert!(!toggle_after(LocomotionState::Mantle, Intents::default()));
        assert!(!toggle_after(
            LocomotionState::AutoVault,
            Intents::default()
        ));
        assert!(!toggle_after(LocomotionState::EdgeLeap, Intents::default()));
    }

    #[test]
    fn wall_jump_away_resets() {
        // Neutral input = leaping away from the wall → drop climb.
        assert!(!toggle_after(LocomotionState::WallJump, Intents::default()));
    }

    #[test]
    fn wall_jump_back_resets() {
        let back = Intents {
            wish_dir: IVec2::new(0, -1),
            ..default()
        }; // is_climbing_down
        assert!(!toggle_after(LocomotionState::WallJump, back));
    }

    #[test]
    fn wall_jump_lateral_keeps_climb() {
        let left = Intents {
            wish_dir: IVec2::new(-1, 0),
            ..default()
        }; // is_climbing_left
        assert!(toggle_after(LocomotionState::WallJump, left));
    }

    #[test]
    fn floor_jump_keeps_climb() {
        // A plain floor jump must never clear climb — player may jump then grab a wall.
        assert!(toggle_after(LocomotionState::Jump, Intents::default()));
    }
}
