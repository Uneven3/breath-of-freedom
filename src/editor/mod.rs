//! In-engine authoring tools. First up: the terrain sculpt brush; future tools
//! (semantic paint, instance placement) join here.
//!
//! Dev-only, gated behind **F5**. While active the tool holds modal input focus,
//! which by itself frees and shows the cursor, stops the camera, and suppresses
//! gameplay input — so the mouse becomes a brush without any per-system gating.
//! The tool reads input and mutates [`Terrain`] (the data); the collider
//! (`world::terrain`) and the mesh (`visuals::terrain`) regenerate from
//! `Changed<Terrain>`. The editor never decides *how* the grid changes — that
//! lives on `Terrain` — only *where and when*.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::color::palettes::css;
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use crate::input::ModalInputFocusRequest;
use crate::world::{GameLayer, Terrain};

/// Metres of height change per second at the brush center while raising/lowering.
const RAISE_RATE: f32 = 4.0;
/// How fast the smooth brush relaxes toward the neighbour average, per second.
const SMOOTH_RATE: f32 = 8.0;
/// Brush radius bounds (metres) for the scroll-wheel resize.
const MIN_RADIUS: f32 = 2.0;
const MAX_RADIUS: f32 = 40.0;
/// Radius change per scroll notch.
const RADIUS_STEP: f32 = 1.5;
/// How far the pick ray reaches into the world.
const PICK_DISTANCE: f32 = 1_000.0;
/// Lift the brush ring just off the surface so it does not z-fight the ground.
const GIZMO_LIFT: f32 = 0.05;

/// Terrain sculpt tool state. Off until F5; radius is scroll-adjustable.
#[derive(Resource)]
struct SculptTool {
    active: bool,
    radius: f32,
}

impl Default for SculptTool {
    fn default() -> Self {
        Self {
            active: false,
            radius: 6.0,
        }
    }
}

/// Marker entity that owns modal input focus while sculpting.
#[derive(Component)]
struct SculptFocus;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SculptTool>();
        app.add_systems(Startup, spawn_focus_owner);
        app.add_systems(
            Update,
            (
                toggle_sculpt,
                adjust_brush,
                sculpt_terrain,
                draw_brush_gizmo,
            ),
        );
    }
}

fn spawn_focus_owner(mut commands: Commands) {
    commands.spawn((Name::new("SculptFocus"), SculptFocus));
}

/// F5 flips the mode, acquiring/releasing modal focus — which is what actually
/// frees the cursor and quiets gameplay input.
fn toggle_sculpt(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<SculptTool>,
    owner: Query<Entity, With<SculptFocus>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if !keys.just_pressed(KeyCode::F5) {
        return;
    }
    let Ok(owner) = owner.single() else {
        return;
    };
    tool.active = !tool.active;
    focus.write(if tool.active {
        ModalInputFocusRequest::Acquire(owner)
    } else {
        ModalInputFocusRequest::Release(owner)
    });
    info!(
        "[editor] terrain sculpt: {}",
        if tool.active {
            "ON (LMB raise / RMB lower / MMB smooth / scroll = size)"
        } else {
            "OFF"
        }
    );
}

/// Scroll to resize the brush while sculpting.
fn adjust_brush(mut tool: ResMut<SculptTool>, scroll: Res<AccumulatedMouseScroll>) {
    if !tool.active || scroll.delta.y == 0.0 {
        return;
    }
    tool.radius = (tool.radius + scroll.delta.y * RADIUS_STEP).clamp(MIN_RADIUS, MAX_RADIUS);
}

/// The world point under the cursor, but only when the ray lands on the terrain
/// (not on a prop in front of it).
fn cursor_terrain_hit(
    spatial: &SpatialQuery,
    window: &Window,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    terrain: Entity,
) -> Option<Vec3> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let filter = SpatialQueryFilter::from_mask(GameLayer::Default);
    let hit = spatial.cast_ray(ray.origin, ray.direction, PICK_DISTANCE, true, &filter)?;
    if hit.entity != terrain {
        return None;
    }
    Some(ray.origin + ray.direction * hit.distance)
}

/// While held: LMB raises, RMB lowers, MMB smooths the terrain under the cursor.
fn sculpt_terrain(
    tool: Res<SculptTool>,
    buttons: Res<ButtonInput<MouseButton>>,
    time: Res<Time>,
    spatial: SpatialQuery,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut terrain: Query<(Entity, &mut Terrain)>,
) {
    if !tool.active {
        return;
    }
    let raise = buttons.pressed(MouseButton::Left);
    let lower = buttons.pressed(MouseButton::Right);
    let smooth = buttons.pressed(MouseButton::Middle);
    if !(raise || lower || smooth) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform)), Ok((entity, mut terrain))) =
        (window.single(), camera.single(), terrain.single_mut())
    else {
        return;
    };
    let Some(hit) = cursor_terrain_hit(&spatial, window, camera, camera_transform, entity) else {
        return;
    };
    let center = Vec2::new(hit.x, hit.z);
    let dt = time.delta_secs();
    if smooth {
        terrain.smooth_area(center, tool.radius, (SMOOTH_RATE * dt).min(1.0));
    } else if raise ^ lower {
        let direction = if raise { 1.0 } else { -1.0 };
        terrain.raise_area(center, tool.radius, direction * RAISE_RATE * dt);
    }
}

/// A flat ring on the ground showing where and how wide the brush bites.
fn draw_brush_gizmo(
    tool: Res<SculptTool>,
    spatial: SpatialQuery,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    terrain: Query<Entity, With<Terrain>>,
    mut gizmos: Gizmos,
) {
    if !tool.active {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform)), Ok(entity)) =
        (window.single(), camera.single(), terrain.single())
    else {
        return;
    };
    let Some(hit) = cursor_terrain_hit(&spatial, window, camera, camera_transform, entity) else {
        return;
    };
    let isometry = Isometry3d::new(
        hit + Vec3::Y * GIZMO_LIFT,
        Quat::from_rotation_arc(Vec3::Z, Vec3::Y),
    );
    gizmos.circle(isometry, tool.radius, css::YELLOW);
}
