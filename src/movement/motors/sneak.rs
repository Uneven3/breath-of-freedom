//! Sneak motor — slow, crouched ground movement.
//!
//! The collision capsule swap for crouching is expressed **declaratively**:
//! the `sync_sneak_collider` system (registered in the plugin) keeps the
//! collider small exactly while the state is Sneak — no enter/exit event
//! bookkeeping needed.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, GroundFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::{apply_locomotion_rotation, body_move_and_slide, move_toward};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;
use crate::movement::{BodyVelocity, Player};

const MAX_SPEED: f32 = 2.5;
const ACCELERATION: f32 = 15.0;
const FRICTION: f32 = 20.0;
const ROTATION_SLERP_SPEED: f32 = 10.0;
const STAMINA_RECOVER_RATE: f32 = 5.0;

/// Crouch capsule: total height 2.0 → 1.2 (design value). Avian's capsule
/// length excludes the hemispheres, so length = 1.2 − 2·0.5 = 0.2.
pub const CROUCH_COLLIDER_LENGTH: f32 = 0.2;
pub const STAND_COLLIDER_LENGTH: f32 = 1.0;

/// Propose SNEAK at PLAYER_REQUESTED with weight 1 (beats Walk's weight 0).
pub fn propose(mut q: Single<(&GroundFacts, &Intents, &mut ProposalBuffer), With<Player>>) {
    let (ground, intents, buffer) = &mut *q;
    if ground.grounded && intents.wants_sneak {
        buffer.0.push(TransitionProposal::new(
            LocomotionState::Sneak,
            Priority::PlayerRequested,
            1,
            "sneak",
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

    // Sneak uses its own slower rotation speed.
    apply_locomotion_rotation(&mut transform, intents.move_dir, dt, ROTATION_SLERP_SPEED);

    let move_dir = Vec3::new(intents.move_dir.x, 0.0, intents.move_dir.y).normalize_or_zero();
    let mut v = vel.0;
    if move_dir != Vec3::ZERO {
        v.x = move_toward(v.x, move_dir.x * MAX_SPEED, ACCELERATION * dt);
        v.z = move_toward(v.z, move_dir.z * MAX_SPEED, ACCELERATION * dt);
    } else {
        v.x = move_toward(v.x, 0.0, FRICTION * dt);
        v.z = move_toward(v.z, 0.0, FRICTION * dt);
    }
    v.y = 0.0;

    stamina.recover(STAMINA_RECOVER_RATE * dt);

    vel.0 = body_move_and_slide(&mas, entity, collider, &mut transform, v, time.delta(), &mut contact);
}

/// Keep the collider crouched exactly while in Sneak, driven by current state
/// each time it changes (declarative, no enter/exit plumbing).
pub fn sync_sneak_collider(
    mut q: Query<(&LocomotionState, &mut Collider), (With<Player>, Changed<LocomotionState>)>,
) {
    for (state, mut collider) in &mut q {
        let length = if *state == LocomotionState::Sneak {
            CROUCH_COLLIDER_LENGTH
        } else {
            STAND_COLLIDER_LENGTH
        };
        *collider = Collider::capsule(0.5, length);
    }
}
