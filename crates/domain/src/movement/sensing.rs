use bevy_ecs::prelude::Component;

#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct GroundSensing {
    pub probe_distance: f32,
    pub ascend_epsilon: f32,
    /// Extra dot the floor must lose before the body is declared off it. Zero
    /// is a single threshold, which chatters whenever the terrain hovers around
    /// the walkable limit — measured on the sculpted canyon, 210 Slide runs of
    /// 1 to 3 ticks in one session. Widening the band costs nothing on ground
    /// that is clearly one thing or the other.
    pub slope_hysteresis_dot: f32,
    /// Ticks without valid floor the body tolerates before it counts as
    /// airborne. Covers the probe missing between stair treads or across a
    /// facet seam without every motor needing its own special case.
    pub ground_grace_ticks: u8,
}

impl GroundSensing {
    pub const PLAYER: Self = Self {
        probe_distance: 0.2,
        ascend_epsilon: 0.1,
        slope_hysteresis_dot: 0.0,
        ground_grace_ticks: 0,
    };
}

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
