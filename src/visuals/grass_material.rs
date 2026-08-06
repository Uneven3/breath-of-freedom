//! Custom WGSL Material Extension for BOTW Grass rendering pipeline.
//!
//! Provides the uniform data structure (`GrassUniform`) and registers
//! `ExtendedMaterial<StandardMaterial, GrassExtension>` with Bevy's PBR pipeline.

use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialPlugin};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::shader::ShaderRef;

#[derive(Clone, Copy, ShaderType, Debug, Reflect)]
pub struct GrassUniform {
    pub root_color: LinearRgba,
    pub tip_color: LinearRgba,
    pub sun_direction: Vec3,
    pub sss_amount: f32,
    pub time: f32,
    /// Where the camera stands, in world XZ. The outermost blades shrink with
    /// their distance to this point, so a chunk is always culled *after* its
    /// blades have already gone to nothing.
    pub focus_xz: Vec2,
    /// How many metres **one** blade takes to grow from nothing to full height.
    /// Short: a single blade growing is invisible, what the eye catches is a
    /// whole band of them growing at once.
    pub growth_ramp: f32,
    /// Over how many metres, inward from each ring's edge, the blades'
    /// individual thresholds are spread by their hash. Long: this is what turns
    /// a wave travelling with the player into a field that thins out with
    /// distance. See `blade_growth` in `grass.wgsl`.
    pub growth_spread: f32,
    /// How far **below** the ground a blade collapses to as it shrinks, in
    /// metres. Collapsing to ground level leaves a flat quad lying coplanar with
    /// the terrain, which z-fights and flickers; see the vertex shader.
    pub growth_sink: f32,
    /// Wind direction in world XZ, normalised, and how far a tip travels at
    /// full gust as a fraction of the blade's height.
    pub wind_dir: Vec2,
    pub wind_strength: f32,
    /// Metres per second the gust front travels across the field.
    pub wind_speed: f32,
    /// How much a blade's colour may drift from the shared gradient. Without it
    /// a field of one green reads as carpet, however dense it is.
    pub tint_variation: f32,
}

impl Default for GrassUniform {
    fn default() -> Self {
        Self {
            // The meadow overrides these with its own authored pair; the
            // defaults exist so the material is usable without one.
            root_color: LinearRgba::from(Color::srgb(0.22, 0.40, 0.18)),
            tip_color: LinearRgba::from(Color::srgb(0.35, 0.65, 0.20)),
            sun_direction: Vec3::new(0.3, 0.8, 0.5).normalize(),
            sss_amount: 0.4,
            time: 0.0,
            focus_xz: Vec2::ZERO,
            // Shrinks nothing by default: the meadow overwrites both every
            // frame, and a material used without one should not shrink.
            growth_ramp: 0.0,
            growth_spread: 0.0,
            growth_sink: 0.0,
            wind_dir: Vec2::new(0.80, 0.60),
            // A tip leaning a fifth of its height at full gust. Grass that
            // bends further reads as wheat.
            wind_strength: 0.22,
            wind_speed: 1.7,
            tint_variation: 0.16,
        }
    }
}

#[derive(Asset, AsBindGroup, TypePath, Debug, Clone)]
pub struct GrassExtension {
    #[uniform(100)]
    pub grass_data: GrassUniform,
    #[texture(101)]
    #[sampler(102)]
    pub interaction_map: Option<Handle<Image>>,
}

impl MaterialExtension for GrassExtension {
    fn vertex_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }

    fn fragment_shader() -> ShaderRef {
        "shaders/grass.wgsl".into()
    }
}

pub type GrassMaterial = ExtendedMaterial<StandardMaterial, GrassExtension>;

pub struct GrassMaterialPlugin;

impl Plugin for GrassMaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(MaterialPlugin::<GrassMaterial>::default());
    }
}
