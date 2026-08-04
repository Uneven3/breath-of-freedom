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
#[cfg(test)]
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

pub use bof_domain::movement::{Actor, ActorId, BodyVelocity, GRAVITY, Player};
pub use bof_simulation::movement::MovementSet;

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(bof_simulation::movement::MovementInfrastructurePlugin);

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
        // After the motors move the body, before attachments follow it: an actor
        // that ended up inside the terrain is lifted back onto the surface. The
        // downward probe cannot catch this — it finds ground right there and
        // reports the body comfortably grounded while it sits under the floor.
        app.add_systems(
            FixedUpdate,
            services::ground::lift_actors_out_of_terrain
                .after(MovementSet::TickActiveMotor)
                .before(MovementSet::SyncAttachments),
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
mod determinism_tests;
