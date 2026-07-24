//! Decide + Act — the enemy's brain.
//!
//! `decide` is the only writer of `EnemyAiState`; `act` translates that state
//! into `Intents`, the same contract the player's hardware brain writes. The
//! brain never touches `Transform`, `BodyVelocity`, or `LocomotionState`.

use bevy::prelude::*;

use super::perception::{AggroTarget, Awareness};
use super::state::EnemyAiState;
use super::{Enemy, Home};
use crate::movement::intents::{Intents, PlanarMoveIntent};

/// Behavior tuning, per enemy. Presets follow the `GroundMovement::PLAYER`
/// pattern.
#[derive(Component, Clone, Copy)]
pub struct EnemyBrainProfile {
    /// Stop closing distance to an alert target inside this radius (m).
    pub engage_distance: f32,
    /// A waypoint / search point counts as reached inside this radius (m).
    pub arrive_radius: f32,
    /// Patrol waypoints are picked within this radius of `Home` (m).
    pub patrol_radius: f32,
    /// Idle pause between patrol waypoints (s).
    pub patrol_pause_secs: f32,
    /// Give up searching after this long without reacquiring sight (s).
    pub search_timeout_secs: f32,
    /// A visible, alerted target inside this radius flips `Alert` →
    /// `Combat` (melee: just past sword reach; archer: shooting range).
    pub attack_range: f32,
    /// Minimum delay between attack starts (melee swings / bow draws).
    pub attack_cadence_secs: f32,
}

impl EnemyBrainProfile {
    pub const BOKOBO: Self = Self {
        engage_distance: 1.6,
        arrive_radius: 0.6,
        patrol_radius: 6.0,
        patrol_pause_secs: 1.5,
        search_timeout_secs: 6.0,
        attack_range: 1.9,
        attack_cadence_secs: 1.2,
    };

    /// Keeps its distance and shoots: the engage ring is where it stands,
    /// the attack range is where it starts drawing.
    pub const BOKOBO_ARCHER: Self = Self {
        engage_distance: 9.0,
        arrive_radius: 0.6,
        patrol_radius: 6.0,
        patrol_pause_secs: 1.5,
        search_timeout_secs: 6.0,
        attack_range: 13.0,
        attack_cadence_secs: 0.8,
    };
}

/// Per-enemy brain bookkeeping (waypoint, timers). A component, never a
/// system `Local` — multi-actor contract.
#[derive(Component, Default)]
pub struct BrainLocal {
    waypoint: Option<Vec3>,
    pause_left: f32,
    step: u32,
    search_elapsed: f32,
}

/// Pure transition rule for `EnemyAiState`. `decide` feeds it and applies
/// the result; tests pin it directly.
///
/// `engaged` = target in sight **and** `Awareness` full ("full threat");
/// `in_attack_range` = that engaged target is inside `attack_range`
/// (irrelevant when not engaged); `suspicious` = the meter crossed
/// `Awareness::SUSPICIOUS`, worth investigating even before full detection.
pub(crate) fn next_ai_state(
    current: EnemyAiState,
    engaged: bool,
    in_attack_range: bool,
    suspicious: bool,
    reached_last_seen: bool,
    search_expired: bool,
) -> EnemyAiState {
    if engaged {
        return if in_attack_range {
            EnemyAiState::Combat
        } else {
            EnemyAiState::Alert
        };
    }
    match current {
        EnemyAiState::Patrol if suspicious => EnemyAiState::Search,
        EnemyAiState::Patrol => EnemyAiState::Patrol,
        EnemyAiState::Alert | EnemyAiState::Combat => EnemyAiState::Search,
        EnemyAiState::Search if search_expired || (reached_last_seen && !suspicious) => {
            EnemyAiState::Patrol
        }
        EnemyAiState::Search => EnemyAiState::Search,
    }
}

type DecideQuery<'a> = (
    &'a Transform,
    &'a EnemyBrainProfile,
    &'a Awareness,
    &'a mut AggroTarget,
    &'a mut BrainLocal,
    &'a mut EnemyAiState,
);

