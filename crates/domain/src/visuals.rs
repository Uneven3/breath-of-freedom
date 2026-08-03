use bevy_ecs::prelude::{Component, Entity};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AppearanceKey(pub &'static str);

impl AppearanceKey {
    pub const COMMON_TREE_1: Self = Self("legacy_tree_common_1");
    pub const COMMON_TREE_2: Self = Self("legacy_tree_common_2");
    pub const COMMON_TREE_3: Self = Self("legacy_tree_common_3");
    pub const COMMON_TREE_4: Self = Self("legacy_tree_common_4");
    pub const COMMON_TREE_5: Self = Self("legacy_tree_common_5");
    pub const PINE_1: Self = Self("legacy_tree_pine_1");
    pub const PINE_2: Self = Self("legacy_tree_pine_2");
    pub const PINE_3: Self = Self("legacy_tree_pine_3");
    pub const PINE_4: Self = Self("legacy_tree_pine_4");
    pub const PINE_5: Self = Self("legacy_tree_pine_5");
    pub const TREE_PINE_A: Self = Self("tree_pine_a");
    pub const TWISTED_TREE_1: Self = Self("legacy_tree_twisted_1");
    pub const TWISTED_TREE_2: Self = Self("legacy_tree_twisted_2");
    pub const TWISTED_TREE_3: Self = Self("legacy_tree_twisted_3");
    pub const TWISTED_TREE_4: Self = Self("legacy_tree_twisted_4");
    pub const TWISTED_TREE_5: Self = Self("legacy_tree_twisted_5");
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VisualSlot {
    Body,
    MainHand,
    OffHand,
    World,
}

#[derive(Component, Debug, Clone, Copy, PartialEq, Eq)]
pub struct AppearanceBinding {
    pub key: AppearanceKey,
    pub slot: VisualSlot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TreeSilhouette {
    Rounded,
    Conical,
    Gnarled,
}

#[derive(Debug, Clone, Copy)]
pub struct TreeProxy {
    pub silhouette: TreeSilhouette,
}

/// Links a disposable presentation entity to its simulation owner.
#[derive(Component, Clone, Copy)]
pub struct VisualOf(pub Entity);
