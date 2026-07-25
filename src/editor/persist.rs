//! The level on disk. **The file is the level**: `Ctrl+S` writes the height
//! grid, `Ctrl+L` reloads it, and `world::terrain` loads it at startup — so a
//! sculpting session survives closing the game, which is the difference between
//! a toy and a tool.

use std::path::Path;

use bevy::prelude::*;

use super::SculptTool;
use super::history::SculptHistory;
use crate::scene::AppState;
use crate::world::{Terrain, terrain_file};

/// Ctrl+S saves, Ctrl+L reloads from disk (discarding unsaved sculpting, but
/// filed in the undo history first so it is not a one-way door).
pub(super) fn save_or_reload(
    tool: Res<SculptTool>,
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
    history.record(&terrain);
    match load_terrain(terrain.bypass_change_detection(), file) {
        Ok(()) => {
            terrain.set_changed();
            info!("[editor] terreno recargado desde {file}");
        }
        Err(error) => warn!("[editor] no se pudo cargar: {error}"),
    }
}

/// Write the grid, creating the level directory the first time.
fn save_terrain(terrain: &Terrain, file: &str) -> Result<(), String> {
    let text = terrain.to_ron()?;
    if let Some(parent) = Path::new(file).parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(file, text).map_err(|error| error.to_string())
}

fn load_terrain(terrain: &mut Terrain, file: &str) -> Result<(), String> {
    let text = std::fs::read_to_string(file).map_err(|error| error.to_string())?;
    terrain.apply_ron(&text)
}
