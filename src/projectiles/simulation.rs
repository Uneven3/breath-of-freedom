//! Fixed-step arrow pool, ballistic flight and authoritative hit outcomes.

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;

use super::data::*;
use crate::combat::motors::attack::HitImpactMessage;
use crate::enemies::perception::{Awareness, DirectThreatMessage};
use crate::movement::GRAVITY;
use crate::movement::constraints::BodyImpulseMessage;
use crate::world::GameLayer;

const TRAIL_EMIT_INTERVAL: f32 = 0.016;

pub(super) fn init_pool(mut commands: Commands) {
    for slot in 0..ARROW_POOL_SIZE {
        commands.spawn((Arrow::pooled(), ArrowPoolSlot(slot), Transform::default()));
    }
}

pub(super) fn spawn_arrows(
    mut spawns: MessageReader<SpawnProjectileMessage>,
    mut arrows: Query<(&ArrowPoolSlot, &mut Arrow, &mut Transform)>,
) {
    for spawn in spawns.read() {
        let Some((_, mut arrow, mut transform)) = arrows
            .iter_mut()
            .filter(|(_, arrow, _)| !arrow.active)
            .min_by_key(|(slot, _, _)| slot.0)
        else {
            warn!(
                "arrow pool exhausted; dropping shot from {:?}",
                spawn.shooter
            );
            continue;
        };
        arrow.active = true;
        arrow.velocity = spawn.velocity;
        arrow.shooter = spawn.shooter;
        arrow.damage = spawn.damage;
        arrow.remaining = FLIGHT_TTL_SECS;
        arrow.stuck = false;
        arrow.trail_timer = 0.0;
        arrow.filter.excluded_entities.clear();
        arrow.filter.excluded_entities.insert(spawn.shooter);
        *transform = Transform::from_translation(spawn.origin)
            .looking_to(Dir3::new(spawn.velocity).unwrap_or(Dir3::NEG_Z), Vec3::Y);
    }
}

type ArrowQuery<'a> = (&'a mut Arrow, &'a mut Transform);

#[derive(SystemParam)]
pub(super) struct HitOutcomes<'w> {
    damage: MessageWriter<'w, crate::health::DamageRequestMessage>,
    impacts: MessageWriter<'w, HitImpactMessage>,
    threats: MessageWriter<'w, DirectThreatMessage>,
    impulses: MessageWriter<'w, BodyImpulseMessage>,
}

#[derive(SystemParam)]
pub(super) struct BallisticWorld<'w, 's> {
    spatial: SpatialQuery<'w, 's>,
    layers: Query<'w, 's, &'static CollisionLayers>,
    targets: Query<
        'w,
        's,
        (
            Option<&'static Awareness>,
            Option<&'static Name>,
            Option<&'static crate::health::HostileInteractionImmunity>,
        ),
    >,
}

pub(super) fn fly_arrows(
    time: Res<Time>,
    mut arrows: Query<ArrowQuery>,
    mut outcomes: HitOutcomes,
    world: BallisticWorld,
    mut trails: MessageWriter<ArrowTrailMessage>,
) {
    let dt = time.delta_secs();
    for (mut arrow, mut transform) in &mut arrows {
        if !arrow.active {
            continue;
        }
        arrow.remaining -= dt;
        if arrow.remaining <= 0.0 {
            arrow.deactivate();
            continue;
        }
        if arrow.stuck {
            continue;
        }

        arrow.velocity.y -= GRAVITY * 0.22 * dt;
        let step = arrow.velocity * dt;
        let Ok(direction) = Dir3::new(step) else {
            continue;
        };
        match world.spatial.cast_ray(
            transform.translation,
            direction,
            step.length(),
            true,
            &arrow.filter,
        ) {
            Some(hit) => {
                let hit_point = transform.translation + *direction * hit.distance;
                let struck_actor = world
                    .layers
                    .get(hit.entity)
                    .is_ok_and(|layers| layers.memberships.has_all(GameLayer::Actor));
                if struck_actor {
                    resolve_arrow_hit(&arrow, hit.entity, hit_point, &world.targets, &mut outcomes);
                    arrow.deactivate();
                } else {
                    transform.translation = hit_point;
                    arrow.stuck = true;
                    arrow.remaining = STUCK_TTL_SECS;
                }
            }
            None => {
                transform.translation += step;
                transform.look_to(direction, Vec3::Y);
            }
        }

        arrow.trail_timer += dt;
        if arrow.trail_timer >= TRAIL_EMIT_INTERVAL {
            arrow.trail_timer -= TRAIL_EMIT_INTERVAL;
            trails.write(ArrowTrailMessage(transform.translation));
        }
    }
}

