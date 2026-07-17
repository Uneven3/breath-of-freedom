//! Perception — what an enemy notices, and how aware it is of it.
//!
//! `perceive` evaluates every `Perceivable` actor against the enemy's sight cone
//! and a line-of-sight ray (masked to `GameLayer::Default`, so only world
//! geometry occludes — actor capsules are invisible to it by layer). Sight is
//! not binary: it fills the per-enemy [`Awareness`] meter over time — faster
//! up close, slower against a sneaking target — and the meter decays when
//! sight is lost. The brain reads the meter's thresholds; Combat will read
//! them too (stealth bonus against a non-alerted enemy — see
//! `docs/ARCHITECTURE.md`).

use avian3d::prelude::*;
use bevy::prelude::*;

use super::Enemy;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};
use crate::world::GameLayer;

/// Marks an actor enemies can notice. Owned by Perception so spawners opt
/// in explicitly (player today; horse, animals, allies as the game grows)
/// instead of `Player` doubling as a perception proxy — a faction component
/// can replace this marker when hostility needs more than one bit.
#[derive(Component)]
pub struct Perceivable;

/// Sight tuning, per enemy. Presets follow the `GroundMovement::PLAYER`
/// pattern.
#[derive(Component, Clone, Copy)]
pub struct Perception {
    /// Maximum distance (m) at which a target can be seen.
    pub sight_range: f32,
    /// Full width of the vision cone, degrees.
    pub fov_deg: f32,
    /// Seconds of continuous sight to fully detect a walking target standing
    /// at the edge of `sight_range`.
    pub detection_secs: f32,
    /// Fill-rate multiplier at point-blank range (lerps down to 1 at the
    /// range edge) — being close gets you spotted almost instantly.
    pub close_range_boost: f32,
    /// Fill-rate multiplier against a target in `Sneak` — why sneaking exists.
    pub sneak_visibility: f32,
    /// Seconds for a full meter to decay back to zero once sight is lost.
    pub awareness_decay_secs: f32,
    /// Hearing radius (m) for a maximally loud target (sprinting).
    /// Omnidirectional — the back does not exist for the ear.
    pub hearing_range: f32,
    /// Loudness multiplier when world geometry blocks the straight line to
    /// the noise: walls muffle sound, they don't stop it.
    pub wall_muffle: f32,
}

impl Perception {
    pub const BOKOBO: Self = Self {
        sight_range: 14.0,
        fov_deg: 150.0,
        detection_secs: 1.4,
        close_range_boost: 5.0,
        sneak_visibility: 0.3,
        awareness_decay_secs: 4.0,
        hearing_range: 9.0,
        wall_muffle: 0.5,
    };
}

/// How aware this enemy is of a threat, `0.0..=1.0`. Written only by
/// `perceive`. This is the contract Combat reads for the stealth rules: a
/// non-alerted enemy takes bonus damage from arrows and sneak strikes; an
/// alerted one has "full threat" and cannot be sneak-struck.
#[derive(Component, Default)]
pub struct Awareness(pub f32);

impl Awareness {
    /// At or above this the enemy is fully alerted (full threat, chases).
    pub const ALERTED: f32 = 1.0;
    /// At or above this the enemy is suspicious (investigates the stimulus).
    pub const SUSPICIOUS: f32 = 0.35;

    pub fn is_alerted(&self) -> bool {
        self.0 >= Self::ALERTED
    }

    pub fn is_suspicious(&self) -> bool {
        self.0 >= Self::SUSPICIOUS
    }
}

/// Output of `perceive`, per enemy. `in_sight` holds the target currently in
/// view (regardless of awareness level); `last_seen` survives losing sight so
/// the brain can investigate/search, and clears when the brain gives up.
#[derive(Component, Default)]
pub struct AggroTarget {
    pub in_sight: Option<Entity>,
    pub last_seen: Option<Vec3>,
}

/// True when `target_pos` falls inside the sight cone anchored at the
/// enemy's transform (distance + field-of-view; occlusion is checked
/// separately with a ray).
pub(crate) fn within_sight_cone(
    enemy: &Transform,
    target_pos: Vec3,
    perception: Perception,
) -> bool {
    let to_target = target_pos - enemy.translation;
    let distance_sq = to_target.length_squared();
    if distance_sq > perception.sight_range * perception.sight_range {
        return false;
    }
    let Some(flat_dir) = Vec3::new(to_target.x, 0.0, to_target.z).try_normalize() else {
        // Directly above/below or coincident: treat as visible (range passed).
        return true;
    };
    let forward = enemy.rotation * Vec3::NEG_Z;
    let flat_forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let half_fov_cos = (perception.fov_deg.to_radians() / 2.0).cos();
    flat_forward.dot(flat_dir) >= half_fov_cos
}

