//! Per-frame semantic locomotion intent snapshot.
//!
//! Controllers translate device, AI, network, or cinematic commands into this
//! component. Motors consume these values without knowing the controller.

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PlanarMoveIntent {
    /// World-space heading the body translates along (XZ).
    pub direction: Vec2,
    /// The same move expressed **relative to the body's facing**: `y < 0` is
    /// toward what the body faces (forward), `y > 0` back, `x` strafe (right+).
    /// Equals `direction` rotated into the facing frame — under `Free` that is
    /// always forward (you face where you go); under lock-on/aim it makes
    /// "strafe left" an explicit fact rather than an emergent side effect of
    /// camera-relative movement. Read by the directional animation axis and debug.
    pub local: Vec2,
    pub strength: f32,
}

/// Coarse facing-relative heading classified from [`PlanarMoveIntent::local`] —
/// the legible name of the movement intent ("I am strafing left") that the
/// directional animation axis and debug read instead of re-deriving it.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StrafeDir {
    #[default]
    Idle,
    Forward,
    Back,
    Left,
    Right,
}

impl PlanarMoveIntent {
    /// Classify the facing-relative move into a cardinal heading. The dominant
    /// axis wins; below a small deadzone it is `Idle`.
    pub fn strafe_dir(&self) -> StrafeDir {
        if self.local.length_squared() < 0.04 {
            return StrafeDir::Idle;
        }
        if self.local.x.abs() > self.local.y.abs() {
            if self.local.x > 0.0 {
                StrafeDir::Right
            } else {
                StrafeDir::Left
            }
        } else if self.local.y < 0.0 {
            StrafeDir::Forward
        } else {
            StrafeDir::Back
        }
    }
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
    pub wants_sprint: bool,
    pub wants_sneak: bool,
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
    fn strafe_dir_names_the_facing_relative_move() {
        let dir = |x: f32, y: f32| PlanarMoveIntent {
            local: Vec2::new(x, y),
            ..default()
        };
        assert_eq!(dir(0.0, -1.0).strafe_dir(), StrafeDir::Forward);
        assert_eq!(dir(0.0, 1.0).strafe_dir(), StrafeDir::Back);
        assert_eq!(dir(-1.0, 0.0).strafe_dir(), StrafeDir::Left);
        assert_eq!(dir(1.0, 0.0).strafe_dir(), StrafeDir::Right);
        assert_eq!(dir(0.0, 0.0).strafe_dir(), StrafeDir::Idle);
        // Dominant axis wins: mostly-forward with slight strafe reads Forward.
        assert_eq!(dir(0.2, -1.0).strafe_dir(), StrafeDir::Forward);
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
