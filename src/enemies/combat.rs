//! Combat Act — turns `EnemyAiState::Combat` into `CombatIntents`, the same
//! contract the player's hardware brain writes (ticket `enemies-combat`).
//!
//! Two archetypes by composition, never by flag: a melee enemy carries
//! `WeaponProfile` (+ combo bookkeeping); an archer carries `DrawStrength` +
//! its own `ControlOrientation`. Combat's motors don't know who wrote the
//! intent. This brain reads `DrawStrength` read-only (§5) and never emits
//! `SpawnProjectileMessage`, writes `CombatState`, or touches another
//! actor's components.

use bevy::prelude::*;

use super::Enemy;
use super::brain::{EnemyBrainProfile, planar_distance_sq};
use super::perception::AggroTarget;
use super::state::EnemyAiState;
use crate::combat::intent::{AttackIntent, CombatIntents};
use crate::combat::motors::aim::{AIM_PIVOT_HEIGHT, DrawStrength};
use crate::combat::weapon::WeaponProfile;
use crate::input::frame::ControlOrientation;

/// The archer releases once the string reaches this charge (0..=1): strong
/// enough to arc across its shooting range, short enough to keep pressure.
const ARCHER_RELEASE_CHARGE: f32 = 0.65;

/// Per-enemy attack-cadence bookkeeping. A component, never a system
/// `Local` (multi-actor contract).
#[derive(Component, Default)]
pub struct EnemyCombatLocal {
    cadence_left: f32,
}

type MeleeActQuery<'a> = (
    &'a Transform,
    &'a EnemyAiState,
    &'a AggroTarget,
    &'a EnemyBrainProfile,
    &'a mut CombatIntents,
    &'a mut EnemyCombatLocal,
);

/// In `Combat` and at reach: one attack press (a single-tick edge) every
/// `attack_cadence_secs`. The cadence outlasts the club's chain window on
/// purpose — graybox bokobos swing single, telegraphed hits.
pub fn act_melee(time: Res<Time>, mut q: Query<MeleeActQuery, (With<Enemy>, With<WeaponProfile>)>) {
    let dt = time.delta_secs();
    for (transform, state, aggro, profile, mut intents, mut local) in &mut q {
        local.cadence_left = (local.cadence_left - dt).max(0.0);
        let mut next = CombatIntents::default();
        if *state == EnemyAiState::Combat
            && local.cadence_left <= 0.0
            && aggro.last_seen.is_some_and(|seen| {
                planar_distance_sq(transform.translation, seen)
                    <= profile.attack_range * profile.attack_range
            })
        {
            next.attack = AttackIntent {
                pressed: true,
                held: true,
            };
            local.cadence_left = profile.attack_cadence_secs;
        }
        *intents = next;
    }
}

type ArcherActQuery<'a> = (
    &'a Transform,
    &'a EnemyAiState,
    &'a AggroTarget,
    &'a EnemyBrainProfile,
    &'a DrawStrength,
    &'a mut CombatIntents,
    &'a mut ControlOrientation,
    &'a mut EnemyCombatLocal,
);

/// In `Combat`: aim the control orientation at the target, keep the bow
/// wanted, and charge-and-release — hold attack until the string reaches
/// `ARCHER_RELEASE_CHARGE`, then let go; the ordinary `aim` motor fires.
pub fn act_archer(time: Res<Time>, mut q: Query<ArcherActQuery, With<Enemy>>) {
    let dt = time.delta_secs();
    for (transform, state, aggro, profile, draw, mut intents, mut orientation, mut local) in &mut q
    {
        local.cadence_left = (local.cadence_left - dt).max(0.0);
        let mut next = CombatIntents::default();
        if *state == EnemyAiState::Combat
            && let Some(seen) = aggro.last_seen
        {
            let eye = transform.translation + Vec3::Y * AIM_PIVOT_HEIGHT;
            let to_target = seen - eye;
            if to_target.length_squared() > f32::EPSILON {
                let (yaw, pitch) = yaw_pitch_toward(to_target);
                orientation.yaw = yaw;
                orientation.pitch = pitch;
                next.wants_aim = true;
                if draw.cooldown <= 0.0 && local.cadence_left <= 0.0 {
                    let charged = draw.factor >= ARCHER_RELEASE_CHARGE;
                    // Held while under-charged; letting go is the release
                    // the aim motor turns into an arrow.
                    next.attack.held = !charged;
                    if charged {
                        local.cadence_left = profile.attack_cadence_secs;
                    }
                }
            }
        }
        *intents = next;
    }
}

