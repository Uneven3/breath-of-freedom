//! Ground service — downward shape-cast grounded probe.
//!
//! Avian's `MoveAndSlide` has no floor snap, so deriving "grounded" from whether the
//! body collided *while moving this tick* flickers on flat ground: Walk zeroes vertical
//! velocity, so the swept move is horizontal-only and never re-touches the floor →
//! grounded flips false → Fall → gravity → contact → grounded true → Walk → repeat.
//!
//! Instead we cast the player's collider straight down a short distance every tick and
//! classify the hit — the idiom from avian's `kinematic_character_3d` example
//! `update_grounded`. This decouples "standing on ground" from "moved into ground" and
//! reads the body's *current* transform (no one-frame contact latency).

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::FLOOR_MIN_UP_DOT;
use crate::movement::{BodyVelocity, Player};

/// How far below the body to look for ground. Matches avian's example default: large
/// enough to bridge the collider skin gap, small enough to clear within ~2 ticks of a
/// jump so it doesn't read the floor while the body is rising off it.
const GROUND_PROBE_DISTANCE: f32 = 0.2;
/// Suppress grounding while rising faster than this (m/s). During a jump's first ticks
/// the body is still within probe range of the floor; without this guard `grounded`
/// would stay true and Walk (PlayerRequested) would out-arbitrate the post-impulse
/// state and zero the upward velocity, cancelling the jump.
const ASCEND_EPSILON: f32 = 0.1;

pub fn ground_service(
    q: Single<
        (
            Entity,
            &Transform,
            &Collider,
            &BodyVelocity,
            &mut GroundFacts,
        ),
        With<Player>,
    >,
    spatial: SpatialQuery,
) {
    let (entity, transform, collider, velocity, mut facts) = q.into_inner();

    let filter = SpatialQueryFilter::from_excluded_entities([entity]);
    let hit = spatial.cast_shape(
        collider,
        transform.translation,
        transform.rotation,
        Dir3::NEG_Y,
        &ShapeCastConfig::from_max_distance(GROUND_PROBE_DISTANCE),
        &filter,
    );

    // A hit counts as floor only if its (world) normal is within the 60° slope limit.
    let floor_normal = hit.and_then(|hit| {
        let normal = transform.rotation * hit.normal1;
        (normal.y > FLOOR_MIN_UP_DOT).then_some(normal)
    });

    let ascending = velocity.0.y > ASCEND_EPSILON;
    facts.grounded = floor_normal.is_some() && !ascending;
    facts.floor_normal = floor_normal.unwrap_or(Vec3::Y);
}
