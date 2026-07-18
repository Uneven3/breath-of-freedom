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
pub mod enemy;
pub mod horse;
pub mod outline;
pub mod player;
pub mod probe;
pub mod toon;
pub mod vfx;

pub use animation::{AnimationDebug, PlayerAnimations};
pub use player::PlayerVisual;
pub use toon::ToonMaterial;

pub(crate) const INTERPOLATION_SPEED: f32 = 20.0;
pub(crate) const SNEAK_Y_OFFSET: f32 = -0.4;

/// Uniform link from any visual root back to its simulation actor, so
/// cross-cutting presentation effects (jelly, hit flash — see
/// `presentation::juice`) treat player/probe/enemy visuals alike.
#[derive(Component, Clone, Copy)]
pub struct VisualOf(pub Entity);

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<ToonMaterial>::default());
        app.add_plugins(outline::OutlinePlugin);
        app.init_resource::<AnimationDebug>();
        app.add_systems(
            Startup,
            (player::spawn_visual, animation::start_loading_animations),
        );
        app.add_systems(
            Update,
            (
                player::link_player_visual,
                player::interpolate_visual,
                player::animate_bow_visual,
                probe::spawn_probe_visual,
                probe::despawn_orphaned_probe_visual,
                probe::interpolate_probe_visual,
                enemy::spawn_enemy_visual,
                enemy::despawn_orphaned_enemy_visual,
                enemy::interpolate_enemy_visual,
                enemy::tint_enemy_visual,
                horse::spawn_horse_visual,
                horse::despawn_orphaned_horse_visual,
                horse::interpolate_horse_visual,
                vfx::spawn_swing_vfx,
                vfx::fade_swing_vfx,
                animation::compile_animation_graph,
                animation::init_player_animation_graph,
                animation::animate_player,
            ),
        );
    }
}
