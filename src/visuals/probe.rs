//! TraversalProbe graybox visual.

use bevy::prelude::*;

use super::{INTERPOLATION_SPEED, SNEAK_Y_OFFSET, VisualOf};
use crate::asset_pipeline::MaterialPalette;
use bof_simulation::movement::body::BodyDimensions;
use bof_simulation::movement::probe_data::TraversalProbe;
use bof_simulation::movement::state::LocomotionState;

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
    palette: Res<MaterialPalette>,
) {
    for (actor, transform, body) in &probes {
        commands.spawn((
            TraversalProbeVisual { actor },
            VisualOf(actor),
            Name::new("TraversalProbeVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            MeshMaterial3d(palette.handle("Probe")),
            *transform,
        ));
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
