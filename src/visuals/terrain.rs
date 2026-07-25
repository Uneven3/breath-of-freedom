//! The terrain visual: a flat-shaded low-poly mesh generated from the world's
//! height grid ([`crate::world::Terrain`]).
//!
//! Presentation only — the mesh is rebuilt whenever the grid changes (including
//! the frame it first appears), so future sculpting shows up live. Each triangle
//! owns its vertices and a single face normal, for the faceted look.

use std::f32::consts::FRAC_1_SQRT_2;

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
///
/// **Refreshes in place.** A sculpt stroke fires this every frame, and minting a
/// fresh 32k-triangle asset each time — allocating it, registering it in the
/// render world, dropping the old one — is what made sculpting cost 20-27 ms a
/// frame while the terrain itself rendered at 60 FPS. Writing over the existing
/// asset keeps the handle, the GPU buffers and the pipeline entry alive.
pub(super) fn sync_terrain_visual(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    terrain: Query<&Terrain, Changed<Terrain>>,
    existing: Query<&Mesh3d, With<TerrainVisual>>,
) {
    let Ok(terrain) = terrain.single() else {
        return;
    };
    if let Ok(current) = existing.single()
        && let Some(mut mesh) = meshes.get_mut(&current.0)
    {
        write_terrain_mesh(terrain, &mut mesh);
        return;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    write_terrain_mesh(terrain, &mut mesh);
    commands.spawn((
        Name::new("TerrainVisual"),
        TerrainVisual,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(palette.handle("GroundGrass")),
        Transform::default(),
    ));
}

/// Flat-shaded mesh: two triangles per grid quad, each triangle wound
/// counter-clockwise seen from above so its face normal points up.
///
/// Writes into `mesh`, replacing whatever it held. The vertex count only depends
/// on the grid resolution, so a refresh reuses the same buffers and the index
/// buffer never has to change.
fn write_terrain_mesh(terrain: &Terrain, mesh: &mut Mesh) {
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
                    let u = (v.x - v.z) * FRAC_1_SQRT_2 / 4.0;
                    let v_uv = (v.x + v.z) * FRAC_1_SQRT_2 / 4.0;
                    uvs.push([u, v_uv]);
                }
            }
        }
    }

    let count = positions.len() as u32;
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    // Sequential indices: only re-issued when the buffer is not already the
    // right length, which after the first build it always is.
    if mesh.indices().map(|i| i.len()) != Some(count as usize) {
        mesh.insert_indices(Indices::U32((0..count).collect()));
    }
}
