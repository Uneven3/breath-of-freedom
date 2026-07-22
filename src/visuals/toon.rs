//! Experimental toon material retained for future visual comparisons.
//!
//! The shipped baseline uses Bevy's `StandardMaterial`; this extension is not
//! registered or instantiated during normal startup.

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
