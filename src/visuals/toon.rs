//! Toon material: the standard PBR material extended with banded lighting
//! (`assets/shaders/toon.wgsl`). Applied to world geometry; actor visuals
//! keep plain `StandardMaterial` for now (their tint/flash systems mutate
//! it), which doubles as an in-game comparison of stepped vs continuous.

use bevy::material::OpaqueRendererMethod;
use bevy::pbr::{ExtendedMaterial, MaterialExtension};
use bevy::prelude::*;
use bevy::render::render_resource::AsBindGroup;
use bevy::shader::ShaderRef;

pub type ToonMaterial = ExtendedMaterial<StandardMaterial, ToonExtension>;

/// Number of discrete light bands (Zelda-style cel look).
const TOON_BANDS: u32 = 4;

#[derive(Asset, AsBindGroup, Reflect, Debug, Clone)]
pub struct ToonExtension {
    // Base material bindings occupy slots 0-99; extensions start at 100.
    #[uniform(100)]
    pub bands: u32,
}

impl Default for ToonExtension {
    fn default() -> Self {
        Self { bands: TOON_BANDS }
    }
}

impl MaterialExtension for ToonExtension {
    fn fragment_shader() -> ShaderRef {
        "shaders/toon.wgsl".into()
    }

    fn deferred_fragment_shader() -> ShaderRef {
        "shaders/toon.wgsl".into()
    }
}

/// A flat-color toon material for graybox geometry. Fully matte: the cel
/// look dies if PBR's specular lobe survives (surfaces read "glassy"), so
/// roughness is maxed and reflectance zeroed — banded diffuse only.
pub fn toon_color(color: Color) -> ToonMaterial {
    ExtendedMaterial {
        base: StandardMaterial {
            base_color: color,
            perceptual_roughness: 1.0,
            metallic: 0.0,
            reflectance: 0.0,
            opaque_render_method: OpaqueRendererMethod::Auto,
            ..default()
        },
        extension: ToonExtension::default(),
    }
}