/// Awareness gained per second while the target is visible: distance and
/// gait modulate the base `1/detection_secs` rate.
pub(crate) fn awareness_fill_rate(distance: f32, perception: Perception, sneaking: bool) -> f32 {
    let closeness = (1.0 - distance / perception.sight_range).clamp(0.0, 1.0);
    let proximity = 1.0 + (perception.close_range_boost - 1.0) * closeness;
    let visibility = if sneaking {
        perception.sneak_visibility
    } else {
        1.0
    };
    proximity * visibility / perception.detection_secs
}

/// Below this planar speed (m/s) locomotion makes no noise at all — standing
/// still is silent regardless of gait.
const NOISE_SPEED_THRESHOLD: f32 = 0.5;

/// How loud a target's locomotion is, `0.0..=1.0` relative to a sprint.
/// Derived read-only from Movement state — Movement does not know it emits
/// noise (see `docs/ARCHITECTURE.md`).
pub(crate) fn locomotion_loudness(state: Option<LocomotionState>, planar_speed: f32) -> f32 {
    if planar_speed < NOISE_SPEED_THRESHOLD {
        return 0.0;
    }
    match state {
        Some(LocomotionState::Sneak) => 0.15,
        Some(LocomotionState::Sprint) => 1.0,
        // Climbing, gliding, air time: quiet rustle.
        Some(
            LocomotionState::Climb
            | LocomotionState::Ladder
            | LocomotionState::Glide
            | LocomotionState::Fall
            | LocomotionState::Jump,
        ) => 0.3,
        // Walk, stairs, traversal moves — and targets with no locomotion
        // state at all — sound like footsteps.
        _ => 0.55,
    }
}

/// Awareness gained per second from a noise `distance` away whose effective
/// audible radius is `audible_radius`. Linear falloff, same base clock as
/// vision.
pub(crate) fn hearing_fill_rate(
    distance: f32,
    audible_radius: f32,
    perception: Perception,
) -> Option<f32> {
    if audible_radius <= 0.0 || distance > audible_radius {
        return None;
    }
    let closeness = (1.0 - distance / audible_radius).clamp(0.0, 1.0);
    // Even a barely-audible noise fills at a floor rate: you noticed *something*.
    Some((0.25 + 0.75 * closeness) / perception.detection_secs)
}

type TargetQuery<'a> = (
    Entity,
    &'a Transform,
    Option<&'a LocomotionState>,
    Option<&'a BodyVelocity>,
);
type EnemyQuery<'a> = (
    Entity,
    &'a Transform,
    &'a Perception,
    &'a mut AggroTarget,
    &'a mut Awareness,
);

pub fn perceive(
    time: Res<Time>,
    spatial: SpatialQuery,
    mut enemies: Query<EnemyQuery, With<Enemy>>,
    targets: Query<TargetQuery, (With<Perceivable>, With<Actor>)>,
) {
    let dt = time.delta_secs();
    for (enemy_entity, enemy_tf, perception, mut aggro, mut awareness) in &mut enemies {
        let seen = targets.iter().find(|(_, target_tf, _, _)| {
            within_sight_cone(enemy_tf, target_tf.translation, *perception)
                && line_of_sight_clear(
                    &spatial,
                    enemy_entity,
                    enemy_tf.translation,
                    target_tf.translation,
                )
        });

        if let Some((target, target_tf, locomotion, _)) = seen {
            let sneaking = matches!(locomotion, Some(LocomotionState::Sneak));
            let distance = enemy_tf.translation.distance(target_tf.translation);
            let rate = awareness_fill_rate(distance, *perception, sneaking);
            awareness.0 = (awareness.0 + rate * dt).min(Awareness::ALERTED);
            aggro.in_sight = Some(target);
            aggro.last_seen = Some(target_tf.translation);
            continue;
        }
        aggro.in_sight = None;

        // Not seen — maybe heard. Omnidirectional, walls muffle instead of
        // blocking, and capped at SUSPICIOUS: a noise alone turns the enemy
        // around to investigate; only sight completes the detection.
        let heard = targets
            .iter()
            .filter_map(|(_, target_tf, locomotion, velocity)| {
                let planar_speed = velocity
                    .map(|v| Vec2::new(v.0.x, v.0.z).length())
                    .unwrap_or(0.0);
                let mut loudness = locomotion_loudness(locomotion.copied(), planar_speed);
                if loudness <= 0.0 {
                    return None;
                }
                if !line_of_sight_clear(
                    &spatial,
                    enemy_entity,
                    enemy_tf.translation,
                    target_tf.translation,
                ) {
                    loudness *= perception.wall_muffle;
                }
                let distance = enemy_tf.translation.distance(target_tf.translation);
                let audible_radius = perception.hearing_range * loudness;
                hearing_fill_rate(distance, audible_radius, *perception)
                    .map(|rate| (rate, target_tf.translation))
            })
            .max_by(|(a, _), (b, _)| a.total_cmp(b));

        match heard {
            Some((rate, noise_pos)) => {
                // Fill toward the suspicion ceiling; never *lower* a meter
                // that is already higher (noise sustains, sight decays).
                if awareness.0 < Awareness::SUSPICIOUS {
                    awareness.0 = (awareness.0 + rate * dt).min(Awareness::SUSPICIOUS);
                }
                aggro.last_seen = Some(noise_pos);
            }
            None => {
                awareness.0 =
                    (awareness.0 - dt / perception.awareness_decay_secs.max(f32::EPSILON)).max(0.0);
            }
        }
    }
}

