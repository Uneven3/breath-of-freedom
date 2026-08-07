//! Custom WGSL Material Extension for BOTW Grass rendering pipeline.
//!
//! Provides the uniform data structure (`GrassUniform`) and registers
//! `ExtendedMaterial<StandardMaterial, GrassExtension>` with Bevy's PBR pipeline.

use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, ShaderType};
use bevy::render::storage::ShaderBuffer;
use bevy::shader::ShaderRef;

use crate::visuals::material_registry::InstrumentedMaterialAppExt;

#[derive(Clone, Copy, ShaderType, Debug, Reflect)]
pub struct GrassUniform {
    pub root_color: LinearRgba,
    pub tip_color: LinearRgba,
    pub sun_direction: Vec3,
    pub sss_amount: f32,
    pub time: f32,
    /// Dónde está la cámara en XZ: un chunk se descarta *después* de que sus
    /// briznas ya encogieron a nada.
    pub focus_xz: Vec2,
    /// Cuántos metros tarda **una** brizna en crecer entera. Corto: una brizna
    /// creciendo es invisible, una banda entera creciendo a la vez no.
    pub growth_ramp: f32,
    /// En cuántos metros se reparten los umbrales por su hash. Largo: es lo que
    /// convierte una ola que viaja con el jugador en un raleo con la distancia.
    pub growth_spread: f32,
    /// Hasta cuánto **bajo** el suelo colapsa una brizna. No cero: coplanar con
    /// el terreno hace z-fighting, que en pantalla es un parpadeo.
    pub growth_sink: f32,
    /// Dirección del viento en XZ y cuánto viaja la punta a ráfaga plena.
    pub wind_dir: Vec2,
    pub wind_strength: f32,
    pub wind_speed: f32,
    /// Cuánto puede apartarse el color de una brizna del degradado común. Sin
    /// esto, un campo de un solo verde se lee como alfombra por densa que sea.
    pub tint_variation: f32,
    /// Cuánto aguanta el color de la raíz antes de ceder al de la punta: **0 es
    /// rampa lineal y 1 su cuadrado**. El porqué, en `BOTWGrass.md`.
    pub gradient_bias: f32,
    /// Desde qué distancia ralea la pradera. Ver `GROWTH_START_M`.
    pub growth_start: f32,
    /// Índice de la vista de diagnóstico. Ver `visuals::grass_debug`.
    pub debug_view: u32,
    /// Ancho de brizna en metros; la vista `subpixel` divide por él.
    pub blade_width: f32,
    /// Los alcances de los anillos: con ellos el shader ancla la ley `1/d` en el
    /// borde interno del anillo de cada brizna. Ver `ring_inner`.
    pub ring_reaches_a: Vec4,
    pub ring_reaches_b: Vec4,
    /// El tamaño de chunk de cada anillo: con él el fragment deduce de qué celda
    /// —o sea de qué draw— salió una brizna.
    pub ring_chunks_a: Vec4,
    pub ring_chunks_b: Vec4,
    /// Qué anillos son cartas: 1 si su primitiva se abre mirando a la cámara.
    /// Por anillo y no como shader def, que duplicaría los draws.
    pub ring_cards_a: Vec4,
    pub ring_cards_b: Vec4,
    /// Medio ancho de una carta, en metros.
    pub card_half_width: f32,
    /// Cuánto se hunde la base bajo el suelo, cuánto puede inclinarse la punta y
    /// a qué altura está la cintura. Los necesita el vertex shader, que ahora
    /// construye la brizna entera.
    pub blade_root_sink: f32,
    pub blade_lean: f32,
    pub blade_waist: f32,
    /// `x` = registros por chunk de este nivel, `y` = su forma (`BladeShape`).
    pub record_layout: UVec4,
    /// La paleta de diagnóstico, un color por anillo.
    pub ring_colors: [Vec4; crate::visuals::grass_debug::PALETTE_SLOTS],
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
            // **Apagado el 2026-08-06 por decisión del usuario** mientras se
            // diagnostica: era el piso de ruido del 5% entre dos capturas del
            // mismo encuadre. El valor jugado y aceptado era 0,22.
            wind_strength: 0.0,
            wind_speed: 1.7,
            tint_variation: 0.16,
            gradient_bias: 1.0,
            growth_start: 1.0e9,
            debug_view: 0,
            // La pradera lo escribe desde `BLADE_WIDTH`, su única fuente.
            blade_width: 0.0,
            ring_reaches_a: Vec4::ZERO,
            ring_reaches_b: Vec4::ZERO,
            ring_chunks_a: Vec4::ONE,
            ring_chunks_b: Vec4::ONE,
            ring_cards_a: Vec4::ZERO,
            ring_cards_b: Vec4::ZERO,
            card_half_width: 0.0,
            blade_root_sink: 0.0,
            blade_lean: 0.0,
            blade_waist: 0.0,
            record_layout: UVec4::ZERO,
            ring_colors: [Vec4::ONE; crate::visuals::grass_debug::PALETTE_SLOTS],
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
    /// Un registro por brizna del nivel que ya no hornea. **No es opcional
    /// aunque el nivel no lo use**: el macro no pasa por `Option`.
    #[storage(103, read_only)]
    pub blade_records: Handle<ShaderBuffer>,
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
        // No `MaterialPlugin` a secas: ver `visuals::material_registry`.
        app.add_instrumented_material::<GrassMaterial>();
    }
}
