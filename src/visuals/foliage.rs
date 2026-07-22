//! Foliage cost control: material policy and a shadow-caster budget.
//!
//! Both fixes target the same measured problem — the forest is expensive in
//! *every* pass, not just the visible one — and both are written to keep
//! working as the world grows.
//!
//! **Materials.** The vendor glTFs declare trunk materials `alphaMode: MASK`
//! and `doubleSided: true`. Their textures are fully opaque (alpha 255
//! everywhere, verified against the source PNGs), so the alpha test buys
//! nothing while it disables early-Z, and back faces of a closed trunk are
//! never visible while they double the fragments rasterised. Trunks are ~70% of
//! forest triangles, so this is the cheapest root-cause fix available. Leaves
//! keep both flags: their textures are 70-82% non-opaque, genuinely cut-out
//! cards that need two-sided rendering.
//!
//! The vendor files are never edited (`AHORA.md`: optimise recipes and
//! derivatives, not sources). The override happens on the loaded material
//! asset, which is what presentation is for.
//!
//! **Shadow casters.** A tree past a modest distance contributes a shadow
//! nobody can resolve, yet it is re-rendered into every cascade. Budgeting
//! casters by distance is the lever that scales: no matter how large the map
//! becomes, only trees near the camera ever cost shadow time.

use bevy::camera::visibility::VisibilityRange;
use bevy::gltf::GltfMaterialName;
use bevy::light::NotShadowCaster;
use bevy::pbr::MeshMaterial3d;
use bevy::prelude::*;
use bevy::render::render_resource::Face;

use super::forest::TreeVisual;
use crate::camera::CameraRig;
use crate::perf::PerfToggles;

/// Substring identifying trunk materials in the Quaternius nature kit.
const TRUNK_MATERIAL: &str = "Bark";

/// Dead band around the caster-budget boundary, in metres.
const HYSTERESIS: f32 = 6.0;

/// Over how many metres a tree fades out at the cull boundary.
///
/// Hiding it outright pops, which reads worse than the cost it saves. Bevy
/// dithers across this band instead — and it evaluates the distance itself, in
/// one tight system, rather than this module walking every mesh each frame.
const LOD_FADE: f32 = 12.0;

/// Marks a foliage mesh whose material policy has been applied, so the fixup
/// runs once per entity and the shadow budget has a flat query to work with
/// instead of walking the hierarchy every frame.
#[derive(Component)]
pub struct FoliageMesh;

/// A foliage mesh made of leaves rather than trunk. Separated because the two
/// have opposite cost profiles: trunks are opaque geometry, leaves are
/// alpha-tested cards that overlap themselves heavily — the expensive part of
/// every pass, and four times over in the shadow cascades.
#[derive(Component)]
pub struct FoliageLeaf;

/// Rewrites trunk materials as they finish loading.
///
/// Mutating the shared material asset is deliberate: every CommonTree instance
/// references the same `Bark_NormalTree` material, so one edit fixes all of
/// them and no per-instance material is created (which would break batching —
/// the opposite of the goal).
pub(super) fn apply_foliage_material_policy(
    mut commands: Commands,
    meshes: Query<
        (Entity, &GltfMaterialName, &MeshMaterial3d<StandardMaterial>),
        Without<FoliageMesh>,
    >,
    parents: Query<&ChildOf>,
    trees: Query<(), With<TreeVisual>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut seen: Local<(u32, u32)>,
) {
    let (mut trunks, mut leaves) = *seen;
    let before = *seen;

    for (entity, material_name, handle) in &meshes {
        if !descends_from_tree(entity, &parents, &trees) {
            continue;
        }
        commands.entity(entity).try_insert(FoliageMesh);

        if !material_name.0.contains(TRUNK_MATERIAL) {
            leaves += 1;
            commands.entity(entity).try_insert(FoliageLeaf);
            continue; // Leaves genuinely need the alpha test and both faces.
        }
        let Some(mut material) = materials.get_mut(&handle.0) else {
            continue;
        };
        if matches!(material.alpha_mode, AlphaMode::Mask(_)) {
            material.alpha_mode = AlphaMode::Opaque;
        }
        material.double_sided = false;
        material.cull_mode = Some(Face::Back);
        trunks += 1;
    }

    // This whole fix depends on assumptions about someone else's asset — that
    // the loader attaches material names, and that the meshes land under the
    // tree root. If either is wrong the system finds nothing and silently
    // changes nothing, which is indistinguishable from "the fix did not help".
    //
    // The running total is reported, not the first batch: glTF scenes finish
    // loading over many frames, so a one-shot report announces whatever
    // happened to be ready that frame and reads as "only 45 of 179 were fixed".
    *seen = (trunks, leaves);
    if *seen != before {
        info!("[visuals] foliage policy applied: {trunks} trunk meshes, {leaves} leaf meshes");
    }
}