/// The only writer of `EnemyAiState`.
pub fn decide(time: Res<Time>, mut q: Query<DecideQuery, With<Enemy>>) {
    let dt = time.delta_secs();
    for (transform, profile, awareness, mut aggro, mut local, mut state) in &mut q {
        let visible = aggro.in_sight.is_some();
        let engaged = visible && awareness.is_alerted();
        let reached_last_seen = aggro.last_seen.is_some_and(|seen| {
            planar_distance_sq(transform.translation, seen)
                <= profile.arrive_radius * profile.arrive_radius
        });

        if *state == EnemyAiState::Search && !visible {
            local.search_elapsed += dt;
        } else {
            local.search_elapsed = 0.0;
        }
        let search_expired = local.search_elapsed >= profile.search_timeout_secs;

        // While in sight, `last_seen` is the target's current position.
        let in_attack_range = visible
            && aggro.last_seen.is_some_and(|seen| {
                planar_distance_sq(transform.translation, seen)
                    <= profile.attack_range * profile.attack_range
            });

        let next = next_ai_state(
            *state,
            engaged,
            in_attack_range,
            awareness.is_suspicious(),
            reached_last_seen,
            search_expired,
        );
        if next == EnemyAiState::Patrol && *state != EnemyAiState::Patrol {
            // Giving up: forget the target completely.
            aggro.last_seen = None;
        }
        if *state != next {
            *state = next;
        }
    }
}

type ActQuery<'a> = (
    Entity,
    &'a Transform,
    &'a Home,
    &'a EnemyBrainProfile,
    &'a AggroTarget,
    &'a EnemyAiState,
    &'a mut BrainLocal,
    &'a mut Intents,
);

/// Translate `EnemyAiState` into this enemy's `Intents` — full overwrite
/// every tick, like every brain in the pipeline.
pub fn act(time: Res<Time>, mut q: Query<ActQuery, With<Enemy>>) {
    let dt = time.delta_secs();
    for (entity, transform, home, profile, aggro, state, mut local, mut intents) in &mut q {
        let pos = transform.translation;
        *intents = match state {
            EnemyAiState::Patrol => patrol_intents(entity, pos, home.0, profile, &mut local, dt),
            EnemyAiState::Alert => aggro.last_seen.map_or_else(Intents::default, |target| {
                chase_intents(pos, target, profile)
            }),
            EnemyAiState::Search => aggro.last_seen.map_or_else(Intents::default, |seen| {
                walk_toward(pos, seen, profile.arrive_radius, false)
            }),
            // Fighting: keep pressing toward the target at a walk — the
            // approach is what keeps the melee facing it (motors rotate
            // toward planar intent); the archer's aim is ControlOrientation.
            EnemyAiState::Combat => aggro.last_seen.map_or_else(Intents::default, |seen| {
                walk_toward(pos, seen, profile.engage_distance, false)
            }),
        };
    }
}

fn patrol_intents(
    entity: Entity,
    pos: Vec3,
    home: Vec3,
    profile: &EnemyBrainProfile,
    local: &mut BrainLocal,
    dt: f32,
) -> Intents {
    let arrived = local.waypoint.is_none_or(|wp| {
        planar_distance_sq(pos, wp) <= profile.arrive_radius * profile.arrive_radius
    });

    if arrived {
        if local.waypoint.take().is_some() {
            local.pause_left = profile.patrol_pause_secs;
        }
        if local.pause_left > 0.0 {
            local.pause_left = (local.pause_left - dt).max(0.0);
            return Intents::default();
        }
        local.step = local.step.wrapping_add(1);
        local.waypoint = Some(patrol_waypoint(
            home,
            profile.patrol_radius,
            entity.index().index(),
            local.step,
        ));
    }

    local.waypoint.map_or_else(Intents::default, |wp| {
        walk_toward(pos, wp, profile.arrive_radius, false)
    })
}

/// Sprint at the target, stopping inside the engage ring (stand and stare —
/// combat picks it up from here when that system exists).
fn chase_intents(pos: Vec3, target: Vec3, profile: &EnemyBrainProfile) -> Intents {
    walk_toward(pos, target, profile.engage_distance, true)
}

