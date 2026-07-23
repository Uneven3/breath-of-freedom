//! Player visual: the rigged GLB scene plus the procedural bow.
//!
//! A standalone mesh entity interpolates toward the kinematic body each render
//! frame, which (a) smooths the 60 Hz fixed-step motion on high-refresh
//! displays and (b) dips −0.4 in Sneak so crouching reads visually even though
//! the collider, not the mesh, is what actually shrinks.

use bevy::prelude::*;

use super::{
    AppearanceBinding, INTERPOLATION_SPEED, PLAYER_APPEARANCE, SNEAK_Y_OFFSET, VisualCatalog,
    VisualOf, VisualSlot,
};
use crate::asset_pipeline::MaterialPalette;
use crate::combat::state::CombatState;
use crate::movement::Player;
use crate::movement::state::LocomotionState;

#[derive(Component)]
pub struct PlayerVisual;

#[derive(Component)]
pub struct BowVisualRoot;

#[derive(Component)]
pub struct BowArrowVisual;

pub(super) fn spawn_visual(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    catalog: Res<VisualCatalog>,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
) {
    let appearance = PLAYER_APPEARANCE;
    let Some(recipe) = catalog.recipe(appearance) else {
        error!("[visuals] player appearance is not registered; visual disabled");
        return;
    };
    commands
        .spawn((
            PlayerVisual,
            AppearanceBinding {
                key: appearance,
                slot: VisualSlot::Body,
            },
            Name::new("PlayerVisual"),
            Transform::from_xyz(0.0, 1.5, 0.0),
            Visibility::default(),
        ))
        .with_children(|parent| {
            // The selected body and its compatible UAL2 clips share one GLB.
            let player_scene = asset_server.load(recipe.scene.clone());

            parent.spawn((
                Name::new(recipe.label.clone()),
                WorldAssetRoot(player_scene),
                recipe.root_transform,
            ));

            // Spawn the Bow Visual Root at Combat's bow socket (the arrow
            // spawn point), so the arrow visibly leaves the bow.
            parent
                .spawn((
                    BowVisualRoot,
                    Name::new("BowVisualRoot"),
                    Visibility::Hidden,
                    Transform::from_translation(crate::combat::motors::aim::BOW_SOCKET_LOCAL)
                        .with_rotation(Quat::from_rotation_y(0.12) * Quat::from_rotation_z(-0.18)),
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

type BodyFilter = (With<Player>, Without<PlayerVisual>);

type UnlinkedPlayerVisual = (With<PlayerVisual>, Without<VisualOf>);

/// The player visual is spawned in `Startup` before the player body's spawn
/// command applies; link them lazily once both exist.
pub(super) fn link_player_visual(
    mut commands: Commands,
    visual: Option<Single<Entity, UnlinkedPlayerVisual>>,
    player: Option<Single<Entity, With<Player>>>,
) {
    if let (Some(visual), Some(player)) = (visual, player) {
        commands.entity(*visual).insert(VisualOf(*player));
    }
}

pub(super) fn interpolate_visual(
    player: Single<(&Transform, &LocomotionState), BodyFilter>,
    mut visual: Single<&mut Transform, With<PlayerVisual>>,
    time: Res<Time>,
) {
    let (body, state) = *player;
    let dt = time.delta_secs();
    let offset = if *state == LocomotionState::Sneak {
        SNEAK_Y_OFFSET
    } else {
        0.0
    };

    // X/Z track the body directly; Y and rotation ease.
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
