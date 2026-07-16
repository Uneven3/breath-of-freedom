//! Orbit follow camera.
//!
//! Presentation-only camera: follows the local actor using the control
//! orientation published by Input, with a landing dip and collision spring arm.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::input::InputSet;
use crate::input::frame::ControlOrientation;
use crate::movement::Player;
use crate::movement::state::LocomotionState;
use crate::visuals::PlayerVisual;

const SPRING_LENGTH: f32 = 6.5;
const LENS_HEIGHT: f32 = 1.5;
/// Aim mode (bow drawn): tighter boom, over-the-shoulder offset, and how fast
/// the camera blends in/out of it.
const AIM_SPRING_LENGTH: f32 = 2.2;
const AIM_SHOULDER_OFFSET: f32 = 0.55;
const AIM_BLEND_PER_SEC: f32 = 10.0;
const LANDING_DIP_INTENSITY: f32 = 0.5;
const LANDING_DIP_RECOVERY: f32 = 8.0;
const FOLLOW_LERP_Y: f32 = 15.0;
/// Keep the camera this far off a surface it would otherwise clip into.
const SPRING_MARGIN: f32 = 0.2;
const SPRING_PROBE_RADIUS: f32 = 0.2;
/// Fade out the player model if the camera is closer than this to prevent clipping.
const FIRST_PERSON_THRESHOLD: f32 = 0.8;

#[derive(Component)]
pub struct CameraRig {
    pub current_dip: f32,
    pub smoothed_y: f32,
    /// 0 = orbit camera, 1 = aim camera; eased toward the player's
    /// `CombatState::Aiming`.
    pub aim_blend: f32,
}

/// Trauma-based screen shake: writers add trauma (0..=1), the offset applied
/// is `trauma²` (small hits barely register, big ones slam), decaying on
/// *real* time so it works through hitstop. Written by
/// `presentation::juice::player_hit_feedback`.
#[derive(Resource, Default)]
pub struct CameraShake {
    trauma: f32,
}

impl CameraShake {
    pub fn add_trauma(&mut self, amount: f32) {
        self.trauma = (self.trauma + amount).min(1.0);
    }
}

const SHAKE_DECAY_PER_SEC: f32 = 2.2;
const SHAKE_MAX_OFFSET: f32 = 0.25;
const SHAKE_MAX_ROLL: f32 = 0.03;

impl Default for CameraRig {
    fn default() -> Self {
        Self {
            current_dip: 0.0,
            smoothed_y: f32::NAN, // initialised to the body on the first follow frame
            aim_blend: 0.0,
        }
    }
}

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraShake>();
        app.add_systems(Startup, (spawn_camera, spawn_crosshair));
        app.add_systems(
            Update,
            (
                camera_landing_dip,
                follow_player,
                apply_camera_shake,
                toggle_crosshair,
            )
                .chain()
                .after(InputSet::UpdateOrientation),
        );
    }
}

/// Center-screen dot, visible only while the aim camera is blended in.
#[derive(Component)]
struct Crosshair;

fn spawn_crosshair(mut commands: Commands) {
    commands.spawn((
        Crosshair,
        Name::new("Crosshair"),
        Node {
            position_type: PositionType::Absolute,
            left: Val::Percent(50.0),
            top: Val::Percent(50.0),
            width: Val::Px(6.0),
            height: Val::Px(6.0),
            margin: UiRect {
                left: Val::Px(-3.0),
                top: Val::Px(-3.0),
                ..default()
            },
            border_radius: BorderRadius::MAX,
            ..default()
        },
        BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.9)),
        Visibility::Hidden,
    ));
}

