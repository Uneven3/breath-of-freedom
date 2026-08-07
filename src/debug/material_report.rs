//! On-demand material breakdown: turns the scene section's `mats=N` total into
//! *which* materials are in view and how far the count could collapse.
//!
//! The scene inventory (`perf::budget`) counts distinct production material
//! handles among visible meshes — the axis the mobile budget flags first — but a
//! bare total cannot say whether those are 60 different looks or 5 looks with 55
//! redundant handles. Standard materials are grouped by a quantised "look";
//! the unique layered terrain material is reported separately because its array
//! is deliberately not interchangeable with a palette material.

use bevy::asset::AssetId;
use bevy::camera::visibility::ViewVisibility;
use bevy::color::Srgba;
use bevy::pbr::MeshMaterial3d;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use super::channel::{DebugAction, DebugActionRequest};
use crate::perf::PerfToggles;
use crate::visuals::DiagnosticViewState;
use crate::visuals::terrain_material::TerrainMaterial;

/// The last breakdown's one-line headline, so the overlay can confirm on screen
/// that a click landed — the log is the detail, but a click needs feedback the
/// moment it happens. Set on every action, even the skipped/empty paths.
#[derive(Resource, Default)]
pub struct MaterialReportNotice {
    /// `Time<Real>` seconds when produced, so the overlay can expire it.
    pub at: f32,
    pub summary: Option<String>,
}

impl MaterialReportNotice {
    fn set(&mut self, at: f32, summary: String) {
        self.at = at;
        self.summary = Some(summary);
    }
}

/// A quantised look. Colour is coarse so near-matches group; roughness/metallic
/// are quantised too; the base texture is exact, so a textured material never
/// folds into a flat one that merely shares its tint.
#[derive(PartialEq, Eq, Hash, Debug)]
struct LookKey {
    base_texture: Option<AssetId<Image>>,
    color: [u32; 3],
    roughness: u32,
    metallic: u32,
}

fn quantise(value: f32, steps: f32) -> u32 {
    // Rounded integers have an exact f32 representation at these small step
    // counts; their bits are therefore a stable, hashable bucket identity.
    (value.clamp(0.0, 1.0) * steps).round().to_bits()
}

fn look_key(material: &StandardMaterial) -> LookKey {
    let color = material.base_color.to_srgba();
    LookKey {
        base_texture: material
            .base_color_texture
            .as_ref()
            .map(|handle| handle.id()),
        color: [
            quantise(color.red, 31.0),
            quantise(color.green, 31.0),
            quantise(color.blue, 31.0),
        ],
        roughness: quantise(material.perceptual_roughness, 20.0),
        metallic: quantise(material.metallic, 20.0),
    }
}

#[derive(Default)]
struct LookStat {
    handles: HashSet<AssetId<StandardMaterial>>,
    meshes: u32,
    color: Srgba,
    roughness: f32,
    metallic: f32,
    textured: bool,
}

type SceneMaterial<'a> = (&'a ViewVisibility, &'a MeshMaterial3d<StandardMaterial>);
type TerrainSceneMaterial<'a> = (&'a ViewVisibility, &'a MeshMaterial3d<TerrainMaterial>);

