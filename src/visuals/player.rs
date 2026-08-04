//! Player graybox visual: the physical capsule plus the procedural bow.
//!
//! A standalone mesh entity interpolates toward the kinematic body each render
//! frame. Its standing and crouched meshes are built from the actor's own
//! [`BodyDimensions`], so presentation cannot drift away from collision.

use bevy::prelude::*;

use super::{INTERPOLATION_SPEED, VisualOf};
use crate::asset_pipeline::MaterialPalette;
use crate::combat::state::CombatState;
use bof_simulation::movement::Player;
use bof_simulation::movement::body::BodyDimensions;
use bof_simulation::movement::motors::sneak::Crouched;

#[derive(Component)]
pub struct PlayerVisual;

#[derive(Component)]
pub(super) struct PlayerCapsuleMeshes {
    standing: Handle<Mesh>,
    crouched: Handle<Mesh>,
}

#[derive(Component)]
pub struct BowVisualRoot;

#[derive(Component)]
pub struct BowArrowVisual;

pub(super) fn spawn_visual(
    mut commands: Commands,
    players: Query<(Entity, &Transform, &BodyDimensions, &Crouched), Added<Player>>,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    state: Res<State<crate::scene::AppState>>,
) {
    for (actor, transform, body, crouched) in &players {
        let standing = meshes.add(Capsule3d::new(body.radius, body.standing_capsule_length));
        let crouched_mesh = meshes.add(Capsule3d::new(body.radius, body.crouched_capsule_length));
        let visible_mesh = if crouched.0 {
            crouched_mesh.clone()
        } else {
            standing.clone()
        };
        commands
            .spawn((
                DespawnOnExit(*state.get()),
                PlayerVisual,
                PlayerCapsuleMeshes {
                    standing,
                    crouched: crouched_mesh,
                },
                VisualOf(actor),
                Name::new("PlayerVisual"),
                Mesh3d(visible_mesh),
                MeshMaterial3d(palette.instance("Player", &mut materials)),
                *transform,
            ))
            .with_children(|parent| {
                // Spawn the Bow Visual Root at Combat's bow socket (the arrow
                // spawn point), so the arrow visibly leaves the bow.
                parent
                    .spawn((
                        BowVisualRoot,
                        Name::new("BowVisualRoot"),
                        Visibility::Hidden,
                        Transform::from_translation(crate::combat::motors::aim::BOW_SOCKET_LOCAL)
                            .with_rotation(
                                Quat::from_rotation_y(0.12) * Quat::from_rotation_z(-0.18),
                            ),
                    ))
                    .with_children(|bow_parent| {
                        let wood_material = palette.handle("Wood");
                        let string_material = palette.handle("String");
                        let steel_material = palette.handle("Steel");
                        let fletch_material = palette.handle("Fletching");

                        // Bow limbs
                        // Handle (center vertical part)
                        bow_parent.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.04, 0.2, 0.04))),
                            MeshMaterial3d(wood_material.clone()),
                            Transform::from_xyz(0.0, 0.0, 0.0),
                            Visibility::default(),
                        ));
                        // Upper limb (angled forward)
                        bow_parent.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.03, 0.45, 0.03))),
                            MeshMaterial3d(wood_material.clone()),
                            Transform::from_xyz(0.0, 0.28, -0.06)
                                .with_rotation(Quat::from_rotation_x(-0.35)),
                            Visibility::default(),
                        ));
                        // Lower limb (angled forward)
                        bow_parent.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.03, 0.45, 0.03))),
                            MeshMaterial3d(wood_material.clone()),
                            Transform::from_xyz(0.0, -0.28, -0.06)
                                .with_rotation(Quat::from_rotation_x(0.35)),
                            Visibility::default(),
                        ));
                        // Bowstring (from top tip to bottom tip)
                        bow_parent.spawn((
                            Mesh3d(meshes.add(Cuboid::new(0.006, 0.95, 0.006))),
                            MeshMaterial3d(string_material),
                            Transform::from_xyz(0.0, 0.0, 0.1),
                            Visibility::default(),
                        ));

                        // Arrow
                        bow_parent
                            .spawn((
                                BowArrowVisual,
                                Name::new("BowArrowVisual"),
                                Visibility::default(),
                                Transform::from_xyz(0.0, 0.0, 0.1), // initially pulled back/resting on string
                            ))
                            .with_children(|arrow_parent| {
                                // Shaft (brown)
                                arrow_parent.spawn((
                                    Mesh3d(meshes.add(Cuboid::new(0.015, 0.015, 0.65))),
                                    MeshMaterial3d(wood_material),
                                    Transform::from_xyz(0.0, 0.0, -0.3), // aligned forward (-Z)
                                    Visibility::default(),
                                ));
                                // Arrowhead (silver/steel)
                                arrow_parent.spawn((
                                    Mesh3d(meshes.add(Cuboid::new(0.035, 0.035, 0.07))),
                                    MeshMaterial3d(steel_material),
                                    Transform::from_xyz(0.0, 0.0, -0.65), // tip of the shaft
                                    Visibility::default(),
                                ));
                                // Fletching (red)
                                arrow_parent.spawn((
                                    Mesh3d(meshes.add(Cuboid::new(0.01, 0.05, 0.08))),
                                    MeshMaterial3d(fletch_material),
                                    Transform::from_xyz(0.0, 0.0, -0.05), // near the back
                                    Visibility::default(),
                                ));
                            });
                    });
            });
    }
}