pub(crate) fn arrow_damage(base: f32, target_alerted: bool) -> (f32, bool) {
    if target_alerted {
        (base, false)
    } else {
        (base * ARROW_STEALTH_MULT, true)
    }
}

fn resolve_arrow_hit(
    arrow: &Arrow,
    target: Entity,
    hit_point: Vec3,
    targets: &Query<(
        Option<&Awareness>,
        Option<&Name>,
        Option<&crate::health::HostileInteractionImmunity>,
    )>,
    outcomes: &mut HitOutcomes,
) {
    let (awareness, name, immunity) = targets.get(target).unwrap_or((None, None, None));
    if immunity.is_some_and(|immunity| immunity.blocks(arrow.shooter)) {
        return;
    }
    let (damage, critical) =
        arrow_damage(arrow.damage, awareness.is_none_or(Awareness::is_alerted));
    if critical {
        info!(
            "[combat] STEALTH SHOT on {}",
            name.map(Name::as_str).unwrap_or("target")
        );
    }
    outcomes.damage.write(crate::health::DamageRequestMessage {
        target,
        amount: damage,
        source: Some(arrow.shooter),
    });
    outcomes.impacts.write(HitImpactMessage {
        target,
        attacker: arrow.shooter,
        position: hit_point,
        damage,
        critical,
        melee: false,
    });
    let planar = Vec3::new(arrow.velocity.x, 0.0, arrow.velocity.z).normalize_or_zero();
    outcomes.impulses.write(BodyImpulseMessage {
        entity: target,
        impulse: planar * ARROW_KNOCKBACK,
    });
    outcomes.threats.write(DirectThreatMessage {
        enemy: target,
        threat_position: hit_point - arrow.velocity.normalize_or_zero() * 6.0,
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[derive(Component)]
    struct TestTarget;

    fn resolve_test_arrow(
        arrows: Query<&Arrow>,
        targets: Query<(
            Option<&Awareness>,
            Option<&Name>,
            Option<&crate::health::HostileInteractionImmunity>,
        )>,
        target: Single<Entity, With<TestTarget>>,
        mut outcomes: HitOutcomes,
    ) {
        resolve_arrow_hit(
            arrows.single().unwrap(),
            *target,
            Vec3::ZERO,
            &targets,
            &mut outcomes,
        );
    }

    #[test]
    fn arrows_reward_hitting_the_unaware() {
        assert_eq!(arrow_damage(12.0, true), (12.0, false));
        assert_eq!(arrow_damage(12.0, false), (48.0, true));
    }

    #[test]
    fn fixed_spawn_uses_the_bounded_pool_without_creating_entities() {
        let mut world = World::new();
        world.init_resource::<Messages<SpawnProjectileMessage>>();
        world.run_system_once(init_pool).unwrap();
        let shooter = world.spawn_empty().id();
        let entity_count = world.query::<Entity>().iter(&world).count();
        for _ in 0..=ARROW_POOL_SIZE {
            world.write_message(SpawnProjectileMessage {
                shooter,
                origin: Vec3::ZERO,
                velocity: Vec3::NEG_Z,
                damage: 1.0,
            });
        }

        world.run_system_once(spawn_arrows).unwrap();

        let active = world
            .query::<&Arrow>()
            .iter(&world)
            .filter(|arrow| arrow.active)
            .count();
        assert_eq!(active, ARROW_POOL_SIZE as usize);
        assert_eq!(world.query::<Entity>().iter(&world).count(), entity_count);
    }

    #[test]
    fn hostile_immunity_blocks_all_arrow_outcomes() {
        let mut world = World::new();
        world.init_resource::<Messages<crate::health::DamageRequestMessage>>();
        world.init_resource::<Messages<HitImpactMessage>>();
        world.init_resource::<Messages<DirectThreatMessage>>();
        world.init_resource::<Messages<BodyImpulseMessage>>();
        let shooter = world.spawn_empty().id();
        let mut arrow = Arrow::pooled();
        arrow.active = true;
        arrow.velocity = Vec3::NEG_Z;
        arrow.shooter = shooter;
        arrow.damage = 10.0;
        arrow.remaining = 1.0;
        world.spawn(arrow);
        world.spawn((
            TestTarget,
            crate::health::HostileInteractionImmunity(shooter),
        ));

        world.run_system_once(resolve_test_arrow).unwrap();

        assert!(
            world
                .resource::<Messages<crate::health::DamageRequestMessage>>()
                .is_empty()
        );
        assert!(world.resource::<Messages<HitImpactMessage>>().is_empty());
        assert!(world.resource::<Messages<DirectThreatMessage>>().is_empty());
        assert!(world.resource::<Messages<BodyImpulseMessage>>().is_empty());
    }
}
