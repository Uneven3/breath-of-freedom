//! Movement plugin — the Broker pipeline.
//!
//! Per-frame flow, expressed as ordered system sets in `FixedUpdate` (pinned to
//! 60 Hz): read intents → assign sensing LOD → sense world → gather proposals →
//! arbitrate → tick active motor. The tick phase chains capability-specific
//! systems whose exact queries keep optional data out of the actor core; each
//! system gates on its owned `LocomotionState`, so exactly one moves each body.
//! This is the per-entity contract that lets multiple `Actor`s run independently. See
//! `docs/ARCHITECTURE.md`.

use bevy::prelude::*;

pub mod abilities;
pub mod attachment;
mod attachment_recovery;
pub(crate) mod attachment_systems;
pub mod body;
pub mod brain;
pub mod bundles;
pub mod constraints;
pub mod control;
pub mod diag;
pub mod facing;
pub mod facts;
pub mod intents;
pub mod link;
pub mod lod;
pub mod motor_common;
pub mod motors;
pub mod probe;
pub mod probe_data;
pub mod proposal;
pub mod sensing;
pub mod services;
pub mod stamina;
pub mod state;

// SPIKE (throwaway, test-only): multi-actor dispatch proof. See spike.rs header.
#[cfg(test)]
mod spike;

use proposal::ProposalBuffer;
use state::LocomotionState;

/// World gravity magnitude (Earth gravity, 9.8 m/s²).
pub const GRAVITY: f32 = 9.8;

/// Marker for the player entity.
#[derive(Component)]
pub struct Player;

/// Generic marker for any movement-capable entity (local player, remote
/// player, AI-controlled actor). Motors dispatch on `Actor`, not `Player` —
/// `Player` narrows to "the local player" for systems (e.g. the camera) that
/// intentionally stay scoped to it.
#[derive(Component)]
pub struct Actor;

/// Our kinematic body velocity — the analog of `CharacterBody3D.velocity`.
/// Kept separate from Avian's `LinearVelocity`: we integrate position ourselves
/// through `move_and_slide`, so the physics engine must not also move us.
#[derive(Component, Default)]
pub struct BodyVelocity(pub Vec3);

/// Ordered phases of the Broker pipeline within `FixedUpdate`.
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

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // Pinned to 60 Hz (Bevy defaults to 64 Hz).
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        // Sensor-cast capture for the debug gizmos (no-op until enabled).
        app.init_resource::<diag::CastTrace>();
        app.add_systems(
            FixedUpdate,
            diag::clear_cast_trace
                .after(MovementSet::ReadIntents)
                .before(MovementSet::SenseWorld),
        );
        // Sensing LOD: decide, per actor, whether SenseWorld casts this tick.
        app.init_resource::<lod::SensingLodConfig>();
        app.add_systems(
            FixedUpdate,
            lod::assign_sensing_lod
                .after(MovementSet::ReadIntents)
                .before(MovementSet::SenseWorld),
        );
        // Constraints and impulses requested by other systems (Combat),
        // applied right before motors propose/tick.
        app.add_message::<constraints::LocomotionConstraintMessage>();
        app.add_message::<constraints::BodyImpulseMessage>();
        app.add_message::<link::ActorLinkRequestMessage>();
        app.add_message::<link::ActorLinkResultMessage>();
        app.init_resource::<link::ActorLinkWorkspace>();
        app.add_systems(PreUpdate, attachment_systems::prepare_actor_link_workspace);
        app.add_systems(
            FixedUpdate,
            (
                constraints::apply_locomotion_constraints,
                constraints::apply_body_impulses,
            )
                .after(MovementSet::SenseWorld)
                .before(MovementSet::GatherProposals),
        );

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

        app.add_message::<probe_data::ProbeToggleRequest>();
        app.add_systems(Update, probe::toggle_spawn);

        app.add_systems(
            FixedUpdate,
            (probe::drive_intents, brain::read_intents)
                .chain()
                .in_set(MovementSet::ReadIntents),
        );
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
            (
                services::ground::ground_service,
                services::ledge::ledge_service,
                services::stairs::stairs_service,
                services::ladder::ladder_service,
                motors::sneak::update_stand_clearance,
            )
                .in_set(MovementSet::SenseWorld),
        );
        app.add_systems(
            FixedUpdate,
            (
                motors::walk::propose,
                motors::fall::propose,
                motors::sprint::propose,
                motors::sneak::propose,
                motors::jump::propose,
                motors::glide::propose,
                motors::climb::propose,
                motors::mantle::propose,
                motors::auto_vault::propose,
                motors::wall_jump::propose,
                motors::edge_leap::propose,
                motors::stairs::propose,
                motors::ladder::propose,
            )
                .in_set(MovementSet::GatherProposals),
        );
        app.add_systems(FixedUpdate, arbitrate.in_set(MovementSet::Arbitrate));
        app.add_systems(
            FixedUpdate,
            motors::jump::pay_accepted_cost
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );
        // Clear climb intent on the relevant transitions, right after the SSoT
        // write and before any motor ticks on it.
        app.add_systems(
            FixedUpdate,
            brain::reset_climb_toggle
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );

        // Tick phase: exact capability queries chained in state order. Each
        // body has one active state and therefore one moving system.
        app.add_systems(
            FixedUpdate,
            (
                motors::stairs::clear_inactive_cache,
                motors::walk::tick_body,
                motors::sprint::tick_body,
                motors::fall::tick_body,
                motors::jump::tick_body,
                motors::auto_vault::tick_body,
                motors::climb::tick_body,
                motors::mantle::tick_body,
                motors::stairs::tick_body,
                motors::ladder::tick_body,
                motors::glide::tick_body,
                motors::sneak::tick_body,
                motors::wall_jump::tick_body,
                motors::edge_leap::tick_body,
            )
                .chain()
                .in_set(MovementSet::TickActiveMotor),
        );
        app.add_systems(
            FixedUpdate,
            attachment_systems::sync_attachments.in_set(MovementSet::SyncAttachments),
        );
        // Decoupled facing (aim/lock-on) resolves after the active motor has
        // moved the body, before attachments sync to the final transform.
        app.add_systems(
            FixedUpdate,
            facing::resolve_facing
                .after(MovementSet::TickActiveMotor)
                .before(MovementSet::SyncAttachments),
        );

        // Declarative crouch-capsule swap (orthogonal to the active state, so it
        // works in Sneak and on Stairs). Runs in FixedUpdate right after the SSoT
        // write so the active motor ticks with the correct capsule this same frame
        // (physics never sees a stale collider).
        app.add_systems(
            FixedUpdate,
            motors::sneak::sync_crouch_collider
                .after(MovementSet::Arbitrate)
                .before(MovementSet::TickActiveMotor),
        );
    }
}

/// `Arbitrate`: pick the winning proposal, write the SSoT `LocomotionState`, then
/// clear the buffer for next frame. This is the *only* writer of
/// `LocomotionState` (see `docs/ARCHITECTURE.md`).
type ArbitrationQuery<'a> = (&'a mut LocomotionState, &'a mut ProposalBuffer);

fn arbitrate(mut q: Query<ArbitrationQuery, attachment::LocomotionActorFilter>) {
    for (mut state, mut buffer) in &mut q {
        let winner = buffer.arbitrate(*state);
        if *state != winner {
            *state = winner;
            // (Activated/Deactivated events land with the motors that need them.)
        }
        buffer.clear();
    }
}

#[cfg(test)]
mod actor_isolation_tests;
#[cfg(test)]
mod control_tests;
