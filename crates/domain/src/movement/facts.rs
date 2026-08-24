//! Fact components — read-only sensor outputs published by Services.
//!
//! Services write these components; motors only read them (Constitution §5).

use bevy_ecs::prelude::*;
use bevy_math::{Vec2, Vec3};

use crate::asset_pipeline::schema::SurfaceKind;

/// Wall contact from `move_and_slide`; grounding comes from the downward probe.
#[derive(Component, Debug, Clone, Default)]
pub struct BodyContact {
    pub on_wall: bool,
    pub wall_normal: Vec3,
}

/// Slope-filtered grounding plus the facts that explain its decision.
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
    /// Distance the probe travelled before touching floor; zero while resting
    /// on it. Passing `probe_distance` is what turns `probe_hit` off.
    pub floor_gap: f32,
    /// The hit normal **before** the slope filter. `floor_normal` falls back to
    /// `Vec3::Y` on rejection, which reads as flat ground to anyone who does not
    /// also check `slope_ok`.
    pub probe_normal: Vec3,
    /// The surface under the probe, read off the hit entity’s `world::Surface`.
    /// Defaults to `Grass` when the hit carries none.
    /// Presentation-only consumer today (`sfx` footsteps); simulation records
    /// it but never branches on it (§20).
    pub surface: SurfaceKind,
}

impl GroundFacts {
    /// On a surface too steep to stand on: not floor, not air. The `Slide`
    /// motor owns this case; treating it as airborne applies free fall to a
    /// body that is resting on a slope.
    pub fn on_steep_ground(&self) -> bool {
        self.probe_hit && !self.slope_ok
    }

    /// Which way gravity pulls along the touched face; zero when it is level.
    pub fn downhill(&self) -> Vec3 {
        let n = self.probe_normal;
        (Vec3::NEG_Y - n * Vec3::NEG_Y.dot(n)).normalize_or_zero()
    }
}

/// Wall/ledge sensor state. Optional positions distinguish absence from origin.
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

    /// Wall hits by height sample, low bit = lowest; bit 6 = only the face probe.
    pub climb_cast_hits: u8,
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
    /// Horizontal direction from base to top, the axis steps advance along.
    pub fn axis(&self) -> Vec3 {
        let d = self.top - self.base;
        Vec3::new(d.x, 0.0, d.z).normalize_or_zero()
    }

    /// Height of the step tread under `world_pos`, clamped to the flight's ends.
    ///
    /// Derived from the facts alone, so it lives with the data: the stairs motor
    /// snaps the body to it, and presentation draws where the motor believes the
    /// feet should be without reaching into simulation (§20).
    pub fn expected_feet_y(&self, world_pos: Vec3) -> f32 {
        let distance = (world_pos - self.base).dot(self.axis());
        if distance <= 0.0 {
            return self.base.y;
        }
        let total_run = self.step_count as f32 * self.step_depth;
        if distance >= total_run {
            return self.base.y + self.step_count as f32 * self.step_rise;
        }
        let index = (distance / self.step_depth).floor();
        self.base.y + (index + 1.0) * self.step_rise
    }
}

/// Published by `LadderService`.
#[derive(Component, Debug, Clone, Default)]
pub struct LadderFacts {
    pub on_ladder: bool,
    pub bottom_y: f32,
    pub top_y: f32,
    pub body_anchor_xz: Vec2,
    pub outward_normal: Vec3,
}
