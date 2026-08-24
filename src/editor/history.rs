//! Undo/redo for authoring, one entry per **stroke**.
//!
//! Per stroke, not per frame: a two-second drag is one thing you did, so it
//! should be one thing you can take back. The unit of history is a full
//! [`TerrainSnapshot`], and it scales with the square of the grid: at 320 cells
//! that is 0.5 MB a stroke and ~16 MB across [`MAX_STEPS`], still cheaper than
//! the bookkeeping a diff would need. At 640 it was 2 MB and ~66 MB.
//!
//! One stack for both authoring layers, not one each. Undo means "take back the
//! last thing I did", and a developer who paints a patch and then sculpts a hill
//! expects two presses to walk back through both — a per-layer stack would make
//! Ctrl+Z depend on which mode happens to be selected, which is how you lose
//! work you thought you had undone.
//!
//! This exists because the destructive brushes (flatten, terrace) make
//! experimenting expensive without it: you stop trying things when a mistake
//! costs you the hillside.

use bevy::prelude::*;

use super::EditorTool;
use crate::world::{Terrain, TerrainSnapshot};

/// How many strokes back you can go before the oldest is dropped.
const MAX_STEPS: usize = 32;

#[derive(Resource, Default)]
pub(crate) struct SculptHistory {
    /// States to go back to, oldest first.
    undo: Vec<TerrainSnapshot>,
    /// States undone, ready to be redone. Cleared by any new stroke.
    redo: Vec<TerrainSnapshot>,
    /// The terrain as it was when the current stroke started, if one is running.
    pending: Option<TerrainSnapshot>,
}

impl SculptHistory {
    /// Freeze the grid at the start of a stroke. Idempotent: called every frame
    /// the stroke runs, it only takes the first snapshot.
    pub fn begin_stroke(&mut self, terrain: &Terrain) {
        if self.pending.is_none() {
            self.pending = Some(terrain.snapshot());
        }
    }

    /// Close the stroke and file it. A stroke that changed nothing (button
    /// pressed and released without moving dirt, or repainting a patch the kind
    /// it already was) leaves no entry, so undo never spends a step doing
    /// nothing visible.
    pub fn end_stroke(&mut self, terrain: &Terrain) {
        let Some(before) = self.pending.take() else {
            return;
        };
        if before == terrain.snapshot() {
            return;
        }
        self.push_undo(before);
        self.redo.clear();
    }

    /// File an already-taken snapshot as a step to come back to, for edits that
    /// are not strokes (loading a file over your work is the one that would
    /// otherwise be unrecoverable). Takes the snapshot rather than the terrain
    /// because the caller has to snapshot *before* the edit and file it *after*
    /// it succeeds.
    pub fn record_snapshot(&mut self, state: TerrainSnapshot) {
        self.push_undo(state);
        self.redo.clear();
    }

    fn push_undo(&mut self, state: TerrainSnapshot) {
        if self.undo.len() == MAX_STEPS {
            self.undo.remove(0);
        }
        self.undo.push(state);
    }

    pub fn undo(&mut self, terrain: &mut Terrain) -> bool {
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        let current = terrain.snapshot();
        if !terrain.restore(&previous) {
            return false;
        }
        self.redo.push(current);
        true
    }

    pub fn redo(&mut self, terrain: &mut Terrain) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = terrain.snapshot();
        if !terrain.restore(&next) {
            return false;
        }
        self.push_undo(current);
        true
    }

    pub fn depth(&self) -> (usize, usize) {
        (self.undo.len(), self.redo.len())
    }

    /// Is a stroke open right now (button down, dirt already moved)?
    pub fn is_stroking(&self) -> bool {
        self.pending.is_some()
    }

    /// Forget everything. Called when a scene ends: these snapshots describe a
    /// terrain that no longer exists, and since every scene's grid has the same
    /// dimensions, [`Terrain::restore`] would happily accept one — undo in the
    /// next scene would silently paste in the previous scene's ground *and* its
    /// painted meaning.
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.pending = None;
    }
}

