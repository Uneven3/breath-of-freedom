//! Horse graybox visual. The simulation entity carries no mesh, material or
//! asset handle; this disposable visual follows it by `VisualOf`.

use bevy::prelude::*;

use super::{INTERPOLATION_SPEED, VisualOf};
use crate::mounts::data::Horse;
use crate::movement::body::BodyDimensions;

#[derive(Component)]
pub(super) struct HorseVisual {
    actor: Entity,
}

pub(super) fn spawn_horse_visual(
    mut commands: Commands,
    horses: Query<(Entity, &Transform, &BodyDimensions), Added<Horse>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (actor, transform, body) in &horses {
        commands.spawn((
            HorseVisual { actor },
            VisualOf(actor),
            Name::new("HorseVisual"),
            Mesh3d(meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length))),
            MeshMaterial3d(materials.add(Color::srgb(0.42, 0.23, 0.1))),
            transform.with_scale(Vec3::new(0.9, 0.9, 1.45)),
        ));
    }
}

pub(super) fn despawn_orphaned_horse_visual(
    mut commands: Commands,
    visuals: Query<(Entity, &HorseVisual)>,
    actors: Query<(), With<Horse>>,
) {
    for (visual, horse) in &visuals {
        if actors.get(horse.actor).is_err() {
            commands.entity(visual).despawn();
        }
    }
}

type HorseActorFilter = (With<Horse>, Without<HorseVisual>);
type HorseVisualFilter = (With<HorseVisual>, Without<Horse>);

pub(super) fn interpolate_horse_visual(
    actors: Query<&Transform, HorseActorFilter>,
    mut visuals: Query<(&mut Transform, &HorseVisual), HorseVisualFilter>,
    time: Res<Time>,
) {
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    for (mut visual, horse) in &mut visuals {
        let Ok(body) = actors.get(horse.actor) else {
            continue;
        };
        visual.translation.x = body.translation.x;
        visual.translation.z = body.translation.z;
        visual.translation.y += (body.translation.y - visual.translation.y) * t;
        visual.rotation = visual.rotation.slerp(body.rotation, t);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn orphaned_horse_visual_despawns_without_touching_simulation() {
        let mut world = World::new();
        let missing_horse = world.spawn_empty().id();
        world.entity_mut(missing_horse).despawn();
        let visual = world
            .spawn((
                HorseVisual {
                    actor: missing_horse,
                },
                VisualOf(missing_horse),
            ))
            .id();

        world
            .run_system_once(despawn_orphaned_horse_visual)
            .unwrap();
        world.flush();

        assert!(world.get_entity(visual).is_err());
    }
}
