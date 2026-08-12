//! Where grass grows — one rule, two consumers.
//!
//! The meadow stamper reads it to decide how many blades a patch of ground
//! gets; the terrain shader reads it to decide how far to tint that same ground
//! toward the colour of a blade's root. They *have* to agree: the entire point
//! of the terrain tint is that the field has no edge, and an edge is precisely
//! what appears the moment the two disagree about where grass ends.
//!
//! So the rule lives here once, in terms of the same two inputs both sides
//! have: the cell's authored meaning and the ground's slope. The shader cannot
//! import Rust, so what crosses to WGSL is not a copy of the logic but its
//! parameters — the thresholds as cosines and the set of grass-bearing kinds as
//! a bitmask. Change the rule here and both sides move together.

use crate::world::TerrainKind;

/// Past this slope the ground reads as rock, and nothing grows.
pub(super) const MAX_SLOPE_DEG: f32 = 45.0;
/// Blades on steep ground are shorter, the way real grass thins out on a bank.
pub(super) const STEEP_SLOPE_DEG: f32 = 35.0;
/// How short, at the steepest slope that still grows anything.
pub(super) const STEEP_SCALE: f32 = 0.65;

/// Whether a cell's authored meaning bears grass at all.
///
/// Stated as an exhaustive match rather than a list so adding a `TerrainKind`
/// forces this decision instead of silently defaulting to bare ground — a new
/// kind that quietly grew no grass would show up as a bald patch nobody
/// authored.
pub(super) const fn kind_bears_grass(kind: TerrainKind) -> bool {
    match kind {
        TerrainKind::ShortGrass | TerrainKind::TallGrass => true,
        TerrainKind::Soil | TerrainKind::Rock | TerrainKind::Sand => false,
    }
}

/// How much grass a spot gets: `0.0` bare, `1.0` full, and a ramp between the
/// steep and the maximum slope.
pub(super) fn coverage(kind: TerrainKind, slope_deg: f32) -> f32 {
    if !kind_bears_grass(kind) || slope_deg > MAX_SLOPE_DEG {
        return 0.0;
    }
    if slope_deg <= STEEP_SLOPE_DEG {
        return 1.0;
    }
    let across = (slope_deg - STEEP_SLOPE_DEG) / (MAX_SLOPE_DEG - STEEP_SLOPE_DEG);
    1.0 - across * (1.0 - STEEP_SCALE)
}

/// The grass-bearing kinds as a bitmask over texture layers, for the terrain
/// shader — which knows a cell by the layer index its vertex colour carries,
/// not by a Rust enum.
pub(super) fn grass_layer_mask() -> u32 {
    TerrainKind::ALL
        .into_iter()
        .filter(|kind| kind_bears_grass(*kind))
        .map(|kind| 1u32 << super::terrain_material::texture_layer(kind))
        .fold(0, |mask, bit| mask | bit)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rock_and_sand_stay_bare_however_flat_they_are() {
        assert_eq!(coverage(TerrainKind::Rock, 0.0), 0.0);
        assert_eq!(coverage(TerrainKind::Sand, 0.0), 0.0);
    }

    #[test]
    fn coverage_falls_off_between_the_steep_and_the_maximum_slope() {
        assert_eq!(coverage(TerrainKind::ShortGrass, 0.0), 1.0);
        assert_eq!(coverage(TerrainKind::ShortGrass, STEEP_SLOPE_DEG), 1.0);
        assert_eq!(
            coverage(TerrainKind::ShortGrass, MAX_SLOPE_DEG),
            STEEP_SCALE
        );
        assert_eq!(coverage(TerrainKind::Soil, MAX_SLOPE_DEG + 0.1), 0.0);
    }

    /// The mask is what crosses into WGSL, so it has to name the same kinds the
    /// Rust side does — a mismatch is exactly the seam this module prevents.
    #[test]
    fn the_shader_mask_names_the_same_kinds_as_the_rule() {
        let mask = grass_layer_mask();
        for kind in TerrainKind::ALL {
            let bit = 1u32 << super::super::terrain_material::texture_layer(kind);
            assert_eq!(
                mask & bit != 0,
                kind_bears_grass(kind),
                "{kind:?} disagrees between the rule and the shader mask"
            );
        }
    }
}
