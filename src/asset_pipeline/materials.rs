use std::collections::HashMap;

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::{GltfExtras, GltfMaterialName, GltfMeshName};
use bevy::prelude::*;

use super::schema::{PALETTE_KEYS, is_collision_name, render_lod};

#[derive(Resource)]
pub struct MaterialPalette {
    handles: HashMap<&'static str, Handle<StandardMaterial>>,
}

impl FromWorld for MaterialPalette {
    fn from_world(world: &mut World) -> Self {
        let asset_server = world.resource::<AssetServer>().clone();
        let mut entries = Vec::with_capacity(PALETTE_KEYS.len());
        for key in PALETTE_KEYS {
            let material = palette_material(key, &asset_server);
            let handle = world
                .resource_mut::<Assets<StandardMaterial>>()
                .add(material);
            entries.push((*key, handle));
        }
        Self {
            handles: entries.into_iter().collect(),
        }
    }
}

impl MaterialPalette {
    pub fn get(&self, key: &str) -> Option<&Handle<StandardMaterial>> {
        self.handles.get(key)
    }

    pub fn handle(&self, key: &str) -> Handle<StandardMaterial> {
        self.get(key).cloned().unwrap_or_else(|| {
            error!("[assets] requested unknown palette key {key}");
            Handle::default()
        })
    }

    /// A fresh, independently-mutable copy of a palette material.
    ///
    /// [`handle`](Self::handle) shares one asset across every user of a key,
    /// which is the point for static geometry (fewer materials, fewer draws).
    /// But anything whose material is mutated *per instance* — the hit flash
    /// recolors a struck target's own material — must own its copy, or the
    /// mutation bleeds across every sibling sharing the handle. Use this there.
    pub fn instance(
        &self,
        key: &str,
        materials: &mut Assets<StandardMaterial>,
    ) -> Handle<StandardMaterial> {
        let base = self
            .get(key)
            .and_then(|handle| materials.get(handle))
            .cloned()
            .unwrap_or_else(|| {
                error!("[assets] requested unknown palette key {key} for a unique instance");
                StandardMaterial::default()
            });
        materials.add(base)
    }
}

fn matte(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.9,
        metallic: 0.0,
        reflectance: 0.2,
        ..default()
    }
}

pub const BOTW_GRASS_ROOT_COLOR: Color = Color::srgb(0.22, 0.40, 0.18);