/// Deterministic pseudo-random waypoint around `home`: golden-angle sequence
/// keyed by entity index + step, so paths look organic but replay identically
/// (and are testable).
pub(crate) fn patrol_waypoint(home: Vec3, radius: f32, entity_index: u32, step: u32) -> Vec3 {
    const GOLDEN_ANGLE: f32 = 2.399_963;
    let n = entity_index.wrapping_mul(7919).wrapping_add(step) as f32;
    let angle = n * GOLDEN_ANGLE;
    // Radius sweeps [0.4, 1.0]·patrol_radius on a second irrational stride so
    // consecutive waypoints vary in reach, not just direction.
    let reach = 0.4 + 0.6 * (n * 0.754_877).fract();
    home + Vec3::new(angle.cos(), 0.0, angle.sin()) * (radius * reach)
}

fn walk_toward(pos: Vec3, target: Vec3, stop_radius: f32, wants_sprint: bool) -> Intents {
    let delta = target - pos;
    let planar = Vec2::new(delta.x, delta.z);
    if planar.length_squared() <= stop_radius * stop_radius {
        return Intents::default();
    }
    Intents {
        planar: PlanarMoveIntent {
            direction: planar.normalize_or_zero(),
            strength: 1.0,
            local: Vec2::ZERO,
        },
        wants_sprint,
        ..default()
    }
}

