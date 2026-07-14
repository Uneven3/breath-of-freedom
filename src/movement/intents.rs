//! Per-frame semantic locomotion intent snapshot.
//!
//! Controllers translate device, AI, network, or cinematic commands into this
//! component. Motors consume these values without knowing the controller.

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlanarMoveIntent {
    pub direction: Vec2,
    pub strength: f32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GaitIntent {
    #[default]
    Walk,
    Sprint,
    Sneak,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct JumpIntent {
    pub held: bool,
    pub pressed: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClimbVerticalIntent {
    #[default]
    Neutral,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum ClimbLateralIntent {
    #[default]
    Neutral,
    Left,
    Right,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ClimbIntent {
    pub requested: bool,
    pub vertical: ClimbVerticalIntent,
    pub lateral: ClimbLateralIntent,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum LadderIntent {
    #[default]
    Hold,
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TraversalActionIntent {
    #[default]
    None,
    Mantle,
    Vault,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum GlideIntent {
    #[default]
    Inactive,
    Requested,
}

#[derive(Component, Debug, Clone, Default)]
pub struct Intents {
    pub planar: PlanarMoveIntent,
    pub gait: GaitIntent,
    pub jump: JumpIntent,
    pub climb: ClimbIntent,
    pub ladder: LadderIntent,
    pub traversal: TraversalActionIntent,
    pub glide: GlideIntent,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn climb_intent_represents_diagonal_motion_with_named_axes() {
        let intent = ClimbIntent {
            vertical: ClimbVerticalIntent::Up,
            lateral: ClimbLateralIntent::Right,
            ..default()
        };

        assert_eq!(intent.vertical, ClimbVerticalIntent::Up);
        assert_eq!(intent.lateral, ClimbLateralIntent::Right);
    }

    #[test]
    fn traversal_action_is_mutually_exclusive() {
        let intents = Intents {
            traversal: TraversalActionIntent::Mantle,
            ..default()
        };

        assert_ne!(intents.traversal, TraversalActionIntent::Vault);
    }
}
