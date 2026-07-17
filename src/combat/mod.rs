//! Combat plugin — the Broker pipeline's sibling.
//!
//! Same shape as Movement (read intents → gather proposals → arbitrate →
//! tick dispatcher), own types (`CombatIntents`/`CombatState`), shared
//! arbitration core (`crate::proposal`). Scheduled after
//! `MovementSet::TickActiveMotor` so hit sweeps read post-move transforms of
//! the same tick. See `docs/ARCHITECTURE.md`.

use bevy::prelude::*;

pub mod brain;
pub mod context;
pub mod context_data;
pub mod intent;
pub mod motors;
pub mod proposal;
pub mod state;
pub mod weapon;

use crate::movement::constraints::LocomotionConstraintMessage;
use crate::movement::{Actor, MovementSet};
use proposal::CombatProposalBuffer;
use state::CombatState;

/// Ordered phases of the combat pipeline within `FixedUpdate`.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum CombatSet {
    ApplyContext,
    ReadIntents,
    GatherProposals,
    Arbitrate,
    TickActiveMotor,
    EmitConstraints,
}

pub struct CombatPlugin;

impl Plugin for CombatPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<motors::attack::MeleeHitMessage>();
        app.add_message::<motors::attack::HitImpactMessage>();
        app.add_message::<motors::aim::BowFiredMessage>();
        app.add_message::<context_data::SetMountedCombatMessage>();

        app.configure_sets(
            FixedUpdate,
            (
                CombatSet::ApplyContext,
                CombatSet::ReadIntents,
                CombatSet::GatherProposals,
                CombatSet::Arbitrate,
                CombatSet::TickActiveMotor,
                CombatSet::EmitConstraints,
            )
                .chain()
                .after(MovementSet::TickActiveMotor),
        );

        app.add_systems(
            FixedUpdate,
            (
                context::apply_mounted_context.in_set(CombatSet::ApplyContext),
                brain::read_intents.in_set(CombatSet::ReadIntents),
                (
                    motors::idle::propose,
                    motors::attack::propose,
                    motors::aim::propose,
                    // Bow release reads the *previous* tick's CombatState
                    // (before arbitration overwrites it): if the player
                    // releases aim on the same tick they press attack,
                    // arbitrate would set Idle before the shoot system ran.
                    motors::aim::shoot_drawn_arrow,
                )
                    .in_set(CombatSet::GatherProposals),
                arbitrate.in_set(CombatSet::Arbitrate),
                (
                    motors::aim::tick_draw_strength,
                    motors::tick_active_motor,
                    motors::attack::sweep_active_swings,
                )
                    .chain()
                    .in_set(CombatSet::TickActiveMotor),
                (motors::attack::resolve_melee_hits, emit_constraints)
                    .in_set(CombatSet::EmitConstraints),
            ),
        );
    }
}

/// `Arbitrate`: pick the winning proposal, write the SSoT `CombatState`,
/// clear the buffer. The only writer of `CombatState`.
fn arbitrate(mut q: Query<(&mut CombatState, &mut CombatProposalBuffer), With<Actor>>) {
    for (mut state, mut buffer) in &mut q {
        let winner = buffer.arbitrate(*state);
        if *state != winner {
            *state = winner;
        }
        buffer.clear();
    }
}

/// `EmitConstraints`: committed combat states forbid sprinting. Movement
/// consumes the message in its own frame (1 tick later, accepted — see
/// `docs/ARCHITECTURE.md`).
fn emit_constraints(
    q: Query<(Entity, &CombatState), With<Actor>>,
    mut writer: MessageWriter<LocomotionConstraintMessage>,
) {
    for (entity, state) in &q {
        if state.commits_the_body() {
            writer.write(LocomotionConstraintMessage::ForbidSprint(entity));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    /// Regression guard for the accepted 1-tick veto window (see the comment
    /// on `emit_constraints`): Combat runs after Movement inside a fixed
    /// tick, so the tick that commits the body still allows sprint; the veto
    /// must land on the *next* tick — never later, and a reordering that
    /// silently widens or removes the window must fail here.
    #[test]
    fn forbid_sprint_veto_lands_exactly_one_tick_after_commitment() {
        use crate::movement::MovementSet;
        use crate::movement::constraints::{
            LocomotionConstraintFacts, LocomotionConstraintMessage, apply_locomotion_constraints,
        };

        let mut app = App::new();
        app.add_message::<LocomotionConstraintMessage>();
        // Production ordering: Movement's sets first, Combat's chained after.
        app.configure_sets(
            FixedUpdate,
            (
                MovementSet::SenseWorld,
                MovementSet::GatherProposals,
                MovementSet::TickActiveMotor,
            )
                .chain(),
        );
        app.configure_sets(
            FixedUpdate,
            CombatSet::EmitConstraints.after(MovementSet::TickActiveMotor),
        );
        app.add_systems(
            FixedUpdate,
            apply_locomotion_constraints
                .after(MovementSet::SenseWorld)
                .before(MovementSet::GatherProposals),
        );
        app.add_systems(
            FixedUpdate,
            emit_constraints.in_set(CombatSet::EmitConstraints),
        );

        let actor = app
            .world_mut()
            .spawn((
                Actor,
                CombatState::Windup,
                LocomotionConstraintFacts::default(),
            ))
            .id();

        // Tick 1: the actor is already committed, but Combat emits at the end
        // of the tick — motors this tick still saw sprint as allowed.
        app.world_mut().run_schedule(FixedUpdate);
        let facts = |app: &App| {
            app.world()
                .entity(actor)
                .get::<LocomotionConstraintFacts>()
                .unwrap()
                .forbid_sprint
        };
        assert!(
            !facts(&app),
            "the commitment tick itself must not yet see the veto (1-tick window)"
        );

        // Tick 2: Movement consumes the message first thing — veto active.
        app.world_mut().run_schedule(FixedUpdate);
        assert!(facts(&app), "the veto must land exactly one tick later");
    }

    #[test]
    fn arbitrate_defaults_to_idle_by_silence() {
        let mut world = World::new();
        let mut buffer = CombatProposalBuffer::default();
        let _ = buffer.push(proposal::TransitionProposal::new(
            CombatState::Idle,
            proposal::Priority::Default,
            proposal::weight::IDLE,
            "idle",
        ));
        let actor = world.spawn((Actor, CombatState::Recovery, buffer)).id();

        world.run_system_once(arbitrate).unwrap();

        assert_eq!(
            *world.entity(actor).get::<CombatState>().unwrap(),
            CombatState::Idle,
            "with only the idle fallback proposing, a finished phase decays to Idle"
        );
    }

    #[test]
    fn committed_states_emit_forbid_sprint_and_idle_does_not() {
        let mut world = World::new();
        world.init_resource::<Messages<LocomotionConstraintMessage>>();
        let committed = world.spawn((Actor, CombatState::Windup)).id();
        world.spawn((Actor, CombatState::Idle));

        world.run_system_once(emit_constraints).unwrap();

        let messages = world.resource::<Messages<LocomotionConstraintMessage>>();
        let mut cursor = messages.get_cursor();
        let emitted: Vec<_> = cursor.read(messages).collect();
        assert_eq!(emitted.len(), 1);
        let LocomotionConstraintMessage::ForbidSprint(entity) = emitted[0];
        assert_eq!(*entity, committed);
    }
}
