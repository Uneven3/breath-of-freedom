//! Sprint motor — faster ground locomotion that drains stamina.
//!
//! Motor-local state (`SprintLock`) is a per-entity component — one instance
//! per actor, so stamina-lock state never bleeds between actors.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts, LedgeFacts, StairsFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

/// Per-actor stamina-lock latch: set when stamina hits zero, cleared once it
/// recovers past `SPRINT_RECHARGE_THRESHOLD`. Was a `Local<bool>`; promoted to
/// a component so it doesn't bleed across actors.
#[derive(Component, Default)]
pub struct SprintLock(pub bool);

const SPRINT_SPEED: f32 = 10.0;
const SPRINT_ACCELERATION: f32 = 25.0;
const SPRINT_DECELERATION: f32 = 35.0;
const STAMINA_COST_PER_SEC: f32 = 10.0;
const SPRINT_RECHARGE_THRESHOLD: f32 = 20.0;

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
            buffer.0.push(TransitionProposal::new(
                LocomotionState::Sprint,
                Priority::Opportunistic,
                0,
                "sprint",
            ));
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a Intents,
    &'a mut Stamina,
    &'a mut BodyContact,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (entity, collider, mut transform, mut vel, intents, mut stamina, mut contact, state) in
        &mut q
    {
        if *state != LocomotionState::Sprint {
            continue;
        }

        apply_locomotion_rotation(&mut transform, intents.move_dir, dt, 15.0);

        let move_dir = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y).normalize_or_zero();
        let mut v = vel.0;
        if move_dir != Vec3::ZERO {
            v.x = move_toward(v.x, move_dir.x * SPRINT_SPEED, SPRINT_ACCELERATION * dt);
            v.z = move_toward(v.z, move_dir.z * SPRINT_SPEED, SPRINT_ACCELERATION * dt);
        } else {
            v.x = move_toward(v.x, 0.0, SPRINT_DECELERATION * dt);
            v.z = move_toward(v.z, 0.0, SPRINT_DECELERATION * dt);
        }
        v.y = 0.0;

        stamina.drain(STAMINA_COST_PER_SEC * dt);

        vel.0 = body_move_and_slide(
            &mas,
            entity,
            collider,
            &mut transform,
            v,
            time.delta(),
            &mut contact,
        );
    }
}
