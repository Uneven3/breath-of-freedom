//! Autonomous integration consumer for the graybox traversal scenario:
//! climb the test wall, mantle its top, turn around, jump off, glide down.
//!
//! This is intentionally a Movement brain, not an enemy implementation: it
//! writes only the probe actor's `Intents` and observes the normal pipeline —
//! every stage advances only when real sensors/arbitration reach its observed
//! condition (historial: `docs/tickets/LOG.md`, probe-mantle-glide).
//!
//! **F6** spawns/despawns the probe on demand at its authored world-space
//! start (facing the graybox test wall), independent of where the player is —
//! the scenario must be reproducible run after run.

use bevy::prelude::*;

use super::abilities::{
    AirborneMovement, ClimbMovement, GlideMovement, GroundMovement, JumpMovement, LadderMovement,
    LedgeTraversal, SneakMovement, SprintMovement, StairsMovement, WallJumpMovement,
};
use super::body::BodyDimensions;
use super::bundles::{
    GlideMovementBundle, GroundMovementBundle, JumpMovementBundle, KinematicActorBundle,
    LadderMovementBundle, LedgeTraversalBundle, SneakMovementBundle, SprintMovementBundle,
    StairsMovementBundle, StaminaBundle, WallJumpMovementBundle,
};
use super::facts::LedgeFacts;
use super::intents::{
    ClimbIntent, ClimbVerticalIntent, GlideIntent, Intents, JumpIntent, PlanarMoveIntent,
    TraversalActionIntent,
};
use super::probe_data::{ProbeCoverage, ProbeScript, ProbeStage, TraversalProbe};
use super::sensing::{GroundSensing, LedgeSensing};
use super::state::LocomotionState;

const STAGE_TIMEOUT_SECS: f32 = 8.0;
const COMPLETE_SETTLE_SECS: f32 = 0.2;
const PROBE_STANDING_CENTER_Y: f32 = 1.125;

/// Authored scenario start: 7 m in front of the graybox test wall (the
/// 10×4×1 box at (0, 2, -10) in `world::setup_world`), facing it down -Z —
/// the direction `ProbeStage::ApproachWall` walks.
const PROBE_SPAWN_POSITION: Vec3 = Vec3::new(0.0, PROBE_STANDING_CENTER_Y, -3.0);

/// F6 toggle: spawns or despawns the traversal probe at its authored start.
pub fn toggle_spawn(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    existing: Query<Entity, With<TraversalProbe>>,
) {
    if !keys.just_pressed(KeyCode::F6) {
        return;
    }

    // If a probe already exists, despawn it (toggle off).
    if let Ok(entity) = existing.single() {
        commands.entity(entity).despawn();
        info!("[debug] TraversalProbe despawned (F6)");
        return;
    }

    let spawn_pos = PROBE_SPAWN_POSITION;

    let dimensions = BodyDimensions {
        radius: 0.45,
        standing_capsule_length: 1.35,
        crouched_capsule_length: 0.85,
    };
    let mut ground = GroundMovement::PLAYER;
    ground.drive.max_forward_speed = 4.0;
    let mut sprint = SprintMovement::PLAYER;
    sprint.drive.max_forward_speed = 7.5;
    let mut sneak = SneakMovement::PLAYER;
    sneak.drive.max_forward_speed = 2.0;

    commands.spawn((
        TraversalProbe,
        Name::new("TraversalProbe"),
        KinematicActorBundle::new(
            Transform::from_translation(spawn_pos),
            dimensions,
            GroundSensing::PLAYER,
        ),
        (
            GroundMovementBundle::new(ground),
            SprintMovementBundle::new(sprint),
            SneakMovementBundle::new(sneak, dimensions),
            StairsMovementBundle::new(StairsMovement::PLAYER),
            StaminaBundle::default(),
            AirborneMovement::PLAYER,
            JumpMovementBundle::new(JumpMovement::PLAYER),
            GlideMovementBundle::new(GlideMovement::PLAYER),
            ClimbMovement::PLAYER,
            LadderMovementBundle::new(LadderMovement::PLAYER),
            LedgeTraversalBundle::new(LedgeTraversal::PLAYER),
            WallJumpMovementBundle::new(WallJumpMovement::PLAYER),
            LedgeSensing::PLAYER,
            ProbeScript::default(),
            ProbeCoverage::default(),
        ),
    ));
    info!(
        "[debug] TraversalProbe spawned at ({:.1}, {:.1}, {:.1}) (F6)",
        spawn_pos.x, spawn_pos.y, spawn_pos.z
    );
}

