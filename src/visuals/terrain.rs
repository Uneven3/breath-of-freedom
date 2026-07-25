//! The terrain visual: a flat-shaded low-poly mesh generated from the world's
//! height grid ([`crate::world::Terrain`]).
//!
//! Presentation only — the mesh is rebuilt whenever the grid changes (including
//! the frame it first appears), so future sculpting shows up live. Each triangle
//! owns its vertices and a single face normal, for the faceted look.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::asset_pipeline::MaterialPalette;
use crate::world::Terrain;

/// Marks the single terrain visual entity, so a rebuild replaces its mesh in
/// place instead of spawning a second one.
#[derive(Component)]
pub(super) struct TerrainVisual;

/// Build or refresh the terrain mesh whenever the grid changes. `Changed`
/// covers the first frame the terrain appears (added implies changed) and every
/// later sculpt edit, with no manual dirty flag to keep in sync.
pub(super) fn sync_terrain_visual(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    terrain: Query<&Terrain, Changed<Terrain>>,
    existing: Query<Entity, With<TerrainVisual>>,
) {
    let Ok(terrain) = terrain.single() else {
        return;
    };
    let mesh = meshes.add(build_terrain_mesh(terrain));
    if let Ok(visual) = existing.single() {
        commands.entity(visual).insert(Mesh3d(mesh));
    } else {
        commands.spawn((
            Name::new("TerrainVisual"),
            TerrainVisual,
            Mesh3d(mesh),
            MeshMaterial3d(palette.handle("GroundGrass")),
            Transform::default(),
        ));
    }
}

/// Flat-shaded mesh: two triangles per grid quad, each triangle wound
/// counter-clockwise seen from above so its face normal points up.
fn build_terrain_mesh(terrain: &Terrain) -> Mesh {
    let cells = terrain.points() - 1;
    let capacity = cells * cells * 6;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(capacity);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity);

    for row in 0..cells {
        for col in 0..cells {
            let a = terrain.point_world_pos(row, col);
            let b = terrain.point_world_pos(row + 1, col);
            let c = terrain.point_world_pos(row + 1, col + 1);
            let d = terrain.point_world_pos(row, col + 1);
            for tri in [[a, d, c], [a, c, b]] {
                let normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
                for v in tri {
                    positions.push([v.x, v.y, v.z]);
                    normals.push([normal.x, normal.y, normal.z]);
                    let u = (v.x * 0.7071 - v.z * 0.7071) / 4.0;
                    let v_uv = (v.x * 0.7071 + v.z * 0.7071) / 4.0;
                    uvs.push([u, v_uv]);
                }
            }
        }
    }

    let count = positions.len() as u32;
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32((0..count).collect()));
    mesh
}
