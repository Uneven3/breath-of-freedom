use bevy::prelude::*;

pub mod charge;
pub mod charge_data;
pub mod control;
pub mod data;
pub mod debug;
mod dismount;
pub mod lifecycle;
mod recovery;
mod transition;

use crate::combat::CombatSet;
use crate::health::HealthSet;
use crate::movement::MovementSet;
use crate::projectiles::ProjectilesSet;
use data::{MountDebugRequest, MountTransitionRequest};

#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MountsSet {
    Request,
    Lifecycle,
    Confirm,
    PostMove,
    Charge,
    DeathCleanup,
}

pub struct MountsPlugin;

impl Plugin for MountsPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<MountDebugRequest>();
        app.add_message::<MountTransitionRequest>();
        app.init_resource::<charge_data::ChargeHitLedger>();
        app.init_resource::<charge_data::ChargeShape>();
        app.add_systems(Update, debug::capture_toggle_request);
        app.configure_sets(
            FixedUpdate,
            (MountsSet::Request, MountsSet::Lifecycle)
                .chain()
                .before(MovementSet::ApplyExternal),
        );
        app.configure_sets(
            FixedUpdate,
            MountsSet::Confirm
                .after(MovementSet::ApplyExternal)
                .before(MovementSet::ReadIntents),
        );
        app.configure_sets(
            FixedUpdate,
            MountsSet::PostMove
                .after(MovementSet::SyncAttachments)
                .before(CombatSet::ApplyContext),
        );
        app.configure_sets(FixedUpdate, MountsSet::DeathCleanup.after(HealthSet::Apply));
        app.configure_sets(
            FixedUpdate,
            MountsSet::Charge
                .after(ProjectilesSet::Simulate)
                .before(HealthSet::Apply),
        );
        // The arbiter owns `Interact` and must resolve before any domain reads
        // its decision; without this the request would land a tick late.
        app.configure_sets(
            FixedUpdate,
            MountsSet::Request.after(crate::interaction::InteractionSet::Arbitrate),
        );
        app.add_systems(
            FixedUpdate,
            (
                debug::process_toggle_requests,
                lifecycle::read_interact_requests,
                lifecycle::reconcile_relationships,
            )
                .chain()
                .in_set(MountsSet::Request),
        );
        // Availability and priority are declared to the arbiter as components,
        // so it never needs to know what makes a horse mountable.
        app.add_systems(
            FixedUpdate,
            (
                lifecycle::sync_horse_interactable,
                lifecycle::sync_rider_override,
            )
                .before(crate::interaction::InteractionSet::Arbitrate),
        );
        app.add_systems(
            FixedUpdate,
            lifecycle::apply_transitions.in_set(MountsSet::Lifecycle),
        );
        app.add_systems(
            FixedUpdate,
            (
                lifecycle::confirm_transitions,
                lifecycle::recover_detached_riders,
            )
                .chain()
                .in_set(MountsSet::Confirm),
        );
        app.add_systems(
            FixedUpdate,
            lifecycle::despawn_released_horses.in_set(MountsSet::PostMove),
        );
        app.add_systems(
            FixedUpdate,
            lifecycle::collect_horse_deaths.in_set(MountsSet::DeathCleanup),
        );
        app.add_systems(
            FixedUpdate,
            (charge::prune_hit_ledger, charge::detect_charge_hits)
                .chain()
                .in_set(MountsSet::Charge),
        );
    }
}

#[cfg(test)]
mod plugin_tests {
    use super::*;
    use avian3d::prelude::*;
    use bevy::time::TimeUpdateStrategy;
    use std::time::Duration;

    use crate::combat::CombatPlugin;
    use crate::combat::context_data::CombatContext;
    use crate::enemies::perception::DirectThreatMessage;
    use crate::health::{DamageRequestMessage, DeathMessage};
    use crate::input::frame::ActiveActions;
    use crate::mounts::data::{MountDebugRequest, MountTransitionRequest, MountedOn};
    use crate::mounts::lifecycle::spawn_horse_bundle;
    use crate::movement::abilities::{
        AirborneMovement, GroundMovement, JumpMovement, JumpStaminaCost, SprintMovement,
        StairsMovement,
    };
    use crate::movement::attachment::{LocomotionEnabled, PendingSafeRecovery};
    use crate::movement::body::BodyDimensions;
    use crate::movement::bundles::{
        GroundMovementBundle, JumpMovementBundle, KinematicActorBundle, SprintMovementBundle,
        StairsMovementBundle, StaminaBundle,
    };
    use crate::movement::control::ControlRedirect;
    use crate::movement::intents::{Intents, JumpIntent};
    use crate::movement::motors::jump::JumpLocal;
    use crate::movement::sensing::GroundSensing;
    use crate::movement::state::LocomotionState;
    use crate::movement::{BodyVelocity, MovementPlugin, Player};
    use crate::projectiles::SpawnProjectileMessage;
    use crate::world::GameLayer;

    #[derive(Clone, Copy)]
    enum PluginOrder {
        MovementFirst,
        MountsFirst,
    }

