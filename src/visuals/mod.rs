//! Presentation visuals, decoupled from the physics bodies.
//!
//! Every simulation actor gets a disposable visual entity that interpolates
//! toward it each render frame; the simulation entity carries no mesh,
//! material or asset handle. One submodule per visual family:
//!
//! - [`player`] — the player's collision-matched capsule, bow, interpolation.
//! - [`enemy`] — enemy capsules + awareness tint.
//! - [`grass`] — the decorative meadow: authored tufts swaying in a wind field.
//! - [`horse`] — horse graybox capsule.
//! - [`probe`] — the TraversalProbe dummy's capsule.
//! - [`vfx`] — transient effects (swing arc placeholder).

use bevy::prelude::*;

mod arrows;
pub mod budget;
pub mod catalog;
mod diagnostic;
pub mod enemy;
pub mod foliage;
pub mod forest;
pub(crate) mod grass;
pub mod grass_material;
pub mod horse;
pub mod player;
pub mod probe;
mod terrain;
pub mod terrain_material;
pub mod vfx;

pub use bof_domain::visuals::VisualOf;
pub use catalog::{AppearanceBinding, AppearanceKey, TreeSilhouette, VisualCatalog, VisualSlot};
pub use player::PlayerVisual;

/// Exponential decay rate for visual smoothing, fed to
/// [`StableInterpolate::smooth_nudge`](bevy::math::StableInterpolate::smooth_nudge).
///
/// It is a *rate*, not a per-frame fraction. The previous form —
/// `(RATE * dt).clamp(0.0, 1.0)` as a lerp factor — was frame-rate dependent
/// in a way that bit exactly where it hurt most: at 20 fps the factor reached
/// 1.0 and the smoothing vanished entirely, so the visuals snapped instead of
/// easing. Low framerate did not just show fewer frames, it silently swapped
/// the follow behaviour for a different one, which also made every benchmark
/// step judge feel on a different curve.
pub(crate) const INTERPOLATION_SPEED: f32 = 20.0;
pub(crate) const SNEAK_Y_OFFSET: f32 = -0.4;

/// Minimal public contract for tools that must ignore the temporary material
/// representation while the diagnostic view enters or leaves overdraw.
#[derive(Resource, Default)]
pub(crate) struct DiagnosticViewState {
    pub overdraw_material_override: bool,
}

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(diagnostic::DiagnosticViewsPlugin);
        app.init_resource::<VisualCatalog>();
        // Startup keeps only what outlives a scene: loaded assets and shared
        // meshes. The player's visual and the meadow are scene content
        // (`crate::scene`), so they are built on entry and die on exit.
        app.add_plugins(grass_material::GrassMaterialPlugin);
        app.add_plugins(terrain_material::TerrainMaterialPlugin);
        app.add_systems(
            Startup,
            (forest::build_tree_proxy_assets, arrows::init_assets),
        );
        for id in crate::scene::SceneId::ALL {
            app.add_systems(
                OnEnter(crate::scene::AppState::Scene(id)),
                grass::spawn_meadow.run_if(crate::scene::scene_has(|c| c.meadow)),
            );
        }
        app.add_systems(
            Update,
            (
                (
                    player::spawn_visual,
                    player::interpolate_visual,
                    player::animate_bow_visual,
                ),
                (probe::spawn_probe_visual, probe::interpolate_probe_visual),
                (
                    enemy::spawn_enemy_visual,
                    enemy::interpolate_enemy_visual,
                    enemy::tint_enemy_visual,
                ),
                (horse::spawn_horse_visual, horse::interpolate_horse_visual),
                // One generic orphan-despawn over VisualOf replaces the three
                // per-family copies (enemy/horse/probe).
                despawn_orphaned_visuals,
                (
                    forest::sync_tree_visuals,
                    forest::finalize_tree_visual_swaps,
                    forest::apply_forest_perf,
                )
                    .chain(),
                (
                    foliage::apply_foliage_material_policy,
                    foliage::apply_foliage_lod,
                    foliage::apply_shadow_caster_budget,
                ),
                // One system, and only when the density dial moves: the field is
                // baked meshes, so nothing walks the blades per frame.
                grass::rebuild_meadow_on_density_change,
                terrain::sync_terrain_visual,
                budget::warn_on_heavy_meshes,
                (vfx::spawn_swing_vfx, vfx::fade_swing_vfx),
                (
                    arrows::sync_visuals,
                    arrows::spawn_trails,
                    arrows::tick_trails,
                )
                    .chain(),
            ),
        );
    }
}

/// The disposable graybox family visuals that follow an actor and must vanish
/// with it. Catalog-driven visuals (player, trees) and pooled arrow visuals own
/// their own lifetime and are deliberately excluded.
type GrayboxFamilyVisual = Or<(
    With<player::PlayerVisual>,
    With<enemy::EnemyVisual>,
    With<horse::HorseVisual>,
    With<probe::TraversalProbeVisual>,
)>;

/// Despawn any disposable graybox family visual (enemy / horse / probe) whose
/// owning actor entity is gone. One system over the shared `VisualOf` link
/// replaces three byte-identical per-family copies.
fn despawn_orphaned_visuals(
    mut commands: Commands,
    visuals: Query<(Entity, &VisualOf), GrayboxFamilyVisual>,
    entities: Query<Entity>,
) {
    for (visual, owner) in &visuals {
        if !entities.contains(owner.0) {
            commands.entity(visual).despawn();
        }
    }
}
