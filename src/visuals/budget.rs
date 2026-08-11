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

use crate::visuals::material_registry::mesh_triangles;

/// A single mesh over this many triangles gets a warning. Sized for graybox: a
/// primitive proxy is hundreds of triangles, so thousands means a detailed
/// model slipped in where a placeholder belongs.
const TRIANGLE_WARN: usize = 2_000;

/// Meshes load asynchronously, so an entity may carry a `Mesh3d` handle for
/// several frames before the asset is ready. This retries until the mesh
/// resolves, then records it as checked so the count runs exactly once.
#[derive(Component)]
pub(super) struct TriangleChecked;

/// A mesh whose triangle count is a design decision, not an accident.
///
/// The watchdog asks "did a detailed model slip in where a placeholder
/// belongs?", and for the terrain grid and the meadow's chunks the answer is
/// permanently no: they are baked in code, their counts are derived, and
/// `perf::budget` already holds them to a declared ceiling in a test. Without
/// this marker the meadow alone put a hundred warnings into a log that is
/// supposed to start silent — and a log that cries wolf about the one thing it
/// cannot fix is a log nobody reads when it warns about something real.
#[derive(Component)]
pub(crate) struct BakedByDesign;

/// Meshes still awaiting a count: everything that is not already checked and not
/// baked on purpose.
type UncheckedMesh<'a> = (Entity, &'a Mesh3d, Option<&'a Name>);
type Unchecked = (Without<TriangleChecked>, Without<BakedByDesign>);

pub(super) fn warn_on_heavy_meshes(
    mut commands: Commands,
    meshes: Res<Assets<Mesh>>,
    pending: Query<UncheckedMesh, Unchecked>,
) {
    for (entity, mesh3d, name) in &pending {
        let Some(mesh) = meshes.get(&mesh3d.0) else {
            continue; // Not loaded yet — check again next frame.
        };
        commands.entity(entity).try_insert(TriangleChecked);

        let triangles = mesh_triangles(mesh);
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
    use bevy::ecs::system::RunSystemOnce;
    use bevy::render::mesh::Indices;
    use bevy::render::mesh::PrimitiveTopology;

    fn mesh_with_triangles(count: usize) -> Mesh {
        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        );
        let index_count = u32::try_from(count * 3).expect("test mesh must fit in u32 indices");
        let indices: Vec<u32> = (0..index_count).collect();
        mesh.insert_indices(Indices::U32(indices));
        mesh
    }

    #[test]
    fn triangle_count_reads_indexed_meshes() {
        // The watchdog's whole job hinges on this arithmetic being right.
        let mesh = mesh_with_triangles(6265); // a real CommonTree primitive
        assert_eq!(mesh_triangles(&mesh), 6265);
        assert!(
            mesh_triangles(&mesh) > TRIANGLE_WARN,
            "such a mesh must trip the warning"
        );
    }

    #[test]
    fn a_graybox_primitive_stays_under_budget() {
        // A cylinder + cone proxy is a few hundred triangles.
        let proxy = mesh_with_triangles(320);
        assert!(mesh_triangles(&proxy) <= TRIANGLE_WARN);
    }

    /// Estos tres tests, a diferencia de los dos de arriba, corren
    /// `warn_on_heavy_meshes` de verdad: un bug en la lógica de producción
    /// (usar siempre `count_vertices()`, invertir la comparación, no
    /// exceptuar `BakedByDesign`, o reprocesar una malla ya chequeada) tiene
    /// que poder tumbarlos.
    #[test]
    fn the_watchdog_marks_a_loaded_heavy_mesh_as_checked() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let handle = world
            .resource_mut::<Assets<Mesh>>()
            .add(mesh_with_triangles(6265));
        let entity = world.spawn((Mesh3d(handle), Name::new("test-mesh"))).id();

        world.run_system_once(warn_on_heavy_meshes).unwrap();

        assert!(world.get::<TriangleChecked>(entity).is_some());
    }

    #[test]
    fn the_watchdog_skips_meshes_baked_by_design() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        let handle = world
            .resource_mut::<Assets<Mesh>>()
            .add(mesh_with_triangles(6265));
        let entity = world.spawn((Mesh3d(handle), BakedByDesign)).id();

        world.run_system_once(warn_on_heavy_meshes).unwrap();

        assert!(world.get::<TriangleChecked>(entity).is_none());
    }

    #[test]
    fn the_watchdog_leaves_an_unloaded_mesh_unmarked_to_retry_later() {
        let mut world = World::new();
        world.init_resource::<Assets<Mesh>>();
        // A handle whose asset was never inserted models the frame gap
        // between spawning `Mesh3d` and the asset finishing its load.
        let handle: Handle<Mesh> = Handle::default();
        let entity = world.spawn(Mesh3d(handle)).id();

        world.run_system_once(warn_on_heavy_meshes).unwrap();

        assert!(world.get::<TriangleChecked>(entity).is_none());
    }
}
