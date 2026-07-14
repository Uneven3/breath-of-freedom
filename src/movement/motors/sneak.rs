//! Sneak motor — slow, crouched ground movement.
//!
//! The collision capsule swap for crouching is expressed **declaratively**:
//! the `sync_sneak_collider` system (registered in the plugin, right after
//! `Arbitrate` in `FixedUpdate` so the active motor ticks with the correct
//! capsule) keeps the collider small exactly while the state is Sneak — no
//! enter/exit event bookkeeping needed. The tick body is the shared
//! `motor_common::ground_locomotion_tick`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::GroundFacts;
use crate::movement::intents::Intents;
use crate::movement::motor_common::{GroundLocomotion, GroundTickQuery, ground_locomotion_tick};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, body};

const PARAMS: GroundLocomotion = GroundLocomotion {
    max_speed: 2.5,
    acceleration: 15.0,
    friction: 20.0,
    // Sneak uses its own slower rotation speed.
    rotation_speed: 10.0,
    stamina_per_sec: 5.0,
};

/// Whether the crouch capsule is currently applied. Lets `sync_sneak_collider`
/// rebuild the collider only when crossing the Sneak boundary, instead of on
/// every state transition (Walk→Sprint used to recreate an identical capsule).
#[derive(Component, Default)]
pub struct Crouched(pub bool);

/// Reusable standing capsule for the overhead-clearance sensor. Keeping it on
/// the actor avoids constructing collision geometry in `FixedUpdate`.
#[derive(Component)]
pub struct StandCollider(pub Collider);

/// Whether the crouched actor has room to restore its standing capsule.
#[derive(Component, Default)]
pub struct StandClearance(pub bool);

/// Update the stand-up clearance before proposal arbitration. A crouched actor
/// keeps Sneak active after the player releases the button until its full-height
/// capsule fits, so it can never expand into a ceiling.
pub fn update_stand_clearance(
    spatial: SpatialQuery,
    mut q: Query<
        (
            Entity,
            &Transform,
            &Crouched,
            &StandCollider,
            &mut StandClearance,
        ),
        With<Actor>,
    >,
) {
    for (entity, transform, crouched, stand_collider, mut clearance) in &mut q {
        if !crouched.0 {
            clearance.0 = true;
            continue;
        }

        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let mut blocked = false;
        spatial.shape_intersections_callback(
            &stand_collider.0,
            standing_center(transform.translation),
            transform.rotation,
            &filter,
            |_| {
                blocked = true;
                false
            },
        );
        clearance.0 = !blocked;
    }
}

/// Propose SNEAK at PLAYER_REQUESTED with weight 1 (beats Walk's weight 0).
pub fn propose(
    mut q: Query<
        (
            &GroundFacts,
            &Intents,
            &Crouched,
            &StandClearance,
            &mut ProposalBuffer,
        ),
        With<Actor>,
    >,
) {
    for (ground, intents, crouched, clearance, mut buffer) in &mut q {
        let must_remain_crouched = crouched.0 && !clearance.0;
        if ground.grounded && (intents.wants_sneak || must_remain_crouched) {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Sneak,
                Priority::PlayerRequested,
                1,
                "sneak",
            ));
        }
    }
}

pub fn tick(mut q: Query<GroundTickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    ground_locomotion_tick(&mut q, &mas, &time, LocomotionState::Sneak, &PARAMS);
}

/// Keep the collider crouched exactly while in Sneak, driven by current state
/// each time it changes (declarative, no enter/exit plumbing).
type ColliderSyncFilter = (With<Actor>, Changed<LocomotionState>);

pub fn sync_sneak_collider(
    mut q: Query<
        (
            &LocomotionState,
            &mut Collider,
            &mut Transform,
            &mut Crouched,
        ),
        ColliderSyncFilter,
    >,
) {
    for (state, mut collider, mut transform, mut crouched) in &mut q {
        let want_crouch = *state == LocomotionState::Sneak;
        if want_crouch == crouched.0 {
            continue;
        }
        let old_half_height = if crouched.0 {
            body::CROUCH_HALF_HEIGHT
        } else {
            body::HALF_HEIGHT
        };
        let new_half_height = if want_crouch {
            body::CROUCH_HALF_HEIGHT
        } else {
            body::HALF_HEIGHT
        };
        crouched.0 = want_crouch;
        let length = if want_crouch {
            body::CROUCH_CAPSULE_LENGTH
        } else {
            body::STAND_CAPSULE_LENGTH
        };
        *collider = Collider::capsule(body::RADIUS, length);
        transform.translation.y += new_half_height - old_half_height;
    }
}

fn standing_center(crouched_center: Vec3) -> Vec3 {
    crouched_center + Vec3::Y * (body::HALF_HEIGHT - body::CROUCH_HALF_HEIGHT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standing_sensor_keeps_the_actor_feet_anchored() {
        let crouched_center = Vec3::new(2.0, body::CROUCH_HALF_HEIGHT, -3.0);
        let standing = standing_center(crouched_center);
        assert_eq!(standing.x, crouched_center.x);
        assert_eq!(standing.z, crouched_center.z);
        assert!((standing.y - body::HALF_HEIGHT).abs() < f32::EPSILON);
    }
}
