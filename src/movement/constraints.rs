//! Locomotion constraints requested by other systems (Combat today).
//!
//! Movement owns the message type — the receiver owns the contract, same
//! pattern as `enemies::DirectThreatMessage` — and is the only one who
//! decides the final `LocomotionState`: a constraint makes motors *abstain*,
//! it never writes state. Emitters re-send every tick while the condition
//! holds; the facts reset each tick, so a constraint expires by silence
//! (no release message to forget).

use bevy::prelude::*;

use super::Actor;

/// A semantic restriction on an actor's locomotion. Combat emits these from
/// its committed states (see `docs/ARCHITECTURE.md`);
/// consumed one tick later, in Movement's frame (~16 ms, accepted).
#[derive(Message, Debug, Clone, Copy)]
pub enum LocomotionConstraintMessage {
    /// The actor is committed to an action (attacking, guarding, aiming):
    /// sprint proposals must abstain. `Sneak` is deliberately never
    /// restricted (stealth + attack is the GDD's damage-bonus combo).
    ForbidSprint(Entity),
    // Interrupt(Entity, InterruptKind) llega con `combat-defense` (Staggered).
}

/// Per-actor constraint facts for this tick, derived from the messages.
/// Motors read this like any other fact; only `apply_locomotion_constraints`
/// writes it.
#[derive(Component, Default, Debug, Clone, Copy)]
pub struct LocomotionConstraintFacts {
    pub forbid_sprint: bool,
}

/// A one-shot velocity impulse on an actor's body (knockback on hit).
/// Movement owns the type (receiver owns the contract); Combat emits it. The
/// impulse adds to `BodyVelocity` once and then the active motor's normal
/// friction/acceleration reabsorbs it — a shove, not a state.
#[derive(Message, Debug, Clone, Copy)]
pub struct BodyImpulseMessage {
    pub entity: Entity,
    pub impulse: Vec3,
}

/// Runs alongside `apply_locomotion_constraints`, before motors tick.
pub fn apply_body_impulses(
    mut messages: MessageReader<BodyImpulseMessage>,
    mut actors: Query<
        &mut crate::movement::BodyVelocity,
        (
            With<Actor>,
            With<crate::movement::attachment::LocomotionEnabled>,
        ),
    >,
) {
    for message in messages.read() {
        if let Ok(mut velocity) = actors.get_mut(message.entity) {
            velocity.0 += message.impulse;
        }
    }
}

/// Runs before `MovementSet::GatherProposals`: reset facts, then apply this
/// tick's messages.
pub fn apply_locomotion_constraints(
    mut messages: MessageReader<LocomotionConstraintMessage>,
    mut actors: Query<&mut LocomotionConstraintFacts, With<Actor>>,
) {
    for mut facts in &mut actors {
        *facts = LocomotionConstraintFacts::default();
    }
    for message in messages.read() {
        match *message {
            LocomotionConstraintMessage::ForbidSprint(entity) => {
                if let Ok(mut facts) = actors.get_mut(entity) {
                    facts.forbid_sprint = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forbid_sprint_targets_only_the_addressed_actor_and_expires() {
        let mut world = World::new();
        world.init_resource::<Messages<LocomotionConstraintMessage>>();
        let committed = world
            .spawn((Actor, LocomotionConstraintFacts::default()))
            .id();
        let free = world
            .spawn((Actor, LocomotionConstraintFacts::default()))
            .id();

        // `register_system` (not `run_system_once`) so the MessageReader
        // cursor persists across runs, like in the real schedule.
        let system = world.register_system(apply_locomotion_constraints);

        world.write_message(LocomotionConstraintMessage::ForbidSprint(committed));
        world.run_system(system).unwrap();

        assert!(
            world
                .entity(committed)
                .get::<LocomotionConstraintFacts>()
                .unwrap()
                .forbid_sprint
        );
        assert!(
            !world
                .entity(free)
                .get::<LocomotionConstraintFacts>()
                .unwrap()
                .forbid_sprint,
            "a constraint on one actor must not leak to another"
        );

        // Next tick without a fresh message: the constraint expires by silence.
        world.run_system(system).unwrap();
        assert!(
            !world
                .entity(committed)
                .get::<LocomotionConstraintFacts>()
                .unwrap()
                .forbid_sprint
        );
    }
}
