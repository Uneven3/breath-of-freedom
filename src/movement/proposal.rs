use super::state::LocomotionState;

pub use crate::proposal::Priority;

pub const MOVEMENT_PROPOSAL_CAPACITY: usize = 32;

pub type TransitionProposal = crate::proposal::TransitionProposal<LocomotionState>;
pub type ProposalBuffer =
    crate::proposal::ProposalBuffer<LocomotionState, MOVEMENT_PROPOSAL_CAPACITY>;