/// The body counts as turned around once its forward points this close to
/// world +Z (the direction back toward spawn).
const FACING_BACK_MIN_DOT: f32 = 0.9;

type ProbeQuery<'a> = (
    &'a mut Intents,
    &'a LocomotionState,
    &'a LedgeFacts,
    &'a Transform,
    &'a mut ProbeScript,
    &'a mut ProbeCoverage,
);

/// `MovementSet::ReadIntents`: scripted AI -> Intents for the probe only.
pub fn drive_intents(time: Res<Time>, mut q: Query<ProbeQuery, With<TraversalProbe>>) {
    for (mut intents, state, ledge, transform, mut script, mut coverage) in &mut q {
        script.elapsed += time.delta_secs();
        if script.stage == ProbeStage::GlideDown && *state == LocomotionState::Glide {
            script.glide_observed = true;
        }
        *intents = stage_intents(script.stage, *state);

        let settled = script.elapsed >= COMPLETE_SETTLE_SECS;
        let facing_back = (transform.rotation * Vec3::NEG_Z).z > FACING_BACK_MIN_DOT;
        if let Some(next_stage) = next_stage(script.stage, *state, ledge, facing_back, settled) {
            coverage.0 |= stage_bit(script.stage);
            info!(
                "TraversalProbe completed {:?}; advancing to {:?}",
                script.stage, next_stage
            );
            script.stage = next_stage;
            script.elapsed = 0.0;
            script.timeout_reported = false;
        } else if script.stage == ProbeStage::GlideDown
            && !script.completed
            && script.glide_observed
            && *state == LocomotionState::Walk
        {
            coverage.0 |= stage_bit(ProbeStage::GlideDown);
            script.completed = true;
            info!(
                "TraversalProbe full traversal scenario completed (climb→mantle→turn→jump→glide)"
            );
        } else if script.elapsed >= STAGE_TIMEOUT_SECS && !script.timeout_reported {
            script.timeout_reported = true;
            warn!(
                "TraversalProbe stalled in {:?} (state={:?}, can_climb={}, mantle_edge={})",
                script.stage, state, ledge.can_climb, ledge.is_at_mantle_edge,
            );
        }
    }
}

fn stage_intents(stage: ProbeStage, state: LocomotionState) -> Intents {
    match stage {
        // Link-style forward input is negative Y in the camera-relative input
        // contract and maps to world -Z at the probe's spawn rotation.
        ProbeStage::ApproachWall => moving(Vec2::NEG_Y),
        ProbeStage::AttachClimb | ProbeStage::HoldAtLip => Intents {
            climb: ClimbIntent {
                requested: true,
                ..default()
            },
            ..default()
        },
        ProbeStage::AscendClimb => Intents {
            climb: ClimbIntent {
                requested: true,
                vertical: ClimbVerticalIntent::Up,
                ..default()
            },
            ..default()
        },
        // Keep the climb attachment alive while the mantle request arbitrates.
        ProbeStage::MantleOntoTop => Intents {
            climb: ClimbIntent {
                requested: true,
                ..default()
            },
            traversal: TraversalActionIntent::Mantle,
            ..default()
        },
        ProbeStage::SettleOnTop => Intents::default(),
        // Positive Y maps to world +Z: back toward the spawn side. The motors
        // rotate the body toward its move direction; the script only observes.
        ProbeStage::TurnAround => moving(Vec2::Y),
        ProbeStage::JumpOff => Intents {
            planar: PlanarMoveIntent {
                direction: Vec2::Y,
                strength: 1.0,
            },
            jump: JumpIntent {
                held: true,
                pressed: true,
            },
            ..default()
        },
        // Request glide only once airborne in Fall/Glide: `glide::propose`
        // needs the fresh-press edge to land *in* Fall — asking during Jump
        // would spend the edge one state too early.
        ProbeStage::GlideDown => {
            let mut intents = moving(Vec2::Y);
            if matches!(state, LocomotionState::Fall | LocomotionState::Glide) {
                intents.glide = GlideIntent::Requested;
            }
            intents
        }
    }
}

