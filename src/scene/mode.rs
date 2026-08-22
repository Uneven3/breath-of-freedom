//! Con qué propósito arrancó el proceso: jugar, o autorar.
//!
//! No es una escena ni una capacidad de escena. `Authoring::terrain_editing`
//! dice que *esta caja* se puede esculpir; esto dice que *esta corrida* existe
//! para esculpir. La misma escena se juega y se autora, así que el jugador no
//! sobra por ser una caja de autoría — sobra por no estar jugando.
//!
//! Vive acá y no en `perf` porque el dueño del arranque es `scene`: el mismo
//! módulo que ya traduce `BOF_SCENE` a un estado inicial.

use bevy::prelude::*;

/// Cómo arrancó el proceso. Se fija una vez en `Startup` y no cambia: alternar
/// en caliente pediría spawnear un jugador a mitad de escena, que es
/// exactamente el camino que `SceneBuild` existe para que no haya dos.
#[derive(Resource, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppMode {
    #[default]
    Game,
    /// World Lab: sin jugador, cámara libre y el pincel ya activo.
    Editor,
}

/// La escena a la que cae el modo editor cuando nadie pidió una con
/// `BOF_SCENE`. `Terreno` es el lienzo de relieve, o sea la caja cuyo propósito
/// entero es autorar (`docs/MAP_EDITOR.md`).
pub const EDITOR_DEFAULT_SCENE: super::SceneId = super::SceneId::Sandbox;

/// Lee `BOF_MODE`. Un valor que no se entiende **no arranca en modo juego en
/// silencio**: avisa y nombra los válidos, por el mismo motivo que `BOF_SCENE`.
pub fn configured_app_mode() -> AppMode {
    let Ok(raw) = std::env::var("BOF_MODE") else {
        return AppMode::Game;
    };
    match raw.to_ascii_lowercase().as_str() {
        "editor" => AppMode::Editor,
        "game" => AppMode::Game,
        other => {
            error!("[scene] BOF_MODE={other} no nombra ningún modo; hay: game, editor");
            AppMode::Game
        }
    }
}

/// Run condition: el proceso arrancó para jugar.
pub fn in_game_mode(mode: Res<AppMode>) -> bool {
    *mode == AppMode::Game
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El default tiene que ser jugar: cualquier otra cosa haría que un
    /// `cargo run` sin variables arrancara sin jugador.
    #[test]
    fn the_default_mode_is_playing() {
        assert_eq!(AppMode::default(), AppMode::Game);
    }
    /// La condición discrimina, y eso hay que probarlo en los dos sentidos: un
    /// `true` constante también pasaría la mitad de esta prueba.
    #[test]
    fn the_run_condition_separates_the_modes() {
        let mut world = World::new();
        world.insert_resource(AppMode::Game);
        assert!(world.run_system_cached(in_game_mode).unwrap_or(false));

        world.insert_resource(AppMode::Editor);
        assert!(!world.run_system_cached(in_game_mode).unwrap_or(true));
    }
}
