//! The single source of truth for the active locomotion mode.
//!
//! Every fact has exactly one owner (Constitution §6/§7); mutually-exclusive
//! states are an enum, never a boolean soup. Only the `Arbitrate` system
//! writes this component (see `docs/architecture/movement.md`).
//!
//! Why a plain component enum rather than Bevy's global `States`? Bevy `States`
//! is a single global resource — perfect for app screens, wrong for per-entity
//! locomotion. A component keeps the SSoT *on the entity*, so multiple actors
//! (player, future AI) each own their own state.

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LocomotionState {
    Idle = 0,
    Walk = 1,
    Sprint = 2,
    Fall = 3,
    Jump = 4,
    AutoVault = 5,
    Climb = 6,
    Mantle = 7,
    Stairs = 8,
    Ladder = 9,
    Glide = 10,
    Sneak = 11,
    WallJump = 12,
    EdgeLeap = 13,
}

impl Default for LocomotionState {
    /// Default state is `Fall` — an ungrounded actor starts airborne.
    fn default() -> Self {
        LocomotionState::Fall
    }
}
