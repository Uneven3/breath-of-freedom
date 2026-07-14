//! Sprint motor — faster ground locomotion that drains stamina.
//!
//! Motor-local state (`SprintLock`) is a per-entity component — one instance
//! per actor, so stamina-lock state never bleeds between actors. The tick body
//! is the shared `motor_common::ground_locomotion_tick`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::facts::{GroundFacts, LedgeFacts, StairsFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{GroundLocomotion, GroundTickQuery, ground_locomotion_tick};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

/// Per-actor stamina-lock latch: set when stamina hits zero, cleared once it
/// recovers past `SPRINT_RECHARGE_THRESHOLD`. Was a `Local<bool>`; promoted to
/// a component so it doesn't bleed across actors.
#[derive(Component, Default)]
pub struct SprintLock(pub bool);

const SPRINT_RECHARGE_THRESHOLD: f32 = 20.0;

const PARAMS: GroundLocomotion = GroundLocomotion {
    max_speed: 10.0,
    acceleration: 25.0,
    friction: 35.0,
    rotation_speed: 15.0,
    stamina_per_sec: -10.0,
};

/// Propose SPRINT at OPPORTUNISTIC priority while grounded, holding sprint, and not
/// stamina-locked. Abstains on stairs (StairsMotor owns the climb) and on a climbable
/// wall the player wants to climb (so ClimbMotor can win).
type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a StairsFacts,
    &'a LedgeFacts,
    &'a Intents,
    &'a Stamina,
    &'a mut SprintLock,
    &'a mut ProposalBuffer,
);

pub fn propose(mut q: Query<ProposeQuery, With<Actor>>) {
    for (ground, stairs, ledge, intents, stamina, mut stamina_locked, mut buffer) in &mut q {
        let cur = stamina.current();
        if cur <= 0.0 {
            stamina_locked.0 = true;
        } else if cur >= SPRINT_RECHARGE_THRESHOLD {
            stamina_locked.0 = false;
        }

        // Abstain so StairsMotor / ClimbMotor can take over.
        if stairs.on_stairs {
            continue;
        }
        if ledge.can_climb && intents.wants_climb {
            continue;
        }

        if ground.grounded && intents.wants_sprint && !stamina_locked.0 {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Sprint,
                Priority::Opportunistic,
                0,
                "sprint",
            ));
        }
    }
}

pub fn tick(mut q: Query<GroundTickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    ground_locomotion_tick(&mut q, &mas, &time, LocomotionState::Sprint, &PARAMS);
}
