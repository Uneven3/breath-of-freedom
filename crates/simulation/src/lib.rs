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
use bevy_app::{App, FixedUpdate, Plugin};
use bevy_ecs::prelude::*;

pub mod combat;
pub mod enemies;
pub mod health;
pub mod interaction;
pub mod inventory;
pub mod mounts;
pub mod movement;
pub mod physics;
pub mod player;
pub mod projectiles;
pub mod time_control;
pub mod world;

/// Installs the authoritative simulation: physics, every gameplay domain, and
/// the ordering between them.
///
/// The order below is the part that used to live in the app's `main`, where it
/// read as five unexplained `configure_sets` calls. It belongs here: only this
/// crate knows why constraints must be emitted before projectiles advance and
/// both before damage lands. What stays composition — which scene builds what,
/// and when — is still the app's.
pub struct SimulationPlugin;

impl Plugin for SimulationPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(PhysicsPlugins::default());
        // The intent frame has to exist even when nobody fills it: headless
        // runs have no input layer, and a missing resource is a panic, not an
        // idle actor. The app's `input` is the only writer.
        app.init_resource::<bof_domain::input::frame::ActiveActions>();

        app.add_plugins((
            world::WorldPlugin,
            movement::MovementPlugin,
            mounts::MountsPlugin,
            combat::CombatPlugin,
            projectiles::ProjectilesPlugin,
            health::HealthPlugin,
            inventory::InventoryPlugin,
            enemies::EnemiesPlugin,
            player::PlayerPlugin,
            interaction::InteractionPlugin,
            time_control::TimeControlPlugin,
        ));

        // Damage is resolved once, at the end: a constraint is emitted, the
        // projectile it may have launched advances, and only then health
        // applies what landed.
        app.configure_sets(
            FixedUpdate,
            (
                combat::CombatSet::EmitConstraints,
                projectiles::ProjectilesSet::Simulate,
                health::HealthSet::Apply,
            )
                .chain(),
        );
        // The inventory touches three domains, so it declares itself against
        // each: it collects after bodies have moved, consumes before combat
        // reads the loadout, and spends durability after combat committed.
        app.configure_sets(
            FixedUpdate,
            inventory::InventorySet::Collect.after(movement::MovementSet::SyncAttachments),
        );
        app.configure_sets(
            FixedUpdate,
            inventory::InventorySet::Consume.before(combat::CombatSet::ApplyContext),
        );
        app.configure_sets(
            FixedUpdate,
            inventory::InventorySet::Durability.after(combat::CombatSet::EmitConstraints),
        );
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

#[cfg(test)]
mod scheduling_audit {
    use bevy_app::{App, FixedUpdate, TaskPoolPlugin};
    use bevy_ecs::prelude::*;
    use bevy_ecs::schedule::Schedules;
    use bevy_time::TimePlugin;
    use bevy_transform::TransformPlugin;

    use super::SimulationPlugin;

    fn simulation_app() -> App {
        let mut app = App::new();
        app.add_plugins((
            TaskPoolPlugin::default(),
            TimePlugin,
            TransformPlugin,
            SimulationPlugin,
        ));
        app
    }

    /// Pares de sistemas que pueden tocar el mismo dato y no tienen orden entre
    /// ellos, así que cuál corre primero lo decide el ejecutor.
    fn ambiguities(app: &mut App) -> usize {
        app.finish();
        // Se saca del mundo en vez de tomarlo prestado: `initialize` inserta
        // recursos, y hacerlo dentro de un `resource_scope` es un panic.
        let mut schedule = app
            .world_mut()
            .resource_mut::<Schedules>()
            .remove(FixedUpdate)
            .expect("FixedUpdate está registrado");
        let _ = schedule.initialize(app.world_mut());
        schedule.graph().conflicting_systems().len()
    }

    #[derive(Component, Default)]
    struct Canary(u32);

    fn bump_one(mut canaries: Query<&mut Canary>) {
        for mut canary in &mut canaries {
            canary.0 += 1;
        }
    }

    fn bump_two(mut canaries: Query<&mut Canary>) {
        for mut canary in &mut canaries {
            canary.0 += 2;
        }
    }

    /// El canario, y tiene que ser **diferencial**: no alcanza con pedir que
    /// el detector reporte algo, porque el proyecto ya reporta de por sí.
    /// Plantar dos escritores sin orden tiene que hacer *subir* la cuenta.
    #[test]
    fn the_detector_catches_a_planted_ambiguity() {
        let baseline = ambiguities(&mut simulation_app());
        let mut planted = simulation_app();
        planted.add_systems(FixedUpdate, (bump_one, bump_two));
        let with_canary = ambiguities(&mut planted);
        assert!(
            with_canary > baseline,
            "plantar dos escritores sin orden no movió la cuenta \
             ({baseline} → {with_canary}): el detector no está midiendo nada"
        );
    }

    /// Pares ambiguos en `FixedUpdate` al 2026-08-04. **Sólo puede bajar.**
    ///
    /// Un par ambiguo es dos sistemas que pueden tocar el mismo dato y no
    /// tienen orden entre ellos: cuál corre primero lo decide el ejecutor, y
    /// con el pool multihilo puede cambiar entre corridas. Muchos son
    /// inofensivos —tocan el mismo componente sobre entidades distintas— pero
    /// **el detector no puede saber cuáles**, y el determinismo es el pilar que
    /// sostiene el co-op (`NORTE.md`).
    ///
    /// Ninguno es de Avian: sus sistemas viven en `FixedPostUpdate` y en su
    /// propio schedule. Los 117 son de gameplay.
    ///
    /// Bajarlo es declarar un orden (`.before`/`.after`) o separar en fases.
    /// Subirlo es agregar una carrera nueva, y eso se discute, no se cuela.
    const FIXED_UPDATE_AMBIGUITIES: usize = 117;

    #[test]
    fn scheduling_ambiguities_only_shrink() {
        let found = ambiguities(&mut simulation_app());
        assert!(
            found <= FIXED_UPDATE_AMBIGUITIES,
            "las ambigüedades de `FixedUpdate` subieron de \
             {FIXED_UPDATE_AMBIGUITIES} a {found}: dos sistemas nuevos pueden \
             tocar el mismo dato sin orden entre ellos"
        );
        assert!(
            found == FIXED_UPDATE_AMBIGUITIES,
            "bajaron de {FIXED_UPDATE_AMBIGUITIES} a {found} — actualizá la \
             constante para que el logro no se pueda perder"
        );
    }
}
