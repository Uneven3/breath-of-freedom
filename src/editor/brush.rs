//! The brush vocabulary and the stroke that applies it.
//!
//! Every brush is one arm of [`BrushKind`] and one method on
//! [`Terrain`] — this file only turns "which button is down, under which
//! cursor" into a call. That is why a seventh brush is a row here, not a system.
//!
//! Rates below are *per second at strength 1.0*, integrated by `dt`, so the feel
//! is frame-rate independent and the strength knob scales all of them alike.

use avian3d::prelude::{SpatialQuery, SpatialQueryFilter};
use bevy::color::palettes::css;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;

use super::{EditorTool, SculptFocus, StrokeAnchor, ToolLayer, history::SculptHistory};
use crate::camera::CameraRig;
use crate::input::ModalInputFocus;
use crate::world::{GameLayer, Terrain, TerrainAccess};

/// Metres of height change per second at the brush centre while raising.
const SCULPT_RATE: f32 = 1.5;
/// How fast smoothing relaxes toward the neighbour average, per second.
const SMOOTH_RATE: f32 = 6.0;
/// How fast flatten/ramp/terrace travel toward their target, per second.
const FLATTEN_RATE: f32 = 3.0;
const RAMP_RATE: f32 = 3.0;
const TERRACE_RATE: f32 = 2.0;
/// Metres of noise amplitude added per second.
const NOISE_RATE: f32 = 0.6;
/// Height of one terrace shelf, in metres. Roughly a body height, so a terrace
/// edge reads as something to mantle rather than a step to walk up.
const TERRACE_STEP: f32 = 2.0;
/// Fixed seed: the noise pattern is a property of the *world position*, so the
/// same spot wrinkles the same way every session and every stroke deepens the
/// wrinkles already there instead of boiling them.
const NOISE_SEED: u32 = 0x5eed_1234;

/// How far the pick ray reaches into the world.
const PICK_DISTANCE: f32 = 1_000.0;
/// Lift the brush ring just off the surface so it does not z-fight the ground.
const GIZMO_LIFT: f32 = 0.05;

/// What the brush does where it bites. The set is chosen for *building a world
/// worth walking through*, not just for moving dirt: shape it, calm it, level
/// somewhere to stand, connect two levels so they are walkable, rough it up so
/// it does not read as CAD, and shelve it so a slope becomes a place.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum BrushKind {
    Sculpt,
    Smooth,
    Flatten,
    Ramp,
    Noise,
    Terrace,
}

impl BrushKind {
    pub const ALL: [BrushKind; 6] = [
        BrushKind::Sculpt,
        BrushKind::Smooth,
        BrushKind::Flatten,
        BrushKind::Ramp,
        BrushKind::Noise,
        BrushKind::Terrace,
    ];

    /// The number key that selects it — its index in [`BrushKind::ALL`], 1-based.
    pub fn key(self) -> KeyCode {
        match self {
            BrushKind::Sculpt => KeyCode::Digit1,
            BrushKind::Smooth => KeyCode::Digit2,
            BrushKind::Flatten => KeyCode::Digit3,
            BrushKind::Ramp => KeyCode::Digit4,
            BrushKind::Noise => KeyCode::Digit5,
            BrushKind::Terrace => KeyCode::Digit6,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BrushKind::Sculpt => "Elevar",
            BrushKind::Smooth => "Suavizar",
            BrushKind::Flatten => "Aplanar",
            BrushKind::Ramp => "Rampa",
            BrushKind::Noise => "Rugosidad",
            BrushKind::Terrace => "Terrazas",
        }
    }

    /// What the mouse buttons mean for this brush, for the HUD.
    pub fn hint(self) -> &'static str {
        match self {
            BrushKind::Sculpt => "LMB sube · RMB baja",
            BrushKind::Smooth => "LMB relaja hacia el promedio",
            BrushKind::Flatten => "LMB nivela al alto donde empezaste",
            BrushKind::Ramp => "LMB arrastra: pendiente recta del inicio al cursor",
            BrushKind::Noise => "LMB agrega rugosidad orgánica",
            BrushKind::Terrace => "LMB escalona en repisas de 2 m",
        }
    }
}