    fn real_app(order: PluginOrder) -> App {
        let mut app = App::new();
        app.add_plugins((
            MinimalPlugins,
            TransformPlugin,
            PhysicsPlugins::default(),
            bevy::asset::AssetPlugin::default(),
            bevy::mesh::MeshPlugin,
        ));
        app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::from_secs_f32(
            1.0 / 60.0,
        )));
        app.init_resource::<ButtonInput<KeyCode>>();
        app.init_resource::<ActiveActions>();
        // `InteractionRequest` is the arbiter's contract; `InteractionPlugin`
        // is not part of this app, so the message is registered by hand.
        app.add_message::<crate::interaction::InteractionRequest>();
        app.add_message::<DirectThreatMessage>();
        app.add_message::<DamageRequestMessage>();
        app.add_message::<DeathMessage>();
        app.add_message::<SpawnProjectileMessage>();
        match order {
            PluginOrder::MovementFirst => {
                app.add_plugins((MovementPlugin, MountsPlugin, CombatPlugin));
            }
            PluginOrder::MountsFirst => {
                app.add_plugins((MountsPlugin, CombatPlugin, MovementPlugin));
            }
        }
        app.finish();
        app
    }

    fn spawn_real_rider(app: &mut App, transform: Transform) -> Entity {
        app.world_mut()
            .spawn((
                Player,
                KinematicActorBundle::new(transform, BodyDimensions::PLAYER, GroundSensing::PLAYER),
                CombatContext::default(),
            ))
            .id()
    }

    fn mount_result(order: PluginOrder) -> (Vec3, Vec3, bool) {
        let mut app = real_app(order);
        let rider = spawn_real_rider(&mut app, Transform::IDENTITY);
        let horse = app.world_mut().spawn(spawn_horse_bundle()).id();
        app.update();
        app.world_mut()
            .write_message(MountTransitionRequest::Mount { rider, horse });
        app.update();
        let rider_entity = app.world().entity(rider);
        let horse_translation = app
            .world()
            .entity(horse)
            .get::<Transform>()
            .unwrap()
            .translation;
        (
            rider_entity.get::<Transform>().unwrap().translation,
            horse_translation,
            rider_entity.get::<CombatContext>().unwrap().is_mounted(),
        )
    }

    #[test]
    fn real_plugins_mount_with_same_tick_saddle_and_context_in_any_order() {
        let movement_first = mount_result(PluginOrder::MovementFirst);
        let mounts_first = mount_result(PluginOrder::MountsFirst);
        assert_eq!(movement_first, mounts_first);
        assert!(movement_first.2);
        assert_eq!(
            movement_first.0,
            movement_first.1 + Vec3::new(0.0, 1.65, 0.0)
        );
    }

    #[test]
    fn production_tick_registration_runs_only_the_active_capability() {
        let mut app = real_app(PluginOrder::MovementFirst);
        app.update();
        let actor = app
            .world_mut()
            .spawn((
                KinematicActorBundle::new(
                    Transform::from_xyz(0.0, 20.0, 0.0),
                    BodyDimensions::PLAYER,
                    GroundSensing::PLAYER,
                ),
                GroundMovementBundle::new(GroundMovement::PLAYER),
                SprintMovementBundle::new(SprintMovement::PLAYER),
                StairsMovementBundle::new(StairsMovement::PLAYER),
                JumpMovementBundle::new(JumpMovement::PLAYER),
                StaminaBundle::default(),
                JumpStaminaCost(20.0),
                AirborneMovement::PLAYER,
            ))
            .id();
        app.world_mut().entity_mut(actor).insert(Intents {
            jump: JumpIntent {
                held: true,
                pressed: true,
            },
            ..default()
        });
        app.world_mut()
            .entity_mut(actor)
            .get_mut::<JumpLocal>()
            .unwrap()
            .coyote = 1.0;

        app.update();

        let entity = app.world().entity(actor);
        assert_eq!(
            *entity.get::<LocomotionState>().unwrap(),
            LocomotionState::Jump
        );
        assert!(
            (entity.get::<BodyVelocity>().unwrap().0.y - JumpMovement::PLAYER.impulse).abs()
                < 0.001
        );
    }

    #[test]
    fn blocked_forced_dismount_releases_relationship_but_keeps_collision_disabled_until_safe() {
        let mut app = real_app(PluginOrder::MovementFirst);
        let rider = spawn_real_rider(&mut app, Transform::IDENTITY);
        let horse = app.world_mut().spawn(spawn_horse_bundle()).id();
        app.update();
        app.world_mut()
            .write_message(MountTransitionRequest::Mount { rider, horse });
        app.update();
        let horse_position = app
            .world()
            .entity(horse)
            .get::<Transform>()
            .unwrap()
            .translation;
        for height in [3.0, 5.0, 8.0, 10.0, 12.0, 14.0, 16.0] {
            app.world_mut().spawn((
                Transform::from_translation(horse_position + Vec3::Y * height),
                RigidBody::Static,
                Collider::sphere(1.5),
                CollisionLayers::new(GameLayer::Default, LayerMask::ALL),
            ));
        }
        app.update();
        app.update();
        app.world_mut()
            .write_message(MountDebugRequest::ToggleHorse);

        app.update();

        assert!(app.world().get_entity(horse).is_err());
        let rider_entity = app.world().entity(rider);
        assert!(!rider_entity.contains::<MountedOn>());
        assert!(!rider_entity.contains::<ControlRedirect>());
        assert!(
            rider_entity.contains::<ColliderDisabled>(),
            "pending={}, enabled={}, position={:?}",
            rider_entity.contains::<PendingSafeRecovery>(),
            rider_entity.contains::<LocomotionEnabled>(),
            rider_entity.get::<Transform>().unwrap().translation
        );
        assert!(rider_entity.contains::<PendingSafeRecovery>());
        assert!(!rider_entity.contains::<LocomotionEnabled>());

        for _ in 0..8 {
            app.update();
            if !app.world().entity(rider).contains::<PendingSafeRecovery>() {
                break;
            }
        }
        let rider_entity = app.world().entity(rider);
        assert!(!rider_entity.contains::<PendingSafeRecovery>());
        assert!(!rider_entity.contains::<ColliderDisabled>());
        assert!(rider_entity.contains::<LocomotionEnabled>());
    }
}