fn toggle_crosshair(
    rig: Single<&CameraRig>,
    mut crosshair: Single<&mut Visibility, With<Crosshair>>,
) {
    **crosshair = if rig.aim_blend > 0.5 {
        Visibility::Inherited
    } else {
        Visibility::Hidden
    };
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Name::new("CameraRig"),
        CameraRig::default(),
        Camera3d::default(),
        Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::Y * 1.5, Vec3::Y),
    ));
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
    player: Single<
        (
            Entity,
            &Transform,
            &ControlOrientation,
            Option<&crate::combat::state::CombatState>,
        ),
        FollowFilter,
    >,
    mut cam: Single<(&mut Transform, &mut CameraRig)>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut player_vis: Query<&mut Visibility, With<PlayerVisual>>,
) {
    let (player_entity, player_transform, orientation, combat_state) = *player;
    let (cam_transform, rig) = &mut *cam;
    let body = player_transform.translation;
    let dt = time.delta_secs();

    // Recover the landing dip, then smooth the pivot Y (handles stairs/steps).
    rig.current_dip = lerp(rig.current_dip, 0.0, LANDING_DIP_RECOVERY * dt);

    // Blend toward the aim camera while the bow is drawn.
    let aiming = matches!(
        combat_state,
        Some(crate::combat::state::CombatState::Aiming)
    );
    let aim_target = if aiming { 1.0 } else { 0.0 };
    rig.aim_blend = lerp(rig.aim_blend, aim_target, AIM_BLEND_PER_SEC * dt);

    // Dynamically cap lens height if there is a low ceiling directly above the player center.
    // This prevents the camera pivot from clipping inside low ceilings or stairs.
    let mut effective_lens_height = LENS_HEIGHT;
    let up_dir = Dir3::Y;
    let filter = SpatialQueryFilter::from_excluded_entities([player_entity]);
    if let Some(hit) = spatial.cast_ray(body, up_dir, LENS_HEIGHT, true, &filter) {
        effective_lens_height = (hit.distance - SPRING_MARGIN).max(0.2);
    }

    let target_y = body.y + effective_lens_height - rig.current_dip;
    if rig.smoothed_y.is_nan() {
        rig.smoothed_y = target_y;
    } else {
        rig.smoothed_y = lerp(rig.smoothed_y, target_y, FOLLOW_LERP_Y * dt);
    }
    let rot = Quat::from_rotation_y(orientation.yaw) * Quat::from_rotation_x(orientation.pitch);
    let dir = rot * Vec3::Z;

    // Aim mode: shift the pivot over the right shoulder so the body doesn't
    // block the shot line.
    let shoulder = rot * Vec3::X * (AIM_SHOULDER_OFFSET * rig.aim_blend);
    let pivot = Vec3::new(body.x, rig.smoothed_y, body.z) + shoulder;

    // Spring arm: pull the camera in if the boom would clip world geometry.
    let base_length = lerp(SPRING_LENGTH, AIM_SPRING_LENGTH, rig.aim_blend);
    let mut length = base_length;
    if let Ok(boom_dir) = Dir3::new(dir)
        && let Some(hit) = spatial.cast_shape(
            &Collider::sphere(SPRING_PROBE_RADIUS),
            pivot,
            Quat::IDENTITY,
            boom_dir,
            &ShapeCastConfig::from_max_distance(base_length),
            &filter,
        )
    {
        length = (hit.distance - SPRING_MARGIN).clamp(0.0, base_length);
    }

    // Hide the player visual if the camera is zoomed too close to prevent rendering internal faces.
    if let Ok(mut vis) = player_vis.single_mut() {
        if length < FIRST_PERSON_THRESHOLD {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Inherited;
        }
    }

    cam_transform.translation = pivot + dir * length;
    // Set camera rotation directly to ControlOrientation rot. This is mathematically
    // identical to look_at(pivot) but avoids NaN / freeze singularities when length == 0.0.
    cam_transform.rotation = rot;
}

/// Runs after `follow_player` (which fully rewrites the camera transform each
/// frame, so the shake offset never accumulates): jitter position and roll by
/// `trauma²` using cheap incommensurate sines as noise.
fn apply_camera_shake(
    real: Res<Time<Real>>,
    mut shake: ResMut<CameraShake>,
    mut cam: Single<&mut Transform, With<CameraRig>>,
) {
    if shake.trauma <= 0.0 {
        return;
    }
    shake.trauma = (shake.trauma - SHAKE_DECAY_PER_SEC * real.delta_secs()).max(0.0);
    let strength = shake.trauma * shake.trauma;
    if strength <= f32::EPSILON {
        return;
    }
    let t = real.elapsed_secs();
    let offset = Vec3::new((t * 61.7).sin(), (t * 73.3).sin(), (t * 53.9).sin())
        * (SHAKE_MAX_OFFSET * strength);
    let roll = (t * 97.1).sin() * SHAKE_MAX_ROLL * strength;
    cam.translation += offset;
    cam.rotation *= Quat::from_rotation_z(roll);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}