fn moving(direction: Vec2) -> Intents {
    Intents {
        planar: PlanarMoveIntent {
            direction,
            strength: 1.0,
        },
        ..default()
    }
}

fn next_stage(
    stage: ProbeStage,
    state: LocomotionState,
    ledge: &LedgeFacts,
    facing_back: bool,
    settled: bool,
) -> Option<ProbeStage> {
    match stage {
        ProbeStage::ApproachWall if ledge.can_climb => Some(ProbeStage::AttachClimb),
        ProbeStage::AttachClimb if state == LocomotionState::Climb => Some(ProbeStage::AscendClimb),
        ProbeStage::AscendClimb if state == LocomotionState::Climb && ledge.is_at_mantle_edge => {
            Some(ProbeStage::HoldAtLip)
        }
        // The settle preserves the original checkpoint: holding below the lip
        // must not mantle by itself.
        ProbeStage::HoldAtLip if settled && state == LocomotionState::Climb => {
            Some(ProbeStage::MantleOntoTop)
        }
        ProbeStage::MantleOntoTop if state == LocomotionState::Mantle => {
            Some(ProbeStage::SettleOnTop)
        }
        ProbeStage::SettleOnTop if state == LocomotionState::Walk => Some(ProbeStage::TurnAround),
        ProbeStage::TurnAround if state == LocomotionState::Walk && facing_back => {
            Some(ProbeStage::JumpOff)
        }
        ProbeStage::JumpOff if state == LocomotionState::Fall => Some(ProbeStage::GlideDown),
        _ => None,
    }
}

