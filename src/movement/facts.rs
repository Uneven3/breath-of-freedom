//! Fact components — read-only sensor outputs published by Services.
//!
//! Services write these components; motors only read them (Constitution §5).

use bevy::prelude::*;

/// Wall-contact snapshot captured by the active motor's `move_and_slide` call
/// this frame. Read by `snap_to_ground`'s riser guard and by the wall-jump /
/// edge-leap launch-normal fallbacks. (Grounded state comes from
/// `GroundService`'s downward probe, not from here.)
#[derive(Component, Debug, Clone, Default)]
pub struct BodyContact {
    pub on_wall: bool,
    pub wall_normal: Vec3,
}

/// Published by `GroundService` (slope-filtered grounded state).
///
/// The three diagnostic fields decompose `grounded` so the debug HUD/logs can
/// say *which* condition dropped it: `grounded = probe_hit && slope_ok &&
/// ascend_dot <= ASCEND_EPSILON`.
#[derive(Component, Debug, Clone, Default)]
pub struct GroundFacts {
    pub grounded: bool,
    pub floor_normal: Vec3,
    /// The downward probe hit *something* within range.
    pub probe_hit: bool,
    /// The hit normal was within the 60° slope limit.
    pub slope_ok: bool,
    /// `velocity · floor_normal` — how fast the body moves *away from* the
    /// surface (only meaningful when `probe_hit && slope_ok`).
    pub ascend_dot: f32,
}

/// Published by `LedgeService` — wall/ledge sensor state (`can_climb`,
/// `can_continue_climb`, wall normals, mantle/vault targets).
///
/// Positional facts are `Option<Vec3>`, not a `Vec3::ZERO` sentinel: `None`
/// can't be confused with a legitimate point at the origin, and consumers are
/// forced to handle the missing case.
#[derive(Component, Debug, Clone, Default)]
pub struct LedgeFacts {
    pub can_climb: bool,
    pub can_continue_climb: bool,
    pub climb_normal: Option<Vec3>,
    pub has_wall_left: bool,
    pub has_wall_right: bool,
    pub has_head_hit: bool,
    /// Wall contact point at waist height.
    pub wall_point: Option<Vec3>,

    pub lip_height: f32,

    pub is_at_mantle_edge: bool,
    pub mantle_ledge_point: Option<Vec3>,
    pub mantle_target_position: Option<Vec3>,

    pub is_vaultable: bool,
    pub vault_target_position: Option<Vec3>,
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
    pub anchor_xz: Vec2,
}
