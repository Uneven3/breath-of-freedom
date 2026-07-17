//! Enemy graybox visuals: capsule + awareness tint.

use bevy::prelude::*;

use super::{INTERPOLATION_SPEED, VisualOf};
use crate::enemies::Enemy;
use crate::enemies::perception::Awareness;
use crate::movement::body::BodyDimensions;

#[derive(Component)]
pub(super) struct EnemyVisual {
    actor: Entity,
}

/// Graybox awareness feedback (the "?/!" stand-in): calm, suspicious, alerted.
const ENEMY_CALM_COLOR: Color = Color::srgb(0.45, 0.2, 0.55);
const ENEMY_SUSPICIOUS_COLOR: Color = Color::srgb(0.9, 0.6, 0.1);
const ENEMY_ALERTED_COLOR: Color = Color::srgb(0.85, 0.12, 0.08);

pub(super) fn spawn_enemy_visual(
    mut commands: Commands,
    enemies: Query<(Entity, &Transform, &BodyDimensions), Added<Enemy>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &enemies {
        commands.spawn((
            EnemyVisual { actor },
            VisualOf(actor),
            Name::new("EnemyVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            // Per-enemy material instance: `tint_enemy_visual` mutates it.
            MeshMaterial3d(materials.add(ENEMY_CALM_COLOR)),
            *transform,
        ));
    }
}

/// Tint each enemy capsule by its `Awareness` tier so playtesting reads the
/// meter without UI. Read-only over simulation state, like every visual.
/// Skips visuals mid hit-flash — the flash owns the material until it expires
/// (`presentation::juice`).
pub(super) fn tint_enemy_visual(
    enemies: Query<&Awareness, With<Enemy>>,
    visuals: Query<
        (&EnemyVisual, &MeshMaterial3d<StandardMaterial>),
        Without<crate::presentation::juice::HitFlash>,
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (enemy_vis, material_handle) in &visuals {
        let Ok(awareness) = enemies.get(enemy_vis.actor) else {
            continue;
        };
        let color = if awareness.is_alerted() {
            ENEMY_ALERTED_COLOR
        } else if awareness.is_suspicious() {
            ENEMY_SUSPICIOUS_COLOR
        } else {
            ENEMY_CALM_COLOR
        };
        if let Some(mut material) = materials.get_mut(&material_handle.0)
            && material.base_color != color
        {
            material.base_color = color;
        }
    }
}

pub(super) fn despawn_orphaned_enemy_visual(
    mut commands: Commands,
    visuals: Query<(Entity, &EnemyVisual)>,
    actors: Query<(), With<Enemy>>,
) {
    for (vis_entity, enemy_vis) in &visuals {
        if actors.get(enemy_vis.actor).is_err() {
            commands.entity(vis_entity).despawn();
        }
    }
}

type EnemyActorQuery<'a> = &'a Transform;
type EnemyActorFilter = (With<Enemy>, Without<EnemyVisual>);
type EnemyVisualQuery<'a> = (&'a mut Transform, &'a EnemyVisual);
type EnemyVisualFilter = (With<EnemyVisual>, Without<Enemy>);

pub(super) fn interpolate_enemy_visual(
    actors: Query<EnemyActorQuery, EnemyActorFilter>,
    mut visuals: Query<EnemyVisualQuery, EnemyVisualFilter>,
    time: Res<Time>,
) {
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    for (mut visual, enemy) in &mut visuals {
        let Ok(body) = actors.get(enemy.actor) else {
            continue;
        };
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual.translation.y += (body.translation.y - visual.translation.y) * t;
        visual.rotation = visual.rotation.slerp(body.rotation, t);
    }
}
