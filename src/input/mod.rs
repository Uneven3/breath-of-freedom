//! Input resolution: hardware is sampled once per render frame and published
//! as fixed-capacity action snapshots for gameplay consumers.

pub mod action;
pub mod frame;

use bevy::app::AppExit;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use action::IntentAction;
use frame::{ActiveActions, ControlOrientation, InputControlledBy, LOCAL_INPUT_SOURCE};

pub use frame::InputConsumeCursor;

const MOUSE_SENSITIVITY: f32 = 0.003;
const PITCH_MIN: f32 = -1.2;
const PITCH_MAX: f32 = 1.2;

/// Whether local pointer input currently controls the local source.
#[derive(Resource)]
pub struct PointerCaptured(pub bool);

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum InputSet {
    UpdateOrientation,
}

pub struct InputPlugin;

impl Plugin for InputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveActions>();
        app.insert_resource(PointerCaptured(true));
        app.add_systems(Startup, grab_cursor);
        app.add_systems(PreUpdate, resolve_local_actions);
        app.add_systems(
            Update,
            (cursor_control, update_local_orientation)
                .chain()
                .in_set(InputSet::UpdateOrientation),
        );
    }
}

fn resolve_local_actions(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    captured: Res<PointerCaptured>,
    mut actions: ResMut<ActiveActions>,
) {
    for (action, key) in LOCAL_HELD_BINDINGS {
        actions.set_pressed(LOCAL_INPUT_SOURCE, action, keys.pressed(key));
    }
    // Attack on left mouse, only while the pointer is captured — the same
    // click that re-captures the cursor must not also swing.
    let attack_held = captured.0 && mouse.pressed(MouseButton::Left);
    actions.set_pressed(LOCAL_INPUT_SOURCE, IntentAction::Attack, attack_held);
    if captured.0 && mouse.just_pressed(MouseButton::Left) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Attack);
    }
    actions.set_pressed(
        LOCAL_INPUT_SOURCE,
        IntentAction::Sprint,
        keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight),
    );
    actions.set_pressed(
        LOCAL_INPUT_SOURCE,
        IntentAction::Sneak,
        keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight),
    );
    if keys.just_pressed(KeyCode::Digit1) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::ClimbToggle);
    }
    if keys.just_pressed(KeyCode::Space) {
        actions.trigger(LOCAL_INPUT_SOURCE, IntentAction::Jump);
    }
}

const LOCAL_HELD_BINDINGS: [(IntentAction, KeyCode); 12] = [
    (IntentAction::MoveForward, KeyCode::KeyW),
    (IntentAction::MoveBack, KeyCode::KeyS),
    (IntentAction::MoveLeft, KeyCode::KeyA),
    (IntentAction::MoveRight, KeyCode::KeyD),
    (IntentAction::Jump, KeyCode::Space),
    (IntentAction::Glide, KeyCode::Space),
    (IntentAction::Mantle, KeyCode::Digit2),
    (IntentAction::Vault, KeyCode::Digit3),
    (IntentAction::LookUp, KeyCode::KeyI),
    (IntentAction::LookDown, KeyCode::KeyK),
    (IntentAction::LookLeft, KeyCode::KeyJ),
    (IntentAction::LookRight, KeyCode::KeyL),
];

fn grab_cursor(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<PointerCaptured>,
) {
    set_cursor(&mut cursor, &mut captured, true);
}

fn cursor_control(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<PointerCaptured>,
    mut exit: MessageWriter<AppExit>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        if captured.0 {
            set_cursor(&mut cursor, &mut captured, false);
        } else {
            exit.write(AppExit::Success);
        }
    } else if !captured.0 && mouse.just_pressed(MouseButton::Left) {
        set_cursor(&mut cursor, &mut captured, true);
    }
}

fn set_cursor(
    cursor: &mut Query<&mut CursorOptions, With<PrimaryWindow>>,
    captured: &mut PointerCaptured,
    grab: bool,
) {
    if let Ok(mut options) = cursor.single_mut() {
        options.grab_mode = if grab {
            CursorGrabMode::Locked
        } else {
            CursorGrabMode::None
        };
        options.visible = !grab;
    }
    captured.0 = grab;
}

fn update_local_orientation(
    motion: Res<AccumulatedMouseMotion>,
    captured: Res<PointerCaptured>,
    mut q: Query<(&InputControlledBy, &mut ControlOrientation)>,
) {
    if !captured.0 || motion.delta == Vec2::ZERO {
        return;
    }
    for (source, mut orientation) in &mut q {
        if source.0 != LOCAL_INPUT_SOURCE {
            continue;
        }
        orientation.yaw -= motion.delta.x * MOUSE_SENSITIVITY;
        orientation.pitch =
            (orientation.pitch - motion.delta.y * MOUSE_SENSITIVITY).clamp(PITCH_MIN, PITCH_MAX);
    }
}
