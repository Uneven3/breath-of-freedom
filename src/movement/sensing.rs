//! Per-actor physical sensing profiles used by Movement services.

use avian3d::prelude::Collider;
use bevy::prelude::*;

pub use bof_domain::movement::sensing::{GroundSensing, LedgeSensing};

/// Prebuilt sphere used by all ledge shape casts for one actor.
#[derive(Component)]
pub struct LedgeCastShape(pub Collider);

impl LedgeCastShape {
    pub fn new(sensing: LedgeSensing) -> Self {
        Self(Collider::sphere(sensing.sphere_radius))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::Actor;

    #[test]
    fn player_profiles_preserve_the_validated_sensor_values() {
        assert_eq!(GroundSensing::PLAYER.probe_distance, 0.2);
        assert_eq!(GroundSensing::PLAYER.ascend_epsilon, 0.1);
        assert_eq!(
            LedgeSensing::PLAYER.height_samples,
            [-0.8, -0.6, -0.2, 0.2, 0.4, 0.6]
        );
        assert_eq!(LedgeSensing::PLAYER.wall_detection_reach, 0.65);
        assert_eq!(LedgeSensing::PLAYER.mantle_max_height, 2.5);
        assert_eq!(LedgeSensing::PLAYER.vault_detection_range, 1.4);
    }

    #[test]
    fn sensor_profiles_select_only_actors_that_opt_in() {
        let mut world = World::new();
        let ground_actor = world.spawn((Actor, GroundSensing::PLAYER)).id();
        let ledge_actor = world.spawn((Actor, LedgeSensing::PLAYER)).id();
        let no_sensor_actor = world.spawn(Actor).id();

        let ground_selected: Vec<_> = world
            .query_filtered::<Entity, (With<Actor>, With<GroundSensing>)>()
            .iter(&world)
            .collect();
        let ledge_selected: Vec<_> = world
            .query_filtered::<Entity, (With<Actor>, With<LedgeSensing>)>()
            .iter(&world)
            .collect();

        assert_eq!(ground_selected, vec![ground_actor]);
        assert_eq!(ledge_selected, vec![ledge_actor]);
        assert!(!ground_selected.contains(&no_sensor_actor));
        assert!(!ledge_selected.contains(&no_sensor_actor));
    }
}
