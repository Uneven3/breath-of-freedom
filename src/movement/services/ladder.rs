//! Ladder service — reports whether the body overlaps a ladder trigger region.
//!
//! Same AABB-overlap approach as the stairs service.

use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::facts::LadderFacts;
use crate::movement::lod::SensingLod;
use crate::world::Ladder;

pub fn ladder_service(
    mut actors: Query<(&Transform, &mut LadderFacts, Option<&SensingLod>), With<Actor>>,
    ladders: Query<&Ladder>,
) {
    for (transform, mut facts, lod) in &mut actors {
        if SensingLod::skips(lod) {
            continue;
        }
        let pos = transform.translation;

        *facts = LadderFacts::default();
        for ladder in &ladders {
            if ladder.contains(pos) {
                facts.on_ladder = true;
                facts.bottom_y = ladder.bottom.y;
                facts.top_y = ladder.top.y;
                facts.body_anchor_xz = Vec2::new(ladder.body_anchor.x, ladder.body_anchor.z);
                facts.outward_normal = ladder.outward_normal;
                break;
            }
        }
    }
}
