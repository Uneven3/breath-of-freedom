use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use crate::combat::motors::attack::HitImpactMessage;
use crate::enemies::Enemy;
use crate::health::{DamageRequestMessage, HostileInteractionImmunity};
use crate::movement::constraints::BodyImpulseMessage;
use crate::movement::{BodyVelocity, intents::Intents};
use crate::world::GameLayer;

use super::charge_data::{ChargeHitLedger, ChargeShape};
use super::data::{Horse, HorseCharge};

const CHARGE_ENTER_SPEED: f32 = 11.0;
const CHARGE_EXIT_SPEED: f32 = 10.0;
const CHARGE_DAMAGE: f32 = 18.0;
const CHARGE_KNOCKBACK: f32 = 9.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RecordHit {
    New,
    Duplicate,
}

fn record_hit(
    ledger: &mut ChargeHitLedger,
    horse: Entity,
    generation: u64,
    enemy: Entity,
) -> RecordHit {
    let key = (horse, generation, enemy);
    if ledger.hits.contains(&key) {
        return RecordHit::Duplicate;
    }
    ledger.hits.insert(key);
    RecordHit::New
}

fn clear_generation(ledger: &mut ChargeHitLedger, horse: Entity, generation: u64) {
    ledger.hits.retain(|(seen_horse, seen_generation, _)| {
        *seen_horse != horse || *seen_generation != generation
    });
}

/// Drop ledger entries whose horse or enemy despawned, so the set doesn't
/// accumulate dead keys. Runs in `FixedUpdate` right before the sweep.
pub fn prune_hit_ledger(
    mut ledger: ResMut<ChargeHitLedger>,
    horses: Query<(), With<Horse>>,
    enemies: Query<(), With<Enemy>>,
) {
    ledger
        .hits
        .retain(|(horse, _, enemy)| horses.get(*horse).is_ok() && enemies.get(*enemy).is_ok());
}

type HorseQuery<'a> = (
    Entity,
    &'a Transform,
    &'a BodyVelocity,
    &'a Intents,
    &'a mut HorseCharge,
);

type EnemyQuery<'a> = (&'a Transform, Option<&'a HostileInteractionImmunity>);

#[derive(SystemParam)]
pub struct ChargeOutcomes<'w> {
    damage: MessageWriter<'w, DamageRequestMessage>,
    impulses: MessageWriter<'w, BodyImpulseMessage>,
    impacts: MessageWriter<'w, HitImpactMessage>,
}

#[derive(SystemParam)]
pub struct ChargeWorld<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    enemies: Query<'w, 's, EnemyQuery<'static>, With<Enemy>>,
    layers: Query<'w, 's, &'static CollisionLayers>,
}

pub fn detect_charge_hits(
    mut horses: Query<HorseQuery, With<Horse>>,
    mut ledger: ResMut<ChargeHitLedger>,
    shape: Res<ChargeShape>,
    world: ChargeWorld,
    mut outcomes: ChargeOutcomes,
) {
    for (horse, transform, velocity, intents, mut charge) in &mut horses {
        let planar_velocity = Vec3::new(velocity.0.x, 0.0, velocity.0.z);
        let speed = planar_velocity.length();
        let was_active = charge.active;
        charge.active = if charge.active {
            intents.wants_sprint && speed > CHARGE_EXIT_SPEED
        } else {
            intents.wants_sprint && speed >= CHARGE_ENTER_SPEED
        };

        if !charge.active {
            if was_active {
                clear_generation(&mut ledger, horse, charge.generation);
            }
            charge.previous_position = transform.translation;
            continue;
        }
        if !was_active {
            charge.generation = charge.generation.wrapping_add(1);
        }

        let start = charge.previous_position;
        let filter = SpatialQueryFilter::from_mask(GameLayer::Actor);
        let generation = charge.generation;
        let knockback_direction = planar_velocity.normalize_or_zero();

        let mut resolve_candidate = |candidate: Entity| {
            let Ok((target_transform, immunity)) = world.enemies.get(candidate) else {
                return true;
            };
            if immunity.is_some_and(|policy| policy.blocks(horse))
                || occluded_by_world(
                    &world.spatial,
                    &world.layers,
                    start,
                    target_transform.translation,
                    horse,
                    candidate,
                )
            {
                return true;
            }
            emit_charge_outcomes(
                horse,
                generation,
                candidate,
                target_transform.translation,
                knockback_direction,
                &mut ledger,
                &mut outcomes,
            );
            true
        };

        if let Some((direction, distance)) = sweep_segment(start, transform.translation) {
            world.spatial.shape_hits_callback(
                &shape.0,
                start,
                Quat::IDENTITY,
                direction,
                &ShapeCastConfig::from_max_distance(distance),
                &filter,
                |hit| resolve_candidate(hit.entity),
            );
        } else {
            world.spatial.shape_intersections_callback(
                &shape.0,
                transform.translation,
                Quat::IDENTITY,
                &filter,
                resolve_candidate,
            );
        }
        charge.previous_position = transform.translation;
    }
}

