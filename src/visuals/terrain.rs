//! The terrain visual: a flat-shaded low-poly mesh generated from the world's
//! height grid ([`crate::world::Terrain`]).
//!
//! Presentation only — the mesh is rebuilt whenever the grid changes (including
//! the frame it first appears), so future sculpting shows up live. Each triangle
//! owns its vertices and a single face normal, for the faceted look.

use bevy::asset::RenderAssetUsages;
use bevy::prelude::*;
use bevy::render::mesh::{Indices, PrimitiveTopology};

use crate::visuals::terrain_material::{TerrainMaterialAssets, semantic_vertex_data};
use crate::world::{NonClimbable, Terrain};

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
    terrain_material: Res<TerrainMaterialAssets>,
    terrain: Query<(&Terrain, Has<NonClimbable>), Changed<Terrain>>,
    existing: Query<&Mesh3d, With<TerrainVisual>>,
) {
    let Ok((terrain, non_climbable)) = terrain.single() else {
        return;
    };
    if let Ok(current) = existing.single()
        && let Some(mut mesh) = meshes.get_mut(&current.0)
    {
        write_terrain_mesh(terrain, !non_climbable, &mut mesh);
        return;
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    );
    write_terrain_mesh(terrain, !non_climbable, &mut mesh);
    commands.spawn((
        Name::new("TerrainVisual"),
        TerrainVisual,
        Mesh3d(meshes.add(mesh)),
        MeshMaterial3d(terrain_material.material.clone()),
        Transform::default(),
    ));
}

/// Flat-shaded mesh: two triangles per grid quad, each triangle wound
/// counter-clockwise seen from above so its face normal points up.
///
/// **The quad is cut along the anti-diagonal `d → b`, to match the collider.**
/// That is not a free choice: the four corners of a cell are only coplanar on
/// even ground, so the other diagonal draws a genuinely different surface, and
/// parry's heightfield — the thing bodies actually stand on — uses this one. This
/// mesh used the main diagonal until 2026-07-26, which put the visible ground up
/// to 0.33 m away from the ground you could walk on wherever the relief twisted.
/// See `world::terrain::Terrain::to_collider`.
///
/// Writes into `mesh`, replacing whatever it held. The vertex count only depends
/// on the grid resolution, so a refresh reuses the same buffers and the index
/// buffer never has to change.
fn write_terrain_mesh(terrain: &Terrain, climbable: bool, mesh: &mut Mesh) {
    let cells = terrain.points() - 1;
    let capacity = cells * cells * 6;
    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(capacity);
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(capacity);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(capacity);
    let mut colors: Vec<[f32; 4]> = Vec::with_capacity(capacity);

    for row in 0..cells {
        for col in 0..cells {
            let a = terrain.point_world_pos(row, col);
            let b = terrain.point_world_pos(row + 1, col);
            let c = terrain.point_world_pos(row + 1, col + 1);
            let d = terrain.point_world_pos(row, col + 1);
            // Per cell, not per vertex: the semantic layer lives on cells, and
            // every vertex of this quad is already its own (flat shading), so the
            // patch edges stay hard. Softening them here would be a lie about a
            // grid whose resolution the author can see.
            let semantics = semantic_vertex_data(terrain.kind(row, col), climbable);
            for tri in [[a, d, b], [d, c, b]] {
                let raw_normal = (tri[1] - tri[0]).cross(tri[2] - tri[0]).normalize_or_zero();
                let normal = raw_normal.lerp(Vec3::Y, 0.75).normalize_or_zero();
                for v in tri {
                    positions.push([v.x, v.y, v.z]);
                    normals.push([normal.x, normal.y, normal.z]);
                    let u = (v.x + 160.0) / 12.0;
                    let v_uv = (v.z + 160.0) / 12.0;
                    uvs.push([u, v_uv]);
                    colors.push(semantics);
                }
            }
        }
    }

    let count = positions.len() as u32;
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    // The layered terrain shader treats colour as a compact semantic payload:
    // texture layer, climbable, flammable, cuttable. Vertices are duplicated
    // per cell, so these values never interpolate across semantic boundaries.
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, colors);
    // Sequential indices: only re-issued when the buffer is not already the
    // right length, which after the first build it always is.
    if mesh.indices().map(|i| i.len()) != Some(count as usize) {
        mesh.insert_indices(Indices::U32((0..count).collect()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::render::mesh::VertexAttributeValues;

    /// Every triangle this mesh emits must lie on the surface `Terrain::height_at`
    /// describes — which is the surface the collider is checked against, so this
    /// is the link that makes "what you see is what you walk on" a chain rather
    /// than two hopes.
    ///
    /// It was the untested gap: `world` checked its grid against the collider and
    /// `visuals` built a mesh from the same grid, and nothing compared *these two*
    /// triangulations. They disagreed by up to 0.33 m wherever the relief twisted.
    ///
    /// Centroids, not vertices: a vertex is a grid point, where both
    /// triangulations agree by construction, so sampling vertices proves nothing.
    /// The inside of the quad is where a wrong diagonal shows.
    #[test]
    fn every_triangle_lies_on_the_surface_the_simulation_walks() {
        let mut terrain = crate::world::Terrain::flat_for_test();
        terrain.raise_area(Vec2::ZERO, 60.0, 12.0);
        terrain.noise_area(Vec2::ZERO, 150.0, 6.0, 7);

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        write_terrain_mesh(&terrain, true, &mut mesh);
        let Some(VertexAttributeValues::Float32x3(positions)) =
            mesh.attribute(Mesh::ATTRIBUTE_POSITION)
        else {
            panic!("the mesh must have float3 positions");
        };
        assert_eq!(
            positions.len() % 3,
            0,
            "positions must form whole triangles"
        );

        let mut worst = 0.0_f32;
        for tri in positions.chunks_exact(3) {
            let centroid = tri
                .iter()
                .fold(Vec3::ZERO, |sum, v| sum + Vec3::from_array(*v))
                / 3.0;
            let error = (centroid.y - terrain.height_at(centroid.xz())).abs();
            worst = worst.max(error);
        }
        assert!(
            worst < 0.001,
            "a drawn triangle sits {worst} m off the walkable surface"
        );
    }
}
