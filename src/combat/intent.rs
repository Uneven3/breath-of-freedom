//! Per-frame semantic combat intent snapshot.
//!
//! Named distinctly from Movement's `Intents` on purpose (agents without
//! shared session memory must never confuse the two). Controllers (hardware
//! brain, `EnemyBrain`) translate their commands into this component; combat
//! motors consume it without knowing the controller.
//!
//! `wants_guard`/`wants_parry` land with `combat-defense` — fields are added
//! when a motor reads them, never before (dead data is a lie waiting to
//! happen).

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AttackIntent {
    /// Edge: a fresh press this tick (buffered by the attack motor).
    pub pressed: bool,
    /// Level: the button is down (future `Charging` reads this).
    pub held: bool,
}

#[derive(Component, Debug, Clone, Copy, Default)]
pub struct CombatIntents {
    pub attack: AttackIntent,
    /// Level: the aim button is down (bow drawn while held).
    pub wants_aim: bool,
}