/// Returns the exact segment traversed since the previous physics tick.
///
/// Charge detection casts across this entire segment rather than sampling the
/// horse only at either endpoint, so its coverage does not depend on speed.
fn sweep_segment(start: Vec3, end: Vec3) -> Option<(Dir3, f32)> {
    let displacement = end - start;
    Dir3::new(displacement)
        .ok()
        .map(|direction| (direction, displacement.length()))
}

fn emit_charge_outcomes(
    horse: Entity,
    generation: u64,
    target: Entity,
    target_position: Vec3,
    knockback_direction: Vec3,
    ledger: &mut ChargeHitLedger,
    outcomes: &mut ChargeOutcomes,
) -> RecordHit {
    if record_hit(ledger, horse, generation, target) == RecordHit::Duplicate {
        return RecordHit::Duplicate;
    }
    outcomes.damage.write(DamageRequestMessage {
        target,
        amount: CHARGE_DAMAGE,
        source: Some(horse),
    });
    outcomes.impulses.write(BodyImpulseMessage {
        entity: target,
        impulse: knockback_direction * CHARGE_KNOCKBACK,
    });
    outcomes.impacts.write(HitImpactMessage {
        target,
        attacker: horse,
        position: target_position,
        damage: CHARGE_DAMAGE,
        critical: false,
        melee: false,
    });
    RecordHit::New
}

