//! In-engine authoring tools. First up: the terrain sculpt brushes; future tools
//! (semantic paint, instance placement) join here.
//!
//! Dev-only, gated behind **F5**. While active the tool holds modal input focus,
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
//! - `brush` — the brush vocabulary and the stroke that applies it.
//! - `history` — undo/redo, one entry per stroke.
//! - `persist` — the level on disk.
//! - `hud` — what is on screen while sculpting.

mod brush;
mod history;
mod hud;
mod persist;

use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;

use crate::input::ModalInputFocusRequest;
use crate::scene::AppState;
use brush::BrushKind;
use history::SculptHistory;

/// Brush radius bounds (metres) for the scroll-wheel resize.
const MIN_RADIUS: f32 = 2.0;
const MAX_RADIUS: f32 = 40.0;
/// Radius change per scroll notch.
const RADIUS_STEP: f32 = 1.5;
/// Strength bounds and step. Strength scales every brush's per-second rate, so
/// one knob covers "shape a mountain" and "nudge a footpath".
const MIN_STRENGTH: f32 = 0.1;
const MAX_STRENGTH: f32 = 3.0;
const STRENGTH_STEP: f32 = 0.1;

/// Terrain sculpt tool state: which brush, how wide, how hard, and the anchor of
/// the stroke in progress.
#[derive(Resource)]
pub(crate) struct SculptTool {
    pub active: bool,
    pub radius: f32,
    pub strength: f32,
    pub kind: BrushKind,
    /// Where the current stroke started, in world XZ plus the ground height
    /// there at press time. `Flatten` levels to that height and `Ramp` runs from
    /// it — both need the value *before* the stroke starts changing it.
    pub anchor: Option<StrokeAnchor>,
}

/// The frozen start of a stroke.
#[derive(Clone, Copy)]
pub(crate) struct StrokeAnchor {
    pub xz: Vec2,
    pub height: f32,
}

impl Default for SculptTool {
    fn default() -> Self {
        Self {
            active: false,
            radius: 6.0,
            strength: 1.0,
            kind: BrushKind::Sculpt,
            anchor: None,
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
                select_brush,
                adjust_brush,
                brush::sculpt_terrain,
                history::undo_redo,
                persist::save_or_reload,
                brush::draw_brush_gizmo,
                hud::update_hud,
            )
                .chain()
                // No sculpting from the menu: there is no terrain to bite, and
                // the menu owns the cursor.
                .run_if(not(in_state(AppState::MainMenu))),
        );
        for id in crate::scene::SceneId::ALL {
            app.add_systems(OnExit(AppState::Scene(id)), leave_sculpting);
        }
    }
}

/// Leaving a scene ends sculpting. Without this the tool would stay "on" across
/// the transition while its focus owner — infrastructure, so it does not
/// despawn — kept holding modal input, freezing the next scene's controls
/// against a terrain that no longer exists.
fn leave_sculpting(
    mut tool: ResMut<SculptTool>,
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

/// Number keys pick the brush; the HUD spells out what each one does.
fn select_brush(keys: Res<ButtonInput<KeyCode>>, mut tool: ResMut<SculptTool>) {
    if !tool.active {
        return;
    }
    for kind in BrushKind::ALL {
        if keys.just_pressed(kind.key()) {
            tool.kind = kind;
        }
    }
}

/// Scroll resizes the brush; Shift+scroll (or `[` / `]`) changes its strength.
fn adjust_brush(
    keys: Res<ButtonInput<KeyCode>>,
    mut tool: ResMut<SculptTool>,
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
    if scroll.delta.y != 0.0 {
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
