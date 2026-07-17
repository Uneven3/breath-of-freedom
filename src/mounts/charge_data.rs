use std::collections::HashSet;

use avian3d::prelude::Collider;
use bevy::prelude::*;

pub const CHARGE_RADIUS: f32 = 0.9;

#[derive(Resource, Default)]
pub struct ChargeHitLedger {
    pub(crate) hits: HashSet<(Entity, u64, Entity)>,
}

#[derive(Resource)]
pub struct ChargeShape(pub Collider);

impl Default for ChargeShape {
    fn default() -> Self {
        Self(Collider::sphere(CHARGE_RADIUS))
    }
}
