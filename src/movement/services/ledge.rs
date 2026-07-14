//! Ledge service — the wall/ledge/vault sensor suite.
//!
//! We issue `SpatialQuery::cast_shape`/`cast_ray` calls each frame for
//! wall/ledge/vault detection (6 forward sphere casts at ankle→head heights,
//! a mantle down-cast, a vault down-cast, two lateral wall rays). Output
//! lands in `LedgeFacts`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::body::BodyDimensions;
use crate::movement::diag::CastTrace;
use crate::movement::facts::LedgeFacts;
use crate::movement::sensing::LedgeSensing;
use crate::movement::state::LocomotionState;
use crate::world::NonClimbable;

const MIN_DIR_SQ: f32 = 0.001;
/// Debug labels for the profiling casts, index-aligned with
/// `LedgeSensing::height_samples`.
const H_CAST_LABELS: [&str; 6] = [
    "ledge_ankle",
    "ledge_knee",
    "ledge_waist",
    "ledge_chest",
    "ledge_limit",
    "ledge_head",
];

/// One forward profiling hit: world contact point + surface normal.
#[derive(Clone, Copy)]
struct Hit {
    entity: Entity,
    point: Vec3,
    normal: Vec3,
}

struct LedgeActor<'a> {
    entity: Entity,
    transform: &'a Transform,
    state: LocomotionState,
    body: BodyDimensions,
    sensing: LedgeSensing,
}

type LedgeServiceQuery<'a> = (
    Entity,
    &'a Transform,
    &'a LocomotionState,
    &'a BodyDimensions,
    &'a LedgeSensing,
    &'a mut LedgeFacts,
);

pub fn ledge_service(
    spatial: SpatialQuery,
    mut q: Query<LedgeServiceQuery, With<Actor>>,
    non_climbable: Query<(), With<NonClimbable>>,
    mut trace: ResMut<CastTrace>,
) {
    for (entity, transform, state, body, sensing, mut facts) in &mut q {
        sense_ledges(
            &spatial,
            LedgeActor {
                entity,
                transform,
                state: *state,
                body: *body,
                sensing: *sensing,
            },
            &mut facts,
            &non_climbable,
            &mut trace,
        );
    }
}

fn sense_ledges(
    spatial: &SpatialQuery,
    actor: LedgeActor,
    facts: &mut LedgeFacts,
    non_climbable: &Query<(), With<NonClimbable>>,
    trace: &mut CastTrace,
) {
    let pos = actor.transform.translation;
    let state = actor.state;
    let body = actor.body;
    let sensing = actor.sensing;
    *facts = LedgeFacts::default();

    let mut facing = actor.transform.rotation * Vec3::NEG_Z;
    facing.y = 0.0;
    facing = if facing.length_squared() > MIN_DIR_SQ {
        facing.normalize()
    } else {
        Vec3::NEG_Z
    };

    let filter = SpatialQueryFilter::from_excluded_entities([actor.entity]);
    let sphere = Collider::sphere(sensing.sphere_radius);
    let facing_dir = Dir3::new(facing).unwrap_or(Dir3::NEG_Z);
    let down = Dir3::NEG_Y;

    // --- 6 forward profiling casts (ankle → head) ---
    let mut hits: [Option<Hit>; 6] = [None; 6];
    let mut min_dist = sensing.wall_detection_reach;
    for (i, &y) in sensing.height_samples.iter().enumerate() {
        let origin = pos + Vec3::new(0.0, y, 0.0);
        let h = spatial.cast_shape(
            &sphere,
            origin,
            Quat::IDENTITY,
            facing_dir,
            &ShapeCastConfig::from_max_distance(sensing.wall_detection_reach),
            &filter,
        );
        trace.record_shape(
            actor.entity,
            H_CAST_LABELS[i],
            origin,
            facing,
            sensing.wall_detection_reach,
            h.map(|h| (h.point1, h.normal1)),
        );
        if let Some(h) = h {
            hits[i] = Some(Hit {
                entity: h.entity,
                point: h.point1,
                normal: h.normal1,
            });
            min_dist = min_dist.min(h.point1.distance(origin));
        }
    }

    let feet_y = pos.y - body.standing_half_height();

    // --- Mantle lip down-cast ---
    let down_origin = pos
        + facing * sensing.forward_sample_offset
        + Vec3::Y * (sensing.mantle_max_height + sensing.down_cast_margin);
    let down_hit = spatial.cast_shape(
        &sphere,
        down_origin,
        Quat::IDENTITY,
        down,
        &ShapeCastConfig::from_max_distance(sensing.mantle_max_height + sensing.down_cast_margin),
        &filter,
    );
    trace.record_shape(
        actor.entity,
        "mantle_down",
        down_origin,
        Vec3::NEG_Y,
        sensing.mantle_max_height + sensing.down_cast_margin,
        down_hit.map(|h| (h.point1, h.normal1)),
    );

    // --- Vault down-cast (positioned just past the nearest wall hit) ---
    let v_dist = min_dist + sensing.vault_distance_margin;
    let vault_down_origin = pos
        + facing * v_dist
        + Vec3::Y * (sensing.vault_detection_range + sensing.down_cast_margin);
    let vault_down_hit = spatial.cast_shape(
        &sphere,
        vault_down_origin,
        Quat::IDENTITY,
        down,
        &ShapeCastConfig::from_max_distance(
            sensing.vault_detection_range + sensing.down_cast_margin + body.standing_half_height(),
        ),
        &filter,
    );
    trace.record_shape(
        actor.entity,
        "vault_down",
        vault_down_origin,
        Vec3::NEG_Y,
        sensing.vault_detection_range + sensing.down_cast_margin + body.standing_half_height(),
        vault_down_hit.map(|h| (h.point1, h.normal1)),
    );

    // --- Vault detection ---
    detect_vault(facts, &actor, facing, &hits, feet_y, vault_down_hit);

    // --- Mantle detection ---
    if let Some(h) = down_hit {
        let mantle_rel_y = h.point1.y - feet_y;
        if mantle_rel_y > 0.0 && mantle_rel_y <= sensing.mantle_max_height {
            facts.mantle_ledge_point = Some(h.point1);
            facts.is_at_mantle_edge = pos.y
                >= (h.point1.y - sensing.mantle_edge_body_offset) - sensing.mantle_edge_tolerance;
            // climb_normal is still unset at this point in the pass, so the
            // mantle forward direction falls back to facing.
            let fwd = facing;
            let mut target = pos + fwd * (body.radius * sensing.mantle_forward_radius_multiplier);
            target.y = h.point1.y + body.standing_half_height() + sensing.mantle_surface_clearance;
            facts.mantle_target_position = Some(target);
        }
    }

    // --- Climb detection (waist hit = index 2) ---
    let knee_hit = hits[1].is_some();
    let head_hit = hits[5].is_some();
    facts.has_head_hit = head_hit;
    if let Some(waist) = hits[2] {
        facts.wall_point = Some(waist.point);
        let angle = facing.angle_between(-waist.normal).to_degrees();
        let climbable = non_climbable.get(waist.entity).is_err();
        // Initial attachment must face the wall, but an actor already in Climb
        // owns a wall-facing yaw in the motor. Do not drop that attachment just
        // because a curved surface or a lateral move briefly makes the sampled
        // normal fall outside the entry cone: a waist hit on climbable geometry
        // is sufficient evidence to continue.
        if can_continue_climb(state, climbable, angle, sensing) {
            facts.climb_normal = Some(waist.normal);
            facts.can_continue_climb = true;
            update_lateral_walls(spatial, &filter, facts, &actor, waist.normal, trace);
        }
        if climbable && angle <= sensing.climb_wall_angle_max_deg && knee_hit && head_hit {
            facts.can_climb = true;
        }
    }

    // --- Lip height ---
    if let Some(h) = down_hit {
        facts.lip_height = h.point1.y - feet_y;
    }
}

