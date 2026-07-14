//! Stairs service — reports whether the body overlaps a stair trigger region.
//!
//! We do a direct AABB overlap test against each `Stairs` marker —
//! deterministic and free of physics-event plumbing. On overlap we copy the
//! stair's geometry into `StairsFacts`.

use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::facts::StairsFacts;
use crate::world::Stairs;

pub fn stairs_service(
    mut actors: Query<(&Transform, &mut StairsFacts), With<Actor>>,
    stairs: Query<&Stairs>,
) {
    for (transform, mut facts) in &mut actors {
        let pos = transform.translation;

        *facts = StairsFacts::default();
        for stair in &stairs {
            if stair.contains(pos) {
                facts.on_stairs = true;
                facts.base = stair.base;
                facts.top = stair.top;
                facts.step_count = stair.step_count;
                facts.step_depth = stair.step_depth;
                facts.step_rise = stair.step_rise;
                break;
            }
        }
    }
}
