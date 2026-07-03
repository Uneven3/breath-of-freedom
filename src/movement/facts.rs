//! Fact components — read-only sensor outputs published by Services.
//!
//! Services write these components; motors only read them (Constitution §5).

use bevy::prelude::*;

/// Raw collision result captured by the active motor's `move_and_slide` call this
/// frame. Read next frame by `GroundService`, reflecting the *previous*
/// `move_and_slide`.
#[derive(Component, Debug, Clone, Default)]
pub struct BodyContact {
    pub grounded: bool,
    pub floor_normal: Vec3,
    pub on_wall: bool,
    pub wall_normal: Vec3,
}

/// Published by `GroundService` (slope-filtered grounded state).
#[derive(Component, Debug, Clone, Default)]
pub struct GroundFacts {
    pub grounded: bool,
    pub floor_normal: Vec3,
}

/// Published by `LedgeService` — wall/ledge sensor state (`can_climb`,
/// `can_continue_climb`, wall normals, mantle/vault targets).
#[derive(Component, Debug, Clone, Default)]
pub struct LedgeFacts {
    pub can_climb: bool,
    pub can_continue_climb: bool,
    pub climb_normal: Vec3,
    pub has_wall_left: bool,
    pub has_wall_right: bool,
    pub has_head_hit: bool,
    /// Wall contact point at waist height.
    pub wall_point: Vec3,

    pub lip_height: f32,
    pub landing_height: f32,
    pub is_occupied: bool,

    pub is_at_mantle_edge: bool,
    pub mantle_ledge_point: Vec3,
    pub mantle_target_position: Vec3,

    pub is_vaultable: bool,
    pub vault_target_position: Vec3,
}

/// Published by `StairsService`. Carries a copy of the active stair's geometry so
/// motors don't need to chase an entity ref.
#[derive(Component, Debug, Clone, Default)]
pub struct StairsFacts {
    pub on_stairs: bool,
    pub base: Vec3,
    pub top: Vec3,
    pub step_count: i32,
    pub step_depth: f32,
    pub step_rise: f32,
}

impl StairsFacts {
    /// Unit vector along the stairs in the horizontal plane.
    pub fn slope_axis(&self) -> Vec3 {
        let d = self.top - self.base;
        Vec3::new(d.x, 0.0, d.z).normalize_or_zero()
    }

    /// Expected feet-Y for a body at `world_pos`, by how far it has progressed along
    /// the stair axis.
    pub fn expected_feet_y(&self, world_pos: Vec3) -> f32 {
        let horiz = self.slope_axis();
        let d = (world_pos - self.base).dot(horiz);
        if d <= 0.0 {
            return self.base.y;
        }
        let total_run = self.step_count as f32 * self.step_depth;
        if d >= total_run {
            return self.base.y + self.step_count as f32 * self.step_rise;
        }
        let idx = (d / self.step_depth).floor() as i32;
        self.base.y + (idx + 1) as f32 * self.step_rise
    }
}

/// Published by `LadderService`.
#[derive(Component, Debug, Clone, Default)]
pub struct LadderFacts {
    pub on_ladder: bool,
    pub top_y: f32,
    pub bottom_y: f32,
    pub anchor_xz: Vec2,
}
