//! Native Bevy material profiles used by authored and graybox visuals.

use bevy::prelude::*;

/// Matte PBR baseline: standard Bevy lighting with restrained highlights.
///
/// Keeping this as `StandardMaterial` lets world geometry share Bevy's normal
/// render path with actors and imported assets. Atmosphere belongs in the
/// palette, lights and environment rather than a mandatory custom shader.
pub fn matte_color(color: Color) -> StandardMaterial {
    StandardMaterial {
        base_color: color,
        perceptual_roughness: 0.9,
        metallic: 0.0,
        reflectance: 0.2,
        ..default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matte_profile_keeps_native_lighting_and_restrains_specular() {
        let color = Color::srgb(0.2, 0.4, 0.6);
        let material = matte_color(color);

        assert_eq!(material.base_color, color);
        assert!(!material.unlit);
        assert_eq!(material.metallic, 0.0);
        assert!(material.perceptual_roughness >= 0.8);
        assert!(material.reflectance <= 0.25);
    }
}
