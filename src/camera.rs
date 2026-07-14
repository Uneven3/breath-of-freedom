//! Orbit follow camera.
//!
//! Mouse look drives yaw/pitch; the rig follows the player with smoothed Y, a
//! landing dip, and a spring-arm that pulls the camera in when it would clip
//! world geometry. Runs in `Update` (render frame).
//!
//! Cursor capture also lives here (presentation, not simulation): ESC frees the
//! cursor (and quits on a second press) and a click recaptures it. Keeping it
//! out of the Brain preserves the Brain's only role: producing `Intents` (see
//! `docs/architecture/movement.md`).

use avian3d::prelude::*;
use bevy::app::AppExit;
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};

use crate::movement::Player;
use crate::movement::state::LocomotionState;

const MOUSE_SENSITIVITY: f32 = 0.003;
const PITCH_MIN: f32 = -1.2;
const PITCH_MAX: f32 = 1.2;
const SPRING_LENGTH: f32 = 6.5;
const LENS_HEIGHT: f32 = 1.5;
const LANDING_DIP_INTENSITY: f32 = 0.5;
const LANDING_DIP_RECOVERY: f32 = 8.0;
const FOLLOW_LERP_Y: f32 = 15.0;
/// Keep the camera this far off a surface it would otherwise clip into.
const SPRING_MARGIN: f32 = 0.2;
const SPRING_PROBE_RADIUS: f32 = 0.2;

/// Whether the OS cursor is captured (locked + hidden). Gates mouse-look so the
/// camera stops rotating while the cursor is free.
#[derive(Resource)]
pub struct MouseCaptured(pub bool);

#[derive(Component)]
pub struct CameraRig {
    pub yaw: f32,
    pub pitch: f32,
    pub current_dip: f32,
    pub smoothed_y: f32,
}

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            current_dip: 0.0,
            smoothed_y: f32::NAN, // initialised to the body on the first follow frame
        }
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(MouseCaptured(true));
        app.add_systems(Startup, (spawn_camera, grab_cursor));
        app.add_systems(
            Update,
            (
                cursor_control,
                mouse_look.run_if(|c: Res<MouseCaptured>| c.0),
                camera_landing_dip,
                follow_player,
            )
                .chain(),
        );
    }
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("CameraRig"),
        CameraRig::default(),
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::Y * 1.5, Vec3::Y),
    ));
}

fn grab_cursor(
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<MouseCaptured>,
) {
    set_cursor(&mut cursor, &mut captured, true);
}

/// ESC frees the cursor (second ESC quits); a left click recaptures it.
fn cursor_control(
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>,
    mut captured: ResMut<MouseCaptured>,
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
    captured: &mut MouseCaptured,
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

fn mouse_look(motion: Res<AccumulatedMouseMotion>, mut rig: Single<&mut CameraRig>) {
    let delta = motion.delta;
    if delta == Vec2::ZERO {
        return;
    }
    rig.yaw -= delta.x * MOUSE_SENSITIVITY;
    rig.pitch = (rig.pitch - delta.y * MOUSE_SENSITIVITY).clamp(PITCH_MIN, PITCH_MAX);
}

/// Add a downward dip when the player lands (Fall → Walk/Sprint).
///
/// Intentionally scoped to `Player`, not `Actor`: the camera always follows
/// the one local human-controlled entity, which stays singular even once
/// other `Actor`-tagged entities (NPCs, remote players) exist — see
/// `docs/architecture/rationale/multi-actor-dispatch.md`.
fn camera_landing_dip(
    player: Single<&LocomotionState, With<Player>>,
    mut rig: Single<&mut CameraRig>,
    mut prev: Local<Option<LocomotionState>>,
) {
    let current = **player;
    if let Some(old) = *prev
        && old == LocomotionState::Fall
        && (current == LocomotionState::Walk || current == LocomotionState::Sprint)
    {
        rig.current_dip += LANDING_DIP_INTENSITY;
    }
    *prev = Some(current);
}

type FollowFilter = (With<Player>, Without<CameraRig>);

/// Intentionally scoped to `Player`, not `Actor` — see `camera_landing_dip`.
fn follow_player(
    player: Single<(Entity, &Transform), FollowFilter>,
    mut cam: Single<(&mut Transform, &mut CameraRig)>,
    spatial: SpatialQuery,
    time: Res<Time>,
) {
    let (player_entity, player_transform) = *player;
    let (cam_transform, rig) = &mut *cam;
    let body = player_transform.translation;
    let dt = time.delta_secs();

    // Recover the landing dip, then smooth the pivot Y (handles stairs/steps).
    rig.current_dip = lerp(rig.current_dip, 0.0, LANDING_DIP_RECOVERY * dt);
    let target_y = body.y + LENS_HEIGHT - rig.current_dip;
    if rig.smoothed_y.is_nan() {
        rig.smoothed_y = target_y;
    } else {
        rig.smoothed_y = lerp(rig.smoothed_y, target_y, FOLLOW_LERP_Y * dt);
    }
    let pivot = Vec3::new(body.x, rig.smoothed_y, body.z);

    let rot = Quat::from_rotation_y(rig.yaw) * Quat::from_rotation_x(rig.pitch);
    let dir = rot * Vec3::Z;

    // Spring arm: pull the camera in if the boom would clip world geometry.
    let mut length = SPRING_LENGTH;
    if let Ok(boom_dir) = Dir3::new(dir) {
        let filter = SpatialQueryFilter::from_excluded_entities([player_entity]);
        if let Some(hit) = spatial.cast_shape(
            &Collider::sphere(SPRING_PROBE_RADIUS),
            pivot,
            Quat::IDENTITY,
            boom_dir,
            &ShapeCastConfig::from_max_distance(SPRING_LENGTH),
            &filter,
        ) {
            length = (hit.distance - SPRING_MARGIN).clamp(0.0, SPRING_LENGTH);
        }
    }

    cam_transform.translation = pivot + dir * length;
    cam_transform.look_at(pivot, Vec3::Y);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
