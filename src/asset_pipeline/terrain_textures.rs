//! Canonical terrain texture contract shared by the build and runtime.
//!
//! Keep this module independent of Bevy: `build.rs` includes it directly so a
//! broken source asset fails before the game is compiled.

pub const TERRAIN_TEXTURE_EDGE: u32 = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TerrainTextureSpec {
    /// Path relative to Bevy's `assets/` root.
    pub path: &'static str,
    /// Honest fallback and semantic-debug colour while the final art is absent.
    pub placeholder_rgb: [u8; 3],
}

/// Array order is part of the shader/mesh contract. Append new kinds; do not
/// reorder existing layers without migrating saved terrain and the mapping in
/// `visuals::terrain_material`.
pub const TERRAIN_TEXTURES: [TerrainTextureSpec; 4] = [
    TerrainTextureSpec {
        path: "textures/terrain/T_GroundSoil_Albedo.png",
        placeholder_rgb: [121, 81, 58],
    },
    TerrainTextureSpec {
        path: "textures/terrain/T_GroundRock_Albedo.png",
        placeholder_rgb: [125, 130, 140],
    },
    TerrainTextureSpec {
        path: "textures/terrain/T_GroundTallGrass_Albedo.png",
        placeholder_rgb: [79, 155, 69],
    },
    TerrainTextureSpec {
        path: "textures/terrain/T_GroundSand_Albedo.png",
        placeholder_rgb: [216, 194, 116],
    },
];
