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
//! Reduced actors are staggered by entity index so a camp of N enemies spreads
//! its casts across the interval window instead of bursting on the same tick.
//!
//! The local player always senses at full rate, as does everyone when no
//! player exists (a safe default for tests and headless worlds).

use bevy::prelude::*;

use super::{Actor, Player};

/// Tuning for the sensing LOD. World- and encounter-scale dependent, so it is
/// a resource meant to be tweaked, not a set of scattered constants.
#[derive(Resource, Clone, Debug)]
pub struct SensingLodConfig {
    /// Distance (m) from the local player within which actors sense every tick.
    pub full_rate_radius: f32,
    /// Beyond the radius, sense once every this many ticks (>= 1). At 60 Hz a
    /// value of 4 means distant actors refresh their facts at 15 Hz.
    pub reduced_interval: u32,
}

impl Default for SensingLodConfig {
    fn default() -> Self {
        Self {
            full_rate_radius: 30.0,
            reduced_interval: 4,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SensingTier {
    #[default]
    Full,
    Reduced,
}

/// Per-actor LOD decision, rewritten every tick by [`assign_sensing_lod`].
/// Defaults to sensing (full rate), so actors sense normally in worlds that
/// never run the assignment system.
#[derive(Component, Debug, Clone, Copy)]
pub struct SensingLod {
    pub tier: SensingTier,
    pub sense_this_tick: bool,
}

impl Default for SensingLod {
    fn default() -> Self {
        Self {
            tier: SensingTier::Full,
            sense_this_tick: true,
        }
    }
}

impl SensingLod {
    /// Guard used by the sensing services: `None` (no LOD component) senses.
    pub fn skips(lod: Option<&SensingLod>) -> bool {
        lod.is_some_and(|lod| !lod.sense_this_tick)
    }
}

type PlayerAnchor<'w, 's> = Single<'w, 's, &'static Transform, (With<Player>, With<Actor>)>;

/// Runs after `ReadIntents`, before `SenseWorld`: classify every actor by
/// distance to the local player and decide whether it senses this tick.
pub fn assign_sensing_lod(
    config: Res<SensingLodConfig>,
    mut tick: Local<u32>,
    player: Option<PlayerAnchor>,
    mut actors: Query<(Entity, &Transform, &mut SensingLod, Has<Player>), With<Actor>>,
) {
    *tick = tick.wrapping_add(1);
    let anchor = player.map(|p| p.translation);

    for (entity, transform, mut lod, is_player) in &mut actors {
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
            SensingTier::Reduced => {
                senses_on(*tick, entity.index().index(), config.reduced_interval)
            }
        };
    }
}

/// Pure stagger rule: a reduced actor senses on the ticks where its
/// entity-index phase lines up with the interval.
fn senses_on(tick: u32, entity_index: u32, interval: u32) -> bool {
    interval <= 1 || tick.wrapping_add(entity_index).is_multiple_of(interval)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn stagger_fires_once_per_interval_per_actor() {
        let interval = 4;
        for entity_index in [0, 1, 2, 7] {
            let fired: Vec<u32> = (0..12)
                .filter(|&tick| senses_on(tick, entity_index, interval))
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
        // Two consecutive entity indices must not cast on the same tick.
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
                Player,
                Transform::from_xyz(0.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();
        let near = world
            .spawn((
                Actor,
                Transform::from_xyz(5.0, 0.0, 0.0),
                SensingLod::default(),
            ))
            .id();
        let far = world
            .spawn((
                Actor,
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
        world.spawn((Actor, Player, Transform::IDENTITY, SensingLod::default()));
        let far = world
            .spawn((
                Actor,
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
