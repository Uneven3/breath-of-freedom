//! Health data: the pool component and the damage/death contract.
//!
//! Pure data (Constitución §6/§19) — the only mutation paths are `Health`'s
//! own methods, same pattern as `movement::Stamina`.

use bevy::prelude::*;

/// Hit-point pool for any actor with a life (player, enemies, practice
/// targets, future mounts). Private fields: only `apply_damage`/`heal_full`
/// mutate it.
#[derive(Component, Debug, Clone)]
pub struct Health {
    current: f32,
    max: f32,
}

impl Health {
    pub fn new(max: f32) -> Self {
        Self { current: max, max }
    }

    /// Subtracts up to `amount`, clamped at zero. Returns what was actually
    /// applied.
    pub fn apply_damage(&mut self, amount: f32) -> f32 {
        let applied = amount.clamp(0.0, self.current);
        self.current -= applied;
        applied
    }

    pub fn heal_full(&mut self) {
        self.current = self.max;
    }

    pub fn is_dead(&self) -> bool {
        self.current <= 0.0
    }

    pub fn current(&self) -> f32 {
        self.current
    }

    pub fn max(&self) -> f32 {
        self.max
    }
}

/// Ask Health to damage `target`. Any system emits it (Combat melee,
/// Projectiles, future hazards); Health validates that the target has a
/// living `Health` and applies it. Owned by Health — the receiver owns the
/// contract. (`source` attribution and `kind` join when a consumer reads
/// them — no field before a system reads it.)
#[derive(Message, Debug, Clone, Copy)]
pub struct DamageRequestMessage {
    pub target: Entity,
    pub amount: f32,
}

// `DamageAppliedMessage { target, amount, remaining }` — the channel systems
// reacting to received damage will listen to (Staggered in `combat-defense`,
// Flee when the enemy brain reads its own Health) — lands with its first
// consumer: no message before a system reads it.

/// `current` crossed to zero. Emitted exactly once per death; what happens
/// next (despawn, respawn, loot) belongs to each actor's owning system,
/// never to Health.
#[derive(Message, Debug, Clone, Copy)]
pub struct DeathMessage {
    pub entity: Entity,
}
