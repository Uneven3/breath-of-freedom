//! Ladder service — reports whether the body overlaps a ladder trigger region.
//!
//! Same AABB-overlap approach as the stairs service.

use bevy::prelude::*;

use crate::movement::Player;
use crate::movement::facts::LadderFacts;
use crate::world::Ladder;

pub fn ladder_service(
    mut player: Single<(&Transform, &mut LadderFacts), With<Player>>,
    ladders: Query<&Ladder>,
) {
    let (transform, facts) = &mut *player;
    let pos = transform.translation;

    **facts = LadderFacts::default();
    for ladder in &ladders {
        if ladder.contains(pos) {
            facts.on_ladder = true;
            facts.top_y = ladder.top.y;
            facts.bottom_y = ladder.bottom.y;
            facts.anchor_xz = Vec2::new(ladder.bottom.x, ladder.bottom.z);
            break;
        }
    }
}