fn can_continue_climb(
    state: LocomotionState,
    climbable: bool,
    facing_angle: f32,
    sensing: LedgeSensing,
) -> bool {
    climbable
        && (state == LocomotionState::Climb || facing_angle <= sensing.continue_climb_angle_max_deg)
}

fn detect_vault(
    facts: &mut LedgeFacts,
    actor: &LedgeActor,
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
            .map(|h| h.normal.y < actor.sensing.steep_face_normal_y_max)
            .unwrap_or(false)
    });
    if !steep_enough {
        return;
    }

    if let Some(h) = vault_down_hit {
        let lip = h.point1;
        let rel_y = lip.y - feet_y;
        if (actor.sensing.vault_min_height..=actor.sensing.vault_detection_range).contains(&rel_y) {
            facts.is_vaultable = true;
            // "Step-up" vault: place the body slightly over the lip.
            let vault_forward = actor.body.radius * actor.sensing.vault_forward_radius_multiplier;
            let mut target = actor.transform.translation + facing * vault_forward;
            target.y =
                lip.y + actor.body.standing_half_height() + actor.sensing.vault_surface_clearance;
            facts.vault_target_position = Some(target);
        }
    }
}

fn update_lateral_walls(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    facts: &mut LedgeFacts,
    actor: &LedgeActor,
    climb_normal: Vec3,
    trace: &mut CastTrace,
) {
    let right_dir = Vec3::Y.cross(climb_normal).normalize_or_zero();
    let cast_dir = Dir3::new(-climb_normal).unwrap_or(Dir3::NEG_Z);
    let left_origin = actor.transform.translation - right_dir * 0.45;
    let right_origin = actor.transform.translation + right_dir * 0.45;
    let left_hit = spatial.cast_ray(
        left_origin,
        cast_dir,
        actor.sensing.lateral_cast_reach,
        true,
        filter,
    );
    let right_hit = spatial.cast_ray(
        right_origin,
        cast_dir,
        actor.sensing.lateral_cast_reach,
        true,
        filter,
    );
    trace.record_ray(
        actor.entity,
        "wall_ray_left",
        left_origin,
        *cast_dir,
        actor.sensing.lateral_cast_reach,
        left_hit.map(|h| (left_origin + *cast_dir * h.distance, h.normal)),
    );
    trace.record_ray(
        actor.entity,
        "wall_ray_right",
        right_origin,
        *cast_dir,
        actor.sensing.lateral_cast_reach,
        right_hit.map(|h| (right_origin + *cast_dir * h.distance, h.normal)),
    );
    facts.has_wall_left = left_hit.is_some();
    facts.has_wall_right = right_hit.is_some();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn active_climb_keeps_a_valid_wall_at_any_facing_angle() {
        assert!(can_continue_climb(
            LocomotionState::Climb,
            true,
            89.0,
            LedgeSensing::PLAYER,
        ));
    }

    #[test]
    fn inactive_actor_still_uses_the_continuation_cone() {
        assert!(can_continue_climb(
            LocomotionState::Walk,
            true,
            45.0,
            LedgeSensing::PLAYER,
        ));
        assert!(!can_continue_climb(
            LocomotionState::Walk,
            true,
            45.1,
            LedgeSensing::PLAYER,
        ));
    }

    #[test]
    fn non_climbable_wall_never_continues() {
        assert!(!can_continue_climb(
            LocomotionState::Climb,
            false,
            0.0,
            LedgeSensing::PLAYER,
        ));
    }
}
