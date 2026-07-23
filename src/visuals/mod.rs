//! Presentation visuals, decoupled from the physics bodies.
//!
//! Every simulation actor gets a disposable visual entity that interpolates
//! toward it each render frame; the simulation entity carries no mesh,
//! material or asset handle. One submodule per visual family:
//!
//! - [`player`] — the player rig (GLB scene), bow visual, body interpolation.
//! - [`animation`] — the player `AnimationGraph`, state→clip mapping and the
//!   F7 debug clip browser.
//! - [`enemy`] — enemy capsules + awareness tint.
//! - [`horse`] — horse graybox capsule.
//! - [`probe`] — the TraversalProbe dummy's capsule.
//! - [`vfx`] — transient effects (swing arc placeholder).

use bevy::prelude::*;

pub mod animation;
pub mod budget;
pub mod catalog;
mod diagnostic;
pub mod enemy;
pub mod foliage;
pub mod forest;
pub mod horse;
pub mod player;
pub mod probe;
pub mod vfx;

pub use animation::{AnimationDebug, PlayerAnimations};
pub use catalog::{
    AppearanceBinding, AppearanceKey, PLAYER_APPEARANCE, TreeSilhouette, VisualCatalog, VisualSlot,
};
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

/// Uniform link from any visual root back to its simulation actor, so
/// cross-cutting presentation effects (jelly, hit flash — see
/// `presentation::juice`) treat player/probe/enemy visuals alike.
#[derive(Component, Clone, Copy)]
pub struct VisualOf(pub Entity);

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
        app.init_resource::<AnimationDebug>();
        app.init_resource::<VisualCatalog>();
        app.add_systems(
            Startup,
            (
                player::spawn_visual,
                animation::start_loading_animations,
                forest::build_tree_proxy_assets,
            ),
        );
        app.add_systems(
            Update,
            (
                (
                    player::link_player_visual,
                    player::interpolate_visual,
                    player::animate_bow_visual,
                ),
                (
                    probe::spawn_probe_visual,
                    probe::despawn_orphaned_probe_visual,
                    probe::interpolate_probe_visual,
                ),
                (
                    enemy::spawn_enemy_visual,
                    enemy::despawn_orphaned_enemy_visual,
                    enemy::interpolate_enemy_visual,
                    enemy::tint_enemy_visual,
                ),
                (
                    horse::spawn_horse_visual,
                    horse::despawn_orphaned_horse_visual,
                    horse::interpolate_horse_visual,
                ),
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
                budget::warn_on_heavy_meshes,
                (vfx::spawn_swing_vfx, vfx::fade_swing_vfx),
                (
                    animation::compile_animation_graph,
                    animation::init_player_animation_graph,
                    animation::animate_player,
                ),
            ),
        );
    }
}
