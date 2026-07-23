//! Forest presentation: a semantic tree kind becomes one of two representation
//! tiers — a cheap procedural proxy (the graybox default) or the detailed glTF
//! scene. Cost is a property of the representation; the simulation never knows
//! which is showing.
//!
//! The proxy is the honest graybox: like the capsule actors and box world, it
//! is a handful of shared primitives, fully instanced. The detailed Quaternius
//! model is an opt-in tier (the `tree-detail` knob) kept for comparison — flip
//! it and the frame time shows exactly what modeled foliage costs.

use bevy::prelude::*;
use bevy::world_serialization::WorldInstance;

use super::foliage::{FoliageLeaf, FoliageMesh};
use super::{
    AppearanceBinding, AppearanceKey, TreeSilhouette, VisualCatalog, VisualOf, VisualSlot,
};
use crate::asset_pipeline::MaterialPalette;
use crate::asset_pipeline::materials::AuthoredVisualRoot;
use crate::perf::PerfToggles;
use crate::world::TreeKind;

/// Which tier a tree's visual is currently built as, stored on the tree so the
/// sync system only rebuilds when the choice actually changes.
#[derive(Component, Clone, Copy, PartialEq, Eq)]
pub(super) enum TreeRepr {
    Proxy,
    Detailed,
}

fn appearance_for(kind: TreeKind) -> AppearanceKey {
    match kind {
        TreeKind::Common1 => AppearanceKey::COMMON_TREE_1,
        TreeKind::Common2 => AppearanceKey::COMMON_TREE_2,
        TreeKind::Common3 => AppearanceKey::COMMON_TREE_3,
        TreeKind::Common4 => AppearanceKey::COMMON_TREE_4,
        TreeKind::Common5 => AppearanceKey::COMMON_TREE_5,
        TreeKind::Pine1 => AppearanceKey::TREE_PINE_A,
        TreeKind::Pine2 => AppearanceKey::PINE_2,
        TreeKind::Pine3 => AppearanceKey::PINE_3,
        TreeKind::Pine4 => AppearanceKey::PINE_4,
        TreeKind::Pine5 => AppearanceKey::PINE_5,
        TreeKind::Twisted1 => AppearanceKey::TWISTED_TREE_1,
        TreeKind::Twisted2 => AppearanceKey::TWISTED_TREE_2,
        TreeKind::Twisted3 => AppearanceKey::TWISTED_TREE_3,
        TreeKind::Twisted4 => AppearanceKey::TWISTED_TREE_4,
        TreeKind::Twisted5 => AppearanceKey::TWISTED_TREE_5,
    }
}

/// Marks a tree's disposable visual root. Exists so the forest benchmark knob
/// can claim `&mut Visibility` over exactly the trees: matching on
/// `AppearanceBinding` instead would overlap `camera::follow_player`, which
/// writes the player visual's `Visibility` for the first-person fade.
#[derive(Component)]
pub struct TreeVisual;

#[derive(Component)]
pub(super) struct PendingTreeVisual {
    owner: Entity,
    target: TreeRepr,
    handle: Handle<WorldAsset>,
    ready_updates: u8,
}

#[derive(Component)]
pub(super) struct FailedTreeVisual {
    target: TreeRepr,
}

#[derive(Component)]
pub(super) struct TreeVisualTransition;

/// Shared meshes and materials for the three proxy silhouettes, built once so
/// every tree of a family references the same handles — one edit, and the
/// renderer instances them. Building a mesh per tree would defeat the batching
/// that makes the proxy cheap in the first place.
#[derive(Resource)]
pub(super) struct TreeProxyAssets {
    rounded: ProxyParts,
    conical: ProxyParts,
    gnarled: ProxyParts,
}

struct ProxyParts {
    trunk_mesh: Handle<Mesh>,
    trunk_material: Handle<StandardMaterial>,
    trunk_height: f32,
    canopy_mesh: Handle<Mesh>,
    canopy_material: Handle<StandardMaterial>,
    /// Local height of the canopy centre above the tree base.
    canopy_y: f32,
}

impl TreeProxyAssets {
    fn parts(&self, silhouette: TreeSilhouette) -> &ProxyParts {
        match silhouette {
            TreeSilhouette::Rounded => &self.rounded,
            TreeSilhouette::Conical => &self.conical,
            TreeSilhouette::Gnarled => &self.gnarled,
        }
    }
}

