//! Generic transition proposals and arbitration.

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Priority {
    Default,
    PlayerRequested,
    Opportunistic,
    Forced,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionProposal<S> {
    pub target_state: S,
    pub category: Priority,
    pub override_weight: u32,
    pub source_id: &'static str,
}

impl<S> TransitionProposal<S> {
    pub fn new(
        target_state: S,
        category: Priority,
        override_weight: u32,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProposalOverflow {
    pub capacity: usize,
    pub source_id: &'static str,
}

#[derive(Component)]
pub struct ProposalBuffer<S, const N: usize> {
    slots: [Option<TransitionProposal<S>>; N],
    len: usize,
}

impl<S, const N: usize> Default for ProposalBuffer<S, N> {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            len: 0,
        }
    }
}

impl<S, const N: usize> ProposalBuffer<S, N> {
    pub fn push(&mut self, proposal: TransitionProposal<S>) -> Result<(), ProposalOverflow> {
        if self.len == N {
            return Err(ProposalOverflow {
                capacity: N,
                source_id: proposal.source_id,
            });
        }

        self.slots[self.len] = Some(proposal);
        self.len += 1;
        Ok(())
    }

    pub fn iter(&self) -> impl Iterator<Item = &TransitionProposal<S>> {
        self.slots[..self.len].iter().filter_map(Option::as_ref)
    }

    pub fn clear(&mut self) {
        for slot in &mut self.slots[..self.len] {
            *slot = None;
        }
        self.len = 0;
    }
}

impl<S: Copy, const N: usize> ProposalBuffer<S, N> {
    pub fn arbitrate(&self, current: S) -> S {
        let mut best: Option<&TransitionProposal<S>> = None;
        for proposal in self.iter() {
            best = match best {
                None => Some(proposal),
                Some(winner)
                    if (proposal.category, proposal.override_weight)
                        > (winner.category, winner.override_weight) =>
                {
                    Some(proposal)
                }
                Some(winner) => Some(winner),
            };
        }

        best.map(|proposal| proposal.target_state)
            .unwrap_or(current)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum TestState {
        Current,
        Walk,
        Jump,
        Sneak,
        Sprint,
    }

    #[test]
    fn empty_buffer_keeps_current_state() {
        let buffer = ProposalBuffer::<TestState, 4>::default();

        assert_eq!(buffer.arbitrate(TestState::Current), TestState::Current);
    }

    #[test]
    fn higher_category_wins() {
        let mut buffer = ProposalBuffer::<TestState, 4>::default();
        let _ = buffer.push(TransitionProposal::new(
            TestState::Walk,
            Priority::PlayerRequested,
            100,
            "walk",
        ));
        let _ = buffer.push(TransitionProposal::new(
            TestState::Jump,
            Priority::Forced,
            0,
            "jump",
        ));

        assert_eq!(buffer.arbitrate(TestState::Current), TestState::Jump);
    }

    #[test]
    fn higher_weight_breaks_ties_within_category() {
        let mut buffer = ProposalBuffer::<TestState, 4>::default();
        let _ = buffer.push(TransitionProposal::new(
            TestState::Walk,
            Priority::PlayerRequested,
            0,
            "walk",
        ));
        let _ = buffer.push(TransitionProposal::new(
            TestState::Sneak,
            Priority::PlayerRequested,
            1,
            "sneak",
        ));

        assert_eq!(buffer.arbitrate(TestState::Current), TestState::Sneak);
    }

    #[test]
    fn exact_tie_keeps_first_inserted() {
        let mut buffer = ProposalBuffer::<TestState, 4>::default();
        let _ = buffer.push(TransitionProposal::new(
            TestState::Walk,
            Priority::PlayerRequested,
            0,
            "walk",
        ));
        let _ = buffer.push(TransitionProposal::new(
            TestState::Sprint,
            Priority::PlayerRequested,
            0,
            "sprint",
        ));

        assert_eq!(buffer.arbitrate(TestState::Current), TestState::Walk);
    }

    #[test]
    fn overflow_does_not_change_accepted_content() {
        let mut buffer = ProposalBuffer::<TestState, 1>::default();
        let accepted =
            TransitionProposal::new(TestState::Walk, Priority::PlayerRequested, 0, "walk");
        let rejected = TransitionProposal::new(TestState::Jump, Priority::Forced, 0, "jump");

        assert_eq!(buffer.push(accepted), Ok(()));
        assert_eq!(
            buffer.push(rejected),
            Err(ProposalOverflow {
                capacity: 1,
                source_id: "jump",
            })
        );

        let proposals = buffer.iter().copied().collect::<Vec<_>>();
        assert_eq!(proposals, vec![accepted]);
    }
}