/// What this frame's buttons ask for. Smoothing is always on the middle button
/// no matter which brush is selected: it is the eraser of terrain sculpting, and
/// reaching for it should never cost a mode switch.
enum Stroke {
    Apply,
    Invert,
    Smooth,
}

/// ¿El click de este frame es del pincel?
///
/// El puntero se comparte: el hub F1 y el inventario se operan clickeando, y
/// todos leen el mismo `ButtonInput` crudo. Sin esta pregunta el pincel contesta
/// sus clicks — clickear un botón del hub cavaba un cráter en el suelo de atrás.
///
/// **Sólo mira paneles.** Hasta el 2026-08-22 también distinguía "la freecam
/// tiene el puntero", porque el botón derecho significaba *mirar* y *bajar el
/// terreno* a la vez: girar la cámara molía un cráter en el punto donde estaba
/// el cursor. Eso ya no puede pasar — los botones de cámara y los del pincel son
/// disjuntos (`MAP_EDITOR.md`), que es el arreglo de verdad y no un arbitraje.
pub(super) fn pointer_is_ours(
    focus: &ModalInputFocus,
    owner: &Query<Entity, With<SculptFocus>>,
    rig: &Query<Entity, With<CameraRig>>,
) -> bool {
    let Ok(owner) = owner.single() else {
        return false;
    };
    let rig = rig.single().ok();
    focus.owns_pointer(owner, |holder| Some(holder) == rig)
}

/// While held: apply the selected brush under the cursor.
#[allow(clippy::too_many_arguments)]
pub(super) fn sculpt_terrain(
    mut tool: ResMut<EditorTool>,
    mut history: ResMut<SculptHistory>,
    buttons: Res<ButtonInput<MouseButton>>,
    keys: Res<ButtonInput<KeyCode>>,
    time: Res<Time>,
    spatial: SpatialQuery,
    focus: Res<ModalInputFocus>,
    owner: Query<Entity, With<SculptFocus>>,
    rig: Query<Entity, With<CameraRig>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut terrain: Query<(Entity, &mut Terrain)>,
) {
    let Ok((entity, mut terrain)) = terrain.single_mut() else {
        return;
    };
    // Los tres gestos del modo esculpir de Blender, sobre **un solo botón**:
    // aplicar, invertir con Ctrl, suavizar con Shift. Que el pincel viva entero
    // en el izquierdo es lo que deja el derecho y el central libres para la
    // cámara — antes RMB era "invertir" *y* "girar", así que girar la vista
    // hundía el terreno bajo el cursor.
    let stroke = if !tool.active
        || tool.layer != ToolLayer::Relief
        || !pointer_is_ours(&focus, &owner, &rig)
        || !buttons.pressed(MouseButton::Left)
    {
        None
    } else if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        Some(Stroke::Smooth)
    } else if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        Some(Stroke::Invert)
    } else {
        Some(Stroke::Apply)
    };
    let Some(stroke) = stroke else {
        // Buttons up — or the pointer went to a panel, which ends the stroke
        // rather than pausing it: what you drew before reaching for the hub is
        // one finished thing you did.
        tool.anchor = None;
        history.end_stroke(&terrain);
        return;
    };
    let (Ok(window), Ok((camera, camera_transform))) = (window.single(), camera.single()) else {
        return;
    };
    let Some(hit) = cursor_terrain_hit(&spatial, window, camera, camera_transform, |hit| {
        hit == entity
    }) else {
        return;
    };

    // The snapshot has to predate the first edit of the stroke, so it is taken
    // here (first frame with a hit) rather than on the button-down edge, which
    // may have landed off the terrain.
    history.begin_stroke(&terrain);
    let anchor = *tool.anchor.get_or_insert(StrokeAnchor {
        xz: Vec2::new(hit.x, hit.z),
        height: hit.y,
    });

    let center = Vec2::new(hit.x, hit.z);
    let radius = tool.radius;
    let step = tool.strength * time.delta_secs();
    match (stroke, tool.brush) {
        (Stroke::Smooth, _) | (_, BrushKind::Smooth) => {
            terrain.smooth_area(center, radius, (SMOOTH_RATE * step).min(1.0));
        }
        (Stroke::Apply, BrushKind::Sculpt) => {
            terrain.raise_area(center, radius, SCULPT_RATE * step);
        }
        (Stroke::Invert, BrushKind::Sculpt) => {
            terrain.raise_area(center, radius, -SCULPT_RATE * step);
        }
        (_, BrushKind::Flatten) => {
            terrain.flatten_area(
                center,
                radius,
                anchor.height,
                (FLATTEN_RATE * step).min(1.0),
            );
        }
        (_, BrushKind::Ramp) => {
            terrain.ramp_area(
                anchor.xz,
                anchor.height,
                center,
                hit.y,
                radius,
                (RAMP_RATE * step).min(1.0),
            );
        }
        (_, BrushKind::Noise) => {
            terrain.noise_area(center, radius, NOISE_RATE * step, NOISE_SEED);
        }
        (_, BrushKind::Terrace) => {
            terrain.terrace_area(center, radius, TERRACE_STEP, (TERRACE_RATE * step).min(1.0));
        }
    }
}