pub(super) fn build_tree_proxy_assets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
) {
    // Trunk radii/heights track `world::forest::tree_collider` so the proxy
    // sits where the collider is — the graybox stays honest about its body.
    let trunk = |meshes: &mut Assets<Mesh>, radius: f32, height: f32| {
        meshes.add(Cylinder::new(radius, height))
    };
    let bark = palette.handle("Bark");

    let rounded = ProxyParts {
        trunk_mesh: trunk(&mut meshes, 0.35, 4.8),
        trunk_material: bark.clone(),
        trunk_height: 4.8,
        canopy_mesh: meshes.add(Sphere::new(2.0)),
        canopy_material: palette.handle("FoliageCommon"),
        canopy_y: 5.4,
    };
    let conical = ProxyParts {
        trunk_mesh: trunk(&mut meshes, 0.3, 4.4),
        trunk_material: bark.clone(),
        trunk_height: 4.4,
        canopy_mesh: meshes.add(Cone {
            radius: 1.8,
            height: 4.5,
        }),
        canopy_material: palette.handle("FoliagePine"),
        canopy_y: 5.2,
    };
    let gnarled = ProxyParts {
        trunk_mesh: trunk(&mut meshes, 0.5, 6.2),
        trunk_material: bark,
        trunk_height: 6.2,
        canopy_mesh: meshes.add(Sphere::new(1.9)),
        canopy_material: palette.handle("FoliageGnarled"),
        canopy_y: 6.6,
    };

    commands.insert_resource(TreeProxyAssets {
        rounded,
        conical,
        gnarled,
    });
}

#[allow(clippy::type_complexity)]
pub(super) fn sync_tree_visuals(
    mut commands: Commands,
    perf: Res<PerfToggles>,
    proxies: Res<TreeProxyAssets>,
    catalog: Res<VisualCatalog>,
    asset_server: Res<AssetServer>,
    trees: Query<(
        Entity,
        &TreeKind,
        Option<&TreeRepr>,
        Option<&Children>,
        Option<&FailedTreeVisual>,
        Option<&TreeVisualTransition>,
    )>,
    tree_visuals: Query<(), With<TreeVisual>>,
) {
    for (owner, kind, current, children, failed, transition) in &trees {
        // Pine1 is the staged reference migration. Other species retain the
        // benchmark-controlled legacy/proxy split until their own replacement.
        let want = if *kind == TreeKind::Pine1 || perf.tree_detail {
            TreeRepr::Detailed
        } else {
            TreeRepr::Proxy
        };
        if failed.is_some_and(|failure| failure.target == want) {
            continue;
        }
        if failed.is_some() {
            commands.entity(owner).remove::<FailedTreeVisual>();
        }
        if current == Some(&want) {
            continue;
        }

        let appearance = appearance_for(*kind);
        if current.is_none() {
            // Establish a visible fallback immediately. Detailed scenes only
            // replace it once Bevy has instantiated the complete scene.
            commands
                .entity(owner)
                .insert((Visibility::default(), TreeRepr::Proxy));
            spawn_proxy(&mut commands, owner, appearance, &proxies, &catalog);
            continue;
        }
        if transition.is_some() {
            continue;
        }

        match want {
            TreeRepr::Proxy => {
                despawn_current_visuals(&mut commands, children, &tree_visuals, None);
                commands.entity(owner).insert(TreeRepr::Proxy);
                spawn_proxy(&mut commands, owner, appearance, &proxies, &catalog);
            }
            TreeRepr::Detailed => {
                if !spawn_pending_detailed(
                    &mut commands,
                    owner,
                    appearance,
                    &asset_server,
                    &catalog,
                ) {
                    commands
                        .entity(owner)
                        .insert(FailedTreeVisual { target: want });
                }
            }
        }
    }
}

fn despawn_current_visuals(
    commands: &mut Commands,
    children: Option<&Children>,
    tree_visuals: &Query<(), With<TreeVisual>>,
    except: Option<Entity>,
) {
    let Some(children) = children else {
        return;
    };
    for child in children.iter() {
        if Some(child) != except && tree_visuals.contains(child) {
            commands.entity(child).despawn();
        }
    }
}

fn spawn_proxy(
    commands: &mut Commands,
    owner: Entity,
    appearance: AppearanceKey,
    proxies: &TreeProxyAssets,
    catalog: &VisualCatalog,
) {
    let Some(proxy) = catalog.tree_proxy(appearance) else {
        warn!("[visuals] no proxy for tree appearance {appearance:?}");
        return;
    };
    let parts = proxies.parts(proxy.silhouette);

    commands.entity(owner).with_children(|tree| {
        tree.spawn((
            Name::new("TreeProxy"),
            TreeVisual,
            VisualOf(owner),
            AppearanceBinding {
                key: appearance,
                slot: VisualSlot::World,
            },
            Transform::default(),
            Visibility::default(),
        ))
        .with_children(|root| {
            // Trunk: opaque body, always a shadow caster.
            root.spawn((
                Mesh3d(parts.trunk_mesh.clone()),
                MeshMaterial3d(parts.trunk_material.clone()),
                Transform::from_xyz(0.0, parts.trunk_height * 0.5, 0.0),
                FoliageMesh,
            ));
            // Canopy: tagged as leaf, so it obeys the leaf-shadow and LOD knobs
            // exactly like the detailed model's foliage.
            root.spawn((
                Mesh3d(parts.canopy_mesh.clone()),
                MeshMaterial3d(parts.canopy_material.clone()),
                Transform::from_xyz(0.0, parts.canopy_y, 0.0),
                FoliageMesh,
                FoliageLeaf,
            ));
        });
    });
}

