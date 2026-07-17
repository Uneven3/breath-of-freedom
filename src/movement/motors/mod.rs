//! Motors — one module per locomotion state. Each exposes `propose` and an
//! exact-query `tick_body`; tick systems are chained and only the system whose
//! state is active moves a given body.

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

use crate::movement::BodyVelocity;
use crate::movement::body::BodyDimensions;
use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::state::LocomotionState;

/// Minimal body row shared by all active motors. Capabilities, optional pools,
/// facts and private state stay in each motor's exact query.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct MotorCore {
    pub entity: Entity,
    pub collider: &'static Collider,
    pub transform: &'static mut Transform,
    pub velocity: &'static mut BodyVelocity,
    pub intents: &'static Intents,
    pub contact: &'static mut BodyContact,
    pub state: &'static LocomotionState,
    pub body: &'static BodyDimensions,
    pub ground: &'static GroundFacts,
}