fn stage_bit(stage: ProbeStage) -> u16 {
    1 << (stage as u16)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::movement::{Actor, Player};
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn probe_brain_does_not_overwrite_player_intents() {
        let mut world = World::new();
        world.init_resource::<Time>();
        let player_intents = Intents {
            wants_sprint: true,
            ..default()
        };
        let player = world.spawn((Actor, Player, player_intents)).id();
        let probe = world
            .spawn((
                TraversalProbe,
                Intents::default(),
                LocomotionState::Fall,
                LedgeFacts::default(),
                Transform::default(),
                ProbeScript::default(),
                ProbeCoverage::default(),
            ))
            .id();

        world.run_system_once(drive_intents).unwrap();

        assert!(world.entity(player).get::<Intents>().unwrap().wants_sprint);
        assert_eq!(
            world
                .entity(probe)
                .get::<Intents>()
                .unwrap()
                .planar
                .direction,
            Vec2::NEG_Y
        );
    }

    /// `next_stage` with no turn/settle context (the early climb stages).
    fn advance(
        stage: ProbeStage,
        state: LocomotionState,
        ledge: &LedgeFacts,
    ) -> Option<ProbeStage> {
        next_stage(stage, state, ledge, false, false)
    }

    #[test]
    fn climb_scenario_requires_the_observed_wall_and_climb_state() {
        let facts = LedgeFacts::default();
        assert_eq!(
            advance(ProbeStage::ApproachWall, LocomotionState::Walk, &facts),
            None
        );

        let climbable_wall = LedgeFacts {
            can_climb: true,
            ..default()
        };
        assert_eq!(
            advance(
                ProbeStage::ApproachWall,
                LocomotionState::Walk,
                &climbable_wall
            ),
            Some(ProbeStage::AttachClimb)
        );
        assert_eq!(
            advance(
                ProbeStage::AttachClimb,
                LocomotionState::Walk,
                &climbable_wall
            ),
            None
        );
        assert_eq!(
            advance(
                ProbeStage::AttachClimb,
                LocomotionState::Climb,
                &climbable_wall
            ),
            Some(ProbeStage::AscendClimb)
        );
    }

    #[test]
    fn climb_ascent_never_requests_jump_or_mantle() {
        let intents = stage_intents(ProbeStage::AscendClimb, LocomotionState::Climb);

        assert!(intents.climb.requested);
        assert_eq!(intents.climb.vertical, ClimbVerticalIntent::Up);
        assert!(!intents.jump.held);
        assert!(!intents.jump.pressed);
        assert_eq!(intents.traversal, TraversalActionIntent::None);
    }

    #[test]
    fn hold_at_lip_only_mantles_after_the_settle() {
        let ledge = LedgeFacts::default();
        assert_eq!(
            next_stage(
                ProbeStage::HoldAtLip,
                LocomotionState::Climb,
                &ledge,
                false,
                false
            ),
            None,
            "before the settle the probe must keep holding (no accidental mantle)"
        );
        assert_eq!(
            next_stage(
                ProbeStage::HoldAtLip,
                LocomotionState::Climb,
                &ledge,
                false,
                true
            ),
            Some(ProbeStage::MantleOntoTop)
        );
    }

    #[test]
    fn turn_around_advances_only_when_grounded_and_facing_back() {
        let ledge = LedgeFacts::default();
        assert_eq!(
            next_stage(
                ProbeStage::TurnAround,
                LocomotionState::Walk,
                &ledge,
                false,
                true
            ),
            None,
            "still mid-turn"
        );
        assert_eq!(
            next_stage(
                ProbeStage::TurnAround,
                LocomotionState::Fall,
                &ledge,
                true,
                true
            ),
            None,
            "walked off the edge mid-turn: no advance, timeout will report it"
        );
        assert_eq!(
            next_stage(
                ProbeStage::TurnAround,
                LocomotionState::Walk,
                &ledge,
                true,
                false
            ),
            Some(ProbeStage::JumpOff)
        );
    }

    #[test]
    fn jump_off_advances_once_airborne() {
        let ledge = LedgeFacts::default();
        assert_eq!(
            next_stage(
                ProbeStage::JumpOff,
                LocomotionState::Jump,
                &ledge,
                true,
                true
            ),
            None,
            "the jump impulse frame is not yet the fall"
        );
        assert_eq!(
            next_stage(
                ProbeStage::JumpOff,
                LocomotionState::Fall,
                &ledge,
                true,
                true
            ),
            Some(ProbeStage::GlideDown)
        );
    }

    #[test]
    fn glide_is_requested_only_while_airborne() {
        assert_eq!(
            stage_intents(ProbeStage::GlideDown, LocomotionState::Jump).glide,
            GlideIntent::Inactive,
            "requesting during Jump would spend the fresh-press edge too early"
        );
        assert_eq!(
            stage_intents(ProbeStage::GlideDown, LocomotionState::Fall).glide,
            GlideIntent::Requested
        );
        assert_eq!(
            stage_intents(ProbeStage::GlideDown, LocomotionState::Glide).glide,
            GlideIntent::Requested,
            "keep holding the request while gliding"
        );
    }

    #[test]
    fn mantle_stage_requests_mantle_while_keeping_the_climb() {
        let intents = stage_intents(ProbeStage::MantleOntoTop, LocomotionState::Climb);
        assert!(intents.climb.requested);
        assert_eq!(intents.traversal, TraversalActionIntent::Mantle);
        assert!(
            !intents.jump.held,
            "the mantle is an explicit request, not a jump"
        );
    }

    #[test]
    fn stage_bits_are_distinct() {
        let stages = [
            ProbeStage::ApproachWall,
            ProbeStage::AttachClimb,
            ProbeStage::AscendClimb,
            ProbeStage::HoldAtLip,
            ProbeStage::MantleOntoTop,
            ProbeStage::SettleOnTop,
            ProbeStage::TurnAround,
            ProbeStage::JumpOff,
            ProbeStage::GlideDown,
        ];
        let mut seen: u16 = 0;
        for stage in stages {
            let bit = stage_bit(stage);
            assert_eq!(seen & bit, 0, "duplicate coverage bit for {stage:?}");
            seen |= bit;
        }
    }
}
