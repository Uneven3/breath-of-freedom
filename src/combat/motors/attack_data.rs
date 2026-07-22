//! Pure per-actor melee state and message contracts.

use avian3d::prelude::Collider;
use bevy::prelude::*;

use crate::combat::state::CombatState;
use crate::combat::weapon::{AttackStep, MAX_COMBO_STEPS, WeaponProfile};

#[derive(Component, Default)]
pub struct ComboLocal {
    pub(crate) step: usize,
    pub(crate) phase_elapsed: f32,
    pub(super) buffered: bool,
    pub(super) last_phase: CombatState,
    pub(super) snapshot: Option<WeaponProfile>,
}

impl ComboLocal {
    pub(crate) fn current_step(&self) -> Option<&AttackStep> {
        self.snapshot.as_ref()?.step(self.step)
    }
}

#[derive(Component, Default)]
pub struct ActiveSwing {
    hit: [Option<Entity>; 8],
    len: usize,
}

impl ActiveSwing {
    pub(super) fn contains(&self, entity: Entity) -> bool {
        self.hit[..self.len]
            .iter()
            .flatten()
            .any(|seen| *seen == entity)
    }

    pub(super) fn insert(&mut self, entity: Entity) -> bool {
        if self.len == self.hit.len() {
            warn!("ActiveSwing full: dropping hit on {entity:?}");
            return false;
        }
        self.hit[self.len] = Some(entity);
        self.len += 1;
        true
    }

    pub(crate) fn clear(&mut self) {
        self.hit = [None; 8];
        self.len = 0;
    }
}

#[derive(Resource)]
pub struct MeleeSweepShapes(Vec<(u32, Collider)>);

impl Default for MeleeSweepShapes {
    fn default() -> Self {
        let mut shapes = Vec::with_capacity(MAX_COMBO_STEPS * 2);
        for profile in WeaponProfile::ALL {
            for step_index in 0..MAX_COMBO_STEPS {
                let Some(step) = profile.step(step_index) else {
                    break;
                };
                let radius = step.reach * 0.5;
                if shapes.iter().all(|(known, _)| *known != radius.to_bits()) {
                    shapes.push((radius.to_bits(), Collider::sphere(radius)));
                }
            }
        }
        Self(shapes)
    }
}

impl MeleeSweepShapes {
    pub(super) fn get(&self, radius: f32) -> Option<&Collider> {
        self.0
            .iter()
            .find(|(bits, _)| *bits == radius.to_bits())
            .map(|(_, shape)| shape)
    }
}

#[derive(Message, Debug, Clone, Copy)]
pub struct MeleeHitMessage {
    pub attacker: Entity,
    pub target: Entity,
    pub attacker_pos: Vec3,
    pub step: usize,
}

#[derive(Message, Debug, Clone, Copy)]
pub struct HitImpactMessage {
    pub target: Entity,
    pub attacker: Entity,
    pub position: Vec3,
    pub damage: f32,
    pub critical: bool,
    pub melee: bool,
}
