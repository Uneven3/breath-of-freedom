//! Ledge service — the wall/ledge/vault sensor suite.
//!
//! We issue `SpatialQuery::cast_shape`/`cast_ray` calls each frame for
//! wall/ledge/vault detection (6 forward sphere casts at ankle→head heights,
//! a mantle down-cast, a vault down-cast, a landing cast, two lateral wall
//! rays). Output lands in `LedgeFacts`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Player;
use crate::movement::facts::LedgeFacts;

const MIN_DIR_SQ: f32 = 0.001;
/// Ankle → head profiling heights.
const H_CAST_Y_OFFSETS: [f32; 6] = [-0.8, -0.6, -0.2, 0.2, 0.4, 0.6];
const SPHERE_RADIUS: f32 = 0.1;
const WALL_DETECTION_REACH: f32 = 0.65;
const DOWN_CAST_MARGIN: f32 = 0.1;
const FORWARD_SAMPLE_OFFSET: f32 = 1.0;
const VAULT_DIST_MARGIN: f32 = 0.2;
const STEEP_FACE_NORMAL_Y_MAX: f32 = 0.75;
const VAULT_FORWARD_RADIUS_MULT: f32 = 1.5;

const VAULT_DETECTION_RANGE: f32 = 1.4;
const VAULT_MIN_HEIGHT: f32 = 0.3;
const VAULT_BODY_HALF_HEIGHT: f32 = 1.0;
const VAULT_LANDING_PROBE_DISTANCE: f32 = 1.6;
const VAULT_LANDING_PROBE_HEIGHT: f32 = 2.0;
const VAULT_SURFACE_CLEARANCE: f32 = 0.08;
const MANTLE_MAX_HEIGHT: f32 = 2.5;
const LATERAL_CAST_REACH: f32 = 1.5;
const MANTLE_BODY_HALF_HEIGHT: f32 = 1.0;
const MANTLE_BODY_RADIUS: f32 = 0.5;
const MANTLE_FORWARD_RADIUS_MULTIPLIER: f32 = 2.0;
const MANTLE_SURFACE_CLEARANCE: f32 = 0.08;
const MANTLE_EDGE_BODY_OFFSET: f32 = 0.33;
const MANTLE_EDGE_TOLERANCE: f32 = 0.05;
const CLIMB_WALL_ANGLE_MAX_DEG: f32 = 30.0;
const CONTINUE_CLIMB_ANGLE_MAX_DEG: f32 = 45.0;

/// One forward profiling hit: world contact point + surface normal.
#[derive(Clone, Copy)]
struct Hit {
    point: Vec3,
    normal: Vec3,
}