/// The yaw/pitch whose `aim::aim_direction` reproduces `direction` — the
/// pure inverse of the player's aim math, so enemy arrows fly by the exact
/// same rules.
pub(crate) fn yaw_pitch_toward(direction: Vec3) -> (f32, f32) {
    let yaw = (-direction.x).atan2(-direction.z);
    let length = direction.length().max(f32::EPSILON);
    let pitch = (direction.y / length).clamp(-1.0, 1.0).asin();
    (yaw, pitch)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::motors::aim::aim_direction;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn yaw_pitch_toward_is_the_inverse_of_aim_direction() {
        for (yaw, pitch) in [
            (0.0_f32, 0.0_f32),
            (1.2, 0.4),
            (-2.4, -0.7),
            (std::f32::consts::FRAC_PI_2, 0.9),
        ] {
            let direction = aim_direction(&ControlOrientation { yaw, pitch }) * 7.3;
            let (recovered_yaw, recovered_pitch) = yaw_pitch_toward(direction);
            let forward = aim_direction(&ControlOrientation {
                yaw: recovered_yaw,
                pitch: recovered_pitch,
            });
            let original = direction.normalize();
            assert!(
                (forward - original).length() < 1e-4,
                "({yaw}, {pitch}): recovered direction {forward} != {original}"
            );
        }
    }

    fn combat_ready_melee(state: EnemyAiState, seen: Option<Vec3>) -> impl Bundle {
        (
            Enemy,
            Transform::from_xyz(0.0, 1.0, 0.0),
            state,
            AggroTarget {
                in_sight: None,
                last_seen: seen,
            },
            EnemyBrainProfile::BOKOBO,
            WeaponProfile::BOKOBO_CLUB,
            CombatIntents::default(),
            EnemyCombatLocal::default(),
        )
    }

    #[test]
    fn melee_presses_once_per_cadence_and_only_in_combat_at_reach() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let in_reach = Vec3::new(1.0, 1.0, 0.0);
        let fighting = world
            .spawn(combat_ready_melee(EnemyAiState::Combat, Some(in_reach)))
            .id();
        let chasing = world
            .spawn(combat_ready_melee(EnemyAiState::Alert, Some(in_reach)))
            .id();
        let out_of_reach = world
            .spawn(combat_ready_melee(
                EnemyAiState::Combat,
                Some(Vec3::new(10.0, 1.0, 0.0)),
            ))
            .id();

        world.run_system_once(act_melee).unwrap();

        let pressed = |world: &World, e: Entity| {
            world
                .entity(e)
                .get::<CombatIntents>()
                .unwrap()
                .attack
                .pressed
        };
        assert!(pressed(&world, fighting), "in combat at reach: swing");
        assert!(!pressed(&world, chasing), "alert is not yet fighting");
        assert!(!pressed(&world, out_of_reach), "out of reach: no whiffing");

        // Same tick cadence bookkeeping: an immediate second run must not
        // re-press (zero dt, cadence still charged).
        world.run_system_once(act_melee).unwrap();
        assert!(
            !pressed(&world, fighting),
            "the cadence must space attack edges apart"
        );
    }

    #[test]
    fn archer_holds_under_charge_and_releases_at_threshold() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let target = Vec3::new(8.0, 1.0, 0.0);
        let archer = |draw: DrawStrength| {
            (
                Enemy,
                Transform::from_xyz(0.0, 1.0, 0.0),
                EnemyAiState::Combat,
                AggroTarget {
                    in_sight: None,
                    last_seen: Some(target),
                },
                EnemyBrainProfile::BOKOBO_ARCHER,
                draw,
                CombatIntents::default(),
                ControlOrientation::default(),
                EnemyCombatLocal::default(),
            )
        };
        let drawing = world.spawn(archer(DrawStrength::default())).id();
        let charged = world
            .spawn(archer(DrawStrength {
                factor: ARCHER_RELEASE_CHARGE + 0.05,
                ..default()
            }))
            .id();

        world.run_system_once(act_archer).unwrap();

        let intents = |world: &World, e: Entity| *world.entity(e).get::<CombatIntents>().unwrap();
        assert!(intents(&world, drawing).wants_aim);
        assert!(
            intents(&world, drawing).attack.held,
            "under the threshold the string keeps pulling"
        );
        assert!(
            !intents(&world, charged).attack.held,
            "at the threshold the string lets go — the aim motor fires"
        );
        let orientation = world.entity(drawing).get::<ControlOrientation>().unwrap();
        let dir = aim_direction(orientation);
        assert!(
            dir.x > 0.9,
            "the archer must aim toward its +X target, got {dir}"
        );
    }
}
