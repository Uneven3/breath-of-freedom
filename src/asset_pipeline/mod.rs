//! Convention-driven boundary between authored GLBs and game systems.
//!
//! Build-time inspection makes spatial profiles available before `Startup`;
//! Bevy scene post-processing remains presentation-only.

mod generated;
pub mod materials;
mod schema;

use std::collections::HashMap;

use bevy::prelude::*;

use generated::{GeneratedAsset, GeneratedColliderKind};
pub use materials::MaterialPalette;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SpatialProfileKey(pub &'static str);

#[derive(Resource, Default)]
pub struct SpatialCatalog {
    profiles: HashMap<SpatialProfileKey, &'static GeneratedAsset>,
}

impl SpatialCatalog {
    pub fn profile(&self, key: SpatialProfileKey) -> Option<&'static GeneratedAsset> {
        self.profiles.get(&key).copied()
    }
}

fn populate_spatial_catalog(mut catalog: ResMut<SpatialCatalog>) {
    for asset in generated::AUTHORED_ASSETS {
        if let Some(profile) = asset.profile {
            let key = SpatialProfileKey(profile);
            let previous = catalog.profiles.insert(key, asset);
            if previous.is_some() {
                error!("[assets] duplicate spatial profile {profile}");
            }
            debug_assert!(catalog.profile(key).is_some());
        }
        debug_assert!(schema::valid_asset_key(asset.key));
        debug_assert!(asset.path.ends_with(".glb"));
        for socket in asset.sockets {
            debug_assert!(schema::valid_socket_name(socket.name));
            debug_assert!(socket.translation.iter().all(|value| value.is_finite()));
            debug_assert!(socket.rotation.iter().all(|value| value.is_finite()));
        }
        for collider in asset.colliders {
            debug_assert!(schema::is_collision_name(collider.name));
            debug_assert!(collider.translation.iter().all(|value| value.is_finite()));
            debug_assert!(collider.rotation.iter().all(|value| value.is_finite()));
            debug_assert!(collider.size.iter().all(|value| value.is_finite()));
            debug_assert!(
                collider
                    .points
                    .iter()
                    .flatten()
                    .all(|value| value.is_finite())
            );
            let _semantic_metadata = (collider.kind, collider.climbable, collider.material_kind);
        }
    }
    debug_assert_eq!(GeneratedColliderKind::ALL.len(), 5);
}

pub struct AssetPipelinePlugin;

impl Plugin for AssetPipelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MaterialPalette>();
        app.init_resource::<SpatialCatalog>();
        app.add_systems(Startup, populate_spatial_catalog);
        app.add_systems(
            Update,
            (
                materials::remap_authored_materials,
                materials::apply_authored_lod,
                materials::strip_collision_rendering,
                materials::validate_authored_extras,
            ),
        );
    }
}

pub fn authored_assets() -> &'static [GeneratedAsset] {
    generated::AUTHORED_ASSETS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appearance_and_spatial_keys_are_distinct_types() {
        let spatial = SpatialProfileKey("tree_pine_trunk");
        assert_eq!(spatial.0, "tree_pine_trunk");
    }
}
