//! Per-frame input snapshot.
//!
//! Modeled as a **Component on the player entity** so an AI brain can drive
//! other entities with the same shape later (input enters the simulation only
//! through a Brain — see `docs/architecture/movement.md`).

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Default)]
pub struct Intents {
    /// Camera-relative world-space direction on the XZ plane.
    /// `x` = strafe (right positive), `y` = forward (forward positive).
    pub move_dir: Vec2,
    /// Raw hardware stick/WASD vector before camera rotation.
    pub raw_input: Vec2,
    /// Discrete -1/0/1 per axis: `x` = left/right, `y` = back/forward.
    pub wish_dir: IVec2,
    /// 0..1 magnitude of the raw input.
    pub input_strength: f32,

    pub wants_jump: bool,
    pub wants_sprint: bool,
    pub wants_sneak: bool,
    pub wants_climb: bool,
    pub wants_mantle: bool,
    pub wants_vault: bool,
    pub wants_glide: bool,
}

impl Intents {
    // Climb/wall-context semantic getters (G4: data carries zero logic beyond
    // pure reads).
    pub fn is_climbing_up(&self) -> bool {
        self.wish_dir.y == 1
    }
    pub fn is_climbing_down(&self) -> bool {
        self.wish_dir.y == -1
    }
    pub fn is_climbing_left(&self) -> bool {
        self.wish_dir.x == -1
    }
    pub fn is_climbing_right(&self) -> bool {
        self.wish_dir.x == 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semantic_getters_follow_wish_dir() {
        let mut i = Intents {
            wish_dir: IVec2::new(1, 1),
            ..default()
        };
        assert!(i.is_climbing_up() && i.is_climbing_right());
        assert!(!i.is_climbing_down() && !i.is_climbing_left());
        i.wish_dir = IVec2::new(-1, -1);
        assert!(i.is_climbing_down() && i.is_climbing_left());
    }
}
