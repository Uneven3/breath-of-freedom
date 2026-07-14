//! Walk motor — flat-ground locomotion.
//!
//! Each motor is two systems: `propose` (runs every frame, in
//! `GatherProposals`) and `tick` (runs only when Walk is the active state, in
//! `TickActiveMotor`). The tick body is the shared
//! `motor_common::ground_locomotion_tick`; only the tuning differs from
//! Sprint/Sneak. See `docs/architecture/movement.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::facts::GroundFacts;
use crate::movement::motor_common::{GroundLocomotion, GroundTickQuery, ground_locomotion_tick};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;

const PARAMS: GroundLocomotion = GroundLocomotion {
    max_speed: 5.0,
    acceleration: 20.0,
    friction: 25.0,
    rotation_speed: 15.0,
    stamina_per_sec: 15.0,
};

/// Propose WALK at PLAYER_REQUESTED priority whenever grounded.
pub fn propose(mut q: Query<(&GroundFacts, &mut ProposalBuffer), With<Actor>>) {
    for (ground, mut buffer) in &mut q {
        if ground.grounded {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Walk,
                Priority::PlayerRequested,
                0,
                "walk",
            ));
        }
    }
}

pub fn tick(mut q: Query<GroundTickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    ground_locomotion_tick(&mut q, &mas, &time, LocomotionState::Walk, &PARAMS);
}