/// Scans once when the hub's "Material breakdown" action fires and logs the
/// table. Read-only over presentation; nothing here mutates the scene (§20).
#[allow(clippy::too_many_arguments)]
pub(super) fn log_material_breakdown(
    mut requests: MessageReader<DebugActionRequest>,
    meshes: Query<SceneMaterial>,
    terrain_meshes: Query<TerrainSceneMaterial>,
    materials: Res<Assets<StandardMaterial>>,
    terrain_materials: Res<Assets<TerrainMaterial>>,
    perf: Res<PerfToggles>,
    diagnostic: Res<DiagnosticViewState>,
    inventory: Res<crate::perf::budget::SceneInventory>,
    time: Res<Time<Real>>,
    mut notice: ResMut<MaterialReportNotice>,
) {
    if !requests
        .read()
        .any(|DebugActionRequest(action)| *action == DebugAction::MaterialBreakdown)
    {
        return;
    }
    let now = time.elapsed_secs();
    // The overdraw diagnostic swaps materials, so a sample taken then would
    // describe the diagnostic, not the production scene.
    if perf.overdraw || diagnostic.overdraw_material_override {
        warn!("[materials] skipped — the overdraw diagnostic is swapping materials");
        notice.set(now, "MATERIALS · omitido (overdraw activo)".into());
        return;
    }

    let mut looks: HashMap<LookKey, LookStat> = HashMap::default();
    let mut handles: HashSet<AssetId<StandardMaterial>> = HashSet::default();
    let mut total_meshes = 0u32;
    let mut terrain_handles: HashSet<AssetId<TerrainMaterial>> = HashSet::default();
    let mut terrain_mesh_count = 0u32;
    let mut terrain_look = None;

    for (visibility, material) in &meshes {
        if !visibility.get() {
            continue; // Frustum-, hierarchy- or range-culled: never submitted.
        }
        let Some(resolved) = materials.get(&material.0) else {
            continue;
        };
        handles.insert(material.0.id());
        total_meshes += 1;

        let stat = looks.entry(look_key(resolved)).or_default();
        stat.handles.insert(material.0.id());
        stat.meshes += 1;
        stat.color = resolved.base_color.to_srgba();
        stat.roughness = resolved.perceptual_roughness;
        stat.metallic = resolved.metallic;
        stat.textured = resolved.base_color_texture.is_some();
    }
    for (visibility, material) in &terrain_meshes {
        if !visibility.get() {
            continue;
        }
        let Some(resolved) = terrain_materials.get(&material.0) else {
            continue;
        };
        terrain_handles.insert(material.0.id());
        terrain_mesh_count += 1;
        terrain_look = Some((
            resolved.base.perceptual_roughness,
            resolved.base.metallic,
            resolved.base.base_color_texture.is_some() || resolved.extension.has_textures(),
        ));
    }

    if handles.is_empty() && terrain_handles.is_empty() {
        info!("[materials] no visible materials to report");
        notice.set(now, "MATERIALS · sin materiales visibles".into());
        return;
    }

    // **La misma omisión, por tercera vez.** Este desglose enumera tipos de
    // material a mano igual que lo hacían el inventario y la vista de overdraw,
    // y le falta `GrassMaterial` desde que existe. Agrupar por *look* sí es
    // específico de `StandardMaterial` —los otros no tienen color base ni
    // rugosidad comparables—, así que la lista no se puede borrar del todo; lo
    // que sí se puede es que no mienta. El inventario cuenta todos los tipos
    // (`visuals::material_registry`), y la diferencia se declara.
    let total_handles = handles.len() + terrain_handles.len();
    let uncharacterised = inventory.materials.saturating_sub(total_handles);
    if uncharacterised > 0 {
        warn!(
            "[materials] {uncharacterised} de {} materiales visibles no entran en este \
             desglose: son de un tipo que agrupa por otra cosa que un look de \
             `StandardMaterial`. El total de la escena está en la sección `mats`.",
            inventory.materials,
        );
    }
    let distinct_looks = looks.len() + terrain_handles.len();
    let reducible = total_handles.saturating_sub(distinct_looks);
    let percent = 100.0 * reducible as f64 / total_handles as f64;
    notice.set(
        now,
        format!(
            "MATERIALS → log · {} mats · {:.0}% reducible",
            total_handles, percent
        ),
    );

    // Most-duplicated looks first: those are the biggest palette wins.
    let mut rows: Vec<&LookStat> = looks.values().collect();
    rows.sort_by_key(|stat| std::cmp::Reverse(stat.handles.len()));

    info!("[materials] ---- breakdown (visible production materials) ----");
    info!(
        "[materials] {} materials across {} meshes → {} distinct looks ({} reducible, {:.0}%)",
        total_handles,
        total_meshes + terrain_mesh_count,
        distinct_looks,
        reducible,
        percent
    );
    info!(
        "[materials] {:<22} {:>5} {:>7} {:>6} {:>6} {:>4}",
        "look (srgb)", "mats", "meshes", "rough", "metal", "tex"
    );
    for stat in rows.iter().take(20) {
        info!(
            "[materials] {:<22} {:>5} {:>7} {:>6.2} {:>6.2} {:>4}",
            format!(
                "({:.2},{:.2},{:.2})",
                stat.color.red, stat.color.green, stat.color.blue
            ),
            stat.handles.len(),
            stat.meshes,
            stat.roughness,
            stat.metallic,
            if stat.textured { "yes" } else { "-" },
        );
    }
    if let Some((roughness, metallic, textured)) = terrain_look {
        info!(
            "[materials] {:<22} {:>5} {:>7} {:>6.2} {:>6.2} {:>4}",
            "terrain texture array",
            terrain_handles.len(),
            terrain_mesh_count,
            roughness,
            metallic,
            if textured { "yes" } else { "-" }
        );
    }
    if rows.len() > 20 {
        info!("[materials] … {} more looks omitted", rows.len() - 20);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn matte(r: f32, g: f32, b: f32) -> StandardMaterial {
        StandardMaterial {
            base_color: Color::srgb(r, g, b),
            perceptual_roughness: 0.9,
            metallic: 0.0,
            ..default()
        }
    }

    #[test]
    fn near_identical_colors_share_a_look() {
        // Two colours a few thousandths apart, away from bin edges, collapse to
        // one look. (Colours straddling a bin edge may split — a conservative
        // under-count of reducibility, acceptable for a diagnostic.)
        assert_eq!(
            look_key(&matte(0.25, 0.45, 0.15)),
            look_key(&matte(0.252, 0.448, 0.152))
        );
    }

    #[test]
    fn distinct_colors_stay_separate() {
        assert_ne!(
            look_key(&matte(0.27, 0.50, 0.22)),
            look_key(&matte(0.42, 0.23, 0.10))
        );
    }

    #[test]
    fn roughness_and_metallic_split_a_shared_color() {
        let mut glossy = matte(0.5, 0.5, 0.5);
        glossy.perceptual_roughness = 0.1;
        assert_ne!(look_key(&matte(0.5, 0.5, 0.5)), look_key(&glossy));
    }
}
