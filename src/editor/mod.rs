//! In-engine authoring tools. First up: the terrain sculpt brushes; future tools
//! (semantic paint, instance placement) join here.
//!
//! Dev-only, gated behind **F5** in scenes that declare terrain authoring. While
//! active the tool holds modal input focus,
//! which by itself frees and shows the cursor, stops the camera, and suppresses
//! gameplay input — so the mouse becomes a brush without any per-system gating.
//!
//! The split that keeps this small: the editor decides **where and when** a
//! brush fires; [`Terrain`](crate::world::Terrain) owns **how** the grid
//! changes. Adding a brush is a method there plus a row in [`BrushKind`] — never
//! a new system. The collider (`world::terrain`) and the mesh
//! (`visuals::terrain`) regenerate from `Changed<Terrain>`, so nothing here
//! touches physics or rendering.
//!
//! - `brush` — the relief brushes and the stroke that applies them.
//! - `paint` — the semantic brush: what the ground *is*, cell by cell.
//! - `history` — undo/redo, one entry per stroke, covering both layers.
//! - `persist` — el nivel en disco, y `repair` dejarlo recorrible.
//! - `hud` — what is on screen while authoring.

mod brush;
mod history;
mod hud;
mod instances;
mod paint;
mod persist;
mod repair;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::input::ModalInputFocusRequest;
use crate::scene::{AppState, scene_allows};
use crate::world::{PropKind, TerrainAccess, TerrainKind};
use brush::BrushKind;
use history::SculptHistory;

/// Brush radius bounds (metres); the floor moves with `CELLS` — see MAP_EDITOR.md.
const MIN_RADIUS: f32 = 3.0;
const MAX_RADIUS: f32 = 40.0;
/// Radius change per scroll notch.
const RADIUS_STEP: f32 = 1.5;
/// Strength bounds and step. Strength scales every brush's per-second rate, so
/// one knob covers "shape a mountain" and "nudge a footpath".
const MIN_STRENGTH: f32 = 0.1;
const MAX_STRENGTH: f32 = 3.0;
const STRENGTH_STEP: f32 = 0.1;

/// Which channel of the level the tool is authoring.
///
/// A map is not one kind of data, so the tool is not one kind of brush. Relief
/// is the shape of the ground; meaning is what that ground *is* — and the two
/// live on different grids, undo differently and answer to different keys. They
/// are layers of the same tool rather than two tools because the author is doing
/// one job: building a place.
///
/// The order here is the order they were built and the order they compose:
/// shape the hill, then say the hill is rock. Instance placement joins as a
/// third layer.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum ToolLayer {
    /// The height grid: the six sculpt brushes.
    Relief,
    /// The semantic grid: what each cell is made of.
    Meaning,
    /// Discrete props placed on top of the ground. No undo yet — see
    /// `instances::place_or_clear_instances`.
    Instances,
}

impl ToolLayer {
    pub fn label(self) -> &'static str {
        match self {
            ToolLayer::Relief => "Relieve",
            ToolLayer::Meaning => "Semántica",
            ToolLayer::Instances => "Instancias",
        }
    }

    fn next(self) -> Self {
        match self {
            ToolLayer::Relief => ToolLayer::Meaning,
            ToolLayer::Meaning => ToolLayer::Instances,
            ToolLayer::Instances => ToolLayer::Relief,
        }
    }
}

/// Authoring tool state: which layer, which brush within it, how wide, how hard,
/// and the anchor of the stroke in progress.
#[derive(Resource)]
pub(crate) struct EditorTool {
    pub active: bool,
    pub layer: ToolLayer,
    /// Shared by both layers: it is the same circle under the same cursor, and
    /// having it change size when you switch layers would be a surprise.
    pub radius: f32,
    /// Relief only. Painting has no rate to scale — see
    /// [`Terrain::paint_area`](crate::world::Terrain::paint_area).
    pub strength: f32,
    /// The selected relief brush.
    pub brush: BrushKind,
    /// The selected kind for the semantic brush.
    pub paint_kind: TerrainKind,
    /// The selected prop for the instance layer.
    pub prop_kind: PropKind,
    /// Where the current stroke started, in world XZ plus the ground height
    /// there at press time. `Flatten` levels to that height and `Ramp` runs from
    /// it — both need the value *before* the stroke starts changing it.
    pub anchor: Option<StrokeAnchor>,
    /// La subida empinada acumulada que el último trazo dejó bajo el cursor.
    ///
    /// El editor no impide pasarse: **lo muestra**. Un tramo empinado más alto
    /// que el alcance del mantle es terreno al que el jugador no puede llegar,
    /// y hasta ahora eso sólo se descubría jugando.
    pub worst_run_metres: f32,
}

