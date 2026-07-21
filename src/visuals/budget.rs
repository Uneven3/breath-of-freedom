//! Triangle-budget watchdog.
//!
//! Graybox has to be cost-honest: if a placeholder is expensive in a way the
//! final asset will not be, every performance number measured against it is a
//! lie. This counts the triangles on each mesh as it loads and warns when one
//! blows the budget, so an over-heavy asset announces itself in the log instead
//! of hiding inside a frame-time regression discovered days later.
//!
//! It is asset-agnostic — any named `Mesh3d` is checked, not just trees.

use bevy::prelude::*;
use bevy::render::mesh::Mesh3d;

/// A single mesh over this many triangles gets a warning. Sized for graybox: a
/// primitive proxy is hundreds of triangles, so thousands means a detailed
/// model slipped in where a placeholder belongs.
const TRIANGLE_WARN: usize = 2_000;

/// Meshes load asynchronously, so an entity may carry a `Mesh3d` handle for
/// several frames before the asset is ready. This retries until the mesh
/// resolves, then records it as checked so the count runs exactly once.
#[derive(Component)]
pub(super) struct TriangleChecked;

pub(super) fn warn_on_heavy_meshes(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    pending: Query<(Entity, &Mesh3d, Option<&Name>), Without<TriangleChecked>>,
) {
    for (entity, mesh3d, name) in &pending {
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue; // Not loaded yet — check again next frame.
        };
        commands.entity(entity).try_insert(TriangleChecked);

        let triangles = match mesh.indices() {
            Some(indices) => indices.len() / 3,
            // Non-indexed meshes list every vertex per triangle.
            None => mesh.count_vertices() / 3,
        };
        if triangles > TRIANGLE_WARN {
            let who = name.map(Name::as_str).unwrap_or("<unnamed mesh>");
            warn!(
                "[budget] {who}: {triangles} triangles (over {TRIANGLE_WARN}) — \
                 too heavy for graybox; needs an LOD or a cheaper representation"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::asset::RenderAssetUsages;
    use bevy::render::mesh::Indices;
    use bevy::render::mesh::PrimitiveTopology;

    fn mesh_with_triangles(count: usize) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        let indices: Vec<u32> = (0..(count * 3) as u32).collect();
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

    #[test]
    fn triangle_count_reads_indexed_meshes() {
        // The watchdog's whole job hinges on this arithmetic being right.
        let mesh = mesh_with_triangles(6265); // a real CommonTree primitive
        let triangles = mesh.indices().unwrap().len() / 3;
        assert_eq!(triangles, 6265);
        assert!(
            triangles > TRIANGLE_WARN,
            "such a mesh must trip the warning"
        );
    }

    #[test]
    fn a_graybox_primitive_stays_under_budget() {
        // A cylinder + cone proxy is a few hundred triangles.
        let proxy = mesh_with_triangles(320);
        assert!(proxy.indices().unwrap().len() / 3 <= TRIANGLE_WARN);
    }
}
