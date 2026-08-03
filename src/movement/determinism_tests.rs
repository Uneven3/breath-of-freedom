use std::time::Duration;

use avian3d::prelude::*;
use bevy::prelude::*;
use bevy::time::TimeUpdateStrategy;

use super::abilities::{
    AirborneMovement, GroundMovement, JumpMovement, JumpStaminaCost, SprintMovement,
};
use super::body::BodyDimensions;
use super::bundles::{
    GroundMovementBundle, JumpMovementBundle, KinematicActorBundle, SprintMovementBundle,
    StaminaBundle,
};
use super::intents::{Intents, JumpIntent, PlanarMoveIntent};
use super::sensing::GroundSensing;
use super::state::LocomotionState;
use super::{ActorId, BodyVelocity, MovementPlugin};

const TICKS: u32 = 120;
const FIXED_STEP: Duration = Duration::from_nanos(16_666_667);
const ACTOR_A: ActorId = ActorId::authored(20_001);
const ACTOR_B: ActorId = ActorId::authored(20_002);

#[derive(Clone, Copy, Debug, PartialEq)]
struct ActorSnapshot {
    id: ActorId,
    translation: Vec3,
    rotation: Quat,
    velocity: Vec3,
    locomotion: LocomotionState,
}

fn headless_app() -> App {
    let mut app = App::new();
    app.add_plugins((
        MinimalPlugins,
        TransformPlugin,
        PhysicsPlugins::default(),
        bevy::asset::AssetPlugin::default(),
        bevy::mesh::MeshPlugin,
    ));
    app.insert_resource(TimeUpdateStrategy::ManualDuration(FIXED_STEP));
    app.init_resource::<crate::input::frame::ActiveActions>();
    app.add_plugins(MovementPlugin);
    app.finish();
    app
}

fn spawn_actor(app: &mut App, id: ActorId, position: Vec3) {
    app.world_mut().spawn((
        KinematicActorBundle::new(
            id,
            Transform::from_translation(position),
            BodyDimensions::PLAYER,
            GroundSensing::PLAYER,
        ),
        GroundMovementBundle::new(GroundMovement::PLAYER),
        SprintMovementBundle::new(SprintMovement::PLAYER),
        StaminaBundle::default(),
        AirborneMovement::PLAYER,
        JumpMovementBundle::new(JumpMovement::PLAYER),
        JumpStaminaCost(20.0),
    ));
}

fn scripted_intents(seed: u32, actor: ActorId, tick: u32) -> Intents {
    let mirrored = (seed ^ actor.value()).is_multiple_of(2);
    let direction = if mirrored {
        Vec2::new(0.8, -0.6)
    } else {
        Vec2::new(-0.8, -0.6)
    };
    let jumping = (60..68).contains(&tick);
    Intents {
        planar: PlanarMoveIntent {
            direction,
            local: Vec2::new(0.0, -1.0),
            strength: 1.0,
        },
        wants_sprint: (30..60).contains(&tick),
        jump: JumpIntent {
            held: jumping,
            pressed: tick == 60,
        },
        ..default()
    }
}

fn snapshot(app: &mut App) -> Vec<ActorSnapshot> {
    let world = app.world_mut();
    let mut query = world.query::<(&ActorId, &Transform, &BodyVelocity, &LocomotionState)>();
    let mut states: Vec<_> = query
        .iter(world)
        .map(|(id, transform, velocity, locomotion)| ActorSnapshot {
            id: *id,
            translation: transform.translation,
            rotation: transform.rotation,
            velocity: velocity.0,
            locomotion: *locomotion,
        })
        .collect();
    states.sort_by_key(|state| state.id);
    states
}

fn replay(seed: u32, reverse_spawn_order: bool, dummy_entities: usize) -> Vec<Vec<ActorSnapshot>> {
    let mut app = headless_app();
    for _ in 0..dummy_entities {
        app.world_mut().spawn_empty();
    }
    app.world_mut().spawn((
        Transform::from_xyz(0.0, -0.5, 0.0),
        RigidBody::Static,
        Collider::cuboid(40.0, 0.5, 40.0),
    ));

    let actors = [
        (ACTOR_A, Vec3::new(-4.0, 1.0, 0.0)),
        (ACTOR_B, Vec3::new(4.0, 1.0, 0.0)),
    ];
    if reverse_spawn_order {
        for (id, position) in actors.into_iter().rev() {
            spawn_actor(&mut app, id, position);
        }
    } else {
        for (id, position) in actors {
            spawn_actor(&mut app, id, position);
        }
    }

    let mut timeline = Vec::with_capacity(TICKS as usize);
    for tick in 0..TICKS {
        let world = app.world_mut();
        let mut query = world.query::<(&ActorId, &mut Intents)>();
        for (id, mut intents) in query.iter_mut(world) {
            *intents = scripted_intents(seed, *id, tick);
        }
        app.update();
        timeline.push(snapshot(&mut app));
    }
    timeline
}

#[test]
fn same_scene_and_seed_replay_identically_for_n_ticks() {
    let first = replay(0xB0F0_5EED, false, 0);
    let reordered = replay(0xB0F0_5EED, true, 17);

    assert_eq!(first, reordered);
    assert_eq!(first.len(), TICKS as usize);
    assert!(
        first.iter().any(|tick| {
            tick.iter()
                .any(|actor| actor.locomotion == LocomotionState::Sprint)
        }),
        "the scenario must exercise more than the idle fallback"
    );
    assert_ne!(
        first,
        replay(0xB0F0_5EEE, true, 3),
        "the authored seed must affect the scripted scenario"
    );
}