/// The frozen start of a stroke.
#[derive(Clone, Copy)]
pub(crate) struct StrokeAnchor {
    pub xz: Vec2,
    pub height: f32,
}

impl Default for EditorTool {
    fn default() -> Self {
        Self {
            active: false,
            layer: ToolLayer::Relief,
            radius: 6.0,
            strength: 1.0,
            brush: BrushKind::Sculpt,
            paint_kind: TerrainKind::Rock,
            prop_kind: PropKind::ALL[0],
            anchor: None,
            worst_run_metres: 0.0,
        }
    }
}

/// Marker entity that owns modal input focus while sculpting.
#[derive(Component)]
struct SculptFocus;

pub struct EditorPlugin;

impl Plugin for EditorPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<EditorTool>();
        app.init_resource::<SculptHistory>();
        // The tool itself is infrastructure — its focus owner and HUD outlive any
        // scene (`crate::scene`); only the terrain it edits is scene content.
        app.add_systems(Startup, (spawn_focus_owner, hud::spawn_hud));
        app.add_systems(
            Update,
            (
                // Ordered: the toggle decides whether this frame sculpts at all,
                // and the knobs must settle before the stroke reads them.
                toggle_sculpt,
                switch_layer,
                select_brush,
                adjust_brush,
                // Only one of these does anything on a given frame: each gates
                // on the active layer, so the pointer is never claimed twice.
                brush::sculpt_terrain,
                paint::paint_terrain,
                instances::place_or_clear_instances,
                history::undo_redo,
                persist::save_or_reload,
                brush::draw_brush_gizmo,
                hud::update_hud,
            )
                .chain()
                // The map tool does not name scenes: the scene table declares
                // where a TerrainFile may be authored. The menu consequently
                // cannot sculpt either, because it declares no capabilities.
                .run_if(scene_allows(|tools| tools.terrain_editing)),
        );
        for id in crate::scene::SceneId::ALL {
            app.add_systems(OnExit(AppState::Scene(id)), leave_sculpting);
            app.add_systems(
                OnEnter(AppState::Scene(id)),
                open_world_lab.run_if(scene_allows(|tools| tools.terrain_editing)),
            );
        }
    }
}

/// Leaving a scene ends sculpting. Without this the tool would stay "on" across
/// the transition while its focus owner — infrastructure, so it does not
/// despawn — kept holding modal input, freezing the next scene's controls
/// against a terrain that no longer exists.
fn leave_sculpting(
    mut tool: ResMut<EditorTool>,
    mut history: ResMut<SculptHistory>,
    owner: Query<Entity, With<SculptFocus>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    history.clear();
    if !tool.active {
        return;
    }
    tool.active = false;
    tool.anchor = None;
    if let Ok(owner) = owner.single() {
        focus.write(ModalInputFocusRequest::Release(owner));
    }
}

/// Abre la escena con el pincel ya activo (`MAP_EDITOR.md`).
///
/// `OnEnter` y no `Startup`: el dueño del foco lo spawnea `Startup`, y
/// `leave_sculpting` apaga la herramienta al salir de cada escena. No toca el
/// foco de la cámara — `owns_pointer` ya acepta al rig como co-dueño.
fn open_world_lab(
    mut tool: ResMut<EditorTool>,
    owner: Query<Entity, With<SculptFocus>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    let Ok(owner) = owner.single() else {
        return;
    };
    if tool.active {
        return;
    }
    tool.active = true;
    tool.anchor = None;
    focus.write(ModalInputFocusRequest::Acquire(owner));
    info!("[editor] World Lab: abierto en modo edición (BOF_MODE=editor)");
}

fn spawn_focus_owner(mut commands: Commands) {
    commands.spawn((Name::new("SculptFocus"), SculptFocus));
}

/// F5 flips the mode, acquiring/releasing modal focus — which is what actually
/// frees the cursor and quiets gameplay input.
fn toggle_sculpt(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<EditorTool>,
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
    tool.anchor = None;
    focus.write(if tool.active {
        ModalInputFocusRequest::Acquire(owner)
    } else {
        ModalInputFocusRequest::Release(owner)
    });
    info!(
        "[editor] terrain sculpt: {}",
        if tool.active { "ON" } else { "OFF" }
    );
}

