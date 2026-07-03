//! Walk motor — flat-ground locomotion.
//!
//! Each motor is two systems: `propose` (runs every frame, in
//! `GatherProposals`) and `tick` (runs only when Walk is the active state, in
//! `TickActiveMotor`). See `docs/architecture/movement.md`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};
use crate::movement::stamina::Stamina;

const MAX_SPEED: f32 = 5.0;
const ACCELERATION: f32 = 20.0;
const FRICTION: f32 = 25.0;
const STAMINA_RECOVER_PER_SEC: f32 = 15.0;

/// Propose WALK at PLAYER_REQUESTED priority whenever grounded.
pub fn propose(mut q: Single<(&GroundFacts, &mut ProposalBuffer), With<Player>>) {
    let (ground, buffer) = &mut *q;
    if ground.grounded {
        buffer.0.push(TransitionProposal::new(
            LocomotionState::Walk,
            Priority::PlayerRequested,
            0,
            "walk",
        ));
    }
}

pub fn tick(
    player: Single<
        (
            Entity,
            &Collider,
            &mut Transform,
            &mut BodyVelocity,
            &Intents,
            &mut Stamina,
            &mut BodyContact,
        ),
        With<Player>,
    >,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    let (entity, collider, mut transform, mut vel, intents, mut stamina, mut contact) =
        player.into_inner();
    let dt = time.delta_secs();

    apply_locomotion_rotation(&mut transform, intents.move_dir, dt, 15.0);

    let move_dir = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y).normalize_or_zero();
    let mut v = vel.0;
    if move_dir != Vec3::ZERO {
        v.x = move_toward(v.x, move_dir.x * MAX_SPEED, ACCELERATION * dt);
        v.z = move_toward(v.z, move_dir.z * MAX_SPEED, ACCELERATION * dt);
    } else {
        v.x = move_toward(v.x, 0.0, FRICTION * dt);
        v.z = move_toward(v.z, 0.0, FRICTION * dt);
    }
    // WalkMotor owns velocity.y in walk mode and is strictly flat-floor.
    v.y = 0.0;

    stamina.recover(STAMINA_RECOVER_PER_SEC * dt);

    vel.0 = body_move_and_slide(&mas, entity, collider, &mut transform, v, time.delta(), &mut contact);
}
