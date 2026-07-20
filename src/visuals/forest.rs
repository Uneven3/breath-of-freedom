//! Forest presentation: semantic tree kinds become disposable scene roots.

use bevy::prelude::*;

use super::{AppearanceBinding, AppearanceKey, VisualCatalog, VisualOf, VisualSlot};
use crate::world::TreeKind;

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

pub(super) fn spawn_tree_visuals(
    mut commands: Commands,
    trees: Query<(Entity, &TreeKind), Added<TreeKind>>,
    asset_server: Res<AssetServer>,
    catalog: Res<VisualCatalog>,
) {
    for (owner, kind) in &trees {
        let appearance = appearance_for(*kind);
        let Some(recipe) = catalog.recipe(appearance) else {
            warn!("[visuals] no recipe for tree appearance {:?}", appearance);
            continue;
        };
        let scene = asset_server.load(recipe.scene);
        let label = recipe.label;
        let root_transform = recipe.root_transform;
        commands
            .entity(owner)
            .insert(Visibility::default())
            .with_child((
                Name::new(label),
                VisualOf(owner),
                AppearanceBinding {
                    key: appearance,
                    slot: VisualSlot::World,
                },
                WorldAssetRoot(scene),
                root_transform,
            ));
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
