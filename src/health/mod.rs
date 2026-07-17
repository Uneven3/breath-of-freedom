//! Health — the shared hit-point pool (see `docs/ARCHITECTURE.md`).
//!
//! One system: `apply_damage` turns `DamageRequestMessage`s into pool
//! mutations and (once per death) `DeathMessage`s. No Broker: a pool has no
//! exclusive states to arbitrate (`docs/ARCHITECTURE.md`).
//! Death consequences live with each actor's owner (Player respawns,
//! enemies/targets despawn).

use bevy::prelude::*;

pub mod data;

pub use data::{DamageRequestMessage, DeathMessage, Health, HostileInteractionImmunity};

use crate::projectiles::ProjectilesSet;

/// Ordering handle for damage application within `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum HealthSet {
    /// Requests → pool mutation + applied/death messages. Runs after every
    /// same-tick damage emitter (melee resolves in
    /// `CombatSet::EmitConstraints`; arrows in `ProjectilesSet::Simulate`,
    /// which is already after it).
    Apply,
}

pub struct HealthPlugin;

impl Plugin for HealthPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<DamageRequestMessage>();
        app.add_message::<DeathMessage>();
        app.configure_sets(
            FixedUpdate,
            HealthSet::Apply.after(ProjectilesSet::Simulate),
        );
        app.add_systems(FixedUpdate, apply_damage.in_set(HealthSet::Apply));
    }
}

/// The only writer of `Health`. Requests against entities without a
/// `Health`, or already dead, are ignored (Constitución §9: game-state
/// conditions are handled, never panicked on).
pub fn apply_damage(
    mut requests: MessageReader<DamageRequestMessage>,
    mut pools: Query<(
        &mut Health,
        Option<&Name>,
        Option<&HostileInteractionImmunity>,
    )>,
    mut deaths: MessageWriter<DeathMessage>,
) {
    for request in requests.read() {
        let Ok((mut health, name, immunity)) = pools.get_mut(request.target) else {
            continue;
        };
        if request
            .source
            .is_some_and(|source| immunity.is_some_and(|immune| immune.blocks(source)))
        {
            continue;
        }
        if health.is_dead() {
            continue;
        }
        let amount = health.apply_damage(request.amount);
        let label = name.map(Name::as_str).unwrap_or("target");
        info!(
            "[health] {label} took {amount:.0} → {:.0}/{:.0}",
            health.current(),
            health.max()
        );
        if health.is_dead() {
            deaths.write(DeathMessage {
                entity: request.target,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn world_with_messages() -> World {
        let mut world = World::new();
        world.init_resource::<Messages<DamageRequestMessage>>();
        world.init_resource::<Messages<DeathMessage>>();
        world
    }

    fn deaths(world: &World) -> Vec<Entity> {
        let messages = world.resource::<Messages<DeathMessage>>();
        let mut cursor = messages.get_cursor();
        cursor.read(messages).map(|d| d.entity).collect()
    }

    #[test]
    fn pool_clamps_and_reports_applied_damage() {
        let mut health = Health::new(30.0);
        assert_eq!(health.apply_damage(10.0), 10.0);
        assert_eq!(health.current(), 20.0);
        assert_eq!(health.apply_damage(50.0), 20.0, "overkill clamps");
        assert!(health.is_dead());
        health.heal_full();
        assert_eq!(health.current(), health.max());
    }

    #[test]
    fn lethal_request_emits_death_exactly_once() {
        let mut world = world_with_messages();
        let victim = world.spawn((Health::new(10.0), Name::new("Victim"))).id();

        for _ in 0..3 {
            world.write_message(DamageRequestMessage {
                target: victim,
                amount: 8.0,
                source: None,
            });
        }
        world.run_system_once(apply_damage).unwrap();

        assert_eq!(
            deaths(&world),
            vec![victim],
            "three requests, one lethal crossing, one DeathMessage"
        );
        assert!(world.entity(victim).get::<Health>().unwrap().is_dead());
    }

    #[test]
    fn requests_against_missing_or_dead_pools_are_ignored() {
        let mut world = world_with_messages();
        let no_pool = world.spawn_empty().id();
        let dead = world.spawn(Health::new(0.0)).id();

        world.write_message(DamageRequestMessage {
            target: no_pool,
            amount: 5.0,
            source: None,
        });
        world.write_message(DamageRequestMessage {
            target: dead,
            amount: 5.0,
            source: None,
        });
        world.run_system_once(apply_damage).unwrap();

        assert!(
            deaths(&world).is_empty(),
            "no pool / already dead: no death re-emission"
        );
    }

    #[test]
    fn immunity_rejects_only_the_matching_damage_source() {
        let mut world = world_with_messages();
        let owner = world.spawn_empty().id();
        let enemy = world.spawn_empty().id();
        let horse = world
            .spawn((Health::new(20.0), HostileInteractionImmunity(owner)))
            .id();
        world.write_message(DamageRequestMessage {
            target: horse,
            amount: 10.0,
            source: Some(owner),
        });
        world.write_message(DamageRequestMessage {
            target: horse,
            amount: 5.0,
            source: Some(enemy),
        });
        world.run_system_once(apply_damage).unwrap();
        assert_eq!(world.entity(horse).get::<Health>().unwrap().current(), 15.0);
    }
}
