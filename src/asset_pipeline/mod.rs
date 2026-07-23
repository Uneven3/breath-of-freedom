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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SpatialCylinder {
    pub radius: f32,
    pub height: f32,
}

#[derive(Resource)]
pub struct SpatialCatalog {
    profiles: HashMap<SpatialProfileKey, &'static GeneratedAsset>,
}

impl Default for SpatialCatalog {
    fn default() -> Self {
        for asset in generated::AUTHORED_ASSETS {
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
                let _semantic_metadata =
                    (collider.kind, collider.climbable, collider.material_kind);
            }
        }
        debug_assert_eq!(GeneratedColliderKind::ALL.len(), 5);
        let profiles = generated::AUTHORED_ASSETS
            .iter()
            .filter_map(|asset| {
                asset
                    .profile
                    .map(|profile| (SpatialProfileKey(profile), asset))
            })
            .collect();
        Self { profiles }
    }
}

impl SpatialCatalog {
    pub fn profile(&self, key: SpatialProfileKey) -> Option<&'static GeneratedAsset> {
        self.profiles.get(&key).copied()
    }

    pub fn cylinder(
        &self,
        profile: SpatialProfileKey,
        helper_name: &str,
    ) -> Option<SpatialCylinder> {
        let collider = self.profile(profile)?.colliders.iter().find(|collider| {
            collider.name == helper_name && collider.kind == GeneratedColliderKind::Cylinder
        })?;
        Some(SpatialCylinder {
            radius: collider.size[0].max(collider.size[2]) * 0.5,
            height: collider.size[1],
        })
    }
}

pub struct AssetPipelinePlugin;

impl Plugin for AssetPipelinePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MaterialPalette>();
        app.init_resource::<SpatialCatalog>();
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

    #[test]
    fn authored_tree_profile_is_available_before_startup() {
        let catalog = SpatialCatalog::default();
        let collider = catalog
            .cylinder(SpatialProfileKey("tree_pine_trunk"), "UCY_Trunk")
            .expect("reference tree cylinder");
        assert!((collider.radius - 0.44).abs() < 0.001);
        assert!((collider.height - 4.4).abs() < 0.001);
    }
}
