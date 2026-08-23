//! Ledge service — the wall/ledge/vault sensor suite.
//!
//! We issue `SpatialQuery::cast_shape`/`cast_ray` calls each frame for
//! wall/ledge/vault detection (6 forward sphere casts at ankle→head heights,
//! a mantle down-cast, a vault down-cast, two lateral wall rays). Output
//! lands in `LedgeFacts`.

use avian3d::prelude::*;
use bevy_ecs::prelude::*;
use bevy_math::prelude::*;
use bevy_transform::prelude::*;

use crate::movement::Actor;
use crate::movement::body::BodyDimensions;
use crate::movement::diag::CastTrace;
use crate::movement::facts::LedgeFacts;
use crate::movement::lod::SensingLod;
use crate::movement::motor_common::FLOOR_MIN_UP_DOT;
use crate::movement::sensing::{LedgeCastShape, LedgeSensing};
use crate::movement::state::LocomotionState;
use crate::world::{GameLayer, NonClimbable};

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
    shape: &'a Collider,
}

type LedgeServiceQuery<'a> = (
    Entity,
    &'a Transform,
    &'a LocomotionState,
    &'a BodyDimensions,
    &'a LedgeSensing,
    &'a LedgeCastShape,
    &'a mut LedgeFacts,
    Option<&'a SensingLod>,
);