/// The world point under the cursor, but only when the ray lands on the terrain
/// (not on a prop in front of it).
pub(super) fn cursor_terrain_hit(
    spatial: &SpatialQuery,
    window: &Window,
    camera: &Camera,
    camera_transform: &GlobalTransform,
    accepts: impl FnOnce(Entity) -> bool,
) -> Option<Vec3> {
    let cursor = window.cursor_position()?;
    let ray = camera.viewport_to_world(camera_transform, cursor).ok()?;
    let filter = SpatialQueryFilter::from_mask(GameLayer::Default);
    let hit = spatial.cast_ray(ray.origin, ray.direction, PICK_DISTANCE, true, &filter)?;
    if !accepts(hit.entity) {
        return None;
    }
    Some(ray.origin + ray.direction * hit.distance)
}

/// A flat ring on the ground showing where and how wide the brush bites — plus,
/// mid-ramp, the line the slope is being laid along, because a ramp you cannot
/// see the ends of is a ramp you cannot aim.
#[allow(clippy::too_many_arguments)]
pub(super) fn draw_brush_gizmo(
    tool: Res<EditorTool>,
    spatial: SpatialQuery,
    focus: Res<ModalInputFocus>,
    owner: Query<Entity, With<SculptFocus>>,
    rig: Query<Entity, With<CameraRig>>,
    window: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &GlobalTransform), With<Camera3d>>,
    terrain: TerrainAccess,
    mut gizmos: Gizmos,
) {
    // Hidden exactly when the brush is deaf, so the ring is an honest read of
    // whether the next click will bite.
    if !tool.active || !pointer_is_ours(&focus, &owner, &rig) {
        return;
    }
    let (Ok(window), Ok((camera, camera_transform))) = (window.single(), camera.single()) else {
        return;
    };
    let Some(hit) = cursor_terrain_hit(&spatial, window, camera, camera_transform, |hit| {
        terrain.contains(hit)
    }) else {
        return;
    };
    let lift = Vec3::Y * GIZMO_LIFT;
    let flat = Quat::from_rotation_arc(Vec3::Z, Vec3::Y);
    // The ring's colour says which layer the click will land on. Without it the
    // two layers look identical under the cursor while doing entirely different
    // things to the level — and painting is silent on flat ground, so there would
    // be no other tell until you saved.
    let ring = match tool.layer {
        ToolLayer::Relief => css::YELLOW,
        ToolLayer::Meaning => css::AQUA,
        ToolLayer::Instances => css::MAGENTA,
    };
    gizmos.circle(Isometry3d::new(hit + lift, flat), tool.radius, ring);

    if tool.layer == ToolLayer::Relief
        && tool.brush == BrushKind::Ramp
        && let Some(anchor) = tool.anchor
    {
        let start = Vec3::new(anchor.xz.x, anchor.height, anchor.xz.y) + lift;
        gizmos.circle(Isometry3d::new(start, flat), tool.radius, css::ORANGE);
        gizmos.line(start, hit + lift, css::ORANGE);
    }
}