/// The key that moves between authoring layers.
///
/// **F6, not Tab.** Tab was the obvious choice and it was already taken: it
/// toggles the inventory (`presentation/inventory_ui`, alongside `I`), which reads
/// raw `ButtonInput` and does not care that the editor is active. One press did
/// both things — switched the layer *and* opened a panel over the terrain, which
/// silences the brush by design — so the layer changed and painting stayed dead.
///
/// The F row is this project's tool row (F1 hub, F3 freecam, F4 waypoint, F7
/// captura, F9 grass lab, F10 menu), and F6 sits next to F5, the key that opens
/// the tool. Since 2026-08-22 both only exist in editor mode, so they no longer
/// compete with anything the game binds.
///
/// The general problem is bigger than this binding: several layers read the
/// The F row is this project's tool row (F1 hub, F4 waypoint, F7 captura, F9
/// grass lab, F10 menu). F5/F6 only exist in editor mode since 2026-08-22.
const SWITCH_LAYER_KEY: KeyCode = KeyCode::F6;

/// F6 moves between authoring layers.
///
/// Switching **ends any open stroke**: the anchor and the history entry belong to
/// the layer that started them, and carrying a half-finished sculpt into the
/// paint layer would file it against the wrong data.
fn switch_layer(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<EditorTool>,
    mut history: ResMut<SculptHistory>,
    terrain: TerrainAccess,
) {
    if !tool.active || !keys.just_pressed(SWITCH_LAYER_KEY) {
        return;
    }
    if let Some(terrain) = terrain.primary() {
        history.end_stroke(terrain);
    }
    tool.anchor = None;
    tool.layer = tool.layer.next();
    info!("[editor] capa: {}", tool.layer.label());
}

/// Number keys pick within the active layer — a brush in relief, a kind in
/// meaning. The HUD spells out what each one is, because the same keys mean two
/// different things depending on the layer.
fn select_brush(keys: Res<ButtonInput<KeyCode>>, mut tool: ResMut<EditorTool>) {
    if !tool.active {
        return;
    }
    match tool.layer {
        ToolLayer::Relief => {
            for kind in BrushKind::ALL {
                if keys.just_pressed(kind.key()) {
                    tool.brush = kind;
                }
            }
        }
        ToolLayer::Meaning => {
            for (index, kind) in TerrainKind::ALL.into_iter().enumerate() {
                if keys.just_pressed(digit_key(index)) {
                    tool.paint_kind = kind;
                }
            }
        }
        ToolLayer::Instances => {
            for (index, kind) in PropKind::ALL.into_iter().enumerate() {
                if keys.just_pressed(digit_key(index)) {
                    tool.prop_kind = kind;
                }
            }
        }
    }
}

/// `Digit1`..`Digit9` by zero-based index. Both palettes are far short of nine
/// entries; anything past that falls back to a key nothing else uses rather than
/// panicking on a table that grew.
fn digit_key(index: usize) -> KeyCode {
    const DIGITS: [KeyCode; 9] = [
        KeyCode::Digit1,
        KeyCode::Digit2,
        KeyCode::Digit3,
        KeyCode::Digit4,
        KeyCode::Digit5,
        KeyCode::Digit6,
        KeyCode::Digit7,
        KeyCode::Digit8,
        KeyCode::Digit9,
    ];
    DIGITS.get(index).copied().unwrap_or(KeyCode::Digit0)
}

/// **F+rueda** cambia el radio; **Shift+F+rueda** (o `[` / `]`) la fuerza.
///
/// La rueda sola era el radio hasta el 2026-08-22 y se la quedó el zoom de la
/// cámara (`MAP_EDITOR.md`). `camera::freecam` es el dueño de `RADIUS_MODIFIER`.
fn adjust_brush(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<EditorTool>,
    scroll: Res<AccumulatedMouseScroll>,
) {
    if !tool.active {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let mut strength_notches = 0.0;
    if keys.just_pressed(KeyCode::BracketRight) {
        strength_notches += 1.0;
    }
    if keys.just_pressed(KeyCode::BracketLeft) {
        strength_notches -= 1.0;
    }
    if scroll.delta.y != 0.0 && keys.pressed(crate::camera::RADIUS_MODIFIER) {
        if shift {
            strength_notches += scroll.delta.y;
        } else {
            tool.radius =
                (tool.radius + scroll.delta.y * RADIUS_STEP).clamp(MIN_RADIUS, MAX_RADIUS);
        }
    }
    if strength_notches != 0.0 {
        tool.strength =
            (tool.strength + strength_notches * STRENGTH_STEP).clamp(MIN_STRENGTH, MAX_STRENGTH);
    }
}