fn descends_from_tree(
    entity: Entity,
    parents: &Query<&ChildOf>,
    trees: &Query<(), With<TreeVisual>>,
) -> bool {
    // Scene roots nest a few levels; walking up is bounded and only happens
    // once per mesh, on the frame its scene finishes loading.
    parents
        .iter_ancestors(entity)
        .any(|ancestor| trees.contains(ancestor))
}

type CasterQuery<'a> = (
    Entity,
    &'a GlobalTransform,
    Has<NotShadowCaster>,
    Has<FoliageLeaf>,
);

/// Hands distance culling to the engine.
///
/// `VisibilityRange` is per-entity and does not propagate to children, which is
/// why this lives here rather than on the tree root: `FoliageMesh` marks the
/// actual meshes, so they can carry it directly. Rewritten only when the knob
/// moves — the per-frame distance test is Bevy's, not ours.
pub(super) fn apply_foliage_lod(
    mut commands: Commands,
    perf: Res<PerfToggles>,
    foliage: Query<Entity, With<FoliageMesh>>,
    added: Query<Entity, Added<FoliageMesh>>,
    mut applied: Local<Option<Option<f32>>>,
) {
    let cull = perf.cull_distance();
    if *applied != Some(cull) {
        *applied = Some(cull);
        for entity in &foliage {
            apply_lod_to_entity(&mut commands, entity, cull);
        }
    } else {
        for entity in &added {
            apply_lod_to_entity(&mut commands, entity, cull);
        }
    }
}

fn apply_lod_to_entity(commands: &mut Commands, entity: Entity, cull: Option<f32>) {
    match cull {
        Some(max) => {
            commands.entity(entity).try_insert(VisibilityRange {
                start_margin: 0.0..0.0,
                end_margin: (max - LOD_FADE).max(0.0)..max,
                use_aabb: false,
            });
        }
        None => {
            commands.entity(entity).remove::<VisibilityRange>();
        }
    }
}

/// Applies the distance budget: past the configured range a tree stops casting
/// into the shadow cascades.
///
/// Presentation only — the trunk collider, `TreeKind` and every `FixedUpdate`
/// result are untouched, so this can never change gameplay
/// (`ARCHITECTURE.md`: LOD and culling only change visual entities).
pub(super) fn apply_shadow_caster_budget(
    mut commands: Commands,
    perf: Res<PerfToggles>,
    camera: Single<&GlobalTransform, With<CameraRig>>,
    foliage: Query<CasterQuery, With<FoliageMesh>>,
) {
    let eye = camera.translation();
    let range = perf.shadow_caster_range();

    for (entity, transform, muted, is_leaf) in &foliage {
        // Hysteresis: a tree must be clearly inside to start casting and
        // clearly outside to stop. Without the gap, running through the forest
        // makes dozens of meshes straddle the threshold every frame, and each
        // flip is an archetype move — the most expensive thing an ECS does.
        // A static benchmark never sees this cost, which is exactly how it got
        // shipped.
        if is_leaf && !perf.leaf_shadows {
            if !muted {
                commands.entity(entity).try_insert(NotShadowCaster);
            }
            continue;
        }
        let distance = transform.translation().distance(eye);
        let should_cast = match range {
            None => true,
            Some(max) if muted => distance <= max - HYSTERESIS,
            Some(max) => distance <= max + HYSTERESIS,
        };
        match (should_cast, muted) {
            (true, true) => {
                commands.entity(entity).remove::<NotShadowCaster>();
            }
            (false, false) => {
                commands.entity(entity).try_insert(NotShadowCaster);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unchanged_lod_only_configures_new_foliage() {
        let mut world = World::new();
        let perf = PerfToggles {
            cull_step: 1,
            ..default()
        };
        world.insert_resource(perf);
        let existing = world.spawn(FoliageMesh).id();
        let system = world.register_system(apply_foliage_lod);

        world.run_system(system).unwrap();
        assert!(world.entity(existing).contains::<VisibilityRange>());

        world.entity_mut(existing).remove::<VisibilityRange>();
        let added = world.spawn(FoliageMesh).id();
        world.run_system(system).unwrap();

        assert!(
            !world.entity(existing).contains::<VisibilityRange>(),
            "unchanged foliage must not be rewritten every frame"
        );
        assert!(world.entity(added).contains::<VisibilityRange>());
    }
}
