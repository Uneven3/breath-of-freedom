//! Idle motor — the always-on fallback, analog of Movement's `walk`/`fall`
//! defaults: proposes `Idle` at `Priority::Default` every frame so the buffer
//! never reaches `arbitrate` empty and expired phases decay to `Idle` by
//! silence.

use bevy::prelude::*;

use crate::combat::proposal::{CombatProposalBuffer, Priority, TransitionProposal, weight};
use crate::combat::state::CombatState;
use crate::movement::Actor;

pub fn propose(mut q: Query<&mut CombatProposalBuffer, With<Actor>>) {
    for mut buffer in &mut q {
        let _ = buffer.push(TransitionProposal::new(
            CombatState::Idle,
            Priority::Default,
            weight::IDLE,
            "idle",
        ));
    }
}
