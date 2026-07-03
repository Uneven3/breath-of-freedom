//! Transition proposals and arbitration.
//!
//! Every motor proposes each frame; the Broker picks exactly one winner by
//! (category, weight). This is the heart of "active motor exclusivity" (see
//! `docs/architecture/movement.md`).

use bevy::prelude::*;

use super::state::LocomotionState;

/// Priority tiers. Higher wins. Deriving `Ord` lets us compare tiers directly
/// in arbitration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Default = 0,
    PlayerRequested = 1,
    Opportunistic = 2,
    Forced = 3,
}

/// A motor's request to become the active state this frame.
#[derive(Debug, Clone)]
pub struct TransitionProposal {
    pub target_state: LocomotionState,
    pub category: Priority,
    pub override_weight: i32,
    pub source_id: &'static str,
}

impl TransitionProposal {
    pub fn new(
        target_state: LocomotionState,
        category: Priority,
        override_weight: i32,
        source_id: &'static str,
    ) -> Self {
        Self {
            target_state,
            category,
            override_weight,
            source_id,
        }
    }
}

/// Proposals gathered this frame, drained by `Arbitrate`. Component on the player.
#[derive(Component, Default)]
pub struct ProposalBuffer(pub Vec<TransitionProposal>);

impl ProposalBuffer {
    /// Pick the winner: higher `category` wins; ties broken by higher
    /// `override_weight`; equal-on-both keeps the first seen (stable). Empty
    /// buffer defaults to `Fall`.
    pub fn arbitrate(&self) -> LocomotionState {
        let mut best: Option<&TransitionProposal> = None;
        for p in &self.0 {
            best = match best {
                None => Some(p),
                Some(b) if p.category > b.category => Some(p),
                Some(b) if p.category == b.category && p.override_weight > b.override_weight => {
                    Some(p)
                }
                Some(b) => Some(b),
            };
        }
        best.map(|p| p.target_state)
            .unwrap_or(LocomotionState::Fall)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_buffer_defaults_to_fall() {
        assert_eq!(ProposalBuffer::default().arbitrate(), LocomotionState::Fall);
    }

    #[test]
    fn higher_category_wins_over_weight() {
        let mut buf = ProposalBuffer::default();
        buf.0.push(TransitionProposal::new(
            LocomotionState::Walk,
            Priority::PlayerRequested,
            100,
            "walk",
        ));
        buf.0.push(TransitionProposal::new(
            LocomotionState::Jump,
            Priority::Forced,
            0,
            "jump",
        ));
        assert_eq!(buf.arbitrate(), LocomotionState::Jump);
    }

    #[test]
    fn weight_breaks_ties_within_category() {
        let mut buf = ProposalBuffer::default();
        buf.0.push(TransitionProposal::new(
            LocomotionState::Walk,
            Priority::PlayerRequested,
            0,
            "walk",
        ));
        buf.0.push(TransitionProposal::new(
            LocomotionState::Sneak,
            Priority::PlayerRequested,
            1,
            "sneak",
        ));
        assert_eq!(buf.arbitrate(), LocomotionState::Sneak);
    }

    #[test]
    fn equal_category_and_weight_keeps_first() {
        let mut buf = ProposalBuffer::default();
        buf.0.push(TransitionProposal::new(
            LocomotionState::Walk,
            Priority::PlayerRequested,
            0,
            "walk",
        ));
        buf.0.push(TransitionProposal::new(
            LocomotionState::Sprint,
            Priority::PlayerRequested,
            0,
            "sprint",
        ));
        assert_eq!(buf.arbitrate(), LocomotionState::Walk);
    }
}
