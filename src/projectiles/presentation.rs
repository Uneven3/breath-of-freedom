//! Disposable arrow meshes and trail particles. Simulation is read-only here.

use bevy::prelude::*;

use super::data::{Arrow, ArrowTrailMessage};
use crate::visuals::VisualOf;

const TRAIL_TTL_SECS: f32 = 0.28;
const TRAIL_PARTICLE_SIZE: f32 = 0.04;

#[derive(Component)]
pub(super) struct TrailParticle {
    remaining: f32,
}

#[derive(Resource)]
pub(super) struct ArrowAssets {
    arrow_mesh: Handle<Mesh>,
    arrow_material: Handle<StandardMaterial>,
    trail_mesh: Handle<Mesh>,
    trail_material: Handle<StandardMaterial>,
}

pub(super) fn init_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.insert_resource(ArrowAssets {
        arrow_mesh: meshes.add(Cuboid::new(0.05, 0.05, 0.55)),
        arrow_material: materials.add(StandardMaterial {
            base_color: Color::srgb(0.9, 0.85, 0.7),
            unlit: true,
            ..default()
        }),
        trail_mesh: meshes.add(Sphere::new(TRAIL_PARTICLE_SIZE)),
        trail_material: materials.add(StandardMaterial {
            base_color: Color::srgba(1.0, 0.95, 0.7, 0.8),
            unlit: true,
            ..default()
        }),
    });
}

pub(super) fn sync_visuals(
    mut commands: Commands,
    assets: Res<ArrowAssets>,
    arrows: Query<(Entity, &Arrow, &Transform)>,
    mut visuals: Query<(Entity, &VisualOf, &mut Transform), Without<Arrow>>,
) {
    for (arrow_entity, arrow, arrow_transform) in &arrows {
        let existing = visuals
            .iter_mut()
            .find(|(_, owner, _)| owner.0 == arrow_entity);
        match (arrow.active, existing) {
            (true, Some((_, _, mut visual_transform))) => {
                *visual_transform = *arrow_transform;
            }
            (true, None) => {
                commands.spawn((
                    Name::new("ArrowVisual"),
                    VisualOf(arrow_entity),
                    Mesh3d(assets.arrow_mesh.clone()),
                    MeshMaterial3d(assets.arrow_material.clone()),
                    *arrow_transform,
                ));
            }
            (false, Some((visual, _, _))) => {
                commands.entity(visual).despawn();
            }
            (false, None) => {}
        }
    }
}

pub(super) fn spawn_trails(
    mut commands: Commands,
    mut trails: MessageReader<ArrowTrailMessage>,
    assets: Res<ArrowAssets>,
) {
    for trail in trails.read() {
        commands.spawn((
            TrailParticle {
                remaining: TRAIL_TTL_SECS,
            },
            Mesh3d(assets.trail_mesh.clone()),
            MeshMaterial3d(assets.trail_material.clone()),
            Transform::from_translation(trail.0),
        ));
    }
}

pub(super) fn tick_trails(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut TrailParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform) in &mut particles {
        particle.remaining -= dt;
        if particle.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.scale = Vec3::splat((particle.remaining / TRAIL_TTL_SECS).max(0.01));
    }
}
