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
use crate::movement::motor_common::is_walkable_floor;
use crate::movement::sensing::{LedgeCastShape, LedgeSensing};
use crate::movement::state::LocomotionState;
use crate::world::{GameLayer, NonClimbable};

const MIN_DIR_SQ: f32 = 0.001;
const WAIST_SAMPLE: usize = 2;
const HEAD_SAMPLE: usize = 5;
/// Los bits 0..5 de `climb_cast_hits` son los seis samples de perfil; éste dice
/// que la cara se encontró **sólo** con el sondeo contra la cara, que es
/// justamente lo que hay que poder distinguir al leer un tick que falló.
const FACE_PROBE_HIT_BIT: u8 = 1 << 6;
/// Por debajo de esto la cara es techo. No es cero porque `90f32.to_radians()`
/// pasa π/2 y deja `cos` en el orden de −1e-7: una pared vertical exacta caería
/// del lado equivocado de la comparación.
const CEILING_NORMAL_Y: f32 = -1e-3;

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
    if let Some(h) = down_hit.filter(|h| is_standable(h.normal1)) {
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

    // --- Climb detection ---
    facts.climb_cast_hits = hits
        .iter()
        .enumerate()
        .filter_map(|(i, hit)| hit.map(|_| 1u8 << i))
        .sum();
    let knee_hit = hits[1].is_some();
    // Se queda con el cast de perfil crudo: `climb.rs` lo usa como veto de
    // ápice (`near_apex`), no como señal de escalada, y ensancharlo ahí soltaría
    // la escalada justo al coronar.
    let head_hit = hits[HEAD_SAMPLE].is_some();
    facts.has_head_hit = head_hit;

    // Seguir escalando acepta **cualquier** cast del torso, no sólo la cintura.
    // Colgar la continuación de un único cast hacía que una arista del
    // heightfield soltara la escalada entera: medido el 2026-08-23, la cintura
    // fallaba un tick, el arbitraje caía a Fall, y al tick siguiente la pared
    // volvía a estar ahí. Empezar sigue exigiendo la cintura.
    if let Some(torso) = hits[2].or(hits[3]).or(hits[1]) {
        facts.wall_point = Some(torso.point);
        let angle = facing.angle_between(-torso.normal).to_degrees();
        let climbable = non_climbable.get(torso.entity).is_err();
        // Initial attachment must face the wall, but an actor already in Climb
        // owns a wall-facing yaw in the motor. Do not drop that attachment just
        // because a curved surface or a lateral move briefly makes the sampled
        // normal fall outside the entry cone.
        if can_continue_climb(state, climbable, angle, sensing) {
            facts.climb_normal = Some(torso.normal);
            facts.can_continue_climb = true;
            update_lateral_walls(spatial, &filter, facts, &actor, torso.normal, trace);
        }
    }
    if let Some(waist) = hits[WAIST_SAMPLE]
        && non_climbable.get(waist.entity).is_err()
        && faces_the_wall(facing, waist.normal, &sensing)
        && knee_hit
    {
        facts.can_climb = head_hit || {
            let found = probe_face_overhead(spatial, &filter, &actor, &waist, trace);
            if found {
                facts.climb_cast_hits |= FACE_PROBE_HIT_BIT;
            }
            found
        };
    }

    // --- Lip height ---
    if let Some(h) = down_hit.filter(|h| is_standable(h.normal1)) {
        facts.lip_height = h.point1.y - feet_y;
    }
}

