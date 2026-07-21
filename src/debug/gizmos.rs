//! Sensor visualisation: draws the casts the services actually issued this
//! fixed tick, the trigger volumes, and per-actor state arrows. Read-only over
//! gameplay state — it renders what sensing already decided, never re-queries
//! the world itself, so what you see is what the simulation saw.

use bevy::color::palettes::css;
use bevy::prelude::*;

use super::DebugConfig;
use crate::movement::diag::CastTrace;
use crate::movement::facts::{BodyContact, GroundFacts, StairsFacts};
use crate::movement::{Actor, BodyVelocity};
use crate::world::{Ladder, Stairs};

/// Draw the sensor casts recorded by the services this fixed tick, the
/// stairs/ladder trigger volumes, and per-actor state arrows.
#[allow(clippy::type_complexity)]
pub(super) fn draw_sensor_gizmos(
    config: Res<DebugConfig>,
    trace: Res<CastTrace>,
    mut gizmos: Gizmos,
    actors: Query<
        (
            &Transform,
            &BodyVelocity,
            &GroundFacts,
            &BodyContact,
            &crate::movement::body::BodyDimensions,
            Option<&StairsFacts>,
        ),
        With<Actor>,
    >,
    stairs: Query<&Stairs>,
    ladders: Query<&Ladder>,
) {
    if !config.show_casts {
        return;
    }

    // --- Recorded sensor casts: line to max range, sphere + normal on hit ---
    for rec in &trace.records {
        let color = cast_color(rec.label);
        let end = rec.origin + rec.dir * rec.max_dist;
        match rec.hit {
            Some((point, normal)) => {
                gizmos.line(rec.origin, point, color);
                // Remaining (unreached) range, dimmed.
                gizmos.line(point, end, color.with_alpha(0.15));
                gizmos.sphere(point, 0.05, color);
                gizmos.arrow(point, point + normal * 0.35, css::HOT_PINK);
            }
            None => gizmos.line(rec.origin, end, color.with_alpha(0.35)),
        }
    }

    // --- Trigger volumes (AABB overlap regions, not physics colliders) ---
    for s in &stairs {
        gizmos.cube(
            Transform::from_translation(s.trigger_center).with_scale(s.trigger_half_extents * 2.0),
            css::DODGER_BLUE,
        );
        // Authored slope line base → top.
        gizmos.line(s.base, s.top, css::DODGER_BLUE);
    }
    for l in &ladders {
        gizmos.cube(
            Transform::from_translation(l.trigger_center).with_scale(l.trigger_half_extents * 2.0),
            css::MEDIUM_PURPLE,
        );
        gizmos.line(l.bottom, l.top, css::MEDIUM_PURPLE);
    }

    // --- Per-actor arrows ---
    for (transform, vel, ground, contact, body, stairs_facts) in &actors {
        let pos = transform.translation;
        // Velocity (scaled down to stay readable).
        if vel.0.length_squared() > 0.001 {
            gizmos.arrow(pos, pos + vel.0 * 0.25, css::GOLD);
        }
        // Floor normal from the feet, green when grounded, red when not.
        let feet = pos - Vec3::Y * body.standing_half_height();
        let n_color = if ground.grounded { css::LIME } else { css::RED };
        gizmos.arrow(feet, feet + ground.floor_normal * 0.6, n_color);
        // Wall contact normal.
        if contact.on_wall {
            gizmos.arrow(pos, pos + contact.wall_normal * 0.6, css::ORANGE_RED);
        }
        // Where the stairs motor believes the feet should be.
        if let Some(sf) = stairs_facts
            && sf.on_stairs
        {
            let mut expected = pos;
            expected.y = crate::movement::motors::stairs::expected_feet_y(sf, pos);
            gizmos.sphere(expected, 0.08, css::DODGER_BLUE);
        }
    }
}

/// Stable color per sensor family so casts are tellable-apart at a glance.
fn cast_color(label: &str) -> Color {
    match label {
        "ground_probe" => css::LIME.into(),
        "mantle_down" => css::AQUA.into(),
        "vault_down" => css::ORANGE.into(),
        "wall_ray_left" | "wall_ray_right" => css::YELLOW.into(),
        // The 6 forward profiling casts.
        _ => css::WHITE.into(),
    }
}