pub(crate) fn planar_distance_sq(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x, a.z).distance_squared(Vec2::new(b.x, b.z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::{Actor, Player};
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn full_threat_always_wins_and_alert_degrades_to_search() {
        for current in [
            EnemyAiState::Patrol,
            EnemyAiState::Alert,
            EnemyAiState::Search,
            EnemyAiState::Combat,
        ] {
            assert_eq!(
                next_ai_state(current, true, false, true, false, false),
                EnemyAiState::Alert
            );
        }
        assert_eq!(
            next_ai_state(EnemyAiState::Alert, false, false, true, false, false),
            EnemyAiState::Search
        );
    }

    #[test]
    fn combat_needs_sight_and_range_and_degrades_like_alert() {
        for current in [
            EnemyAiState::Patrol,
            EnemyAiState::Alert,
            EnemyAiState::Search,
            EnemyAiState::Combat,
        ] {
            assert_eq!(
                next_ai_state(current, true, true, true, false, false),
                EnemyAiState::Combat,
                "engaged and in range always fights"
            );
        }
        assert_eq!(
            next_ai_state(EnemyAiState::Combat, true, false, true, false, false),
            EnemyAiState::Alert,
            "target stepped out of range while visible: chase again"
        );
        assert_eq!(
            next_ai_state(EnemyAiState::Combat, false, false, true, false, false),
            EnemyAiState::Search,
            "losing sight mid-fight degrades to search, like Alert"
        );
    }

    #[test]
    fn suspicion_triggers_investigation_not_alert() {
        assert_eq!(
            next_ai_state(EnemyAiState::Patrol, false, false, true, false, false),
            EnemyAiState::Search,
            "a suspicious enemy investigates"
        );
        assert_eq!(
            next_ai_state(EnemyAiState::Patrol, false, false, false, false, false),
            EnemyAiState::Patrol,
            "below the suspicion threshold nothing happens"
        );
    }

    #[test]
    fn search_gives_up_on_calm_arrival_or_timeout() {
        assert_eq!(
            next_ai_state(EnemyAiState::Search, false, false, false, true, false),
            EnemyAiState::Patrol,
            "arrived and no longer suspicious: give up"
        );
        assert_eq!(
            next_ai_state(EnemyAiState::Search, false, false, true, true, false),
            EnemyAiState::Search,
            "arrived but still suspicious: keep looking around"
        );
        assert_eq!(
            next_ai_state(EnemyAiState::Search, false, false, true, false, true),
            EnemyAiState::Patrol,
            "timeout beats lingering suspicion"
        );
        assert_eq!(
            next_ai_state(EnemyAiState::Search, false, false, false, false, false),
            EnemyAiState::Search
        );
    }

    #[test]
    fn patrol_waypoints_are_deterministic_and_within_radius() {
        let home = Vec3::new(10.0, 1.0, 8.0);
        let radius = 6.0;
        for step in 0..32 {
            let a = patrol_waypoint(home, radius, 42, step);
            let b = patrol_waypoint(home, radius, 42, step);
            assert_eq!(a, b, "same inputs must give the same waypoint");
            assert!(
                planar_distance_sq(a, home) <= radius * radius + f32::EPSILON,
                "waypoint {a} escapes the patrol radius"
            );
        }
        assert_ne!(
            patrol_waypoint(home, radius, 1, 3),
            patrol_waypoint(home, radius, 2, 3),
            "different enemies must not share a path"
        );
    }

    #[test]
    fn act_does_not_touch_player_intents() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let player = world
            .spawn((
                Actor,
                Player,
                Intents {
                    wants_sprint: true,
                    ..default()
                },
            ))
            .id();
        let enemy = world
            .spawn((
                Enemy,
                Transform::from_xyz(0.0, 1.0, 0.0),
                Home(Vec3::new(0.0, 1.0, 0.0)),
                EnemyBrainProfile::BOKOBO,
                AggroTarget {
                    in_sight: Some(player),
                    last_seen: Some(Vec3::new(5.0, 1.0, 0.0)),
                },
                Awareness(1.0),
                EnemyAiState::Alert,
                BrainLocal::default(),
                Intents::default(),
            ))
            .id();

        world.run_system_once(act).unwrap();

        assert!(
            world.entity(player).get::<Intents>().unwrap().wants_sprint,
            "player intents must stay untouched"
        );
        let enemy_intents = world.entity(enemy).get::<Intents>().unwrap();
        assert!(enemy_intents.wants_sprint);
        assert!(
            enemy_intents.planar.direction.x > 0.9,
            "alert enemy must move toward its target"
        );
    }

    #[test]
    fn aggro_and_state_do_not_bleed_between_enemies() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let seen_pos = Vec3::new(3.0, 1.0, 0.0);
        let hunter = world
            .spawn((
                Enemy,
                Transform::from_xyz(0.0, 1.0, 0.0),
                Home(Vec3::ZERO),
                EnemyBrainProfile::BOKOBO,
                AggroTarget {
                    in_sight: Some(Entity::PLACEHOLDER),
                    last_seen: Some(seen_pos),
                },
                Awareness(1.0),
                EnemyAiState::Patrol,
                BrainLocal::default(),
                Intents::default(),
            ))
            .id();
        let idle = world
            .spawn((
                Enemy,
                Transform::from_xyz(20.0, 1.0, 20.0),
                Home(Vec3::new(20.0, 1.0, 20.0)),
                EnemyBrainProfile::BOKOBO,
                AggroTarget::default(),
                Awareness::default(),
                EnemyAiState::Patrol,
                BrainLocal::default(),
                Intents::default(),
            ))
            .id();

        world.run_system_once(decide).unwrap();

        assert_eq!(
            *world.entity(hunter).get::<EnemyAiState>().unwrap(),
            EnemyAiState::Alert
        );
        assert_eq!(
            *world.entity(idle).get::<EnemyAiState>().unwrap(),
            EnemyAiState::Patrol,
            "an enemy with no aggro must not inherit its neighbor's alert"
        );
    }

    #[test]
    fn alert_stops_inside_the_engage_ring() {
        let profile = EnemyBrainProfile::BOKOBO;
        let intents = chase_intents(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(profile.engage_distance * 0.5, 1.0, 0.0),
            &profile,
        );
        assert_eq!(intents.planar.direction, Vec2::ZERO);

        let intents = chase_intents(
            Vec3::new(0.0, 1.0, 0.0),
            Vec3::new(profile.engage_distance * 3.0, 1.0, 0.0),
            &profile,
        );
        assert!(intents.planar.direction.length() > 0.9);
    }
}