/// An unmistakable threat (taking damage) aimed at one enemy: jumps its
/// meter straight to `ALERTED` and marks where it came from, bypassing the
/// senses. Owned by Enemies; Health/Combat emit it when they exist (the
/// receiving system owns the message type, same pattern as
/// `health::DamageRequestMessage`).
#[derive(Message, Debug, Clone, Copy)]
pub struct DirectThreatMessage {
    pub enemy: Entity,
    pub threat_position: Vec3,
}

/// Runs between `perceive` and `brain::decide`, so a hit this tick alerts
/// the brain this same tick.
pub fn receive_direct_threats(
    mut messages: MessageReader<DirectThreatMessage>,
    mut enemies: Query<(&mut AggroTarget, &mut Awareness), With<Enemy>>,
) {
    for message in messages.read() {
        if let Ok((mut aggro, mut awareness)) = enemies.get_mut(message.enemy) {
            awareness.0 = Awareness::ALERTED;
            aggro.last_seen = Some(message.threat_position);
        }
    }
}

/// One ray from enemy center to target center against world geometry only
/// (`GameLayer::Default`): any hit before the target means a wall is in the
/// way.
fn line_of_sight_clear(spatial: &SpatialQuery, enemy: Entity, from: Vec3, to: Vec3) -> bool {
    let to_target = to - from;
    let distance = to_target.length();
    let Ok(dir) = Dir3::new(to_target) else {
        // Coincident positions: nothing can be in between.
        return true;
    };
    let filter = SpatialQueryFilter::from_mask(GameLayer::Default).with_excluded_entities([enemy]);
    spatial
        .cast_ray(from, dir, distance, true, &filter)
        .is_none()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn enemy_at_origin_facing_neg_z() -> Transform {
        Transform::from_xyz(0.0, 1.0, 0.0)
    }

    #[test]
    fn sees_target_in_front_within_range() {
        assert!(within_sight_cone(
            &enemy_at_origin_facing_neg_z(),
            Vec3::new(0.0, 1.0, -5.0),
            Perception::BOKOBO,
        ));
    }

    #[test]
    fn does_not_see_target_behind() {
        assert!(!within_sight_cone(
            &enemy_at_origin_facing_neg_z(),
            Vec3::new(0.0, 1.0, 5.0),
            Perception::BOKOBO,
        ));
    }

    #[test]
    fn does_not_see_target_out_of_range() {
        assert!(!within_sight_cone(
            &enemy_at_origin_facing_neg_z(),
            Vec3::new(0.0, 1.0, -(Perception::BOKOBO.sight_range + 1.0)),
            Perception::BOKOBO,
        ));
    }

    #[test]
    fn cone_edge_respects_fov() {
        // 150° cone → 75° half-angle. 80° off-forward must be out, 70° in.
        let range = Perception::BOKOBO.sight_range - 1.0;
        let at_angle = |deg: f32| {
            let rad = deg.to_radians();
            Vec3::new(rad.sin() * range, 1.0, -rad.cos() * range)
        };
        assert!(within_sight_cone(
            &enemy_at_origin_facing_neg_z(),
            at_angle(70.0),
            Perception::BOKOBO,
        ));
        assert!(!within_sight_cone(
            &enemy_at_origin_facing_neg_z(),
            at_angle(80.0),
            Perception::BOKOBO,
        ));
    }

    #[test]
    fn height_difference_alone_does_not_blind() {
        // Player on a ledge right above the cone's flat projection.
        assert!(within_sight_cone(
            &enemy_at_origin_facing_neg_z(),
            Vec3::new(0.0, 4.0, -3.0),
            Perception::BOKOBO,
        ));
    }

    #[test]
    fn sneaking_fills_awareness_slower_than_walking() {
        let p = Perception::BOKOBO;
        let d = p.sight_range * 0.5;
        assert!(awareness_fill_rate(d, p, true) < awareness_fill_rate(d, p, false));
        let ratio = awareness_fill_rate(d, p, true) / awareness_fill_rate(d, p, false);
        assert!((ratio - p.sneak_visibility).abs() < 1e-5);
    }

    #[test]
    fn closer_targets_are_detected_faster() {
        let p = Perception::BOKOBO;
        assert!(
            awareness_fill_rate(1.0, p, false) > awareness_fill_rate(p.sight_range - 1.0, p, false)
        );
    }

    #[test]
    fn edge_of_range_walking_detection_takes_detection_secs() {
        let p = Perception::BOKOBO;
        let rate = awareness_fill_rate(p.sight_range, p, false);
        assert!((rate * p.detection_secs - 1.0).abs() < 1e-5);
    }

    #[test]
    fn awareness_thresholds_classify_levels() {
        assert!(!Awareness(0.0).is_suspicious());
        assert!(Awareness(Awareness::SUSPICIOUS).is_suspicious());
        assert!(!Awareness(0.9).is_alerted());
        assert!(Awareness(1.0).is_alerted());
    }

    #[test]
    fn loudness_orders_gaits_and_standing_is_silent() {
        let speed = 4.0;
        let sneak = locomotion_loudness(Some(LocomotionState::Sneak), speed);
        let walk = locomotion_loudness(Some(LocomotionState::Walk), speed);
        let sprint = locomotion_loudness(Some(LocomotionState::Sprint), speed);
        assert!(sprint > walk && walk > sneak && sneak > 0.0);
        assert_eq!(
            locomotion_loudness(Some(LocomotionState::Sprint), 0.0),
            0.0,
            "standing still is silent regardless of gait"
        );
    }

    #[test]
    fn hearing_is_bounded_by_the_audible_radius() {
        let p = Perception::BOKOBO;
        assert!(hearing_fill_rate(p.hearing_range * 0.5, p.hearing_range, p).is_some());
        assert!(hearing_fill_rate(p.hearing_range + 0.1, p.hearing_range, p).is_none());
        assert!(
            hearing_fill_rate(1.0, 0.0, p).is_none(),
            "silence: no radius"
        );
    }

    #[test]
    fn closer_noises_fill_faster() {
        let p = Perception::BOKOBO;
        let near = hearing_fill_rate(1.0, p.hearing_range, p);
        let far = hearing_fill_rate(p.hearing_range - 0.5, p.hearing_range, p);
        assert!(near > far, "{near:?} must exceed {far:?}");
    }

    #[test]
    fn direct_threat_alerts_only_the_addressed_enemy() {
        use bevy::ecs::system::RunSystemOnce;

        let mut world = World::new();
        world.init_resource::<Messages<DirectThreatMessage>>();
        let hit = world
            .spawn((Enemy, AggroTarget::default(), Awareness::default()))
            .id();
        let bystander = world
            .spawn((Enemy, AggroTarget::default(), Awareness::default()))
            .id();

        let threat_pos = Vec3::new(3.0, 1.0, -2.0);
        world.write_message(DirectThreatMessage {
            enemy: hit,
            threat_position: threat_pos,
        });
        world.run_system_once(receive_direct_threats).unwrap();

        assert!(world.entity(hit).get::<Awareness>().unwrap().is_alerted());
        assert_eq!(
            world.entity(hit).get::<AggroTarget>().unwrap().last_seen,
            Some(threat_pos)
        );
        assert!(
            !world
                .entity(bystander)
                .get::<Awareness>()
                .unwrap()
                .is_suspicious(),
            "a hit on one enemy must not alert another"
        );
    }
}