fn palette_material(key: &str, asset_server: &AssetServer) -> StandardMaterial {
    let color = match key {
        "Bark" => Color::srgb(0.33, 0.24, 0.15),
        "Fletching" => Color::srgb(0.85, 0.15, 0.15),
        "FoliageCommon" => Color::srgb(0.32, 0.58, 0.22),
        "FoliageCard" => Color::srgb(0.32, 0.58, 0.22),
        "FoliageDry" => Color::srgb(0.58, 0.52, 0.28),
        "FoliageGnarled" => Color::srgb(0.36, 0.45, 0.20),
        "FoliagePine" => Color::srgb(0.16, 0.40, 0.24),
        "FoliageWildflowers" => Color::srgb(0.35, 0.55, 0.25),
        "GrayboxFloor" => Color::srgb(0.22, 0.40, 0.18),
        "GrayboxProp" => Color::srgb(0.55, 0.50, 0.45),
        "GrayboxVault" => Color::srgb(0.70, 0.50, 0.30),
        "GroundDirt" => Color::srgb(0.30, 0.22, 0.14),
        "GroundGrass" => BOTW_GRASS_ROOT_COLOR,
        "GroundLeaves" => Color::srgb(0.38, 0.28, 0.16),
        "GroundPath" => Color::srgb(0.48, 0.38, 0.26),
        "Horse" => Color::srgb(0.42, 0.23, 0.10),
        "Ladder" => Color::srgb(0.50, 0.35, 0.20),
        "Moon" => Color::srgb(0.85, 0.90, 1.00),
        "PickupFood" => Color::srgb(0.80, 0.20, 0.20),
        "PickupMaterial" => Color::srgb(0.45, 0.30, 0.15),
        "PickupWeapon" => Color::srgb(0.60, 0.60, 0.65),
        "Player" => Color::srgb(0.18, 0.42, 0.78),
        "Probe" => Color::srgb(0.85, 0.30, 0.25),
        "Steel" => Color::srgb(0.70, 0.70, 0.75),
        "String" => Color::srgb(0.90, 0.90, 0.95),
        "Sun" => Color::srgb(1.00, 0.93, 0.75),
        "Target" => Color::srgb(0.85, 0.25, 0.20),
        "TargetPost" => Color::srgb(0.35, 0.30, 0.25),
        "TreeTrunk" => Color::srgb(0.40, 0.30, 0.20),
        "Wood" => Color::srgb(0.40, 0.25, 0.15),
        unknown => {
            error!("[assets] palette schema has no color for {unknown}");
            Color::srgb(1.0, 0.0, 1.0)
        }
    };
    let mut material = matte(color);
    if matches!(key, "Sun" | "Moon" | "String") {
        material.unlit = true;
    }
    if matches!(
        key,
        "FoliageCommon"
            | "FoliageDry"
            | "FoliageCard"
            | "FoliageGnarled"
            | "FoliagePine"
            | "FoliageWildflowers"
    ) {
        material.cull_mode = None;
        material.double_sided = true;
        material.perceptual_roughness = 0.95;
        material.reflectance = 0.05;
    }
    match key {
        "FoliageCard" => {
            material.base_color = Color::WHITE;
            material.base_color_texture =
                Some(asset_server.load("textures/props/T_GrassCard_Albedo.png"));
            material.alpha_mode = AlphaMode::Mask(0.4);
        }
        "GroundGrass" => {
            material.base_color = Color::WHITE;
            material.base_color_texture = Some(load_repeating_texture(
                asset_server,
                "textures/terrain/T_GroundGrass_Albedo.png",
            ));
            material.normal_map_texture = Some(load_repeating_texture(
                asset_server,
                "textures/terrain/T_GroundGrass_Normal.png",
            ));
        }
        "GroundDirt" => {
            material.base_color = Color::WHITE;
            material.base_color_texture = Some(load_repeating_texture(
                asset_server,
                "textures/terrain/T_GroundDirt_Albedo.png",
            ));
        }
        "GroundPath" => {
            material.base_color = Color::WHITE;
            material.base_color_texture = Some(load_repeating_texture(
                asset_server,
                "textures/terrain/T_GroundPath_Albedo.png",
            ));
        }
        "GroundLeaves" => {
            material.base_color = Color::WHITE;
            material.base_color_texture = Some(load_repeating_texture(
                asset_server,
                "textures/terrain/T_GroundLeaves_Albedo.png",
            ));
        }
        _ => {}
    }
    material
}

#[derive(Component)]
pub struct AuthoredVisualRoot;

#[derive(Component)]
pub(super) struct AuthoredMaterialApplied;

type UnmappedMaterial = (
    With<MeshMaterial3d<StandardMaterial>>,
    Without<AuthoredMaterialApplied>,
);

pub(super) fn remap_authored_materials(
    mut commands: Commands,
    palette: Res<MaterialPalette>,
    meshes: Query<(Entity, &GltfMaterialName), UnmappedMaterial>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<AuthoredVisualRoot>>,
) {
    for (entity, material_name) in &meshes {
        if !parents
            .iter_ancestors(entity)
            .any(|ancestor| roots.contains(ancestor))
        {
            continue;
        }
        let Some(key) = material_name.0.strip_prefix("M_") else {
            error!(
                "[assets] authored mesh uses non-palette material {}",
                material_name.0
            );
            continue;
        };
        let Some(handle) = palette.get(key) else {
            error!("[assets] authored mesh uses unknown palette key {key}");
            continue;
        };
        commands
            .entity(entity)
            .try_insert((MeshMaterial3d(handle.clone()), AuthoredMaterialApplied));
    }
}

#[derive(Component)]
pub(super) struct AuthoredLodApplied;

