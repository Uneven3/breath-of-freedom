//! Ladder motor — climb a ladder, snapping to its anchor rail.
//!
//! Stateless: reads `LadderFacts` each frame.

use avian3d::prelude::*;
use bevy::prelude::*;

use crate::movement::facts::{BodyContact, LadderFacts};
use crate::movement::intents::Intents;
use crate::movement::motor_common::body_move_and_slide;
use crate::movement::proposal::{Priority, ProposalBuffer, TransitionProposal};
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, BodyVelocity};

const CLIMB_SPEED: f32 = 2.5;
const SNAP_LERP: f32 = 12.0;
const TOP_EXIT_BUMP: f32 = 3.0;
const MOVE_DIR_THRESHOLD_SQ: f32 = 0.01;
const TOP_EXIT_CLEARANCE: f32 = 0.1;

pub fn propose(
    mut q: Query<
        (
            &LadderFacts,
            &Intents,
            &LocomotionState,
            &mut ProposalBuffer,
        ),
        With<Actor>,
    >,
) {
    for (ladder, intents, current, mut buffer) in &mut q {
        if !ladder.on_ladder {
            continue;
        }
        // Jump releases the latch (and gates re-entry until Space is let go —
        // otherwise holding a move key would re-latch on the very next frame).
        if intents.wants_jump {
            continue;
        }
        // Latch on once entered; release only when the player jumps or leaves the area.
        if *current == LocomotionState::Ladder
            || intents.move_dir.length_squared() > MOVE_DIR_THRESHOLD_SQ
            || intents.wants_climb
        {
            let _ = buffer.push(TransitionProposal::new(
                LocomotionState::Ladder,
                Priority::Forced,
                0,
                "ladder",
            ));
        }
    }
}

type TickQuery<'a> = (
    Entity,
    &'a Collider,
    &'a mut Transform,
    &'a mut BodyVelocity,
    &'a mut BodyContact,
    &'a Intents,
    &'a LadderFacts,
    &'a LocomotionState,
);

pub fn tick(mut q: Query<TickQuery, With<Actor>>, mas: MoveAndSlide, time: Res<Time>) {
    let dt = time.delta_secs();
    for (entity, collider, mut transform, mut vel, mut contact, intents, ladder, state) in &mut q {
        if *state != LocomotionState::Ladder {
            continue;
        }

        // Forward press → raw_input.y negative → ascend.
        let mut v = Vec3::new(0.0, -intents.raw_input.y * CLIMB_SPEED, 0.0);

        // Snap X/Z onto the ladder anchor rail.
        let t = (SNAP_LERP * dt).clamp(0.0, 1.0);
        transform.translation.x = transform.translation.x.lerp(ladder.anchor_xz.x, t);
        transform.translation.z = transform.translation.z.lerp(ladder.anchor_xz.y, t);

        // Auto-exit bump at the top while still pressing forward.
        if transform.translation.y >= ladder.top_y - TOP_EXIT_CLEARANCE && intents.raw_input.y < 0.0
        {
            v.y = TOP_EXIT_BUMP;
        }

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

#[cfg(test)]
mod tests {
    //! Covers the latch/release rules of `propose`; the rail-snap tick is
    //! play-tested.
    use super::*;
    use crate::movement::Actor;
    use bevy::ecs::system::RunSystemOnce;

    fn proposals(intents: Intents, state: LocomotionState) -> Vec<TransitionProposal> {
        let mut world = World::new();
        let e = world
            .spawn((
                Actor,
                LadderFacts {
                    on_ladder: true,
                    ..default()
                },
                intents,
                state,
                ProposalBuffer::default(),
            ))
            .id();
        world.run_system_once(propose).expect("propose runs");
        world
            .entity(e)
            .get::<ProposalBuffer>()
            .unwrap()
            .iter()
            .cloned()
            .collect()
    }

    #[test]
    fn latched_ladder_keeps_proposing() {
        let out = proposals(Intents::default(), LocomotionState::Ladder);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].target_state, LocomotionState::Ladder);
    }

    #[test]
    fn jump_releases_the_latch() {
        let out = proposals(
            Intents {
                wants_jump: true,
                ..default()
            },
            LocomotionState::Ladder,
        );
        assert!(out.is_empty(), "jump must release the ladder latch");
    }

    #[test]
    fn held_jump_blocks_re_entry() {
        // Falling next to the ladder while still holding Space and a move key
        // must not re-latch (that would undo the jump release instantly).
        let out = proposals(
            Intents {
                wants_jump: true,
                move_dir: Vec2::new(0.0, 1.0),
                ..default()
            },
            LocomotionState::Fall,
        );
        assert!(out.is_empty());
    }
}
