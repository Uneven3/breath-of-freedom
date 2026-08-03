use super::state::LocomotionState;

pub use crate::proposal::Priority;

pub const MOVEMENT_PROPOSAL_CAPACITY: usize = 32;

pub type TransitionProposal = crate::proposal::TransitionProposal<LocomotionState>;
pub type ProposalBuffer =
    crate::proposal::ProposalBuffer<LocomotionState, MOVEMENT_PROPOSAL_CAPACITY>;

/// Tie-break weights for every motor, in one place.
///
/// Arbitration compares `(Priority, weight)`, so a weight only matters against
/// proposals in the **same** category; an exact tie falls back to system
/// execution order, which Bevy does not guarantee. Any two motors that can
/// co-propose in the same category must therefore differ here — keeping the
/// full table in one module makes that total order auditable at a glance
/// (`arbitration_matrix::co_proposing_motors_never_tie_on_priority_and_weight`
/// pins it, allowing an explicit list of mutually-exclusive exceptions).
pub mod weight {
    /// Default: WALK beats FALL — a grounded actor prefers walking.
    pub const FALL: u32 = 0;
    pub const WALK: u32 = 1;

    /// PlayerRequested: a climb start beats gaits and a glide hand-off
    /// (`glide` downgrades to PlayerRequested on a climbable wall exactly so
    /// CLIMB wins); an explicit vault beats everything else requested.
    pub const GLIDE: u32 = 0;
    pub const SNEAK: u32 = 1;
    pub const SPRINT: u32 = 2;
    pub const CLIMB: u32 = 5;
    pub const AUTO_VAULT: u32 = 20;

    /// Forced: JUMP must beat the sticky STAIRS/LADDER holds; the specialized
    /// climb-state launches (WALL_JUMP, and the more deliberate EDGE_LEAP and
    /// MANTLE) outrank a plain jump; EDGE_LEAP beats MANTLE because it
    /// requires explicit lateral input at an open edge, while MANTLE also
    /// fires on a bare jump at the lip. A running AUTO_VAULT arc yields to
    /// nothing.
    pub const STAIRS: u32 = 0;
    pub const LADDER: u32 = 0;
    pub const JUMP: u32 = 1;
    pub const WALL_JUMP: u32 = 5;
    pub const MANTLE: u32 = 10;
    pub const EDGE_LEAP: u32 = 11;

    // Compile-time pin of the per-category total order between motors that
    // can co-propose. An accidental reorder fails the build, not a play test.
    const _: () = {
        // Default category.
        assert!(WALK > FALL);
        // PlayerRequested category.
        assert!(SNEAK > GLIDE);
        assert!(SPRINT > SNEAK);
        assert!(CLIMB > SPRINT);
        assert!(AUTO_VAULT > CLIMB);
        // Forced category.
        assert!(JUMP > STAIRS);
        assert!(JUMP > LADDER);
        assert!(WALL_JUMP > JUMP);
        assert!(MANTLE > WALL_JUMP);
        assert!(EDGE_LEAP > MANTLE);
        assert!(AUTO_VAULT > EDGE_LEAP);
    };
}

/// The whole arbitration surface, as a build-checked table.
///
/// Correctness of `arbitrate` depends on an N×N "who beats whom" matrix that
/// used to live only in a reviewer's head. This module writes it down as data
/// (one row per motor) and asserts the two properties that keep it sound as the
/// motor count grows (swim, dive, combat): every `LocomotionState` is owned by
/// exactly one motor, and no two motors that could co-propose share a
/// `(Priority, weight)` key — a tie would be broken by Bevy's unspecified
/// system order (the bug already fixed for jump-vs-stairs and edge_leap-vs-mantle).
#[cfg(test)]
mod arbitration_matrix {
    use super::{LocomotionState, ProposalBuffer, TransitionProposal, weight};
    use crate::proposal::Priority::{self, *};
    use LocomotionState::*;

    /// One motor: the state it owns (its tick self-gates on this) and every
    /// `(Priority, weight)` its `propose` can push. **Keep in sync with each
    /// motor's `propose`; a new motor MUST add a row.** These are the exact
    /// values grepped from every `TransitionProposal::new` call site.
    struct Motor {
        name: &'static str,
        state: LocomotionState,
        emits: &'static [(Priority, u32)],
    }

    const MATRIX: &[Motor] = &[
        Motor {
            name: "walk",
            state: Walk,
            emits: &[(Default, weight::WALK)],
        },
        Motor {
            name: "fall",
            state: Fall,
            emits: &[(Default, weight::FALL)],
        },
        Motor {
            name: "sprint",
            state: Sprint,
            emits: &[(PlayerRequested, weight::SPRINT)],
        },
        Motor {
            name: "sneak",
            state: Sneak,
            emits: &[(PlayerRequested, weight::SNEAK)],
        },
        // glide continues in the air as Forced, but downgrades to PlayerRequested
        // on a climbable wall so Climb out-arbitrates it.
        Motor {
            name: "glide",
            state: Glide,
            emits: &[(PlayerRequested, weight::GLIDE), (Forced, weight::GLIDE)],
        },
        // climb starts as a fresh request, then holds as a Continuation.
        Motor {
            name: "climb",
            state: Climb,
            emits: &[
                (PlayerRequested, weight::CLIMB),
                (Continuation, weight::CLIMB),
            ],
        },
        // a vault starts on request and, once launched, its arc is Forced.
        Motor {
            name: "auto_vault",
            state: AutoVault,
            emits: &[
                (PlayerRequested, weight::AUTO_VAULT),
                (Forced, weight::AUTO_VAULT),
            ],
        },
        Motor {
            name: "jump",
            state: Jump,
            emits: &[(Forced, weight::JUMP)],
        },
        Motor {
            name: "wall_jump",
            state: WallJump,
            emits: &[(Forced, weight::WALL_JUMP)],
        },
        Motor {
            name: "mantle",
            state: Mantle,
            emits: &[(Forced, weight::MANTLE)],
        },
        Motor {
            name: "edge_leap",
            state: EdgeLeap,
            emits: &[(Forced, weight::EDGE_LEAP)],
        },
        Motor {
            name: "stairs",
            state: Stairs,
            emits: &[(Forced, weight::STAIRS)],
        },
        Motor {
            name: "ladder",
            state: Ladder,
            emits: &[(Forced, weight::LADDER)],
        },
    ];

