//! What the ground **is**, cell by cell — the level's semantic layer.
//!
//! The height grid says what shape the ground has; this says what it is *made
//! of*. It is data for the simulation and nothing else: no mesh, no texture, no
//! colour is named here. Presentation reads the kind and decides how to show it,
//! the same separation that lets `TreeKind` live in `world` while
//! `VisualCatalog` picks the model — which is what allowed swapping the forest
//! to procedural proxies without touching a line of simulation.
//!
//! **Properties come from the table, not from the brush.** You paint one
//! meaning — "this is tall grass" — and everything that follows from it (what it
//! sounds like underfoot, whether it burns, whether a blade clears it) is looked
//! up here. Two things fall out of that: a cell can never be flammable stone,
//! and changing what tall grass *means* is one row edited, not every level
//! repainted.

use serde::{Deserialize, Serialize};

use crate::asset_pipeline::schema::SurfaceKind;

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
    /// Exposed stone. Nothing grows on it and nothing burns.
    Rock,
    /// Waist-high grass — the cover that hides you and the fuel that spreads
    /// fire. The reason this layer exists at all.
    TallGrass,
    Sand,
}

/// Everything that follows from a cell's kind. One row per [`TerrainKind`].
pub struct KindProps {
    pub kind: TerrainKind,
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

const KINDS: &[KindProps] = &[
    KindProps {
        kind: TerrainKind::Soil,
        label: "Tierra",
        surface: SurfaceKind::Dirt,
        flammable: false,
        cuttable: false,
    },
    KindProps {
        kind: TerrainKind::Rock,
        label: "Roca",
        surface: SurfaceKind::Stone,
        flammable: false,
        cuttable: false,
    },
    KindProps {
        kind: TerrainKind::TallGrass,
        label: "Pasto largo",
        surface: SurfaceKind::Grass,
        flammable: true,
        cuttable: true,
    },
    KindProps {
        kind: TerrainKind::Sand,
        label: "Arena",
        surface: SurfaceKind::Sand,
        flammable: false,
        cuttable: false,
    },
];

impl TerrainKind {
    /// Every kind, in the order the editor offers them.
    pub const ALL: [TerrainKind; 4] = [
        TerrainKind::Soil,
        TerrainKind::Rock,
        TerrainKind::TallGrass,
        TerrainKind::Sand,
    ];

    pub fn props(self) -> &'static KindProps {
        KINDS
            .iter()
            .find(|props| props.kind == self)
            // Unreachable while `KINDS` covers the enum, which a test pins.
            .expect("every TerrainKind has a row in KINDS")
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
    fn every_kind_has_exactly_one_row() {
        // `props` panics on a missing row, and it is called from the ground probe
        // every tick. A new variant without a row has to fail here, not there.
        assert_eq!(KINDS.len(), TerrainKind::ALL.len());
        for kind in TerrainKind::ALL {
            let matches = KINDS.iter().filter(|props| props.kind == kind).count();
            assert_eq!(matches, 1, "{kind:?} should have exactly one row");
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
    fn the_table_is_what_makes_a_kind_mean_something() {
        // Pins the intent of the layer rather than the values: tall grass is the
        // kind that burns and cuts, rock is the kind that does neither. If these
        // ever swap, the map author's mental model is broken and no other test
        // would notice.
        assert!(TerrainKind::TallGrass.flammable());
        assert!(TerrainKind::TallGrass.cuttable());
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
    fn each_kind_is_distinguishable_underfoot() {
        // The layer is only authorable if the author can tell what they painted
        // by walking on it. Two kinds mapping to one surface are invisible to the
        // only consumer that exists today.
        for kind in TerrainKind::ALL {
            let twins: Vec<TerrainKind> = TerrainKind::ALL
                .into_iter()
                .filter(|other| *other != kind && other.surface() == kind.surface())
                .collect();
            assert!(
                twins.is_empty(),
                "{kind:?} sounds exactly like {twins:?} underfoot"
            );
        }
    }
}
