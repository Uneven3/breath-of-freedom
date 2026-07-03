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
