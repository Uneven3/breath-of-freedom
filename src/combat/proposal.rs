//! Combat's instantiation of the shared proposal/arbitration core.
//!
//! First real second consumer of `crate::proposal` — the bet made in
//! `rationale/proposal-arbitration-core.md`. Same rules as Movement: weights
//! of co-proposing motors live together here, ties are design bugs.

use crate::combat::state::CombatState;

pub const COMBAT_PROPOSAL_CAPACITY: usize = 8;

pub type CombatProposalBuffer =
    crate::proposal::ProposalBuffer<CombatState, COMBAT_PROPOSAL_CAPACITY>;
pub type TransitionProposal = crate::proposal::TransitionProposal<CombatState>;
pub use crate::proposal::Priority;

/// Tie-break weights for every combat motor, in one place.
pub mod weight {
    /// The always-on fallback (analog of Movement's WALK/FALL).
    pub const IDLE: u32 = 0;
    /// Drawing/holding the bow — loses to a melee start from Idle (attack
    /// press + aim held the same tick swings; the drawn bow shoots instead).
    pub const AIM: u32 = 1;
    /// Holding the current attack phase while its timer runs.
    pub const ATTACK_HOLD: u32 = 2;
    /// Advancing to the next phase of the same strike (timer elapsed).
    pub const ATTACK_ADVANCE: u32 = 3;
    /// Chaining into the next combo step (buffered press inside the window)
    /// — must beat the hold of `Recovery`.
    pub const ATTACK_CHAIN: u32 = 4;
}

const _: () = {
    assert!(weight::AIM > weight::IDLE);
    assert!(weight::ATTACK_HOLD > weight::AIM);
    assert!(weight::ATTACK_ADVANCE > weight::ATTACK_HOLD);
    assert!(weight::ATTACK_CHAIN > weight::ATTACK_ADVANCE);
};
