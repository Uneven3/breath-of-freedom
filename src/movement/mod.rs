//! Movement plugin — the Broker pipeline.
//!
//! Per-frame flow, expressed as ordered system sets in `FixedUpdate` (pinned to
//! 60 Hz): read intents → sense world → gather proposals → arbitrate → tick
//! active motor. Only the active motor's `tick` system runs each frame, gated
//! by a run condition on `LocomotionState` — this is what enforces "exactly
//! one motor moves the body" structurally, not just by convention. See
//! `docs/architecture/movement.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

pub mod brain;
pub mod facts;
pub mod intents;
pub mod motor_common;
pub mod motors;
pub mod proposal;
pub mod services;
pub mod stamina;
pub mod state;

// SPIKE (throwaway, test-only): multi-actor dispatch proof. See spike.rs header.
#[cfg(test)]
mod spike;

use brain::ClimbToggle;
use facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use intents::Intents;
use proposal::ProposalBuffer;
use stamina::Stamina;
use state::LocomotionState;

/// World gravity magnitude (Earth gravity, 9.8 m/s²).
pub const GRAVITY: f32 = 9.8;

/// Marker for the player entity.
#[derive(Component)]
pub struct Player;

/// Our kinematic body velocity — the analog of `CharacterBody3D.velocity`.
/// Kept separate from Avian's `LinearVelocity`: we integrate position ourselves
/// through `move_and_slide`, so the physics engine must not also move us.
#[derive(Component, Default)]
pub struct BodyVelocity(pub Vec3);

/// Ordered phases of the Broker pipeline within `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum MovementSet {
    ReadIntents,
    SenseWorld,
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
}

pub struct MovementPlugin;

impl Plugin for MovementPlugin {
    fn build(&self, app: &mut App) {
        // Pinned to 60 Hz (Bevy defaults to 64 Hz).
        app.insert_resource(Time::<Fixed>::from_hz(60.0));
        app.init_resource::<ClimbToggle>();

        app.configure_sets(
            FixedUpdate,
            (
                MovementSet::ReadIntents,
                MovementSet::SenseWorld,
                MovementSet::GatherProposals,
                MovementSet::Arbitrate,
                MovementSet::TickActiveMotor,
            )
                .chain(),
        );

        app.add_systems(Startup, spawn_player);

        app.add_systems(
            FixedUpdate,
            brain::read_intents.in_set(MovementSet::ReadIntents),
        );
        app.add_systems(
            FixedUpdate,
            (
                services::ground::ground_service,
                services::ledge::ledge_service,
                services::stairs::stairs_service,
                services::ladder::ladder_service,
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
        // Clear climb intent on the relevant transitions, right after the SSoT write.
        app.add_systems(
            FixedUpdate,
            brain::reset_climb_toggle.after(MovementSet::Arbitrate),
        );

        // Tick systems: exactly one runs, selected by the active state.
        app.add_systems(
            FixedUpdate,
            (
                motors::walk::tick.run_if(in_loco_state(LocomotionState::Walk)),
                motors::fall::tick.run_if(in_loco_state(LocomotionState::Fall)),
                motors::sprint::tick.run_if(in_loco_state(LocomotionState::Sprint)),
                motors::sneak::tick.run_if(in_loco_state(LocomotionState::Sneak)),
                motors::jump::tick.run_if(in_loco_state(LocomotionState::Jump)),
                motors::glide::tick.run_if(in_loco_state(LocomotionState::Glide)),
                motors::climb::tick.run_if(in_loco_state(LocomotionState::Climb)),
                motors::mantle::tick.run_if(in_loco_state(LocomotionState::Mantle)),
                motors::auto_vault::tick.run_if(in_loco_state(LocomotionState::AutoVault)),
                motors::wall_jump::tick.run_if(in_loco_state(LocomotionState::WallJump)),
                motors::edge_leap::tick.run_if(in_loco_state(LocomotionState::EdgeLeap)),
                motors::stairs::tick.run_if(in_loco_state(LocomotionState::Stairs)),
                motors::ladder::tick.run_if(in_loco_state(LocomotionState::Ladder)),
            )
                .in_set(MovementSet::TickActiveMotor),
        );

        // Declarative collider swap for sneak.
        app.add_systems(Update, motors::sneak::sync_sneak_collider);
    }
}

/// Run condition: true when the player is in `target`. Our analog of Bevy's
/// `in_state`, but reading the per-entity `LocomotionState` component.
pub fn in_loco_state(
    target: LocomotionState,
) -> impl Fn(Query<&LocomotionState, With<Player>>) -> bool + Clone {
    move |q: Query<&LocomotionState, With<Player>>| {
        q.single().map(|s| *s == target).unwrap_or(false)
    }
}

/// `Arbitrate`: pick the winning proposal, write the SSoT `LocomotionState`, then
/// clear the buffer for next frame. This is the *only* writer of
/// `LocomotionState` (see `docs/architecture/movement.md`).
fn arbitrate(mut q: Single<(&mut LocomotionState, &mut ProposalBuffer), With<Player>>) {
    let (state, buffer) = &mut *q;
    let winner = buffer.arbitrate();
    if **state != winner {
        **state = winner;
        // (Activated/Deactivated events land with the motors that need them.)
    }
    buffer.0.clear();
}

fn spawn_player(mut commands: Commands) {
    // The Player is an invisible kinematic collider; the mesh lives on a separate
    // PlayerVisual entity that interpolates toward this body (see `visuals.rs`).
    // Radius 0.5, total height 2.0 ⇒ Avian cylinder length 1.0 (excludes hemispheres).
    commands.spawn((
        Player,
        Name::new("Player"),
        Transform::from_xyz(0.0, 1.5, 0.0),
        RigidBody::Kinematic,
        Collider::capsule(0.5, 1.0),
        BodyVelocity::default(),
        Intents::default(),
        LocomotionState::default(),
        ProposalBuffer::default(),
        Stamina::default(),
        // Nested tuples keep us under Bevy's 15-element tuple-bundle arity limit.
        (
            BodyContact::default(),
            GroundFacts::default(),
            LedgeFacts::default(),
            StairsFacts::default(),
            LadderFacts::default(),
        ),
        // Per-motor shared phase state (read by both propose and tick systems).
        (
            motors::jump::JumpPhase::default(),
            motors::mantle::MantleState::default(),
            motors::auto_vault::VaultState::default(),
            motors::wall_jump::WallJumpState::default(),
            motors::edge_leap::EdgeLeapState::default(),
        ),
    ));
}
