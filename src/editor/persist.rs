//! The level on disk. **The file is the level**: `Ctrl+S` writes the height grid
//! *and the semantic layer*, `Ctrl+L` reloads them, and `world::terrain` loads
//! the file at startup — so an authoring session survives closing the game, which
//! is the difference between a toy and a tool.

use std::path::Path;

use bevy::prelude::*;

use super::EditorTool;
use super::history::SculptHistory;
use crate::scene::AppState;
use crate::world::{Terrain, terrain_file};

/// Ctrl+S saves, Ctrl+L reloads from disk (discarding unsaved sculpting, but
/// filed in the undo history first so it is not a one-way door).
pub(super) fn save_or_reload(
    tool: Res<EditorTool>,
    mut history: ResMut<SculptHistory>,
    keys: Res<ButtonInput<KeyCode>>,
    state: Res<State<AppState>>,
    mut terrain: Query<&mut Terrain>,
) {
    if !tool.active {
        return;
    }
    let control = keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight);
    if !control {
        return;
    }
    let save = keys.just_pressed(KeyCode::KeyS);
    let reload = keys.just_pressed(KeyCode::KeyL);
    if !(save || reload) {
        return;
    }
    let (Ok(mut terrain), Some(file)) = (terrain.single_mut(), terrain_file(&state)) else {
        return;
    };
    if save {
        match save_terrain(&terrain, file) {
            Ok(()) => info!("[editor] terreno guardado en {file}"),
            Err(error) => warn!("[editor] no se pudo guardar: {error}"),
        }
        return;
    }
    // Snapshot first so the load is undoable, but only keep the step if the load
    // actually happened: a failed read left the grid alone, and an undo step
    // that restores what is already on screen is a step that does nothing.
    let before = terrain.snapshot();
    match load_terrain(terrain.bypass_change_detection(), file) {
        Ok(()) => {
            terrain.set_changed();
            history.record_snapshot(before);
            info!("[editor] terreno recargado desde {file}");
        }
        Err(error) => warn!("[editor] no se pudo cargar: {error}"),
    }
}

/// Write the grid, creating the level directory the first time.
///
/// Writes a temporary file and renames it over the target, because the file
/// **is** the level: a crash or a full disk halfway through a plain write leaves
/// a truncated grid that loads as garbage, and the session it described is gone.
/// `rename` within a directory is atomic, so the level on disk is always either
/// the previous save or the complete new one.
fn save_terrain(terrain: &Terrain, file: &str) -> Result<(), String> {
    let text = terrain.to_ron()?;
    let path = Path::new(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let temporary = path.with_extension("ron.tmp");
    std::fs::write(&temporary, text).map_err(|error| error.to_string())?;
    std::fs::rename(&temporary, path).map_err(|error| error.to_string())
}

fn load_terrain(terrain: &mut Terrain, file: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    terrain.apply_ron(&text)
}