fn occluded_by_world(
    spatial: &SpatialQuery,
    layers: &Query<&CollisionLayers>,
    origin: Vec3,
    target: Vec3,
    horse: Entity,
    enemy: Entity,
) -> bool {
    let delta = target - origin;
    let Ok(direction) = Dir3::new(delta) else {
        return false;
    };
    let filter = SpatialQueryFilter::DEFAULT;
    spatial
        .cast_ray_predicate(
            origin,
            direction,
            delta.length(),
            true,
            &filter,
            &|entity| {
                entity != horse
                    && entity != enemy
                    && !layers
                        .get(entity)
                        .is_ok_and(|value| value.memberships.has_all(GameLayer::Actor))
            },
        )
        .is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    fn emit_twice(
        horse: Single<Entity, With<Horse>>,
        enemy: Single<(Entity, &Transform), With<Enemy>>,
        mut ledger: ResMut<ChargeHitLedger>,
        mut outcomes: ChargeOutcomes,
    ) {
        for _ in 0..2 {
            emit_charge_outcomes(
                *horse,
                1,
                enemy.0,
                enemy.1.translation,
                Vec3::X,
                &mut ledger,
                &mut outcomes,
            );
        }
    }

    fn spatial_charge_outcome_counts(enemy_position: Vec3, wall: bool) -> (usize, usize, usize) {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            PhysicsPlugins::default(),
            bevy::asset::AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
        ));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )));
        app.init_resource::<ChargeHitLedger>();
        app.init_resource::<ChargeShape>();
        app.init_resource::<Messages<DamageRequestMessage>>();
        app.init_resource::<Messages<BodyImpulseMessage>>();
        app.init_resource::<Messages<HitImpactMessage>>();
        app.finish();

        app.world_mut().spawn((
            Horse,
            Transform::from_xyz(30.0, 0.0, 0.0),
            BodyVelocity(Vec3::X * 12.0),
            Intents {
                wants_sprint: true,
                ..default()
            },
            HorseCharge::new(Vec3::ZERO),
            RigidBody::Kinematic,
            Collider::sphere(0.5),
            CollisionLayers::new(GameLayer::Actor, LayerMask::ALL),
        ));
        app.world_mut().spawn((
            Enemy,
            Transform::from_translation(enemy_position),
            RigidBody::Static,
            Collider::sphere(0.5),
            CollisionLayers::new(GameLayer::Actor, LayerMask::ALL),
        ));
        if wall {
            app.world_mut().spawn((
                Transform::from_xyz(7.5, 0.0, 0.0),
                RigidBody::Static,
                Collider::cuboid(0.5, 5.0, 5.0),
                CollisionLayers::new(GameLayer::Default, LayerMask::ALL),
            ));
        }

        // Populate Avian's collider tree from the current simulation world,
        // then run charge detection against that real spatial-query backend.
        app.update();
        app.world_mut().run_system_once(prune_hit_ledger).unwrap();
        app.world_mut().run_system_once(detect_charge_hits).unwrap();

        let damage = app.world().resource::<Messages<DamageRequestMessage>>();
        let mut damage_cursor = damage.get_cursor();
        let damage_count = damage_cursor.read(damage).count();
        let impulses = app.world().resource::<Messages<BodyImpulseMessage>>();
        let mut impulse_cursor = impulses.get_cursor();
        let impulse_count = impulse_cursor.read(impulses).count();
        let impacts = app.world().resource::<Messages<HitImpactMessage>>();
        let mut impact_cursor = impacts.get_cursor();
        let impact_count = impact_cursor.read(impacts).count();
        (damage_count, impulse_count, impact_count)
    }

    #[test]
    fn hysteresis_rearms_only_after_leaving_the_exit_threshold() {
        let mut charge = HorseCharge::new(Vec3::ZERO);
        let update = |charge: &mut HorseCharge, speed: f32, sprint: bool| {
            charge.active = if charge.active {
                sprint && speed > CHARGE_EXIT_SPEED
            } else {
                sprint && speed >= CHARGE_ENTER_SPEED
            };
        };
        update(&mut charge, 11.1, true);
        assert!(charge.active);
        update(&mut charge, 10.5, true);
        assert!(charge.active, "threshold jitter must not rearm");
        update(&mut charge, 9.9, true);
        assert!(!charge.active);
        update(&mut charge, 10.5, true);
        assert!(!charge.active);
        update(&mut charge, 11.1, true);
        assert!(charge.active);
    }

    #[test]
    fn high_speed_sweep_covers_the_entire_tick_displacement() {
        let (direction, distance) = sweep_segment(Vec3::ZERO, Vec3::new(30.0, 0.0, 0.0))
            .expect("non-zero displacement must produce a sweep");

        assert_eq!(direction, Dir3::X);
        assert_eq!(distance, 30.0);
        assert!(sweep_segment(Vec3::ONE, Vec3::ONE).is_none());
    }

    #[test]
    fn spatial_sweep_hits_an_enemy_between_high_speed_endpoints() {
        assert_eq!(
            spatial_charge_outcome_counts(Vec3::new(15.0, 0.0, 0.0), false),
            (1, 1, 1)
        );
    }

    #[test]
    fn world_geometry_occludes_charge_outcomes() {
        assert_eq!(
            spatial_charge_outcome_counts(Vec3::new(15.0, 0.0, 0.0), true),
            (0, 0, 0)
        );
    }

    #[test]
    fn spatial_sweep_rejects_an_enemy_outside_vertical_reach() {
        assert_eq!(
            spatial_charge_outcome_counts(Vec3::new(15.0, 4.0, 0.0), false),
            (0, 0, 0)
        );
    }

    #[test]
    fn ledger_handles_more_than_sixteen_targets_and_two_horses() {
        let mut world = World::new();
        let horses = [world.spawn_empty().id(), world.spawn_empty().id()];
        let enemies = (0..20)
            .map(|_| world.spawn_empty().id())
            .collect::<Vec<_>>();
        let mut ledger = ChargeHitLedger::default();

        for horse in horses {
            for enemy in &enemies {
                assert_eq!(record_hit(&mut ledger, horse, 1, *enemy), RecordHit::New);
                assert_eq!(
                    record_hit(&mut ledger, horse, 1, *enemy),
                    RecordHit::Duplicate
                );
            }
        }

        assert_eq!(ledger.hits.len(), 40);
        clear_generation(&mut ledger, horses[0], 1);
        for enemy in enemies {
            assert_eq!(record_hit(&mut ledger, horses[0], 2, enemy), RecordHit::New);
        }
    }

    #[test]
    fn one_enemy_gets_damage_knockback_and_feedback_once_per_generation() {
        let mut world = World::new();
        world.init_resource::<ChargeHitLedger>();
        world.init_resource::<Messages<DamageRequestMessage>>();
        world.init_resource::<Messages<BodyImpulseMessage>>();
        world.init_resource::<Messages<HitImpactMessage>>();
        let horse = world.spawn(Horse).id();
        let enemy = world
            .spawn((Enemy, Transform::from_xyz(2.0, 0.0, 0.0)))
            .id();

        world.run_system_once(emit_twice).unwrap();

        let damage = world.resource::<Messages<DamageRequestMessage>>();
        let mut damage_cursor = damage.get_cursor();
        let damage = damage_cursor.read(damage).collect::<Vec<_>>();
        assert_eq!(damage.len(), 1);
        assert_eq!(damage[0].target, enemy);
        assert_eq!(damage[0].source, Some(horse));
        let impulses = world.resource::<Messages<BodyImpulseMessage>>();
        let mut impulse_cursor = impulses.get_cursor();
        assert_eq!(impulse_cursor.read(impulses).count(), 1);
        let impacts = world.resource::<Messages<HitImpactMessage>>();
        let mut impact_cursor = impacts.get_cursor();
        assert_eq!(impact_cursor.read(impacts).count(), 1);
    }
}
