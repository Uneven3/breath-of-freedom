//! TraversalProbe graybox visual.

use bevy::prelude::*;

use super::{INTERPOLATION_SPEED, SNEAK_Y_OFFSET, VisualOf};
use crate::movement::body::BodyDimensions;
use crate::movement::probe_data::TraversalProbe;
use crate::movement::state::LocomotionState;

#[derive(Component)]
pub(super) struct TraversalProbeVisual {
    actor: Entity,
}

type ProbeActorQuery<'a> = (&'a Transform, &'a LocomotionState);
type ProbeActorFilter = (With<TraversalProbe>, Without<TraversalProbeVisual>);
type ProbeVisualQuery<'a> = (&'a mut Transform, &'a TraversalProbeVisual);
type ProbeVisualFilter = (With<TraversalProbeVisual>, Without<TraversalProbe>);

pub(super) fn spawn_probe_visual(
    mut commands: Commands,
    probes: Query<(Entity, &Transform, &BodyDimensions), Added<TraversalProbe>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &probes {
        commands.spawn((
            TraversalProbeVisual { actor },
            VisualOf(actor),
            Name::new("TraversalProbeVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            MeshMaterial3d(materials.add(Color::srgb(0.85, 0.3, 0.25))),
            *transform,
        ));
    }
}

/// Despawn orphaned probe visuals when their actor entity is gone.
pub(super) fn despawn_orphaned_probe_visual(
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

pub(super) fn interpolate_probe_visual(
    actors: Query<ProbeActorQuery, ProbeActorFilter>,
    mut visuals: Query<ProbeVisualQuery, ProbeVisualFilter>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut visual, probe) in &mut visuals {
        let Ok((body, state)) = actors.get(probe.actor) else {
            continue;
        };
        let offset = if *state == LocomotionState::Sneak {
            SNEAK_Y_OFFSET
        } else {
            0.0
        };
        let target_y = body.translation.y + offset;
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual
            .translation
            .y
            .smooth_nudge(&target_y, INTERPOLATION_SPEED, dt);
        visual
            .rotation
            .smooth_nudge(&body.rotation, INTERPOLATION_SPEED, dt);
    }
}
