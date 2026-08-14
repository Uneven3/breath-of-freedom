//! The instance-placement layer: discrete objects an author drops on the
//! terrain, as **data** — never a model, a path, or a material. Presentation
//! resolves a [`PropKind`] to whatever it should look like, the same split
//! [`crate::world`]'s `TerrainKind` already draws between meaning and render.

use bevy_math::Vec2;
use serde::{Deserialize, Serialize};

/// What a placed instance semantically is. Deliberately small today — the
/// candidates being judged for glb-mesh foliage — and deliberately without a
/// `Default`: unlike an unpainted terrain cell, there is no "instance nobody
/// placed" that needs a neutral meaning.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum PropKind {
    GrassA,
    GrassB,
    GrassC,
    GrassDryA,
    GrassTallA,
    GrassVeryShortA,
}

impl PropKind {
    /// Every kind, in the order the editor offers them.
    pub const ALL: [PropKind; 6] = [
        PropKind::GrassA,
        PropKind::GrassB,
        PropKind::GrassC,
        PropKind::GrassDryA,
        PropKind::GrassTallA,
        PropKind::GrassVeryShortA,
    ];

    /// What the editor calls it. Spanish, like the rest of the tool's HUD.
    pub fn label(self) -> &'static str {
        match self {
            PropKind::GrassA => "Pasto A",
            PropKind::GrassB => "Pasto B",
            PropKind::GrassC => "Pasto C",
            PropKind::GrassDryA => "Pasto seco",
            PropKind::GrassTallA => "Pasto alto",
            PropKind::GrassVeryShortA => "Pasto ras",
        }
    }
}

/// "Here is a `kind`" — a row in the level's instance layer. Nothing else:
/// no handle, no path, no material.
///
/// Height is deliberately absent. It is sampled from the terrain wherever
/// this is spawned, exactly like the graybox layout's `Anchor::Ground` —
/// storing an absolute `y` would leave the instance floating the first time
/// someone sculpts beneath it, and sculpting beneath it is the normal use of
/// this tool.
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
pub struct Instance {
    pub kind: PropKind,
    /// World-space position in metres. Never resampled when a saved level's
    /// grid resolution or extent changes — unlike terrain heights, this is
    /// already in metres, not a fraction of the grid.
    pub xz: Vec2,
    pub yaw: f32,
    pub scale: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_are_unique_and_non_empty() {
        let mut labels: Vec<&str> = PropKind::ALL.iter().map(|kind| kind.label()).collect();
        let total = labels.len();
        labels.sort_unstable();
        labels.dedup();
        assert_eq!(labels.len(), total, "two prop kinds share a label");
        assert!(PropKind::ALL.iter().all(|kind| !kind.label().is_empty()));
    }
}
