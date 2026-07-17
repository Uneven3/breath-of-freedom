//! Sneak motor — slow, crouched ground movement.
//!
//! The collision capsule swap for crouching is expressed **declaratively** and
//! orthogonally to the active state: the `sync_crouch_collider` system
//! (registered in the plugin, right after `Arbitrate` in `FixedUpdate` so the
//! active motor ticks with the correct capsule) keeps the collider small while
//! the actor wants to crouch during *any* ground locomotion (flat-ground Sneak
//! or Stairs), driven by the crouch gait and stand-up clearance — no enter/exit
//! event bookkeeping needed. The tick body is the shared
//! `motor_common::ground_locomotion_step`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::Actor;
use crate::movement::abilities::{SneakMovement, SprintMovement};
use crate::movement::body::BodyDimensions;
use crate::movement::facts::GroundFacts;
use crate::movement::intents::Intents;
use crate::movement::lod::SensingLod;
use crate::movement::motor_common::{GroundDriveStep, ground_drive_step};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::stamina::Stamina;
use crate::movement::state::LocomotionState;

/// Per-actor sneak stamina-lock latch: set when stamina hits zero, cleared once it
/// recovers past `SNEAK_RECHARGE_THRESHOLD`.
#[derive(Component, Default)]
pub struct SneakLock(pub bool);

const SNEAK_RECHARGE_THRESHOLD: f32 = 20.0;

/// Whether the crouch capsule is currently applied. Lets `sync_crouch_collider`
/// rebuild the collider only when the desired crouch actually changes, and lets
/// other ground motors (e.g. Stairs) read the physical form without recomputing it.
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
    Option<&'a SensingLod>,
);

pub fn update_stand_clearance(
    spatial: SpatialQuery,
    mut q: Query<
        StandClearanceQuery,
        (
            With<Actor>,
            With<crate::movement::attachment::LocomotionEnabled>,
        ),
    >,
) {
    for (entity, transform, crouched, stand_collider, body, mut clearance, lod) in &mut q {
        if SensingLod::skips(lod) {
            continue;
        }
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

/// Propose SNEAK at PLAYER_REQUESTED (outranks Walk's DEFAULT fallback).
type SneakProposalQuery<'a> = (
    &'a GroundFacts,
    &'a Intents,
    &'a Crouched,
    &'a StandClearance,
    &'a Stamina,
    &'a mut SneakLock,
    &'a mut ProposalBuffer,
);
type SneakProposalFilter = (
    With<Actor>,
    With<SneakMovement>,
    With<crate::movement::attachment::LocomotionEnabled>,
);

pub fn propose(mut q: Query<SneakProposalQuery, SneakProposalFilter>) {
    for (ground, intents, crouched, clearance, stamina, mut sneak_lock, mut buffer) in &mut q {
        let cur = stamina.current();
        if cur <= 0.0 {
            sneak_lock.0 = true;
        } else if cur >= SNEAK_RECHARGE_THRESHOLD {
            sneak_lock.0 = false;
        }

        let must_remain_crouched = crouched.0 && !clearance.0;
        let can_sneak = !sneak_lock.0 || must_remain_crouched;

        if ground.grounded && ((intents.wants_sneak && can_sneak) || must_remain_crouched) {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Sneak,
                Priority::PlayerRequested,
                weight::SNEAK,
                "sneak",
            ));
        }
    }
}

type TickQuery<'a> = (
    MotorCore,
    &'a SneakMovement,
    Option<&'a SprintMovement>,
    Option<&'a mut Stamina>,
);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, sprint, mut stamina) in &mut actors {
        let mut sneak_profile = movement.drive;
        if row.intents.planar.direction.length_squared() > 0.01 {
            let sprint_cost = sprint.map_or(0.0, |value| value.drive.stamina_per_sec.abs());
            sneak_profile.stamina_per_sec = -sprint_cost * movement.sprinting_stamina_factor;
        }
        ground_drive_step(
            GroundDriveStep {
                entity: row.entity,
                collider: row.collider,
                transform: &mut row.transform,
                velocity: &mut row.velocity,
                intents: row.intents,
                stamina: stamina.as_deref_mut(),
                contact: &mut row.contact,
                ground: row.ground,
                state: *row.state,
            },
            LocomotionState::Sneak,
            &mas,
            &time,
            &sneak_profile,
        );
    }
}

/// Crouch is a modifier orthogonal to the active state: an actor can be crouched
/// while Walking or on Stairs, not only in the flat-ground `Sneak` gait. So the
/// crouch capsule follows these ground-locomotion states, and within them the
/// crouch *intent* (below), rather than `state == Sneak`.
fn is_ground_locomotion(state: LocomotionState) -> bool {
    matches!(
        state,
        LocomotionState::Walk
            | LocomotionState::Sprint
            | LocomotionState::Sneak
            | LocomotionState::Stairs
    )
}

