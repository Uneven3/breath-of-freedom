//! Sneak motor — slow, crouched ground movement.
//!
//! The collision capsule swap for crouching is expressed **declaratively**:
//! the `sync_sneak_collider` system (registered in the plugin, right after
//! `Arbitrate` in `FixedUpdate` so the active motor ticks with the correct
//! capsule) keeps the collider small exactly while the state is Sneak — no
//! enter/exit event bookkeeping needed. The tick body is the shared
//! `motor_common::ground_locomotion_step`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::GroundMovement;
use crate::movement::body::BodyDimensions;
use crate::movement::facts::GroundFacts;
use crate::movement::intents::Intents;
use crate::movement::motor_common::{
    GroundLocomotionStep, GroundTickQuery, ground_locomotion_step,
};
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;

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
type StandClearanceQuery<'a> = (
    Entity,
    &'a Transform,
    &'a Crouched,
    &'a StandCollider,
    &'a BodyDimensions,
    &'a mut StandClearance,
);

pub fn update_stand_clearance(
    spatial: SpatialQuery,
    mut q: Query<StandClearanceQuery, With<Actor>>,
) {
    for (entity, transform, crouched, stand_collider, body, mut clearance) in &mut q {
        if !crouched.0 {
            clearance.0 = true;
            continue;
        }

        let filter = SpatialQueryFilter::from_excluded_entities([entity]);
        let mut blocked = false;
        spatial.shape_intersections_callback(
            &stand_collider.0,
            standing_center(transform.translation, *body),
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
type SneakProposalQuery<'a> = (
    &'a GroundFacts,
    &'a Intents,
    &'a Crouched,
    &'a StandClearance,
    &'a mut ProposalBuffer,
);
type SneakProposalFilter = (With<Actor>, With<GroundMovement>);

pub fn propose(mut q: Query<SneakProposalQuery, SneakProposalFilter>) {
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

pub fn tick(
    mut q: Query<(&GroundMovement, GroundTickQuery), With<Actor>>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (ground_movement, row) in &mut q {
        let (
            entity,
            collider,
            mut transform,
            mut velocity,
            intents,
            mut stamina,
            mut contact,
            ground,
            state,
        ) = row;
        ground_locomotion_step(
            GroundLocomotionStep {
                entity,
                collider,
                transform: &mut transform,
                velocity: &mut velocity,
                intents,
                stamina: &mut stamina,
                contact: &mut contact,
                ground,
                state: *state,
            },
            LocomotionState::Sneak,
            &mas,
            &time,
            &ground_movement.sneak,
        );
    }
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
            &BodyDimensions,
        ),
        ColliderSyncFilter,
    >,
) {
    for (state, mut collider, mut transform, mut crouched, body) in &mut q {
        let want_crouch = *state == LocomotionState::Sneak;
        if want_crouch == crouched.0 {
            continue;
        }
        let old_half_height = if crouched.0 {
            body.crouched_half_height()
        } else {
            body.standing_half_height()
        };
        let new_half_height = if want_crouch {
            body.crouched_half_height()
        } else {
            body.standing_half_height()
        };
        crouched.0 = want_crouch;
        *collider = if want_crouch {
            body.crouched_collider()
        } else {
            body.standing_collider()
        };
        transform.translation.y += new_half_height - old_half_height;
    }
}

fn standing_center(crouched_center: Vec3, body: BodyDimensions) -> Vec3 {
    crouched_center + Vec3::Y * (body.standing_half_height() - body.crouched_half_height())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn standing_sensor_keeps_the_actor_feet_anchored() {
        let body = BodyDimensions::PLAYER;
        let crouched_center = Vec3::new(2.0, body.crouched_half_height(), -3.0);
        let standing = standing_center(crouched_center, body);
        assert_eq!(standing.x, crouched_center.x);
        assert_eq!(standing.z, crouched_center.z);
        assert!((standing.y - body.standing_half_height()).abs() < f32::EPSILON);
    }

    #[test]
    fn only_ground_movement_actors_propose_sneak() {
        let mut world = World::new();
        let capable = world
            .spawn((
                Actor,
                GroundMovement::PLAYER,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    wants_sneak: true,
                    ..default()
                },
                Crouched::default(),
                StandClearance::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let incapable = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    wants_sneak: true,
                    ..default()
                },
                Crouched::default(),
                StandClearance::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(capable)
                .get::<ProposalBuffer>()
                .is_some_and(|buffer| buffer.iter().next().is_some())
        );
        assert!(
            world
                .entity(incapable)
                .get::<ProposalBuffer>()
                .is_some_and(|buffer| buffer.iter().next().is_none())
        );
    }
}
