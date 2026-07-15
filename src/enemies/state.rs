//! The single source of truth for an enemy's AI decision.
//!
//! Upstream of `LocomotionState` (and of the future `CombatState`): the brain
//! decides *what the enemy wants*, Movement decides *how the body does it*.
//! An enum component, not markers — the states are mutually exclusive, and
//! only `brain::decide` writes it (see
//! `rationale/per-entity-state-idioms.md`).
//!
//! `Combat` and `Flee` (see `docs/architecture/enemies.md`) join this enum
//! when the Combat and Health systems exist — adding a variant is a compile
//! error until `brain::act` handles it, same contract as `LocomotionState`.

use bevy::prelude::*;

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EnemyAiState {
    /// Wander around `Home`, pausing between waypoints.
    #[default]
    Patrol,
    /// A target is visible: close the distance.
    Alert,
    /// Target lost: head to its last known position, then give up.
    Search,
}
