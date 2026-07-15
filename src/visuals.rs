//! Player visual mesh, decoupled from the physics body.
//!
//! A standalone mesh entity interpolates toward the kinematic body each render
//! frame, which (a) smooths the 60 Hz fixed-step motion on high-refresh
//! displays and (b) dips −0.4 in Sneak so crouching reads visually even though
//! the collider, not the mesh, is what actually shrinks.

use bevy::prelude::*;

use crate::movement::Player;
use crate::movement::body::BodyDimensions;
use crate::movement::probe_data::TraversalProbe;
use crate::movement::state::LocomotionState;

const INTERPOLATION_SPEED: f32 = 20.0;
const SNEAK_Y_OFFSET: f32 = -0.4;

#[derive(Component)]
struct PlayerVisual;

#[derive(Component)]
struct TraversalProbeVisual {
    actor: Entity,
}

type ProbeActorQuery<'a> = (&'a Transform, &'a LocomotionState);
type ProbeActorFilter = (With<TraversalProbe>, Without<TraversalProbeVisual>);
type ProbeVisualQuery<'a> = (&'a mut Transform, &'a TraversalProbeVisual);
type ProbeVisualFilter = (With<TraversalProbeVisual>, Without<TraversalProbe>);

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_visual);
        app.add_systems(
            Update,
            (
                spawn_probe_visual,
                despawn_orphaned_probe_visual,
                interpolate_visual,
                interpolate_probe_visual,
            ),
        );
    }
}

fn spawn_visual(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        PlayerVisual,
        Name::new("PlayerVisual"),
        Mesh3d(meshes.add(Capsule3d::new(
            BodyDimensions::PLAYER.radius,
            BodyDimensions::PLAYER.standing_capsule_length,
        ))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.6, 0.9))),
        Transform::from_xyz(0.0, 1.5, 0.0),
    ));
}

type BodyFilter = (With<Player>, Without<PlayerVisual>);

fn interpolate_visual(
    player: Single<(&Transform, &LocomotionState), BodyFilter>,
    mut visual: Single<&mut Transform, With<PlayerVisual>>,
    time: Res<Time>,
) {
    let (body, state) = *player;
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    let offset = if *state == LocomotionState::Sneak {
        SNEAK_Y_OFFSET
    } else {
        0.0
    };

    // X/Z track the body directly; Y and rotation interpolate.
    let target_y = body.translation.y + offset;
    visual.translation.x = body.translation.x;
    visual.translation.z = body.translation.z;
    visual.translation.y += (target_y - visual.translation.y) * t;
    visual.rotation = visual.rotation.slerp(body.rotation, t);
}

fn spawn_probe_visual(
    mut commands: Commands,
    probes: Query<(Entity, &Transform, &BodyDimensions), Added<TraversalProbe>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &probes {
        commands.spawn((
            TraversalProbeVisual { actor },
            Name::new("TraversalProbeVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            MeshMaterial3d(materials.add(Color::srgb(0.85, 0.3, 0.25))),
            *transform,
        ));
    }
}

/// Despawn orphaned probe visuals when their actor entity is gone.
fn despawn_orphaned_probe_visual(
    mut commands: Commands,
    visuals: Query<(Entity, &TraversalProbeVisual)>,
    actors: Query<(), With<TraversalProbe>>,
) {
    for (vis_entity, probe_vis) in &visuals {
        if actors.get(probe_vis.actor).is_err() {
            commands.entity(vis_entity).despawn();
        }
    }
}

fn interpolate_probe_visual(
    actors: Query<ProbeActorQuery, ProbeActorFilter>,
    mut visuals: Query<ProbeVisualQuery, ProbeVisualFilter>,
    time: Res<Time>,
) {
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    for (mut visual, probe) in &mut visuals {
        let Ok((body, state)) = actors.get(probe.actor) else {
            continue;
        };
        let offset = if *state == LocomotionState::Sneak {
            SNEAK_Y_OFFSET
        } else {
            0.0
        };
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual.translation.y += (body.translation.y + offset - visual.translation.y) * t;
        visual.rotation = visual.rotation.slerp(body.rotation, t);
    }
}
