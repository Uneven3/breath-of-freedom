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
    /// Where the camera stands, in world XZ. A chunk is always culled *after*
    /// its blades have already shrunk to nothing.
    pub focus_xz: Vec2,
    /// How many metres **one** blade takes to grow to full height. Short — one
    /// blade growing is invisible, a whole band growing at once is not.
    pub growth_ramp: f32,
    /// Over how many metres the blades' thresholds are spread by their hash.
    /// Long — this is what turns a wave travelling with the player into a field
    /// that thins with distance.
    pub growth_spread: f32,
    /// How far **below** the ground a blade collapses to. Not zero: coplanar
    /// with the terrain it z-fights, which on screen is a flicker.
    pub growth_sink: f32,
    /// Dirección del viento en XZ y cuánto viaja la punta a ráfaga plena.
    pub wind_dir: Vec2,
    pub wind_strength: f32,
    pub wind_speed: f32,
    /// How much a blade's colour may drift from the shared gradient. Without it
    /// a field of one green reads as carpet, however dense it is.
    pub tint_variation: f32,
    /// How far the root's colour holds before the tip's takes over. **0 is a
    /// linear ramp and 1 is its square.** Linear is why the field read as a pale
    /// haze: a canopy is mostly tips, so the meadow landed on the midpoint of
    /// its two colours. Two curves and not `pow`, que son dos transcendentales
    /// por fragmento en un frame fill-bound.
    pub gradient_bias: f32,
    /// Desde qué distancia ralea la pradera. Ver `GROWTH_START_M`.
    pub growth_start: f32,
    /// Los alcances de los anillos, para que el shader deduzca el borde interno
    /// del anillo de cada brizna y ancle ahí su ley `1/d`. Ver `ring_inner`.
    pub ring_reaches_a: Vec4,
    pub ring_reaches_b: Vec4,
}

impl Default for GrassUniform {
    fn default() -> Self {
        Self {
            // The meadow overrides these with its own authored pair.
            root_color: LinearRgba::from(Color::srgb(0.22, 0.40, 0.18)),
            tip_color: LinearRgba::from(Color::srgb(0.35, 0.65, 0.20)),
            sun_direction: Vec3::new(0.3, 0.8, 0.5).normalize(),
            sss_amount: 0.4,
            time: 0.0,
            focus_xz: Vec2::ZERO,
            // Shrinks nothing by default; the meadow overwrites these.
            growth_ramp: 0.0,
            growth_spread: 0.0,
            growth_sink: 0.0,
            wind_dir: Vec2::new(0.80, 0.60),
            // **Apagado el 2026-08-06 por decisión del usuario**: confunde más
            // de lo que aporta mientras se diagnostica, y además es el piso de
            // ruido del 5% que impedía comparar dos capturas del mismo encuadre.
            // El shader queda: con amplitud cero no cuesta, y 0,22 —un quinto de
            // la altura a ráfaga plena— era el valor jugado y aceptado.
            wind_strength: 0.0,
            wind_speed: 1.7,
            tint_variation: 0.16,
            gradient_bias: 1.0,
            growth_start: 1.0e9,
            ring_reaches_a: Vec4::ZERO,
            ring_reaches_b: Vec4::ZERO,
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
