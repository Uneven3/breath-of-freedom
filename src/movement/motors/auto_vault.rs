//! Auto-vault motor — kinematic hop over a waist-high obstacle.
//!
//! Same phase-component pattern as mantle, sharing its
//! `motor_common::KinematicArc`.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::abilities::LedgeTraversal;
use crate::movement::facts::{GroundFacts, LedgeFacts};
use crate::movement::intents::{Intents, TraversalActionIntent};
use crate::movement::motor_common::{KinematicArc, body_move_and_slide};
use crate::movement::motors::MotorCore;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal, weight};
use crate::movement::state::LocomotionState;

const MIN_SPEED: f32 = 0.01;
const MIN_DURATION: f32 = 0.1;

#[derive(Component, Default)]
pub struct VaultState {
    pub(crate) arc: KinematicArc,
}

type ProposeQuery<'a> = (
    &'a GroundFacts,
    &'a LedgeFacts,
    &'a Intents,
    &'a LocomotionState,
    &'a VaultState,
    &'a mut ProposalBuffer,
);

type ProposeFilter = (
    crate::movement::attachment::LocomotionActorFilter,
    With<LedgeTraversal>,
);

pub fn propose(mut q: Query<ProposeQuery, ProposeFilter>) {
    for (ground, ledge, intents, current, state, mut buffer) in &mut q {
        if *current == LocomotionState::AutoVault && state.arc.running {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::AutoVault,
                Priority::Forced,
                weight::AUTO_VAULT,
                "auto_vault",
            ));
            continue;
        }

        if ground.grounded
            && ledge.is_vaultable
            && intents.traversal == TraversalActionIntent::Vault
        {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::AutoVault,
                Priority::PlayerRequested,
                weight::AUTO_VAULT,
                "auto_vault",
            ));
        }
    }
}

type TickQuery<'a> = (
    MotorCore,
    &'a LedgeTraversal,
    &'a LedgeFacts,
    &'a mut VaultState,
);

pub fn tick_body(
    mut actors: Query<TickQuery, crate::movement::attachment::LocomotionActorFilter>,
    mas: MoveAndSlide,
    time: Res<Time>,
) {
    for (mut row, movement, ledge, mut state) in &mut actors {
        if *row.state != LocomotionState::AutoVault {
            continue;
        }
        let dt = time.delta_secs();

        if !state.arc.running
            && !begin_vault(&mut state, row.transform.translation, ledge, movement)
        {
            continue;
        }

        row.transform.translation = state.arc.step(dt, movement.vault.arc_height);
        row.velocity.0 = Vec3::ZERO;
        body_move_and_slide(
            &mas,
            row.entity,
            row.collider,
            &mut row.transform,
            Vec3::ZERO,
            time.delta(),
            &mut row.contact,
        );
    }
}

fn begin_vault(
    state: &mut VaultState,
    pos: Vec3,
    ledge: &LedgeFacts,
    movement: &LedgeTraversal,
) -> bool {
    let Some(target) = ledge.vault_target_position else {
        return false;
    };
    let duration = (pos.distance(target) / movement.vault.speed.max(MIN_SPEED)).max(MIN_DURATION);
    state.arc.begin(pos, target, duration);
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::Actor;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn actor_without_ledge_traversal_cannot_propose_auto_vault() {
        let mut world = World::new();
        let entity = world
            .spawn((
                Actor,
                GroundFacts {
                    grounded: true,
                    ..default()
                },
                LedgeFacts {
                    is_vaultable: true,
                    ..default()
                },
                Intents {
                    traversal: crate::movement::intents::TraversalActionIntent::Vault,
                    ..default()
                },
                LocomotionState::Walk,
                VaultState::default(),
                ProposalBuffer::default(),
            ))
            .id();

        world.run_system_once(propose).unwrap();

        assert!(
            world
                .entity(entity)
                .get::<ProposalBuffer>()
                .unwrap()
                .iter()
                .next()
                .is_none()
        );
    }
}