pub fn ledge_service(
    spatial: SpatialQuery,
    mut q: Single<(Entity, &Transform, &mut LedgeFacts), With<Player>>,
) {
    let (entity, transform, facts) = &mut *q;
    let pos = transform.translation;
    **facts = LedgeFacts::default();

    let mut facing = transform.rotation * Vec3::NEG_Z;
    facing.y = 0.0;
    facing = if facing.length_squared() > MIN_DIR_SQ {
        facing.normalize()
    } else {
        Vec3::NEG_Z
    };

    let filter = SpatialQueryFilter::from_excluded_entities([*entity]);
    let sphere = Collider::sphere(SPHERE_RADIUS);
    let facing_dir = Dir3::new(facing).unwrap_or(Dir3::NEG_Z);
    let down = Dir3::NEG_Y;

    // --- 6 forward profiling casts (ankle → head) ---
    let mut hits: [Option<Hit>; 6] = [None; 6];
    let mut min_dist = WALL_DETECTION_REACH;
    for (i, &y) in H_CAST_Y_OFFSETS.iter().enumerate() {
        let origin = pos + Vec3::new(0.0, y, 0.0);
        if let Some(h) = spatial.cast_shape(
            &sphere,
            origin,
            Quat::IDENTITY,
            facing_dir,
            &ShapeCastConfig::from_max_distance(WALL_DETECTION_REACH),
            &filter,
        ) {
            hits[i] = Some(Hit {
                point: h.point1,
                normal: h.normal1,
            });
            min_dist = min_dist.min(h.point1.distance(origin));
        }
    }

    let feet_y = pos.y - VAULT_BODY_HALF_HEIGHT;

    // --- Mantle lip down-cast ---
    let down_origin =
        pos + facing * FORWARD_SAMPLE_OFFSET + Vec3::Y * (MANTLE_MAX_HEIGHT + DOWN_CAST_MARGIN);
    let down_hit = spatial.cast_shape(
        &sphere,
        down_origin,
        Quat::IDENTITY,
        down,
        &ShapeCastConfig::from_max_distance(MANTLE_MAX_HEIGHT + DOWN_CAST_MARGIN),
        &filter,
    );

    // --- Vault landing cast ---
    let landing_origin =
        pos + facing * VAULT_LANDING_PROBE_DISTANCE + Vec3::Y * VAULT_LANDING_PROBE_HEIGHT;
    let landing_hit = spatial.cast_shape(
        &sphere,
        landing_origin,
        Quat::IDENTITY,
        down,
        &ShapeCastConfig::from_max_distance(VAULT_LANDING_PROBE_HEIGHT + 0.2),
        &filter,
    );

    // --- Vault down-cast (positioned just past the nearest wall hit) ---
    let v_dist = min_dist + VAULT_DIST_MARGIN;
    let vault_down_origin =
        pos + facing * v_dist + Vec3::Y * (VAULT_DETECTION_RANGE + DOWN_CAST_MARGIN);
    let vault_down_hit = spatial.cast_shape(
        &sphere,
        vault_down_origin,
        Quat::IDENTITY,
        down,
        &ShapeCastConfig::from_max_distance(
            VAULT_DETECTION_RANGE + DOWN_CAST_MARGIN + VAULT_BODY_HALF_HEIGHT,
        ),
        &filter,
    );

    // --- Vault detection ---
    detect_vault(facts, pos, facing, &hits, feet_y, vault_down_hit);

    // --- Mantle detection ---
    if let Some(h) = down_hit {
        let mantle_rel_y = h.point1.y - feet_y;
        if mantle_rel_y > 0.0 && mantle_rel_y <= MANTLE_MAX_HEIGHT {
            facts.mantle_ledge_point = h.point1;
            facts.is_at_mantle_edge =
                pos.y >= (h.point1.y - MANTLE_EDGE_BODY_OFFSET) - MANTLE_EDGE_TOLERANCE;
            // climb_normal is still zero at this point in the pass, so the mantle
            // forward direction falls back to facing.
            let fwd = facing;
            facts.mantle_target_position =
                pos + fwd * (MANTLE_BODY_RADIUS * MANTLE_FORWARD_RADIUS_MULTIPLIER);
            facts.mantle_target_position.y =
                facts.mantle_ledge_point.y + MANTLE_BODY_HALF_HEIGHT + MANTLE_SURFACE_CLEARANCE;
        }
    }

    // --- Climb detection (waist hit = index 2) ---
    let knee_hit = hits[1].is_some();
    let head_hit = hits[5].is_some();
    facts.has_head_hit = head_hit;
    if let Some(waist) = hits[2] {
        facts.wall_point = waist.point;
        let angle = facing.angle_between(-waist.normal).to_degrees();
        if angle <= CLIMB_WALL_ANGLE_MAX_DEG {
            facts.climb_normal = waist.normal;
            if knee_hit && head_hit {
                facts.can_climb = true;
                update_lateral_walls(&spatial, &filter, facts, pos);
            }
        }
        // can_continue_climbing uses the same waist hit with a looser angle gate.
        if facing.angle_between(-waist.normal).to_degrees() <= CONTINUE_CLIMB_ANGLE_MAX_DEG {
            facts.can_continue_climb = true;
        }
    }

    // --- Lip / landing heights ---
    if let Some(h) = down_hit {
        facts.lip_height = h.point1.y - feet_y;
    }
    if let Some(h) = landing_hit {
        facts.landing_height = h.point1.y - feet_y;
        facts.is_occupied = true;
    }
}

fn detect_vault(
    facts: &mut LedgeFacts,
    pos: Vec3,
    facing: Vec3,
    hits: &[Option<Hit>; 6],
    feet_y: f32,
    vault_down_hit: Option<ShapeHitData>,
) {
    // Ankle(0) Knee(1) Waist(2) Chest(3) hit, Limit(4) and Head(5) miss.
    let obstacle_hit =
        hits[0].is_some() || hits[1].is_some() || hits[2].is_some() || hits[3].is_some();
    if !(obstacle_hit && hits[4].is_none() && hits[5].is_none()) {
        return;
    }

    let steep_enough = (0..4).any(|i| {
        hits[i]
            .map(|h| h.normal.y < STEEP_FACE_NORMAL_Y_MAX)
            .unwrap_or(false)
    });
    if !steep_enough {
        return;
    }

    if let Some(h) = vault_down_hit {
        let lip = h.point1;
        let rel_y = lip.y - feet_y;
        if rel_y >= VAULT_MIN_HEIGHT && rel_y <= VAULT_DETECTION_RANGE {
            facts.is_vaultable = true;
            // "Step-up" vault: place the body slightly over the lip.
            let vault_forward = MANTLE_BODY_RADIUS * VAULT_FORWARD_RADIUS_MULT;
            facts.vault_target_position = pos + facing * vault_forward;
            facts.vault_target_position.y =
                lip.y + VAULT_BODY_HALF_HEIGHT + VAULT_SURFACE_CLEARANCE;
        }
    }
}

fn update_lateral_walls(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    facts: &mut LedgeFacts,
    pos: Vec3,
) {
    let right_dir = Vec3::Y.cross(facts.climb_normal).normalize_or_zero();
    let cast_dir = Dir3::new(-facts.climb_normal).unwrap_or(Dir3::NEG_Z);
    let left_origin = pos - right_dir * 0.45;
    let right_origin = pos + right_dir * 0.45;
    facts.has_wall_left = spatial
        .cast_ray(left_origin, cast_dir, LATERAL_CAST_REACH, true, filter)
        .is_some();
    facts.has_wall_right = spatial
        .cast_ray(right_origin, cast_dir, LATERAL_CAST_REACH, true, filter)
        .is_some();
}
