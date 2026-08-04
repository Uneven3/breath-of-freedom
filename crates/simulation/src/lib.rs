//! Authoritative, headless simulation for Breath of Freedom.
//!
//! Gameplay systems move here incrementally. This crate can depend on physics
//! but never on rendering or presentation (§20).

#![cfg_attr(
    test,
    allow(
        clippy::expect_used,
        clippy::float_cmp,
        clippy::unwrap_used,
        reason = "tests may panic when a required simulation invariant is broken"
    )
)]

use avian3d::prelude::PhysicsPlugins;
use bevy_app::{App, Plugin};

pub mod health;
pub mod interaction;
pub mod inventory;
pub mod physics;
pub mod projectiles;
pub mod time_control;

/// Installs the authoritative physics and, progressively, gameplay systems.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default());
    }
}

#[cfg(test)]
mod tests {
    use core::time::Duration;

    use avian3d::prelude::{Collider, LinearVelocity, RigidBody};
    use bevy_app::{App, TaskPoolPlugin};
    use bevy_time::{TimePlugin, TimeUpdateStrategy};
    use bevy_transform::{TransformPlugin, components::Transform};

    use super::SimulationPlugin;

    #[test]
    fn physics_advances_without_window_or_renderer() {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            TransformPlugin,
            SimulationPlugin,
        ))
        .insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f64(
            1.0 / 60.0,
        )));

        let body = app
            .world_mut()
            .spawn((
                RigidBody::Dynamic,
                Collider::sphere(0.5),
                Transform::from_xyz(0.0, 2.0, 0.0),
            ))
            .id();

        app.finish();
        for _ in 0..4 {
            app.update();
        }

        let velocity = app
            .world()
            .get::<LinearVelocity>(body)
            .expect("Avian must initialize velocity for a dynamic body");
        assert!(velocity.y < 0.0, "gravity must advance in the headless app");
    }
}
