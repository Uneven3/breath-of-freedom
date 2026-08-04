//! Sensing LOD — distance-based level of detail for the `SenseWorld` phase.
//!
//! The sensing services fire several shape/ray casts per actor per fixed tick.
//! That is the right spend for actors near the player, and a waste for a camp
//! of enemies far away whose facts nobody can see flicker. `assign_sensing_lod`
//! classifies every actor each tick (before `SenseWorld`); each service then
//! skips actors whose `SensingLod` says this is not their tick. Skipped actors
//! keep their previous `*Facts` — staleness is bounded by
//! `SensingLodConfig::reduced_interval` ticks.
//!
//! Reduced actors are staggered by stable actor identity so a camp of N enemies
//! spreads its casts across the interval window without depending on spawn order.
//!
//! The local player always senses at full rate, as does everyone when no
//! player exists (a safe default for tests and headless worlds).

use bevy_ecs::prelude::*;
use bevy_transform::components::Transform;

use super::{Actor, ActorId, Player};

pub use bof_domain::movement::lod::{SensingLod, SensingLodConfig, SensingTier};

type PlayerAnchor<'w, 's> = Single<'w, 's, &'static Transform, (With<Player>, With<Actor>)>;

/// Runs after `ReadIntents`, before `SenseWorld`: classify every actor by
/// distance to the local player and decide whether it senses this tick.
pub fn assign_sensing_lod(
    config: Res<SensingLodConfig>,
    mut tick: Local<u32>,
    player: Option<PlayerAnchor>,
    mut actors: Query<(&ActorId, &Transform, &mut SensingLod, Has<Player>), With<Actor>>,
) {
    *tick = tick.wrapping_add(1);
    let anchor = player.map(|p| p.translation);

    for (actor_id, transform, mut lod, is_player) in &mut actors {
        let tier = match anchor {
            Some(anchor)
                if !is_player
                    && transform.translation.distance_squared(anchor)
                        > config.full_rate_radius * config.full_rate_radius =>
            {
                SensingTier::Reduced
            }
            _ => SensingTier::Full,
        };
        lod.tier = tier;
        lod.sense_this_tick = match tier {
            SensingTier::Full => true,
            SensingTier::Reduced => senses_on(*tick, actor_id.value(), config.reduced_interval),
        };
    }
}

/// Pure stagger rule: a reduced actor senses on the ticks where its authored
/// identity phase lines up with the interval.
fn senses_on(tick: u32, actor_id: u32, interval: u32) -> bool {
    interval <= 1 || tick.wrapping_add(actor_id).is_multiple_of(interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy_ecs::system::RunSystemOnce;

    #[test]
    fn stagger_fires_once_per_interval_per_actor() {
        let interval = 4;
        for actor_id in [0, 1, 2, 7] {
            let fired: Vec<u32> = (0..12)
                .filter(|&tick| senses_on(tick, actor_id, interval))
                .collect();
            assert_eq!(fired.len(), 3, "3 fires in 12 ticks at interval 4");
            assert!(
                fired.windows(2).all(|w| w[1] - w[0] == interval),
                "fires must be evenly spaced"
            );
        }
    }

    #[test]
    fn stagger_spreads_adjacent_actors_across_ticks() {
        // Two consecutive authored IDs must not cast on the same tick.
        assert_ne!(senses_on(8, 0, 4), senses_on(8, 1, 4));
    }

    #[test]
    fn degenerate_intervals_always_sense() {
        assert!(senses_on(7, 3, 0));
        assert!(senses_on(7, 3, 1));
    }

    #[test]
    fn player_is_always_full_rate_and_distant_actors_reduce() {
        let mut world = World::new();
        world.insert_resource(SensingLodConfig {
            full_rate_radius: 10.0,
            reduced_interval: 4,
        });
        let player = world
            .spawn((
                Actor,
                ActorId::PLAYER,
                Player,
                Transform::from_xyz(0.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();
        let near = world
            .spawn((
                Actor,
                ActorId::authored(10),
                Transform::from_xyz(5.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();
        let far = world
            .spawn((
                Actor,
                ActorId::authored(11),
                Transform::from_xyz(50.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();

        world.run_system_once(assign_sensing_lod).unwrap();

        assert_eq!(
            world.entity(player).get::<SensingLod>().unwrap().tier,
            SensingTier::Full
        );
        assert_eq!(
            world.entity(near).get::<SensingLod>().unwrap().tier,
            SensingTier::Full
        );
        assert_eq!(
            world.entity(far).get::<SensingLod>().unwrap().tier,
            SensingTier::Reduced
        );
        assert!(
            world
                .entity(near)
                .get::<SensingLod>()
                .unwrap()
                .sense_this_tick
        );
    }

    #[test]
    fn without_a_player_everyone_senses_at_full_rate() {
        let mut world = World::new();
        world.insert_resource(SensingLodConfig::default());
        let lone = world
            .spawn((
                Actor,
                ActorId::authored(12),
                Transform::from_xyz(1000.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();

        world.run_system_once(assign_sensing_lod).unwrap();

        let lod = world.entity(lone).get::<SensingLod>().unwrap();
        assert_eq!(lod.tier, SensingTier::Full);
        assert!(lod.sense_this_tick);
    }

    #[test]
    fn reduced_actor_senses_exactly_once_per_interval() {
        let mut world = World::new();
        world.insert_resource(SensingLodConfig {
            full_rate_radius: 10.0,
            reduced_interval: 4,
        });
        world.spawn((
            Actor,
            ActorId::PLAYER,
            Player,
            Transform::IDENTITY,
            SensingLod::default(),
        ));
        let far = world
            .spawn((
                Actor,
                ActorId::authored(13),
                Transform::from_xyz(50.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();

        // `register_system` (not `run_system_once`) so the tick `Local`
        // persists across runs.
        let system = world.register_system(assign_sensing_lod);
        let mut sensed = 0;
        for _ in 0..8 {
            world.run_system(system).unwrap();
            if world
                .entity(far)
                .get::<SensingLod>()
                .unwrap()
                .sense_this_tick
            {
                sensed += 1;
            }
        }
        assert_eq!(sensed, 2, "2 senses in 8 ticks at interval 4");
    }
}
