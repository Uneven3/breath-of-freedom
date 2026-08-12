//! What the ground **is**, cell by cell — the level's semantic layer.
//!
//! The height grid owns shape; this layer owns meaning without naming render
//! assets. Properties come from one table, so changing a kind never repaints a
//! level and impossible combinations cannot be authored.

use serde::{Deserialize, Serialize};

use bof_domain::asset_pipeline::schema::SurfaceKind;

/// What a terrain cell is made of.
///
/// Deliberately small. Each arm has to be a thing the player can tell apart by
/// standing on it, or it is a distinction the map author cannot author.
///
/// **Water is not here on purpose.** A pool is not a property of a cell: it
/// needs a surface height, and depth comes from the relief underneath it. It
/// belongs to the water plane the swim/dive motors are planned against, not to
/// this grid.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Hash, Serialize, Deserialize)]
pub enum TerrainKind {
    /// Bare earth. The default: an unpainted level is all soil.
    #[default]
    Soil,
    /// Low ground cover. It gives the world its ordinary grass texture, but is
    /// neither the cuttable fuel nor the visual wall of [`Self::TallGrass`].
    ShortGrass,
    /// Exposed stone. Nothing grows on it and nothing burns.
    Rock,
    /// Waist-high grass — the cover that hides you and the fuel that spreads
    /// fire. The reason this layer exists at all.
    TallGrass,
    Sand,
}

/// Everything that follows from a cell's kind. One row per [`TerrainKind`].
pub struct KindProps {
    /// What the editor calls it. Spanish, like the rest of the tool's HUD.
    pub label: &'static str,
    /// What it sounds like underfoot. `movement` records it into `GroundFacts`,
    /// `sfx` turns it into a sound — the simulation never branches on it (§20).
    pub surface: SurfaceKind,
    /// Fire can take hold and spread here.
    pub flammable: bool,
    /// A blade clears it.
    pub cuttable: bool,
}

static KINDS: &[KindProps] = &[
    KindProps {
        label: "Tierra",
        surface: SurfaceKind::Dirt,
        flammable: false,
        cuttable: false,
    },
    KindProps {
        label: "Pasto corto",
        surface: SurfaceKind::Grass,
        flammable: false,
        cuttable: false,
    },
    KindProps {
        label: "Pasto largo",
        surface: SurfaceKind::Grass,
        flammable: true,
        cuttable: true,
    },
    KindProps {
        label: "Roca",
        surface: SurfaceKind::Stone,
        flammable: false,
        cuttable: false,
    },
    KindProps {
        label: "Arena",
        surface: SurfaceKind::Sand,
        flammable: false,
        cuttable: false,
    },
];

impl TerrainKind {
    /// Every kind, in the order the editor offers them.
    pub const ALL: [TerrainKind; 5] = [
        TerrainKind::Soil,
        TerrainKind::ShortGrass,
        TerrainKind::TallGrass,
        TerrainKind::Rock,
        TerrainKind::Sand,
    ];

    pub fn props(self) -> &'static KindProps {
        match self {
            TerrainKind::Soil => &KINDS[0],
            TerrainKind::ShortGrass => &KINDS[1],
            TerrainKind::TallGrass => &KINDS[2],
            TerrainKind::Rock => &KINDS[3],
            TerrainKind::Sand => &KINDS[4],
        }
    }

    pub fn label(self) -> &'static str {
        self.props().label
    }

    /// The surface an actor standing on this cell reports.
    pub fn surface(self) -> SurfaceKind {
        self.props().surface
    }

    /// Whether fire spreads here. No consumer yet — the mechanic comes later;
    /// this is the data it will read, and the editor HUD already shows it so an
    /// author can see what they painted.
    pub fn flammable(self) -> bool {
        self.props().flammable
    }

    /// Whether a blade clears it. Same status as [`TerrainKind::flammable`].
    pub fn cuttable(self) -> bool {
        self.props().cuttable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_maps_to_exactly_one_row() {
        assert_eq!(KINDS.len(), TerrainKind::ALL.len());
        for (kind, expected) in TerrainKind::ALL.into_iter().zip(KINDS) {
            assert!(std::ptr::eq(kind.props(), expected));
        }
    }

    #[test]
    fn labels_are_unique_and_non_empty() {
        // The label is how the author picks a kind in the HUD; two kinds sharing
        // one makes the palette unusable.
        let mut labels: Vec<&str> = KINDS.iter().map(|props| props.label).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "two kinds share a label");
        assert!(KINDS.iter().all(|props| !props.label.is_empty()));
    }

    #[test]
    fn grass_cover_and_tall_grass_are_not_the_same_authored_meaning() {
        // Tall grass is fuel and cuttable cover; short grass is ordinary ground
        // cover. Sharing footstep audio is fine — their gameplay contract is not.
        assert!(TerrainKind::TallGrass.flammable());
        assert!(TerrainKind::TallGrass.cuttable());
        assert!(!TerrainKind::ShortGrass.flammable());
        assert!(!TerrainKind::ShortGrass.cuttable());
        assert!(!TerrainKind::Rock.flammable());
        assert!(!TerrainKind::Rock.cuttable());
    }

    #[test]
    fn an_unpainted_level_is_soil() {
        // `Default` is what fills a level with no semantic layer on disk, so it
        // has to be the neutral ground, not a special material.
        assert_eq!(TerrainKind::default(), TerrainKind::Soil);
    }

    #[test]
    fn bare_soil_is_the_neutral_default_not_grass_cover() {
        assert_eq!(TerrainKind::Soil.surface(), SurfaceKind::Dirt);
        assert_eq!(TerrainKind::ShortGrass.surface(), SurfaceKind::Grass);
        assert_eq!(TerrainKind::TallGrass.surface(), SurfaceKind::Grass);
    }
}
