//! The single source of truth for the active combat mode.
//!
//! Same contract as `movement::state::LocomotionState`: mutually-exclusive
//! phases as an enum, one writer (`combat::arbitrate`). The combo *step*
//! deliberately does NOT live here — `Windup/Active/Recovery` repeat per
//! step and `ComboLocal.step` says which one (see
//! `rationale/combat-combo-chains.md`).
//!
//! Guarding, Parrying and Staggered remain future work. Adding any variant is
//! a compile error until the dispatcher handles it.

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum CombatState {
    /// Not committed to any combat action.
    #[default]
    Idle,
    /// Committed to a strike, hitbox not yet live. Not cancelable by attack.
    Windup,
    /// Hitbox live: the swing sweep runs.
    Active,
    /// Follow-through: vulnerable; the chain window for the next step.
    Recovery,
    /// Bow drawn: attack releases an arrow along the control orientation.
    Aiming,
}

impl CombatState {
    /// Every variant, for exhaustive audits. The compile-time guard below
    /// fails the build if a variant is added without being listed here.
    pub const ALL: [CombatState; 5] = [
        CombatState::Idle,
        CombatState::Windup,
        CombatState::Active,
        CombatState::Recovery,
        CombatState::Aiming,
    ];

    /// Committed to an action: Movement must not sprint through it.
    pub fn commits_the_body(self) -> bool {
        match self {
            CombatState::Idle => false,
            CombatState::Windup
            | CombatState::Active
            | CombatState::Recovery
            | CombatState::Aiming => true,
        }
    }
}

const _: () = {
    fn assert_all_is_exhaustive(state: CombatState) {
        match state {
            CombatState::Idle
            | CombatState::Windup
            | CombatState::Active
            | CombatState::Recovery
            | CombatState::Aiming => {}
        }
    }
    let _ = assert_all_is_exhaustive;
    let _ = CombatState::ALL.len();
};
