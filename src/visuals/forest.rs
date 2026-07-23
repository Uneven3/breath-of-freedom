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

use super::foliage::{FoliageLeaf, FoliageMesh};
use super::{
    AppearanceBinding, AppearanceKey, TreeSilhouette, VisualCatalog, VisualOf, VisualSlot,
};
use crate::asset_pipeline::MaterialPalette;
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
        TreeKind::Pine1 => AppearanceKey::PINE_1,
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
    trees: Query<(Entity, &TreeKind, Option<&TreeRepr>, Option<&Children>)>,
    tree_visuals: Query<(), With<TreeVisual>>,
) {
    let want = if perf.tree_detail {
        TreeRepr::Detailed
    } else {
        TreeRepr::Proxy
    };

    for (owner, kind, current, children) in &trees {
        if current == Some(&want) {
            continue;
        }
        // Drop the previous tier's visual before building the new one.
        if let Some(children) = children {
            for &child in children {
                if tree_visuals.contains(child) {
                    commands.entity(child).despawn();
                }
            }
        }

        let appearance = appearance_for(*kind);
        commands.entity(owner).insert((Visibility::default(), want));
        match want {
            TreeRepr::Proxy => spawn_proxy(&mut commands, owner, appearance, &proxies, &catalog),
            TreeRepr::Detailed => {
                spawn_detailed(&mut commands, owner, appearance, &asset_server, &catalog)
            }
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

fn spawn_detailed(
    commands: &mut Commands,
    owner: Entity,
    appearance: AppearanceKey,
    asset_server: &AssetServer,
    catalog: &VisualCatalog,
) {
    let Some(recipe) = catalog.recipe(appearance) else {
        warn!("[visuals] no recipe for tree appearance {appearance:?}");
        return;
    };
    let scene = asset_server.load(recipe.scene.clone());
    commands.entity(owner).with_children(|tree| {
        tree.spawn((
            Name::new(recipe.label.clone()),
            TreeVisual,
            VisualOf(owner),
            AppearanceBinding {
                key: appearance,
                slot: VisualSlot::World,
            },
            WorldAssetRoot(scene),
            recipe.root_transform,
        ));
    });
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
