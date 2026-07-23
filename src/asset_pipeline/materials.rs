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
        let mut entries = Vec::with_capacity(PALETTE_KEYS.len());
        for key in PALETTE_KEYS {
            let material = palette_material(key);
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

fn palette_material(key: &str) -> StandardMaterial {
    let color = match key {
        "Bark" => Color::srgb(0.30, 0.18, 0.09),
        "Fletching" => Color::srgb(0.85, 0.15, 0.15),
        "FoliageCommon" => Color::srgb(0.27, 0.50, 0.22),
        "FoliageGnarled" => Color::srgb(0.36, 0.45, 0.20),
        "FoliagePine" => Color::srgb(0.16, 0.40, 0.24),
        "GrayboxFloor" => Color::srgb(0.40, 0.45, 0.40),
        "GrayboxProp" => Color::srgb(0.55, 0.50, 0.45),
        "GrayboxVault" => Color::srgb(0.70, 0.50, 0.30),
        "Moon" => Color::srgb(0.85, 0.90, 1.00),
        "Steel" => Color::srgb(0.70, 0.70, 0.75),
        "String" => Color::srgb(0.90, 0.90, 0.95),
        "Sun" => Color::srgb(1.00, 0.93, 0.75),
        "Target" => Color::srgb(0.85, 0.25, 0.20),
        "Wood" => Color::srgb(0.40, 0.25, 0.15),
        unknown => {
            error!("[assets] palette schema has no color for {unknown}");
            Color::srgb(1.0, 0.0, 1.0)
        }
    };
    matte(color)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_palette_material_is_matte_and_non_metallic() {
        for key in PALETTE_KEYS {
            let material = palette_material(key);
            assert!(material.perceptual_roughness >= 0.8, "{key}");
            assert_eq!(material.metallic, 0.0, "{key}");
        }
    }

    #[test]
    fn duplicate_palette_requests_return_the_same_handle() {
        let mut world = World::new();
        world.init_resource::<Assets<StandardMaterial>>();
        world.init_resource::<MaterialPalette>();
        let palette = world.resource::<MaterialPalette>();
        assert_eq!(palette.get("Bark"), palette.get("Bark"));
    }
}
