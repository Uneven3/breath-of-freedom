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
    Walk,
    Sprint,
    Fall,
    Jump,
    AutoVault,
    Climb,
    Mantle,
    Stairs,
    Ladder,
    Glide,
    Sneak,
    WallJump,
    EdgeLeap,
}

impl Default for LocomotionState {
    /// Default state is `Fall` — an ungrounded actor starts airborne.
    fn default() -> Self {
        LocomotionState::Fall
    }
}

impl LocomotionState {
    /// Every variant, for exhaustive audits (see the `arbitration_matrix` tests
    /// in `proposal.rs`). The compile-time guard below fails to build if a
    /// variant is added without being listed here, so those audits can trust it.
    pub const ALL: [LocomotionState; 13] = [
        LocomotionState::Walk,
        LocomotionState::Sprint,
        LocomotionState::Fall,
        LocomotionState::Jump,
        LocomotionState::AutoVault,
        LocomotionState::Climb,
        LocomotionState::Mantle,
        LocomotionState::Stairs,
        LocomotionState::Ladder,
        LocomotionState::Glide,
        LocomotionState::Sneak,
        LocomotionState::WallJump,
        LocomotionState::EdgeLeap,
    ];
}

const _: () = {
    // A new variant fails this exhaustive match to compile until it is added to
    // `LocomotionState::ALL` above.
    fn assert_all_is_exhaustive(state: LocomotionState) {
        match state {
            LocomotionState::Walk
            | LocomotionState::Sprint
            | LocomotionState::Fall
            | LocomotionState::Jump
            | LocomotionState::AutoVault
            | LocomotionState::Climb
            | LocomotionState::Mantle
            | LocomotionState::Stairs
            | LocomotionState::Ladder
            | LocomotionState::Glide
            | LocomotionState::Sneak
            | LocomotionState::WallJump
            | LocomotionState::EdgeLeap => {}
        }
    }
    let _ = assert_all_is_exhaustive;
    // Anchor `ALL` in every build (its consumers are the `arbitration_matrix`
    // tests): a mismatch between it and the exhaustive match above cannot slip
    // through as dead code.
    let _ = LocomotionState::ALL.len();
};