/// Un borde sólo cuenta como vault o mantle si su superficie **se puede
/// pisar**. Es la misma frontera que separa caminar de caer
/// ([`is_walkable_floor`]), escrita una sola vez.
///
/// Sobre una ladera continua el down-cast devuelve la propia cara, no un
/// techo: medido el 2026-08-23 sobre una de 74°, daba una "repisa" a 0,93 m de
/// los pies con la normal de la pared. Eso encendía `is_vaultable`, y el motor
/// de vault se comía una pared entera creyéndola bordillo.
fn is_standable(normal: Vec3) -> bool {
    is_walkable_floor(normal)
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

/// ¿La cara sigue existiendo a la altura de la cabeza?
///
/// Los seis casts de perfil salen del **mismo eje vertical** con alcance fijo,
/// así que en una cara reclinada la superficie se aleja `Δaltura / tan(θ)` y el
/// de la cabeza no llega: medido el 2026-08-22, el umbral efectivo era ~77°
/// aunque la configuración declarara 60.
///
/// Sondear desde el contacto de la cintura **contra la normal de la cara** saca
/// de la aritmética la inclinación y la guiñada: el punto a `rise` metros sobre
/// el contacto queda exactamente `rise · normal.y` afuera del plano, que es un
/// producto y no una tangente. Sin división, sin singularidad cerca del límite
/// caminable, y con el alcance acotado por debajo del metro.
fn probe_face_overhead(
    spatial: &SpatialQuery,
    filter: &SpatialQueryFilter,
    actor: &LedgeActor,
    waist: &Hit,
    trace: &mut CastTrace,
) -> bool {
    let sensing = &actor.sensing;
    let rise = sensing.height_samples[HEAD_SAMPLE] - sensing.height_samples[WAIST_SAMPLE];
    let Some(probe) = probe_above_contact(sensing, waist, rise) else {
        return false;
    };
    let hit = spatial.cast_shape(
        actor.shape,
        probe.origin,
        Quat::IDENTITY,
        probe.into_face,
        &ShapeCastConfig::from_max_distance(probe.reach),
        filter,
    );
    trace.record_shape(
        actor.entity,
        "ledge_face_overhead",
        probe.origin,
        *probe.into_face,
        probe.reach,
        hit.map(|h| (h.point1, h.normal1)),
    );
    hit.is_some()
}

/// Desde dónde, hacia dónde y cuánto sondear la cara por encima del contacto.
struct FaceProbe {
    origin: Vec3,
    into_face: Dir3,
    reach: f32,
}

/// `None` donde sondear sería incorrecto, no sólo inútil: una cara caminable
/// convertida en pared dejaría escalable cualquier rampa, y un techo daría un
/// alcance negativo.
fn probe_above_contact(sensing: &LedgeSensing, waist: &Hit, rise: f32) -> Option<FaceProbe> {
    if is_walkable_floor(waist.normal) || waist.normal.y < CEILING_NORMAL_Y {
        return None;
    }
    let into_face = Dir3::new(-waist.normal).ok()?;
    // El sondeo tiene que nacer fuera de la cara: por debajo del radio de la
    // esfera el cast arranca penetrando y su resultado deja de significar nada.
    let clearance = sensing.sphere_radius * 2.0;
    let outside_the_plane = rise * waist.normal.y.max(0.0);
    Some(FaceProbe {
        origin: waist.point + Vec3::Y * rise + waist.normal * clearance,
        into_face,
        reach: outside_the_plane + clearance,
    })
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
        if is_vaultable_lip(lip.y - feet_y, h.normal1, &actor.sensing) {
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

/// Separada de [`detect_vault`] porque `ShapeHitData` no se puede construir en
/// un test: acá entra el par (altura, normal) que decide todo.
fn is_vaultable_lip(rel_y: f32, normal: Vec3, sensing: &LedgeSensing) -> bool {
    is_standable(normal)
        && (sensing.vault_min_height..=sensing.vault_detection_range).contains(&rel_y)
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
    use crate::movement::motor_common::{FLOOR_MIN_UP_DOT, WALKABLE_LIMIT_DEG};

    /// Normal de una cara inclinada `degrees` sobre la horizontal, encarada
    /// hacia -Z (el actor mira a -Z por defecto).
    fn face_normal(degrees: f32) -> Vec3 {
        let t = degrees.to_radians();
        Vec3::new(0.0, t.cos(), t.sin())
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

    fn contact_at_origin(degrees: f32) -> Hit {
        Hit {
            entity: Entity::PLACEHOLDER,
            point: Vec3::ZERO,
            normal: face_normal(degrees),
        }
    }

    const RISE: f32 = 0.8;

    /// El sondeo sólo entra donde el cast de perfil no podía llegar. Una cara
    /// caminable no se sondea: si se sondeara, cualquier rampa de 30° tendría
    /// superficie "a la altura de la cabeza" y se volvería escalable.
    #[test]
    fn a_walkable_face_is_never_probed() {
        let sensing = LedgeSensing::PLAYER;
        for degrees in [0.0_f32, 30.0, WALKABLE_LIMIT_DEG] {
            assert!(
                probe_above_contact(&sensing, &contact_at_origin(degrees), RISE).is_none(),
                "{degrees}° se camina: no debe sondearse como pared"
            );
        }
        assert!(
            probe_above_contact(&sensing, &contact_at_origin(WALKABLE_LIMIT_DEG + 1.0), RISE)
                .is_some(),
            "pasado el límite no se camina, y ahí el sondeo es la única salida"
        );
    }

    /// Un techo daría un alcance negativo, así que se descarta antes.
    #[test]
    fn an_overhang_is_never_probed() {
        let sensing = LedgeSensing::PLAYER;
        let ceiling = Hit {
            entity: Entity::PLACEHOLDER,
            point: Vec3::ZERO,
            normal: Vec3::NEG_Y,
        };
        assert!(probe_above_contact(&sensing, &ceiling, RISE).is_none());
    }

    /// **La trampa del float.** `90f32.to_radians()` pasa π/2, así que el coseno
    /// de una vertical exacta da ~−7,5e-8. Con el umbral en cero, la pared más
    /// escalable de todas caía del lado del techo.
    #[test]
    fn an_exactly_vertical_wall_is_still_probed() {
        let sensing = LedgeSensing::PLAYER;
        let probe = probe_above_contact(&sensing, &contact_at_origin(90.0), RISE)
            .expect("una pared vertical tiene que sondearse");
        let clearance = sensing.sphere_radius * 2.0;
        assert!(
            (probe.reach - clearance).abs() < 1e-4,
            "en vertical la cara no se aleja: el alcance es sólo la holgura de salida"
        );
        assert!((probe.origin.y - RISE).abs() < 1e-4);
    }

    /// El sondeo llega a la cara, y no mucho más allá: el alcance vale
    /// exactamente lo que el punto sondeado se separa del plano.
    #[test]
    fn the_probe_reaches_the_plane_it_asks_about() {
        let sensing = LedgeSensing::PLAYER;
        for degrees in [50.0_f32, 60.0, 74.0, 81.0, 90.0] {
            let contact = contact_at_origin(degrees);
            let probe = probe_above_contact(&sensing, &contact, RISE)
                .expect("una cara no caminable se sondea");
            // Distancia del origen al plano que pasa por el contacto, medida
            // sobre la normal — que es la dirección en la que viaja el cast.
            let to_plane = (probe.origin - contact.point).dot(contact.normal);
            assert!(
                to_plane > sensing.sphere_radius,
                "a {degrees}° el sondeo nace penetrando la cara y no mide nada"
            );
            assert!(
                probe.reach + sensing.sphere_radius >= to_plane,
                "a {degrees}° el sondeo se queda corto y la pared se declara inexistente"
            );
        }
    }

    /// La cota que hace innecesaria una constante de tope: como sólo se sondean
    /// caras no caminables, `normal.y` está por debajo de `FLOOR_MIN_UP_DOT` y
    /// el alcance no puede crecer sin límite.
    #[test]
    fn the_walkable_gate_is_what_bounds_the_probe() {
        let sensing = LedgeSensing::PLAYER;
        let ceiling = RISE * FLOOR_MIN_UP_DOT + sensing.sphere_radius * 2.0;
        for degrees in [45.1_f32, 50.0, 60.0, 74.0, 81.0, 90.0] {
            let probe = probe_above_contact(&sensing, &contact_at_origin(degrees), RISE)
                .expect("una cara no caminable se sondea");
            assert!(
                probe.reach <= ceiling,
                "a {degrees}° el alcance {} pasó la cota {ceiling}",
                probe.reach
            );
        }
    }

    /// **El bug del 2026-08-23.** El down-cast de vault sobre una ladera
    /// continua devuelve la propia cara, y a 74° la altura cae dentro del rango
    /// de bordillo. Sin mirar la normal, el acantilado entero se declaraba
    /// saltable y la escalada quedaba vetada.
    #[test]
    fn a_slope_is_not_a_vaultable_lip() {
        let sensing = LedgeSensing::PLAYER;
        assert!(
            !is_vaultable_lip(0.93, face_normal(74.0), &sensing),
            "la cara de 74° medida en el cañón no es un bordillo"
        );
    }

    /// Un bordillo de verdad sigue siéndolo: techo plano, altura en rango.
    #[test]
    fn a_flat_topped_obstacle_is_still_vaultable() {
        let sensing = LedgeSensing::PLAYER;
        assert!(is_vaultable_lip(0.93, Vec3::Y, &sensing));
        assert!(!is_vaultable_lip(0.1, Vec3::Y, &sensing), "demasiado bajo");
        assert!(!is_vaultable_lip(2.0, Vec3::Y, &sensing), "demasiado alto");
    }

    /// La frontera de "pisable" es la misma que la de caminar, no una nueva —
    /// y se escribe **relativa al umbral**, no con dos números: cuando el
    /// límite bajó de 60° a 45°, la versión con literales afirmaba que 59° se
    /// pisa, que había dejado de ser cierto.
    #[test]
    fn the_standable_limit_is_the_walkable_limit() {
        let limit = WALKABLE_LIMIT_DEG;
        assert!(is_standable(face_normal(limit - 1.0)));
        assert!(!is_standable(face_normal(limit + 1.0)));
    }

    /// El límite exacto cuenta como piso, en los dos lados de la frontera: es
    /// la costura que `is_walkable_floor` unificó.
    #[test]
    fn the_limit_itself_is_standable_and_not_a_wall() {
        let limit = WALKABLE_LIMIT_DEG;
        assert!(is_standable(face_normal(limit)));
        assert!(
            probe_above_contact(&LedgeSensing::PLAYER, &contact_at_origin(limit), RISE).is_none(),
            "el límite exacto es piso: no se sondea como pared"
        );
    }
}