pub(super) fn apply_authored_lod(
    mut commands: Commands,
    meshes: Query<(Entity, &GltfMeshName), Without<AuthoredLodApplied>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<AuthoredVisualRoot>>,
) {
    for (entity, mesh_name) in &meshes {
        if !parents
            .iter_ancestors(entity)
            .any(|ancestor| roots.contains(ancestor))
        {
            continue;
        }
        let Some((_, level)) = render_lod(&mesh_name.0) else {
            continue;
        };
        let range = match level {
            0 => VisibilityRange {
                start_margin: 0.0..0.0,
                end_margin: 24.0..30.0,
                use_aabb: false,
            },
            1 => VisibilityRange {
                start_margin: 20.0..26.0,
                end_margin: 48.0..58.0,
                use_aabb: false,
            },
            _ => VisibilityRange {
                start_margin: 50.0..56.0,
                end_margin: 64.0..70.0,
                use_aabb: false,
            },
        };
        commands
            .entity(entity)
            .try_insert((range, AuthoredLodApplied));
    }
}

#[derive(Component)]
pub(super) struct CollisionRenderingStripped;

type RenderedCollisionHelper = (With<Mesh3d>, Without<CollisionRenderingStripped>);

pub(super) fn strip_collision_rendering(
    mut commands: Commands,
    meshes: Query<(Entity, &GltfMeshName), RenderedCollisionHelper>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<AuthoredVisualRoot>>,
) {
    for (entity, mesh_name) in &meshes {
        if !is_collision_name(&mesh_name.0)
            || !parents
                .iter_ancestors(entity)
                .any(|ancestor| roots.contains(ancestor))
        {
            continue;
        }
        commands
            .entity(entity)
            .remove::<Mesh3d>()
            .remove::<MeshMaterial3d<StandardMaterial>>()
            .try_insert(CollisionRenderingStripped);
    }
}

#[derive(Component)]
pub(super) struct AuthoredExtrasValidated;

pub(super) fn validate_authored_extras(
    mut commands: Commands,
    extras: Query<(Entity, &GltfExtras), Without<AuthoredExtrasValidated>>,
    parents: Query<&ChildOf>,
    roots: Query<(), With<AuthoredVisualRoot>>,
) {
    for (entity, extras) in &extras {
        if !parents
            .iter_ancestors(entity)
            .any(|ancestor| roots.contains(ancestor))
        {
            continue;
        }
        if let Err(error) = serde_json::from_str::<serde_json::Value>(&extras.value) {
            error!("[assets] malformed authored GltfExtras: {error}");
            continue;
        }
        commands.entity(entity).try_insert(AuthoredExtrasValidated);
    }
}

fn load_repeating_texture(asset_server: &AssetServer, path: &'static str) -> Handle<Image> {
    use bevy::image::{
        ImageAddressMode, ImageFilterMode, ImageLoaderSettings, ImageSampler,
        ImageSamplerDescriptor,
    };
    asset_server
        .load_builder()
        .with_settings(|settings: &mut ImageLoaderSettings| {
            settings.sampler = ImageSampler::Descriptor(ImageSamplerDescriptor {
                address_mode_u: ImageAddressMode::Repeat,
                address_mode_v: ImageAddressMode::Repeat,
                address_mode_w: ImageAddressMode::Repeat,
                mag_filter: ImageFilterMode::Linear,
                min_filter: ImageFilterMode::Linear,
                mipmap_filter: ImageFilterMode::Linear,
                anisotropy_clamp: 16,
                ..default()
            });
        })
        .load(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asset_test_app() -> App {
        use bevy::asset::AssetApp;
        let mut app = App::new();
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default()));
        // `palette_material` loads terrain albedo textures, which allocates
        // `Image` handles — the asset type must be registered or that panics.
        app.init_asset::<bevy::image::Image>();
        app.finish();
        app
    }

    #[test]
    fn every_palette_material_is_matte_and_non_metallic() {
        let app = asset_test_app();
        let asset_server = app.world().resource::<AssetServer>().clone();
        for key in PALETTE_KEYS {
            let material = palette_material(key, &asset_server);
            assert!(material.perceptual_roughness >= 0.8, "{key}");
            assert_eq!(material.metallic, 0.0, "{key}");
        }
    }

    #[test]
    fn duplicate_palette_requests_return_the_same_handle() {
        let mut app = asset_test_app();
        app.init_resource::<Assets<StandardMaterial>>();
        app.init_resource::<MaterialPalette>();
        let palette = app.world().resource::<MaterialPalette>();
        assert_eq!(palette.get("Bark"), palette.get("Bark"));
    }
}
