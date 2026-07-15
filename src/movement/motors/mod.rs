//! Motors — one module per locomotion state. Each exposes `propose` (always
//! runs, its own system) and `tick_body` (runs only when active, called from
//! the single [`tick_active_motor`] dispatcher below).
//!
//! The dispatcher's exhaustive `match` on `LocomotionState` is the per-entity
//! "exactly one motor moves each body" invariant, enforced by the compiler: a
//! new state fails to build until it has exactly one tick arm. This replaces
//! the old convention of 13 separate tick systems each self-gating with
//! `if *state != X { continue }` (see `rationale/multi-actor-dispatch.md`).

use avian3d::prelude::*;
use bevy::ecs::query::QueryData;
use bevy::prelude::*;

pub mod auto_vault;
pub mod climb;
pub mod edge_leap;
pub mod fall;
pub mod glide;
pub mod jump;
pub mod ladder;
pub mod mantle;
pub mod sneak;
pub mod sprint;
pub mod stairs;
pub mod walk;
pub mod wall_jump;

use crate::movement::abilities::{
    AirborneMovement, ClimbMovement, GlideMovement, GroundMovement, JumpMovement, LadderMovement,
    LedgeTraversal, WallJumpMovement,
};
use crate::movement::body::BodyDimensions;
use crate::movement::facts::{BodyContact, GroundFacts, LadderFacts, LedgeFacts, StairsFacts};
use crate::movement::intents::Intents;
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

/// The union of every motor's tick row. Required fields are the
/// `KinematicActorBundle` contract; capabilities and per-motor state are
/// `Option` — a motor `tick_body` early-returns when its capability is absent
/// (which arbitration already prevents: a motor only proposes for actors that
/// carry its capability).
#[derive(QueryData)]
#[query_data(mutable)]
pub struct MotorTick {
    pub entity: Entity,
    pub collider: &'static Collider,
    pub transform: &'static mut Transform,
    pub velocity: &'static mut BodyVelocity,
    pub intents: &'static Intents,
    pub stamina: &'static mut Stamina,
    pub contact: &'static mut BodyContact,
    pub state: &'static LocomotionState,
    pub body: &'static BodyDimensions,
    pub ground: &'static GroundFacts,
    pub ledge: &'static LedgeFacts,
    pub stairs: &'static StairsFacts,
    pub ladder: &'static LadderFacts,
    pub ground_movement: Option<&'static GroundMovement>,
    pub airborne_movement: Option<&'static AirborneMovement>,
    pub jump_movement: Option<&'static JumpMovement>,
    pub glide_movement: Option<&'static GlideMovement>,
    pub climb_movement: Option<&'static ClimbMovement>,
    pub ledge_traversal: Option<&'static LedgeTraversal>,
    pub wall_jump_movement: Option<&'static WallJumpMovement>,
    pub ladder_movement: Option<&'static LadderMovement>,
    pub jump_phase: Option<&'static jump::JumpPhase>,
    pub crouched: Option<&'static sneak::Crouched>,
    pub mantle_state: Option<&'static mut mantle::MantleState>,
    pub vault_state: Option<&'static mut auto_vault::VaultState>,
    pub wall_jump_state: Option<&'static mut wall_jump::WallJumpState>,
    pub edge_leap_state: Option<&'static mut edge_leap::EdgeLeapState>,
    pub stairs_local: Option<&'static mut stairs::StairsLocal>,
}

/// `TickActiveMotor`: one query pass over all actors, dispatching each to the
/// single motor that owns its `LocomotionState` this frame.
pub fn tick_active_motor(mut q: Query<MotorTick, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    for mut row in &mut q {
        let state = *row.state;
        // Stairs' tread cache survives only while Stairs is active; its tick
        // arm can't clear it (it no longer runs), so the exit path lives here.
        if state != LocomotionState::Stairs
            && let Some(local) = row.stairs_local.as_mut()
        {
            local.0 = None;
        }
        match state {
            LocomotionState::Walk => walk::tick_body(&mut row, &mas, &time),
            LocomotionState::Sprint => sprint::tick_body(&mut row, &mas, &time),
            LocomotionState::Fall => fall::tick_body(&mut row, &mas, &time),
            LocomotionState::Jump => jump::tick_body(&mut row, &mas, &time),
            LocomotionState::AutoVault => auto_vault::tick_body(&mut row, &mas, &time),
            LocomotionState::Climb => climb::tick_body(&mut row, &mas, &time),
            LocomotionState::Mantle => mantle::tick_body(&mut row, &mas, &time),
            LocomotionState::Stairs => stairs::tick_body(&mut row, &mas, &time),
            LocomotionState::Ladder => ladder::tick_body(&mut row, &mas, &time),
            LocomotionState::Glide => glide::tick_body(&mut row, &mas, &time),
            LocomotionState::Sneak => sneak::tick_body(&mut row, &mas, &time),
            LocomotionState::WallJump => wall_jump::tick_body(&mut row, &mas, &time),
            LocomotionState::EdgeLeap => edge_leap::tick_body(&mut row, &mas, &time),
        }
    }
}
