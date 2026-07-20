//! Presentation-owned mapping from stable appearance identities to assets.
//! Simulation components never store these keys, paths, or asset handles.

use std::collections::HashMap;

use bevy::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppearanceKey(pub u32);

impl AppearanceKey {
    pub const PLAYER_RANGER_FEMALE: Self = Self(1);
    pub const PLAYER_RANGER_MALE: Self = Self(2);
    pub const COMMON_TREE_1: Self = Self(100);
    pub const COMMON_TREE_2: Self = Self(101);
    pub const COMMON_TREE_3: Self = Self(102);
    pub const COMMON_TREE_4: Self = Self(103);
    pub const COMMON_TREE_5: Self = Self(104);
    pub const PINE_1: Self = Self(110);
    pub const PINE_2: Self = Self(111);
    pub const PINE_3: Self = Self(112);
    pub const PINE_4: Self = Self(113);
    pub const PINE_5: Self = Self(114);
    pub const TWISTED_TREE_1: Self = Self(120);
    pub const TWISTED_TREE_2: Self = Self(121);
    pub const TWISTED_TREE_3: Self = Self(122);
    pub const TWISTED_TREE_4: Self = Self(123);
    pub const TWISTED_TREE_5: Self = Self(124);
}

pub const PLAYER_APPEARANCE: AppearanceKey = AppearanceKey::PLAYER_RANGER_FEMALE;

/// Where a disposable visual is attached relative to its simulation owner.
/// Equipment visuals will use `MainHand`/`OffHand`; world props use `World`.
#[allow(dead_code)] // Reserved until the selected FBX/Blend assets are converted to glTF.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisualSlot {
    Body,
    MainHand,
    OffHand,
    World,
}

/// Lives on a visual root alongside `VisualOf`, never on its simulation owner.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceBinding {
    pub key: AppearanceKey,
    pub slot: VisualSlot,
}

#[derive(Debug, Clone)]
pub struct VisualRecipe {
    pub label: &'static str,
    pub scene: &'static str,
    pub animation_source: Option<&'static str>,
    /// Normalizes source-library scale, orientation, and pivot.
    pub root_transform: Transform,
}

/// Presentation registry. Quaternius libraries may coexist here even when
/// they use different source scales, pivots, rigs, or animation catalogs.
#[derive(Resource)]
pub struct VisualCatalog {
    recipes: HashMap<AppearanceKey, VisualRecipe>,
}

impl Default for VisualCatalog {
    fn default() -> Self {
        let mut recipes = HashMap::new();
        for (key, label, scene, animation_source) in [
            (
                AppearanceKey::PLAYER_RANGER_FEMALE,
                "Ranger female",
                "game/characters/ranger_female.glb#Scene0",
                "game/characters/ranger_female.glb",
            ),
            (
                AppearanceKey::PLAYER_RANGER_MALE,
                "Ranger male",
                "game/characters/ranger_male.glb#Scene0",
                "game/characters/ranger_male.glb",
            ),
        ] {
            recipes.insert(
                key,
                VisualRecipe {
                    label,
                    scene,
                    animation_source: Some(animation_source),
                    root_transform: Transform::from_xyz(0.0, -1.0, 0.0)
                        .with_rotation(Quat::from_rotation_y(std::f32::consts::PI))
                        .with_scale(Vec3::splat(2.0 / 1.79)),
                },
            );
        }
        recipes.insert(
            AppearanceKey::COMMON_TREE_1,
            VisualRecipe {
                label: "Quaternius common tree 1",
                scene: "Stylized Nature MegaKit[Standard]/glTF/CommonTree_1.gltf#Scene0",
                animation_source: None,
                root_transform: Transform::from_xyz(0.0, 0.24, 0.0),
            },
        );
        let tree_recipes = [
            (
                AppearanceKey::COMMON_TREE_2,
                "Quaternius common tree 2",
                "Stylized Nature MegaKit[Standard]/glTF/CommonTree_2.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::COMMON_TREE_3,
                "Quaternius common tree 3",
                "Stylized Nature MegaKit[Standard]/glTF/CommonTree_3.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::COMMON_TREE_4,
                "Quaternius common tree 4",
                "Stylized Nature MegaKit[Standard]/glTF/CommonTree_4.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::COMMON_TREE_5,
                "Quaternius common tree 5",
                "Stylized Nature MegaKit[Standard]/glTF/CommonTree_5.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::PINE_1,
                "Quaternius pine 1",
                "Stylized Nature MegaKit[Standard]/glTF/Pine_1.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::PINE_2,
                "Quaternius pine 2",
                "Stylized Nature MegaKit[Standard]/glTF/Pine_2.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::PINE_3,
                "Quaternius pine 3",
                "Stylized Nature MegaKit[Standard]/glTF/Pine_3.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::PINE_4,
                "Quaternius pine 4",
                "Stylized Nature MegaKit[Standard]/glTF/Pine_4.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::PINE_5,
                "Quaternius pine 5",
                "Stylized Nature MegaKit[Standard]/glTF/Pine_5.gltf#Scene0",
                0.24,
            ),
            (
                AppearanceKey::TWISTED_TREE_1,
                "Quaternius twisted tree 1",
                "Stylized Nature MegaKit[Standard]/glTF/TwistedTree_1.gltf#Scene0",
                0.20,
            ),
            (
                AppearanceKey::TWISTED_TREE_2,
                "Quaternius twisted tree 2",
                "Stylized Nature MegaKit[Standard]/glTF/TwistedTree_2.gltf#Scene0",
                0.20,
            ),
            (
                AppearanceKey::TWISTED_TREE_3,
                "Quaternius twisted tree 3",
                "Stylized Nature MegaKit[Standard]/glTF/TwistedTree_3.gltf#Scene0",
                0.20,
            ),
            (
                AppearanceKey::TWISTED_TREE_4,
                "Quaternius twisted tree 4",
                "Stylized Nature MegaKit[Standard]/glTF/TwistedTree_4.gltf#Scene0",
                0.20,
            ),
            (
                AppearanceKey::TWISTED_TREE_5,
                "Quaternius twisted tree 5",
                "Stylized Nature MegaKit[Standard]/glTF/TwistedTree_5.gltf#Scene0",
                0.20,
            ),
        ];
        for (key, label, scene, ground_offset) in tree_recipes {
            recipes.insert(
                key,
                VisualRecipe {
                    label,
                    scene,
                    animation_source: None,
                    root_transform: Transform::from_xyz(0.0, ground_offset, 0.0),
                },
            );
        }
        Self { recipes }
    }
}

impl VisualCatalog {
    pub fn recipe(&self, key: AppearanceKey) -> Option<&VisualRecipe> {
        self.recipes.get(&key)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_appearances_are_explicit() {
        let catalog = VisualCatalog::default();

        assert_eq!(
            catalog.recipe(AppearanceKey::COMMON_TREE_1).unwrap().label,
            "Quaternius common tree 1"
        );
        assert!(catalog.recipe(AppearanceKey(u32::MAX)).is_none());
        assert!(
            catalog
                .recipe(AppearanceKey::PLAYER_RANGER_FEMALE)
                .is_some()
        );
        assert!(catalog.recipe(AppearanceKey::PLAYER_RANGER_MALE).is_some());
    }
}