/// Keep the collider crouched while the actor wants to be crouched during ground
/// locomotion — driven by the crouch gait (or a still-blocked stand-up), not by
/// the `Sneak` state, so it composes with Stairs too. Declarative: compares the
/// desired crouch against the applied one (`Crouched`) and rebuilds only on a
/// change. Runs each frame after `Arbitrate` (crouch tracks intent, which can
/// change without a state transition), before the active motor ticks.
type CrouchSyncQuery<'a> = (
    &'a LocomotionState,
    &'a Intents,
    &'a StandClearance,
    &'a SneakLock,
    &'a mut Crouched,
    &'a mut Collider,
    &'a mut Transform,
    &'a BodyDimensions,
);

pub fn sync_crouch_collider(
    mut q: Query<
        CrouchSyncQuery,
        (
            With<Actor>,
            With<crate::movement::attachment::LocomotionEnabled>,
        ),
    >,
) {
    for (state, intents, clearance, sneak_lock, mut crouched, mut collider, mut transform, body) in
        &mut q
    {
        let must_remain_crouched = crouched.0 && !clearance.0;
        let can_sneak = !sneak_lock.0 || must_remain_crouched;
        let want_crouch = is_ground_locomotion(*state)
            && ((intents.wants_sneak && can_sneak) || must_remain_crouched);
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
    use crate::movement::abilities::SneakMovement;
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
                crate::movement::attachment::LocomotionEnabled,
                SneakMovement::PLAYER,
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
                Stamina::default(),
                SneakLock::default(),
                ProposalBuffer::default(),
            ))
            .id();
        let incapable = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
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
                Stamina::default(),
                SneakLock::default(),
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

    #[test]
    fn sneak_stamina_drain_and_lock() {
        let mut world = World::new();
        let mut exhausted = Stamina::default();
        exhausted.drain(100.0);

        let actor = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                SneakMovement::PLAYER,
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
                exhausted,
                SneakLock::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(actor)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none(),
            "exhausted stamina must lock out sneak proposal in the open"
        );
        assert!(
            world.entity(actor).get::<SneakLock>().unwrap().0,
            "SneakLock must latch to true when stamina hits zero"
        );
    }

    #[test]
    fn sneak_stamina_must_remain_crouched_ignores_lock() {
        let mut world = World::new();
        let mut exhausted = Stamina::default();
        exhausted.drain(100.0);

        let actor = world
            .spawn((
                Actor,
                crate::movement::attachment::LocomotionEnabled,
                SneakMovement::PLAYER,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                Intents {
                    wants_sneak: true,
                    ..default()
                },
                Crouched(true),
                StandClearance(false), // No headroom
                exhausted,
                SneakLock(true), // Already locked
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(actor)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_some(),
            "must remain crouched (no headroom) must propose Sneak even with exhausted stamina"
        );
    }

    fn crouch_actor(
        state: LocomotionState,
        wants_sneak: bool,
        crouched: bool,
        clearance: bool,
    ) -> impl Bundle {
        let body = BodyDimensions::PLAYER;
        (
            Actor,
            crate::movement::attachment::LocomotionEnabled,
            state,
            Intents {
                wants_sneak,
                ..default()
            },
            StandClearance(clearance),
            Crouched(crouched),
            SneakLock::default(),
            if crouched {
                body.crouched_collider()
            } else {
                body.standing_collider()
            },
            Transform::from_xyz(0.0, 5.0, 0.0),
            body,
        )
    }

    /// Crouch is orthogonal to the active state: on Stairs (where `Forced` Stairs
    /// beats `PlayerRequested` Sneak, so the state is never Sneak) the capsule
    /// must still crouch from the Sneak gait, feet staying anchored.
    #[test]
    fn crouch_capsule_applies_on_stairs_via_intent() {
        let mut world = World::new();
        let body = BodyDimensions::PLAYER;
        let e = world
            .spawn(crouch_actor(LocomotionState::Stairs, true, false, true))
            .id();

        world.run_system_once(sync_crouch_collider).unwrap();

        assert!(
            world.entity(e).get::<Crouched>().unwrap().0,
            "must crouch on stairs from the Sneak gait"
        );
        let y = world.entity(e).get::<Transform>().unwrap().translation.y;
        let drop = body.standing_half_height() - body.crouched_half_height();
        assert!(
            (y - (5.0 - drop)).abs() < 1e-4,
            "feet must stay anchored as the capsule shrinks"
        );
    }

    /// Leaving ground locomotion (a jump) with headroom lets the capsule stand.
    #[test]
    fn crouch_releases_when_state_leaves_ground_locomotion() {
        let mut world = World::new();
        let e = world
            .spawn(crouch_actor(LocomotionState::Jump, true, true, true))
            .id();

        world.run_system_once(sync_crouch_collider).unwrap();

        assert!(
            !world.entity(e).get::<Crouched>().unwrap().0,
            "airborne with clearance must stand up"
        );
    }

    /// Button released, but no headroom: `StandClearance` keeps it crouched.
    #[test]
    fn crouch_holds_without_clearance_after_release() {
        let mut world = World::new();
        let e = world
            .spawn(crouch_actor(LocomotionState::Walk, false, true, false))
            .id();

        world.run_system_once(sync_crouch_collider).unwrap();

        assert!(
            world.entity(e).get::<Crouched>().unwrap().0,
            "no clearance must keep it crouched even after release"
        );
    }
}
