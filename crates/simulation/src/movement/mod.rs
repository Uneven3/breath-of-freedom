//! Headless movement infrastructure shared by every locomotion motor.

use bevy_app::{App, FixedUpdate, Plugin, PreUpdate};
use bevy_ecs::prelude::*;
use bevy_time::{Fixed, Time};

pub mod attachment;
mod attachment_recovery;
pub mod attachment_systems;
pub mod constraints;
pub mod control;
pub mod diag;
pub mod link;
pub mod lod;

pub mod intents {
    pub use bof_domain::movement::intents::*;
}

pub use bof_domain::movement::{Actor, ActorId, BodyVelocity, Player};

/// Ordered phases of the movement broker within `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MovementSet {
    ApplyExternal,
    ReadIntents,
    ControlRedirect,
    SenseWorld,
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
    SyncAttachments,
}

/// Installs scheduling and services that are independent from concrete motors.
pub struct MovementInfrastructurePlugin;

impl Plugin for MovementInfrastructurePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        app.init_resource::<diag::CastTrace>();
        app.init_resource::<lod::SensingLodConfig>();
        app.add_message::<constraints::LocomotionConstraintMessage>();
        app.add_message::<constraints::BodyImpulseMessage>();
        app.add_message::<link::ActorLinkRequestMessage>();
        app.add_message::<link::ActorLinkResultMessage>();
        app.init_resource::<link::ActorLinkWorkspace>();

        app.configure_sets(
            FixedUpdate,
            (
                MovementSet::ApplyExternal,
                MovementSet::ReadIntents,
                MovementSet::ControlRedirect,
                MovementSet::SenseWorld,
                MovementSet::GatherProposals,
                MovementSet::Arbitrate,
                MovementSet::TickActiveMotor,
                MovementSet::SyncAttachments,
            )
                .chain(),
        );

        app.add_systems(PreUpdate, attachment_systems::prepare_actor_link_workspace);
        app.add_systems(
            FixedUpdate,
            (
                attachment_systems::apply_actor_link_requests,
                attachment_systems::recover_orphaned_attachments,
                attachment_systems::recover_pending_safe_poses,
            )
                .chain()
                .in_set(MovementSet::ApplyExternal),
        );
        app.add_systems(
            FixedUpdate,
            attachment_systems::redirect_controls.in_set(MovementSet::ControlRedirect),
        );
        app.add_systems(
            FixedUpdate,
            (diag::clear_cast_trace, lod::assign_sensing_lod)
                .after(MovementSet::ReadIntents)
                .before(MovementSet::SenseWorld),
        );
        app.add_systems(
            FixedUpdate,
            (
                constraints::apply_locomotion_constraints,
                constraints::apply_body_impulses,
            )
                .after(MovementSet::SenseWorld)
                .before(MovementSet::GatherProposals),
        );
        app.add_systems(
            FixedUpdate,
            attachment_systems::sync_attachments.in_set(MovementSet::SyncAttachments),
        );
    }
}

#[cfg(test)]
mod control_tests;
