//! Orbit follow camera.
//!
//! Presentation-only camera: follows the local actor using the control
//! orientation published by Input, with a landing dip and collision spring arm.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::input::frame::ControlOrientation;
use crate::movement::Player;
use crate::movement::state::LocomotionState;
use crate::visuals::PlayerVisual;

use crate::combat::motors::aim::{AIM_PIVOT_HEIGHT, AIM_SHOULDER_OFFSET};

const SPRING_LENGTH: f32 = 6.5;
/// Free-orbit pivot height, camera feel only. While aiming the pivot blends
/// down to Combat's `AIM_PIVOT_HEIGHT` (§20: the aim line is simulation) so
/// at full blend the crosshair ray and the aim ray are the same line.
const LENS_HEIGHT: f32 = 1.5;
/// Aim mode (bow drawn): tighter boom, shoulder offset shared with Combat
/// (see `LENS_HEIGHT`), and how fast the camera blends in/out of it.
const AIM_SPRING_LENGTH: f32 = 3.6;
const AIM_BLEND_PER_SEC: f32 = 10.0;
/// FOV eases toward its aim/draw target so firing (draw factor snapping to
/// zero) doesn't pop the lens.
const FOV_LERP_PER_SEC: f32 = 12.0;
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
        // Orientation resolves in PreUpdate (see InputPlugin), so Update-time
        // camera systems always read the current frame's orientation.
        app.add_systems(
            Update,
            (
                camera_landing_dip,
                follow_player,
                apply_camera_shake,
                toggle_crosshair,
            )
                .chain(),
        );
    }
}

/// Center-screen dot, visible only while the aim camera is blended in.
#[derive(Component)]
pub struct Crosshair;

#[derive(Component)]
pub struct CrosshairRing;

fn spawn_crosshair(mut commands: Commands) {
    commands
        .spawn((
            Crosshair,
            Name::new("Crosshair"),
            Node {
                position_type: PositionType::Absolute,
                left: Val::Percent(50.0),
                top: Val::Percent(50.0),
                width: Val::Px(0.0),
                height: Val::Px(0.0),
                ..default()
            },
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            // Center dot (4px solid white)
            parent.spawn((
                Name::new("CrosshairDot"),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(4.0),
                    height: Val::Px(4.0),
                    margin: UiRect {
                        left: Val::Px(-2.0),
                        top: Val::Px(-2.0),
                        ..default()
                    },
                    border_radius: BorderRadius::MAX,
                    ..default()
                },
                BackgroundColor(Color::srgba(1.0, 1.0, 1.0, 0.95)),
            ));

            // Outer accuracy ring (shrinks on draw/charge)
            parent.spawn((
                CrosshairRing,
                Name::new("CrosshairRing"),
                Node {
                    position_type: PositionType::Absolute,
                    width: Val::Px(64.0),
                    height: Val::Px(64.0),
                    margin: UiRect {
                        left: Val::Px(-32.0),
                        top: Val::Px(-32.0),
                        ..default()
                    },
                    border_radius: BorderRadius::MAX,
                    border: UiRect::all(Val::Px(1.5)),
                    ..default()
                },
                BorderColor::all(Color::srgba(1.0, 1.0, 1.0, 0.75)),
            ));
        });
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
#[allow(clippy::type_complexity)]
fn follow_player(
    player: Single<
        (
            Entity,
            &Transform,
            &ControlOrientation,
            Option<&crate::combat::state::CombatState>,
            Option<&crate::combat::motors::aim::DrawStrength>,
        ),
        FollowFilter,
    >,
    mut cam: Single<(&mut Transform, &mut CameraRig, &mut Projection)>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut player_vis: Query<&mut Visibility, With<PlayerVisual>>,
) {
    let (player_entity, player_transform, orientation, combat_state, draw_strength) = *player;
    let (cam_transform, rig, proj) = &mut *cam;
    let body = player_transform.translation;
    let dt = time.delta_secs();

    // Adjust perspective camera field-of-view dynamically to simulate focus and weight.
    if let Projection::Perspective(ref mut persp) = **proj {
        let default_fov = std::f32::consts::FRAC_PI_4; // ~45 deg half fov (90 deg full)
        let draw_factor = draw_strength.map_or(0.0, |d| d.factor);
        let target_fov = default_fov - (0.12 * rig.aim_blend) - (0.16 * draw_factor);
        persp.fov = lerp(persp.fov, target_fov, FOV_LERP_PER_SEC * dt);
    }

    // Recover the landing dip, then smooth the pivot Y (handles stairs/steps).
    rig.current_dip = lerp(rig.current_dip, 0.0, LANDING_DIP_RECOVERY * dt);

    // Blend toward the aim camera while the bow is drawn.
    let aiming = matches!(
        combat_state,
        Some(crate::combat::state::CombatState::Aiming)
    );
    let aim_target = if aiming { 1.0 } else { 0.0 };
    rig.aim_blend = lerp(rig.aim_blend, aim_target, AIM_BLEND_PER_SEC * dt);

    // Blend the pivot down to Combat's aim pivot while aiming, so the
    // crosshair ray coincides with the arrow's aim ray at full blend.
    let blended_lens_height = lerp(LENS_HEIGHT, AIM_PIVOT_HEIGHT, rig.aim_blend);

    // Dynamically cap lens height if there is a low ceiling directly above the player center.
    // This prevents the camera pivot from clipping inside low ceilings or stairs.
    let mut effective_lens_height = blended_lens_height;
    let up_dir = Dir3::Y;
    let filter = SpatialQueryFilter::from_excluded_entities([player_entity]);
    if let Some(hit) = spatial.cast_ray(body, up_dir, blended_lens_height, true, &filter) {
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