/// Ctrl+Z steps back, Ctrl+Y (or Ctrl+Shift+Z) steps forward.
pub(super) fn undo_redo(
    tool: Res<EditorTool>,
    mut history: ResMut<SculptHistory>,
    keys: Res<ButtonInput<KeyCode>>,
    mut terrain: Query<&mut Terrain>,
) {
    if !tool.active {
        return;
    }
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !control {
        return;
    }
    let shift = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let undo = keys.just_pressed(KeyCode::KeyZ) && !shift;
    let redo = keys.just_pressed(KeyCode::KeyY) || (keys.just_pressed(KeyCode::KeyZ) && shift);
    if !(undo || redo) {
        return;
    }
    // Not mid-stroke. The open stroke's "before" snapshot predates the step we
    // would undo, so filing it on button-up would push the undone state back in
    // as a new entry — the stack ends up describing a history that never
    // happened. Finish the drag first; the step is still there afterwards.
    if history.is_stroking() {
        info!("[editor] undo/redo ignorado: hay un trazo en curso");
        return;
    }
    let Ok(mut terrain) = terrain.single_mut() else {
        return;
    };
    // Only reborrow mutably when the step actually lands, so a no-op undo does
    // not flag `Changed<Terrain>` and rebuild the collider and mesh for nothing.
    let applied = if undo {
        history.undo(terrain.bypass_change_detection())
    } else {
        history.redo(terrain.bypass_change_detection())
    };
    if applied {
        terrain.set_changed();
    }
    info!(
        "[editor] {} {}",
        if undo { "undo" } else { "redo" },
        if applied {
            "aplicado"
        } else {
            "sin nada que hacer"
        }
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world::Terrain;

    fn hill() -> Terrain {
        Terrain::flat_for_test()
    }

    #[test]
    fn a_stroke_becomes_one_undo_step() {
        let mut terrain = hill();
        let mut history = SculptHistory::default();
        let flat = terrain.snapshot();
        // Many frames, one stroke.
        history.begin_stroke(&terrain);
        for _ in 0..10 {
            terrain.raise_area(Vec2::ZERO, 10.0, 0.1);
            history.begin_stroke(&terrain);
        }
        history.end_stroke(&terrain);
        assert_eq!(history.depth(), (1, 0));
        assert!(history.undo(&mut terrain));
        assert_eq!(terrain.snapshot(), flat);
    }

    #[test]
    fn an_empty_stroke_files_nothing() {
        let terrain = hill();
        let mut history = SculptHistory::default();
        history.begin_stroke(&terrain);
        history.end_stroke(&terrain);
        assert_eq!(history.depth(), (0, 0));
    }

    #[test]
    fn redo_replays_what_undo_took_back() {
        let mut terrain = hill();
        let mut history = SculptHistory::default();
        history.begin_stroke(&terrain);
        terrain.raise_area(Vec2::ZERO, 10.0, 3.0);
        history.end_stroke(&terrain);
        let sculpted = terrain.snapshot();

        assert!(history.undo(&mut terrain));
        assert_ne!(terrain.snapshot(), sculpted);
        assert!(history.redo(&mut terrain));
        assert_eq!(terrain.snapshot(), sculpted);
    }

    #[test]
    fn a_new_stroke_drops_the_redo_branch() {
        let mut terrain = hill();
        let mut history = SculptHistory::default();
        history.begin_stroke(&terrain);
        terrain.raise_area(Vec2::ZERO, 10.0, 3.0);
        history.end_stroke(&terrain);
        history.undo(&mut terrain);
        assert_eq!(history.depth(), (0, 1));

        history.begin_stroke(&terrain);
        terrain.raise_area(Vec2::new(20.0, 0.0), 10.0, 3.0);
        history.end_stroke(&terrain);
        assert_eq!(history.depth(), (1, 0));
    }

    #[test]
    fn a_stroke_in_progress_blocks_undo() {
        // Ctrl+Z mid-drag used to undo a step whose "before" the open stroke
        // still held, so releasing the button filed the undone state right back
        // in. The stack has to describe what actually happened.
        let mut terrain = hill();
        let mut history = SculptHistory::default();
        history.begin_stroke(&terrain);
        terrain.raise_area(Vec2::ZERO, 10.0, 1.0);
        assert!(history.is_stroking());
        history.end_stroke(&terrain);
        assert!(!history.is_stroking());

        history.begin_stroke(&terrain);
        terrain.raise_area(Vec2::new(20.0, 0.0), 10.0, 1.0);
        assert!(history.is_stroking(), "the second drag is still open");
    }

    #[test]
    fn undoing_an_empty_history_is_harmless() {
        let mut terrain = hill();
        let mut history = SculptHistory::default();
        assert!(!history.undo(&mut terrain));
        assert!(!history.redo(&mut terrain));
    }

    #[test]
    fn the_history_stops_growing_at_the_cap() {
        let mut terrain = hill();
        let mut history = SculptHistory::default();
        for i in 0..MAX_STEPS + 10 {
            history.begin_stroke(&terrain);
            terrain.raise_area(Vec2::new(i as f32, 0.0), 6.0, 1.0);
            history.end_stroke(&terrain);
        }
        assert_eq!(history.depth().0, MAX_STEPS);
    }
}
