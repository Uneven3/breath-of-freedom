//! Per-actor physical sensing profiles used by Movement services.

use avian3d::prelude::Collider;
use bevy::prelude::*;

/// Configuration for GroundService's downward shape cast.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GroundSensing {
    pub probe_distance: f32,
    pub ascend_epsilon: f32,
}

impl GroundSensing {
    pub const PLAYER: Self = Self {
        probe_distance: 0.2,
        ascend_epsilon: 0.1,
    };
}

/// Configuration for LedgeService's wall, ledge, and vault probes.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct LedgeSensing {
    pub height_samples: [f32; 6],
    pub sphere_radius: f32,
    pub wall_detection_reach: f32,
    pub down_cast_margin: f32,
    pub forward_sample_offset: f32,
    pub vault_distance_margin: f32,
    pub steep_face_normal_y_max: f32,
    pub vault_forward_radius_multiplier: f32,
    pub vault_detection_range: f32,
    pub vault_min_height: f32,
    pub vault_surface_clearance: f32,
    pub mantle_max_height: f32,
    pub lateral_cast_reach: f32,
    pub mantle_forward_radius_multiplier: f32,
    pub mantle_surface_clearance: f32,
    pub mantle_edge_body_offset: f32,
    pub mantle_edge_tolerance: f32,
    pub climb_wall_angle_max_deg: f32,
    pub continue_climb_angle_max_deg: f32,
}

impl LedgeSensing {
    pub const PLAYER: Self = Self {
        height_samples: [-0.8, -0.6, -0.2, 0.2, 0.4, 0.6],
        sphere_radius: 0.1,
        wall_detection_reach: 0.65,
        down_cast_margin: 0.1,
        forward_sample_offset: 1.0,
        vault_distance_margin: 0.2,
        steep_face_normal_y_max: 0.75,
        vault_forward_radius_multiplier: 1.5,
        vault_detection_range: 1.4,
        vault_min_height: 0.3,
        vault_surface_clearance: 0.08,
        mantle_max_height: 2.5,
        lateral_cast_reach: 1.5,
        mantle_forward_radius_multiplier: 2.0,
        mantle_surface_clearance: 0.08,
        mantle_edge_body_offset: 0.33,
        mantle_edge_tolerance: 0.05,
        climb_wall_angle_max_deg: 30.0,
        continue_climb_angle_max_deg: 45.0,
    };
}

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
