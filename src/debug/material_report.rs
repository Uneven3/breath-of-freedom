//! On-demand material breakdown: turns the scene section's `mats=N` total into
//! *which* materials are in view and how far the count could collapse.
//!
//! The scene inventory (`perf::budget`) counts distinct `StandardMaterial`
//! handles among visible meshes — the axis the mobile budget flags first — but a
//! bare total cannot say whether those are 60 different looks or 5 looks with 55
//! redundant handles. This groups the visible materials by a quantised "look"
//! (colour + roughness + metallic + base texture); materials that land in the
//! same bucket are visually interchangeable and could share one handle. The
//! headline number is how many are reducible, so a shared-material palette has a
//! measured target instead of a guess.

use bevy::asset::AssetId;
use bevy::camera::visibility::ViewVisibility;
use bevy::color::Srgba;
use bevy::pbr::MeshMaterial3d;
use bevy::platform::collections::{HashMap, HashSet};
use bevy::prelude::*;

use super::channel::{DebugAction, DebugActionRequest};
use crate::perf::PerfToggles;
use crate::visuals::DiagnosticViewState;

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
    color: [u8; 3],
    roughness: u8,
    metallic: u8,
}

fn quantise(value: f32, steps: f32) -> u8 {
    (value.clamp(0.0, 1.0) * steps).round() as u8
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

/// Scans once when the hub's "Material breakdown" action fires and logs the
/// table. Read-only over presentation; nothing here mutates the scene (§20).
pub(super) fn log_material_breakdown(
    mut requests: MessageReader<DebugActionRequest>,
    meshes: Query<SceneMaterial>,
    materials: Res<Assets<StandardMaterial>>,
    perf: Res<PerfToggles>,
    diagnostic: Res<DiagnosticViewState>,
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

    if handles.is_empty() {
        info!("[materials] no visible StandardMaterials to report");
        notice.set(now, "MATERIALS · sin materiales visibles".into());
        return;
    }

    let distinct_looks = looks.len();
    let reducible = handles.len().saturating_sub(distinct_looks);
    let percent = 100.0 * reducible as f64 / handles.len() as f64;
    notice.set(
        now,
        format!(
            "MATERIALS → log · {} mats · {:.0}% reducible",
            handles.len(),
            percent
        ),
    );

    // Most-duplicated looks first: those are the biggest palette wins.
    let mut rows: Vec<&LookStat> = looks.values().collect();
    rows.sort_by_key(|stat| std::cmp::Reverse(stat.handles.len()));

    info!("[materials] ---- breakdown (visible StandardMaterials) ----");
    info!(
        "[materials] {} materials across {} meshes → {} distinct looks ({} reducible, {:.0}%)",
        handles.len(),
        total_meshes,
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
