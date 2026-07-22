use avian3d::prelude::Collider;
use bevy::prelude::*;

pub const CHARGE_RADIUS: f32 = 0.9;
const MAX_CHARGE_HITS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ChargeHitKey {
    pub horse: Entity,
    pub generation: u64,
    pub enemy: Entity,
}

#[derive(Resource)]
pub struct ChargeHitLedger {
    hits: [Option<ChargeHitKey>; MAX_CHARGE_HITS],
    len: usize,
}

impl Default for ChargeHitLedger {
    fn default() -> Self {
        Self {
            hits: [None; MAX_CHARGE_HITS],
            len: 0,
        }
    }
}

impl ChargeHitLedger {
    pub(crate) fn contains(&self, key: ChargeHitKey) -> bool {
        self.hits[..self.len].contains(&Some(key))
    }

    pub(crate) fn insert(&mut self, key: ChargeHitKey) -> bool {
        if self.len == self.hits.len() {
            return false;
        }
        self.hits[self.len] = Some(key);
        self.len += 1;
        true
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(ChargeHitKey) -> bool) {
        let mut write = 0;
        for read in 0..self.len {
            let Some(key) = self.hits[read] else {
                continue;
            };
            if keep(key) {
                self.hits[write] = Some(key);
                write += 1;
            }
        }
        self.hits[write..self.len].fill(None);
        self.len = write;
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.len
    }
}

#[derive(Resource)]
pub struct ChargeShape(pub Collider);

impl Default for ChargeShape {
    fn default() -> Self {
        Self(Collider::sphere(CHARGE_RADIUS))
    }
}
