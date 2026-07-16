//! Weapon data: combos are data, phases are state.
//!
//! A weapon's combo chain is an array of [`AttackStep`]s — adding a weapon is
//! adding a preset const, zero new code (see
//! `rationale/combat-combo-chains.md`). In graybox the profile lives directly
//! on the actor; when Equipment (Inventory) exists, equipping inserts/removes
//! this component — the component IS the "is armed" boolean.

use bevy::prelude::*;

// `WeaponClass` (OneHanded/TwoHanded/Spear, GDD §8) lands with the
// `combat-weapon-classes` phase — no field before a system reads it.

/// One strike of a combo chain. Pure data (Constitución §6).
#[derive(Debug, Clone, Copy)]
pub struct AttackStep {
    /// Committed, not cancelable by attack input.
    pub windup_secs: f32,
    /// Hitbox live.
    pub active_secs: f32,
    /// Follow-through; also hosts the chain window.
    pub recovery_secs: f32,
    /// A buffered press chains into the next step while
    /// `Recovery` has run less than this. `0.0` = chain never (finisher).
    pub chain_window_secs: f32,
    /// Multiplier over `WeaponProfile::base_damage`.
    pub damage_mult: f32,
    /// How far in front of the body the strike reaches (m).
    pub reach: f32,
    /// Horizontal width of the swing, degrees, centered on facing.
    pub arc_deg: f32,
}

pub const MAX_COMBO_STEPS: usize = 4;

/// The wielded weapon: capability + tuning. Presets follow the
/// `GroundMovement::PLAYER` pattern.
#[derive(Component, Debug, Clone, Copy)]
pub struct WeaponProfile {
    pub base_damage: f32,
    combo: [Option<AttackStep>; MAX_COMBO_STEPS],
}

impl WeaponProfile {
    /// Graybox one-handed sword: two quick cuts into a heavier finisher.
    /// Values are first-pass; they get tuned at the `combat-melee-combo`
    /// feeling checkpoint (Constitución §10/§11).
    pub const GRAYBOX_SWORD: Self = Self {
        base_damage: 10.0,
        combo: [
            Some(AttackStep {
                windup_secs: 0.16,
                active_secs: 0.12,
                recovery_secs: 0.30,
                chain_window_secs: 0.28,
                damage_mult: 1.0,
                reach: 1.7,
                arc_deg: 100.0,
            }),
            Some(AttackStep {
                windup_secs: 0.14,
                active_secs: 0.12,
                recovery_secs: 0.32,
                chain_window_secs: 0.28,
                damage_mult: 1.1,
                reach: 1.7,
                arc_deg: 110.0,
            }),
            Some(AttackStep {
                windup_secs: 0.24,
                active_secs: 0.14,
                recovery_secs: 0.50,
                chain_window_secs: 0.0,
                damage_mult: 1.6,
                reach: 2.0,
                arc_deg: 120.0,
            }),
            None,
        ],
    };

    pub fn step(&self, index: usize) -> Option<&AttackStep> {
        self.combo.get(index).and_then(Option::as_ref)
    }

    pub fn has_step(&self, index: usize) -> bool {
        self.step(index).is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn graybox_sword_chain_is_contiguous_and_ends_in_a_finisher() {
        let sword = WeaponProfile::GRAYBOX_SWORD;
        let mut steps = 0;
        while sword.has_step(steps) {
            steps += 1;
        }
        assert!(steps >= 2, "a combo needs at least two steps to chain");
        for i in steps..MAX_COMBO_STEPS {
            assert!(sword.step(i).is_none(), "chain must be contiguous");
        }
        let finisher = sword.step(steps - 1).unwrap();
        assert_eq!(
            finisher.chain_window_secs, 0.0,
            "the last step must not chain"
        );
        assert!(
            finisher.damage_mult > sword.step(0).unwrap().damage_mult,
            "the finisher pays off the commitment"
        );
    }
}
