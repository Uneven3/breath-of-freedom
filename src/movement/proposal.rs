use super::state::LocomotionState;

pub use crate::proposal::Priority;

pub const MOVEMENT_PROPOSAL_CAPACITY: usize = 32;

pub type TransitionProposal = crate::proposal::TransitionProposal<LocomotionState>;
pub type ProposalBuffer =
    crate::proposal::ProposalBuffer<LocomotionState, MOVEMENT_PROPOSAL_CAPACITY>;

/// Tie-break weights for every motor, in one place.
///
/// Arbitration compares `(Priority, weight)`, so a weight only matters against
/// proposals in the **same** category; an exact tie falls back to system
/// execution order, which Bevy does not guarantee. Any two motors that can
/// co-propose in the same category must therefore differ here — keeping the
/// full table in one module makes that total order auditable at a glance
/// (`weights_disambiguate_co_proposing_motors` pins it).
pub mod weight {
    /// Default: WALK beats FALL — a grounded actor prefers walking.
    pub const FALL: u32 = 0;
    pub const WALK: u32 = 1;

    /// PlayerRequested: a climb start beats gaits and a glide hand-off
    /// (`glide` downgrades to PlayerRequested on a climbable wall exactly so
    /// CLIMB wins); an explicit vault beats everything else requested.
    pub const GLIDE: u32 = 0;
    pub const SNEAK: u32 = 1;
    pub const SPRINT: u32 = 2;
    pub const CLIMB: u32 = 5;
    pub const AUTO_VAULT: u32 = 20;

    /// Forced: JUMP must beat the sticky STAIRS/LADDER holds; the specialized
    /// climb-state launches (WALL_JUMP, and the more deliberate EDGE_LEAP and
    /// MANTLE) outrank a plain jump; EDGE_LEAP beats MANTLE because it
    /// requires explicit lateral input at an open edge, while MANTLE also
    /// fires on a bare jump at the lip. A running AUTO_VAULT arc yields to
    /// nothing.
    pub const STAIRS: u32 = 0;
    pub const LADDER: u32 = 0;
    pub const JUMP: u32 = 1;
    pub const WALL_JUMP: u32 = 5;
    pub const MANTLE: u32 = 10;
    pub const EDGE_LEAP: u32 = 11;

    // Compile-time pin of the per-category total order between motors that
    // can co-propose. An accidental reorder fails the build, not a play test.
    const _: () = {
        // Default category.
        assert!(WALK > FALL);
        // PlayerRequested category.
        assert!(SNEAK > GLIDE);
        assert!(SPRINT > SNEAK);
        assert!(CLIMB > SPRINT);
        assert!(AUTO_VAULT > CLIMB);
        // Forced category.
        assert!(JUMP > STAIRS);
        assert!(JUMP > LADDER);
        assert!(WALL_JUMP > JUMP);
        assert!(MANTLE > WALL_JUMP);
        assert!(EDGE_LEAP > MANTLE);
        assert!(AUTO_VAULT > EDGE_LEAP);
    };
}
