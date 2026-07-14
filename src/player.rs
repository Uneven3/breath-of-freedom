//! Local player composition.
//!
//! Movement provides capabilities; who gets them is a scene concern. This
//! plugin assembles the locally-controlled player out of the kinematic actor
//! contract, its movement capability bundles, and the local input binding —
//! the same pieces an AI or network controller composes differently (see
//! `docs/architecture/movement.md` § bundles).

use bevy::prelude::*;

use crate::movement::Player;
use crate::movement::abilities::{
    AirborneMovement, ClimbMovement, GlideMovement, GroundMovement, JumpMovement, LadderMovement,
    LedgeTraversal, WallJumpMovement,
};
use crate::movement::body::BodyDimensions;
use crate::movement::brain::ClimbInputState;
use crate::movement::bundles::{
    GlideMovementBundle, GroundMovementBundle, JumpMovementBundle, KinematicActorBundle,
    LedgeTraversalBundle, WallJumpMovementBundle,
};
use crate::movement::sensing::{GroundSensing, LedgeSensing};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_player);
    }
}

fn spawn_player(mut commands: Commands) {
    // The Player is an invisible kinematic collider; the mesh lives on a separate
    // PlayerVisual entity that interpolates toward this body (see `visuals.rs`).
    // Capsule dimensions live in `body` (shared with services and motors).
    let body_dimensions = BodyDimensions::PLAYER;
    commands.spawn((
        Player,
        crate::input::frame::InputControlledBy(crate::input::frame::LOCAL_INPUT_SOURCE),
        crate::input::frame::ControlOrientation::default(),
        Name::new("Player"),
        KinematicActorBundle::new(
            Transform::from_xyz(0.0, 1.5, 0.0),
            body_dimensions,
            GroundSensing::PLAYER,
        ),
        (
            GroundMovementBundle::new(GroundMovement::PLAYER, body_dimensions),
            AirborneMovement::PLAYER,
            JumpMovementBundle::new(JumpMovement::PLAYER),
            GlideMovementBundle::new(GlideMovement::PLAYER),
            ClimbMovement::PLAYER,
            LadderMovement::PLAYER,
            LedgeTraversalBundle::new(LedgeTraversal::PLAYER),
            WallJumpMovementBundle::new(WallJumpMovement::PLAYER),
            LedgeSensing::PLAYER,
            ClimbInputState::default(),
            crate::input::InputConsumeCursor::default(),
        ),
    ));
}