fn spawn_pending_detailed(
    commands: &mut Commands,
    owner: Entity,
    appearance: AppearanceKey,
    asset_server: &AssetServer,
    catalog: &VisualCatalog,
) -> bool {
    let Some(recipe) = catalog.recipe(appearance) else {
        warn!("[visuals] no recipe for tree appearance {appearance:?}");
        return false;
    };
    let scene: Handle<WorldAsset> = asset_server.load(recipe.scene.clone());
    let mut pending = commands.spawn((
        Name::new(recipe.label.clone()),
        PendingTreeVisual {
            owner,
            target: TreeRepr::Detailed,
            handle: scene.clone(),
            ready_updates: 0,
        },
        VisualOf(owner),
        AppearanceBinding {
            key: appearance,
            slot: VisualSlot::World,
        },
        WorldAssetRoot(scene),
        recipe.root_transform,
        Visibility::Hidden,
    ));
    if !appearance.0.starts_with("legacy_") {
        pending.insert(AuthoredVisualRoot);
    }
    let pending_entity = pending.id();
    commands
        .entity(owner)
        .add_child(pending_entity)
        .insert(TreeVisualTransition);
    true
}

pub(super) fn finalize_tree_visual_swaps(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    mut pending_visuals: Query<(Entity, &mut PendingTreeVisual, Option<&WorldInstance>)>,
    owner_children: Query<&Children>,
    tree_visuals: Query<(), With<TreeVisual>>,
) {
    for (pending_entity, mut pending, instance) in &mut pending_visuals {
        if matches!(
            asset_server.get_load_state(pending.handle.id()),
            Some(bevy::asset::LoadState::Failed(_))
        ) {
            error!(
                "[visuals] tree appearance failed to load; keeping proxy for {:?}",
                pending.owner
            );
            commands.entity(pending_entity).despawn();
            commands
                .entity(pending.owner)
                .insert(FailedTreeVisual {
                    target: pending.target,
                })
                .remove::<TreeVisualTransition>();
            continue;
        }
        if instance.is_none() || !asset_server.is_loaded_with_dependencies(pending.handle.id()) {
            continue;
        }
        if pending.ready_updates == 0 {
            pending.ready_updates = 1;
            continue;
        }

        let children = owner_children.get(pending.owner).ok();
        despawn_current_visuals(&mut commands, children, &tree_visuals, Some(pending_entity));
        commands
            .entity(pending_entity)
            .insert((TreeVisual, Visibility::Inherited))
            .remove::<PendingTreeVisual>();
        commands
            .entity(pending.owner)
            .insert(pending.target)
            .remove::<(FailedTreeVisual, TreeVisualTransition)>();
    }
}

/// Whole-forest visibility toggle. Distance culling is not here: it lives on
/// the meshes as `VisibilityRange` (`visuals::foliage`), because the engine
/// evaluates that far more cheaply than a per-frame walk from this module.
///
/// Presentation only — the `TreeKind` entity keeps its trunk collider either
/// way, so locomotion, sweeps and `FixedUpdate` results are byte-identical with
/// the forest visible or hidden (`ARCHITECTURE.md`: "LOD, culling e instancing
/// solo cambian entidades/recetas visuales").
pub(super) fn apply_forest_perf(
    perf: Res<PerfToggles>,
    mut trees: Query<&mut Visibility, With<TreeVisual>>,
) {
    for mut visibility in &mut trees {
        let wanted = if perf.forest_visible {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
        if *visibility != wanted {
            *visibility = wanted;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tree_kind_has_a_registered_recipe() {
        let catalog = VisualCatalog::default();
        let kinds = [
            TreeKind::Common1,
            TreeKind::Common2,
            TreeKind::Common3,
            TreeKind::Common4,
            TreeKind::Common5,
            TreeKind::Pine1,
            TreeKind::Pine2,
            TreeKind::Pine3,
            TreeKind::Pine4,
            TreeKind::Pine5,
            TreeKind::Twisted1,
            TreeKind::Twisted2,
            TreeKind::Twisted3,
            TreeKind::Twisted4,
            TreeKind::Twisted5,
        ];

        for kind in kinds {
            assert!(catalog.recipe(appearance_for(kind)).is_some());
        }
    }
}
