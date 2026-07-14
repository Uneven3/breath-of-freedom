//! Ladder service — reports whether the body overlaps a ladder trigger region.
//!
//! Same AABB-overlap approach as the stairs service.

use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::facts::LadderFacts;
use crate::world::Ladder;

pub fn ladder_service(
    mut actors: Query<(&Transform, &mut LadderFacts), With<Actor>>,
    ladders: Query<&Ladder>,
) {
    for (transform, mut facts) in &mut actors {
        let pos = transform.translation;

        *facts = LadderFacts::default();
        for ladder in &ladders {
            if ladder.contains(pos) {
                facts.on_ladder = true;
                facts.top_y = ladder.top.y;
                facts.anchor_xz = Vec2::new(ladder.bottom.x, ladder.bottom.z);
                break;
            }
        }
    }
}