    /// Motor pairs that legitimately share a `(Priority, weight)` because their
    /// gating facts are mutually exclusive, so they can never co-propose on the
    /// same actor in the same frame. Each entry is a promise the pipeline is not
    /// otherwise proving — justify it before adding one:
    /// - `glide`/`stairs`, `glide`/`ladder`: glide's Forced emission needs the
    ///   actor airborne with `state == Glide`; stairs/ladder need the actor
    ///   grounded (or already sticky in their own state) on authored geometry.
    /// - `stairs`/`ladder`: an actor is never snapped to an authored staircase
    ///   and attached to a ladder at the same time (distinct authored volumes).
    const EXCLUSIVE: &[(&str, &str)] = &[
        ("glide", "stairs"),
        ("glide", "ladder"),
        ("stairs", "ladder"),
    ];

    fn declared_exclusive(a: &str, b: &str) -> bool {
        EXCLUSIVE
            .iter()
            .any(|&(x, y)| (x == a && y == b) || (x == b && y == a))
    }

    #[test]
    fn every_state_is_owned_by_exactly_one_motor() {
        for state in LocomotionState::ALL {
            let owners: Vec<_> = MATRIX
                .iter()
                .filter(|m| m.state == state)
                .map(|m| m.name)
                .collect();
            assert_eq!(
                owners.len(),
                1,
                "{state:?} must be owned by exactly one motor, got {owners:?}"
            );
        }
        assert_eq!(
            MATRIX.len(),
            LocomotionState::ALL.len(),
            "a MATRIX row owns a state missing from LocomotionState::ALL"
        );
    }

    #[test]
    fn co_proposing_motors_never_tie_on_priority_and_weight() {
        let emissions: Vec<(Priority, u32, &str)> = MATRIX
            .iter()
            .flat_map(|m| m.emits.iter().map(move |&(cat, w)| (cat, w, m.name)))
            .collect();

        for (i, &(cat_a, w_a, a)) in emissions.iter().enumerate() {
            for &(cat_b, w_b, b) in &emissions[i + 1..] {
                if a == b || cat_a != cat_b || w_a != w_b {
                    continue;
                }
                assert!(
                    declared_exclusive(a, b),
                    "{a} and {b} both emit ({cat_a:?}, {w_a}); on an actor where both \
                     apply, arbitration is a tie broken by Bevy's unspecified system \
                     order. Give one a distinct weight in `movement::proposal::weight`, \
                     or add ({a:?}, {b:?}) to EXCLUSIVE with a justification."
                );
            }
        }
    }

    #[test]
    fn matrix_emissions_match_the_pinned_arbitration_outcomes() {
        // Spot-check that the table's keys reproduce the two order-independent
        // outcomes already asserted against the real motors elsewhere.
        let key = |name: &str| -> (Priority, u32) {
            let m = MATRIX.iter().find(|m| m.name == name).unwrap();
            *m.emits.last().unwrap()
        };
        assert!(key("jump") > key("stairs"), "jump must beat stairs");
        assert!(
            key("edge_leap") > key("mantle"),
            "edge_leap must beat mantle"
        );
    }

    #[test]
    fn arbitrate_is_deterministic_across_emitted_keys() {
        // The generic buffer must pick the same winner regardless of push order
        // for every pair of distinct keys the matrix can emit.
        let emissions: Vec<(Priority, u32)> = MATRIX
            .iter()
            .flat_map(|m| m.emits.iter().copied())
            .collect();
        for &(cat_a, w_a) in &emissions {
            for &(cat_b, w_b) in &emissions {
                if (cat_a, w_a) == (cat_b, w_b) {
                    continue;
                }
                let mut forward = ProposalBuffer::default();
                forward
                    .push(TransitionProposal::new(Walk, cat_a, w_a, "a"))
                    .unwrap();
                forward
                    .push(TransitionProposal::new(Sprint, cat_b, w_b, "b"))
                    .unwrap();
                let mut backward = ProposalBuffer::default();
                backward
                    .push(TransitionProposal::new(Sprint, cat_b, w_b, "b"))
                    .unwrap();
                backward
                    .push(TransitionProposal::new(Walk, cat_a, w_a, "a"))
                    .unwrap();
                assert_eq!(
                    forward.arbitrate(Fall),
                    backward.arbitrate(Fall),
                    "({cat_a:?},{w_a}) vs ({cat_b:?},{w_b}) depends on push order"
                );
            }
        }
    }
}
