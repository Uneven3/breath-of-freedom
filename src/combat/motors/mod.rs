//! Combat motors — one module per family of `CombatState`s.
//!
//! Same contract as `movement::motors`: `propose` always runs; the tick phase
//! is a single dispatcher whose exhaustive `match` on `CombatState` is the
//! compiler-checked "exactly one motor owns each state" invariant, from day
//! one (the lesson already paid for in `rationale/multi-actor-dispatch.md`).

use bevy::ecs::query::QueryData;
use bevy::prelude::*;

pub mod aim;
pub mod attack;
pub mod idle;

use crate::combat::state::CombatState;
use crate::movement::Actor;

/// The union of every combat motor's tick row. Grows as states land.
#[derive(QueryData)]
#[query_data(mutable)]
pub struct CombatMotorTick {
    pub state: &'static CombatState,
    pub combo: Option<&'static mut attack::ComboLocal>,
    pub swing: Option<&'static mut attack::ActiveSwing>,
}

/// `CombatSet::TickActiveMotor`: one query pass, exhaustive dispatch.
pub fn tick_active_motor(mut q: Query<CombatMotorTick, With<Actor>>, time: Res<Time>) {
    for mut row in &mut q {
        // The combo clock runs for every phase (it also detects phase entry);
        // the match owns per-state behavior.
        attack::tick_phase_clock(&mut row, &time);
        match *row.state {
            CombatState::Idle => {}
            CombatState::Windup | CombatState::Recovery => {}
            // The Active sweep lives in `attack::sweep_active_swings` (it
            // needs read-only access to *other* actors' transforms, which
            // this mutable row query cannot alias).
            CombatState::Active => {}
            // The bow release lives in `aim::shoot_drawn_arrow`
            // (runs in GatherProposals to read the pre-arbitration state).
            CombatState::Aiming => {}
        }
    }
}