pub fn ledge_service(
    spatial: SpatialQuery,
    mut q: Query<
        LedgeServiceQuery,
        (
            With<Actor>,
            With<crate::movement::attachment::LocomotionEnabled>,
        ),
    >,
    non_climbable: Query<(), With<NonClimbable>>,
    mut trace: ResMut<CastTrace>,
) {
    for (entity, transform, state, body, sensing, shape, mut facts, lod) in &mut q {
        if SensingLod::skips(lod) {
            continue;
        }
        sense_ledges(
            &spatial,
            LedgeActor {
                entity,
                transform,
                state: *state,
                body: *body,
                sensing: *sensing,
                shape: &shape.0,
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

    // Mask to `Default` (world geometry): actors live on `GameLayer::Actor`,
    // so no capsule — player, probe, future enemies — reads as climbable
    // wall, mantle lip, or vault obstacle. Bodies still collide physically.
    let filter =
        SpatialQueryFilter::from_mask(GameLayer::Default).with_excluded_entities([actor.entity]);
    let sphere = actor.shape;
    let facing_dir = Dir3::new(facing).unwrap_or(Dir3::NEG_Z);
    let down = Dir3::NEG_Y;

    // --- 6 forward profiling casts (ankle → head) ---
    let mut hits: [Option<Hit>; 6] = [None; 6];
    let mut min_dist = sensing.wall_detection_reach;
    for (i, &y) in sensing.height_samples.iter().enumerate() {
        let origin = pos + Vec3::new(0.0, y, 0.0);
        let h = spatial.cast_shape(
            sphere,
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
        sphere,
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
        sphere,
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
        if climbable && faces_the_wall(facing, waist.normal, &sensing) && knee_hit
            && (head_hit || leans_back_out_of_reach(waist.normal, facts))
        {
            facts.can_climb = true;
        }
    }

    // --- Lip height ---
    if let Some(h) = down_hit {
        facts.lip_height = h.point1.y - feet_y;
    }
}

/// ¿El actor mira la pared? **Sólo guiñada**, con la normal aplanada al plano
/// horizontal.
///
/// `facing.angle_between(-normal)` mezcla dos cosas distintas: cuánto se
/// desvía el actor y cuán inclinada está la cara. Para una cara de θ perfecta
/// encarada de frente ese ángulo vale `90 - θ`, así que a 60° gastaba los 30°
/// de cono enteros en la inclinación y no dejaba **ni un grado** de tolerancia
/// de guiñada. Separarlos es lo que hace alcanzable la regla "si no se puede
/// caminar, se puede escalar".
fn faces_the_wall(facing: Vec3, normal: Vec3, sensing: &LedgeSensing) -> bool {
    let Some(into_wall) = Dir3::new(Vec3::new(-normal.x, 0.0, -normal.z)).ok() else {
        // Normal vertical: es piso o techo, no una pared que encarar.
        return false;
    };
    facing.angle_between(*into_wall).to_degrees() <= sensing.climb_wall_angle_max_deg
}

/// La cara se inclina hacia atrás lo suficiente como para que el cast de la
/// cabeza no pueda alcanzarla, y no es un obstáculo bajo.
///
/// Los seis casts salen del eje del cuerpo con alcance fijo, así que en una
/// cara inclinada la superficie se aleja `Δaltura / tan(θ)` y el de la cabeza
/// falla: medido el 2026-08-22, el umbral efectivo para `head_hit` es ~77°
/// aunque la configuración declare 60. Sin esto, todo el terreno esculpido —que
/// llega a 67-81°— quedaba imposible de escalar aunque también fuera imposible
/// de caminar.
///
/// `lip_height` es el discriminador que reemplaza al cast que falta: viene del
/// down-cast de mantle un metro adelante, y separa una cara alta de un bordillo
/// igual que `head_hit` lo hacía. Por eso no se afloja el contrato de vault.
fn leans_back_out_of_reach(normal: Vec3, facts: &LedgeFacts) -> bool {
    let up = normal.y.clamp(-1.0, 1.0);
    let too_steep_to_walk = up < FLOOR_MIN_UP_DOT;
    too_steep_to_walk && up >= 0.0 && !facts.is_vaultable
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

#[cfg(test)]
mod sloped_wall_tests {
    use super::*;

    /// Normal de una cara inclinada `degrees` sobre la horizontal, encarada
    /// hacia -Z (el actor mira a -Z por defecto).
    fn face_normal(degrees: f32) -> Vec3 {
        let t = degrees.to_radians();
        Vec3::new(0.0, t.cos(), t.sin())
    }

    fn tall_face() -> LedgeFacts {
        LedgeFacts {
            is_vaultable: false,
            ..LedgeFacts::default()
        }
    }

    /// **El bug del 2026-08-22.** El cono de guiñada se gastaba entero en la
    /// inclinación: `facing.angle_between(-normal)` vale `90 - θ`, así que a
    /// 60° daba exactamente los 30° del límite y no quedaba **ni un grado**
    /// para desviarse. Es diferencial a propósito — el predicado nuevo tiene
    /// que aceptar la cara inclinada *y* seguir rechazando la guiñada.
    #[test]
    fn a_sloped_face_no_longer_eats_the_whole_yaw_cone() {
        let sensing = LedgeSensing::PLAYER;
        let facing = Vec3::NEG_Z;
        for degrees in [60.0_f32, 65.0, 70.0, 80.0, 90.0] {
            assert!(
                faces_the_wall(facing, face_normal(degrees), &sensing),
                "una cara de {degrees}° encarada de frente tiene que contar como pared"
            );
        }
    }

    #[test]
    fn looking_away_from_the_wall_still_fails() {
        let sensing = LedgeSensing::PLAYER;
        let sideways = Vec3::new(1.0, 0.0, 0.0);
        assert!(
            !faces_the_wall(sideways, face_normal(90.0), &sensing),
            "mirando 90° al costado no se engancha una pared"
        );
    }

    /// Piso y techo no son paredes por más que la guiñada dé.
    #[test]
    fn a_flat_surface_is_never_a_wall_to_face() {
        let sensing = LedgeSensing::PLAYER;
        assert!(!faces_the_wall(Vec3::NEG_Z, Vec3::Y, &sensing));
        assert!(!faces_the_wall(Vec3::NEG_Z, Vec3::NEG_Y, &sensing));
    }

    /// El reemplazo de `head_hit` sólo entra donde el cast no podía llegar: la
    /// cara ya es demasiado empinada para caminarla, y no es un bordillo.
    #[test]
    fn the_head_hit_stand_in_only_covers_unwalkable_faces() {
        assert!(
            leans_back_out_of_reach(face_normal(70.0), &tall_face()),
            "70° no se camina y no es bordillo: tiene que poder escalarse"
        );
        assert!(
            !leans_back_out_of_reach(face_normal(45.0), &tall_face()),
            "45° se camina: no debe convertirse en pared"
        );
    }

    /// El contrato de vault no se afloja: un obstáculo bajo sigue siendo vault
    /// aunque su cara sea vertical.
    #[test]
    fn a_vaultable_obstacle_never_becomes_a_wall() {
        let vaultable = LedgeFacts {
            is_vaultable: true,
            ..LedgeFacts::default()
        };
        assert!(!leans_back_out_of_reach(face_normal(85.0), &vaultable));
    }

    /// Un techo (normal apuntando hacia abajo) no es cara escalable.
    #[test]
    fn an_overhang_is_not_a_climbable_face() {
        assert!(!leans_back_out_of_reach(Vec3::NEG_Y, &tall_face()));
    }
}
