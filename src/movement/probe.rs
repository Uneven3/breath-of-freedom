//! Autonomous integration consumer for the focused graybox climb scenario.
//!
//! This is intentionally a Movement brain, not an enemy implementation: it
//! writes only the probe actor's `Intents` and observes the normal pipeline.
//!
//! **F6** spawns/despawns the probe on demand near the player's position.

use bevy::prelude::*;

use super::Player;
use super::abilities::{
    AirborneMovement, ClimbMovement, GlideMovement, GroundMovement, JumpMovement, LadderMovement,
    LedgeTraversal, WallJumpMovement,
};
use super::body::BodyDimensions;
use super::bundles::{
    GlideMovementBundle, GroundMovementBundle, JumpMovementBundle, KinematicActorBundle,
    LedgeTraversalBundle, WallJumpMovementBundle,
};
use super::facts::LedgeFacts;
use super::intents::{ClimbIntent, ClimbVerticalIntent, Intents, PlanarMoveIntent};
use super::probe_data::{ProbeCoverage, ProbeScript, ProbeStage, TraversalProbe};
use super::sensing::{GroundSensing, LedgeSensing};
use super::state::LocomotionState;

const STAGE_TIMEOUT_SECS: f32 = 8.0;
const COMPLETE_SETTLE_SECS: f32 = 0.2;
const PROBE_STANDING_CENTER_Y: f32 = 1.125;

/// F6 toggle: spawns or despawns the traversal probe near the player.
pub fn toggle_spawn(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    player: Option<Single<&Transform, With<Player>>>,
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

    // Spawn near the player: 3 m behind their facing direction.
    let spawn_pos = if let Some(player_tf) = player {
        let behind = player_tf.rotation * Vec3::Z * 3.0;
        let mut pos = player_tf.translation + behind;
        pos.y = PROBE_STANDING_CENTER_Y;
        pos
    } else {
        Vec3::new(0.0, PROBE_STANDING_CENTER_Y, -3.0)
    };

    let dimensions = BodyDimensions {
        radius: 0.45,
        standing_capsule_length: 1.35,
        crouched_capsule_length: 0.85,
    };
    let mut ground = GroundMovement::PLAYER;
    ground.walk.max_speed = 4.0;
    ground.sprint.max_speed = 7.5;
    ground.sneak.max_speed = 2.0;

    commands.spawn((
        TraversalProbe,
        Name::new("TraversalProbe"),
        KinematicActorBundle::new(
            Transform::from_translation(spawn_pos),
            dimensions,
            GroundSensing::PLAYER,
        ),
        (
            GroundMovementBundle::new(ground, dimensions),
            AirborneMovement::PLAYER,
            JumpMovementBundle::new(JumpMovement::PLAYER),
            GlideMovementBundle::new(GlideMovement::PLAYER),
            ClimbMovement::PLAYER,
            LadderMovement::PLAYER,
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

type ProbeQuery<'a> = (
    &'a mut Intents,
    &'a LocomotionState,
    &'a LedgeFacts,
    &'a mut ProbeScript,
    &'a mut ProbeCoverage,
);

/// `MovementSet::ReadIntents`: scripted AI -> Intents for the probe only.
pub fn drive_intents(time: Res<Time>, mut q: Query<ProbeQuery, With<TraversalProbe>>) {
    for (mut intents, state, ledge, mut script, mut coverage) in &mut q {
        script.elapsed += time.delta_secs();
        *intents = stage_intents(script.stage);

        if let Some(next_stage) = next_stage(script.stage, *state, ledge) {
            coverage.0 |= stage_bit(script.stage);
            info!(
                "TraversalProbe completed {:?}; advancing to {:?}",
                script.stage, next_stage
            );
            script.stage = next_stage;
            script.elapsed = 0.0;
            script.timeout_reported = false;
        } else if script.stage == ProbeStage::HoldAtLip
            && !script.completed
            && script.elapsed >= COMPLETE_SETTLE_SECS
            && *state == LocomotionState::Climb
        {
            coverage.0 |= stage_bit(ProbeStage::HoldAtLip);
            script.completed = true;
            info!("TraversalProbe climb scenario completed without mantle");
        } else if script.elapsed >= STAGE_TIMEOUT_SECS && !script.timeout_reported {
            script.timeout_reported = true;
            warn!(
                "TraversalProbe stalled in {:?} (state={:?}, can_climb={}, mantle_edge={})",
                script.stage, state, ledge.can_climb, ledge.is_at_mantle_edge,
            );
        }
    }
}

fn stage_intents(stage: ProbeStage) -> Intents {
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

fn next_stage(stage: ProbeStage, state: LocomotionState, ledge: &LedgeFacts) -> Option<ProbeStage> {
    match stage {
        ProbeStage::ApproachWall if ledge.can_climb => Some(ProbeStage::AttachClimb),
        ProbeStage::AttachClimb if state == LocomotionState::Climb => Some(ProbeStage::AscendClimb),
        ProbeStage::AscendClimb if state == LocomotionState::Climb && ledge.is_at_mantle_edge => {
            Some(ProbeStage::HoldAtLip)
        }
        _ => None,
    }
}

fn stage_bit(stage: ProbeStage) -> u8 {
    1 << (stage as u8)
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
            gait: crate::movement::intents::GaitIntent::Sprint,
            ..default()
        };
        let player = world.spawn((Actor, Player, player_intents)).id();
        let probe = world
            .spawn((
                TraversalProbe,
                Intents::default(),
                LocomotionState::Fall,
                LedgeFacts::default(),
                ProbeScript::default(),
                ProbeCoverage::default(),
            ))
            .id();

        world.run_system_once(drive_intents).unwrap();

        assert_eq!(
            world.entity(player).get::<Intents>().unwrap().gait,
            crate::movement::intents::GaitIntent::Sprint
        );
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

    #[test]
    fn climb_scenario_requires_the_observed_wall_and_climb_state() {
        let facts = LedgeFacts::default();
        assert_eq!(
            next_stage(ProbeStage::ApproachWall, LocomotionState::Walk, &facts),
            None
        );

        let climbable_wall = LedgeFacts {
            can_climb: true,
            ..default()
        };
        assert_eq!(
            next_stage(
                ProbeStage::ApproachWall,
                LocomotionState::Walk,
                &climbable_wall
            ),
            Some(ProbeStage::AttachClimb)
        );
        assert_eq!(
            next_stage(
                ProbeStage::AttachClimb,
                LocomotionState::Walk,
                &climbable_wall
            ),
            None
        );
        assert_eq!(
            next_stage(
                ProbeStage::AttachClimb,
                LocomotionState::Climb,
                &climbable_wall
            ),
            Some(ProbeStage::AscendClimb)
        );
    }

    #[test]
    fn climb_ascent_never_requests_jump_or_mantle() {
        let intents = stage_intents(ProbeStage::AscendClimb);

        assert!(intents.climb.requested);
        assert_eq!(intents.climb.vertical, ClimbVerticalIntent::Up);
        assert!(!intents.jump.held);
        assert!(!intents.jump.pressed);
        assert_eq!(
            intents.traversal,
            crate::movement::intents::TraversalActionIntent::None
        );
    }
}
