//! The single source of truth for the active locomotion mode.
//!
//! Every fact has exactly one owner (Constitution §6/§7); mutually-exclusive
//! states are an enum, never a boolean soup. Only the `Arbitrate` system
//! writes this component (see `docs/ARCHITECTURE.md`).
//!
//! Why a plain component enum rather than Bevy's global `States`? Bevy `States`
//! is a single global resource — perfect for app screens, wrong for per-entity
//! locomotion. A component keeps the SSoT *on the entity*, so multiple actors
//! (player, future AI) each own their own state.

use bevy_ecs::prelude::*;

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
    /// Touching a surface too steep to walk, without the player asking to
    /// climb it. Not airborne — the body is on the face, sliding down it.
    Slide,
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
    pub const ALL: [LocomotionState; 14] = [
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
        LocomotionState::Slide,
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
            | LocomotionState::EdgeLeap
            | LocomotionState::Slide => {}
        }
    }
    let _ = assert_all_is_exhaustive;
    // Anchor `ALL` in every build (its consumers are the `arbitration_matrix`
    // tests): a mismatch between it and the exhaustive match above cannot slip
    // through as dead code.
    let _ = LocomotionState::ALL.len();
};

/// Present while an actor's own locomotion may run. Removed while the body is
/// carried or driven by something else (a mount), so the broker's queries skip
/// it without every system having to ask.
#[derive(Component, Debug, Clone, Copy, Default)]
pub struct LocomotionEnabled;

/// Whether the crouch capsule is currently applied. Lets `sync_crouch_collider`
/// rebuild the collider only when the desired crouch actually changes, and lets
/// other ground motors (e.g. Stairs) read the physical form without recomputing
/// it. Presentación lo lee para agachar la cápsula visual.
#[derive(Component, Default)]
pub struct Crouched(pub bool);