type PlayerActorQuery<'a> = (&'a Transform, &'a Crouched);
type PlayerVisualQuery<'a> = (
    &'a mut Transform,
    &'a mut Mesh3d,
    &'a PlayerCapsuleMeshes,
    &'a VisualOf,
);
type PlayerVisualFilter = (With<PlayerVisual>, Without<Player>);

pub(super) fn interpolate_visual(
    players: Query<PlayerActorQuery, With<Player>>,
    mut visuals: Query<PlayerVisualQuery, PlayerVisualFilter>,
    time: Res<Time>,
) {
    let dt = time.delta_secs();
    for (mut transform, mut mesh, capsule_meshes, owner) in &mut visuals {
        let Ok((body, crouched)) = players.get(owner.0) else {
            continue;
        };
        mesh.0 = if crouched.0 {
            capsule_meshes.crouched.clone()
        } else {
            capsule_meshes.standing.clone()
        };
        transform.translation.x = body.translation.x;
        transform.translation.z = body.translation.z;
        transform
            .translation
            .y
            .smooth_nudge(&body.translation.y, INTERPOLATION_SPEED, dt);
        transform
            .rotation
            .smooth_nudge(&body.rotation, INTERPOLATION_SPEED, dt);
    }
}

#[allow(clippy::type_complexity)]
pub(super) fn animate_bow_visual(
    player: Single<(&CombatState, &crate::combat::motors::aim::DrawStrength), With<Player>>,
    mut bow_root: Query<(&mut Visibility, &Children), With<BowVisualRoot>>,
    mut arrow: Query<
        (&mut Visibility, &mut Transform),
        (With<BowArrowVisual>, Without<BowVisualRoot>),
    >,
) {
    let (state, draw) = *player;
    let aiming = matches!(state, CombatState::Aiming);

    for (mut visibility, children) in &mut bow_root {
        *visibility = if aiming {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };

        if aiming {
            // The arrow is only visible if the player is actively drawing the string or holding a charge.
            let arrow_visible = draw.charging || draw.factor > 0.0;

            for &child in children {
                if let Ok((mut arrow_vis, mut transform)) = arrow.get_mut(child) {
                    *arrow_vis = if arrow_visible {
                        Visibility::Inherited
                    } else {
                        Visibility::Hidden
                    };

                    if arrow_visible {
                        // Pull the arrow back by factor * 0.42 meters
                        transform.translation.z = 0.1 + draw.factor * 0.42;
                    }
                }
            }
        }
    }
}
