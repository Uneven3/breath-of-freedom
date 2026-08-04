//! El nivel en disco, y el deshacer en memoria.
//!
//! Formato RON, con resolución y extent adentro: `apply_ron` **remuestrea en
//! espacio de mundo** si difieren, así cambiar `CELLS` o `WORLD_SIZE` no
//! huerfaniza los niveles guardados. El canal semántico viaja run-length en el
//! mismo archivo, y una lista de runs que no suma se **rechaza** en vez de
//! correr el mapa de lugar en silencio.


use super::{
    KindRun, MAX_HEIGHT, MIN_HEIGHT, Terrain, TerrainFile, TerrainSnapshot, bounded_grid_index,
    grid_xz, sample_bilinear,
};
use crate::world::terrain_kind::TerrainKind;

impl Terrain {
    pub fn snapshot(&self) -> TerrainSnapshot {
        TerrainSnapshot {
            heights: self.heights.clone(),
            kinds: self.kinds.clone(),
        }
    }

    /// Put a [`Terrain::snapshot`] back. Ignores a snapshot of the wrong size
    /// (a stale history across a resolution change) rather than panicking.
    pub fn restore(&mut self, snapshot: &TerrainSnapshot) -> bool {
        if snapshot.heights.len() != self.heights.len() || snapshot.kinds.len() != self.kinds.len()
        {
            return false;
        }
        // Compare before copying: undoing a paint-only stroke leaves the relief
        // untouched, and claiming otherwise would rebuild the collider for a
        // heightfield that did not move.
        if self.heights != snapshot.heights {
            self.heights.copy_from_slice(&snapshot.heights);
            self.relief_revision = self.relief_revision.wrapping_add(1);
        }
        self.kinds.copy_from_slice(&snapshot.kinds);
        true
    }

    // ---- brushes: the vocabulary the sculpt tool draws with ----------------

    /// Raise (or lower, with negative `delta`) the grid around `center`, with a
    /// smooth falloff to `radius`.
    ///
    /// The stroke relaxes itself as it lifts (see [`RELAX_PER_METRE`]) — without
    /// that, holding the button builds a tent with a sharp apex instead of a
    pub fn to_ron(&self) -> Result<String, String> {
        let config = ron::ser::PrettyConfig::default().compact_arrays(true);
        ron::ser::to_string_pretty(
            &TerrainFile {
                points: self.points,
                extent: self.extent,
                heights: self.heights.clone(),
                kinds: encode_kinds(&self.kinds),
            },
            config,
        )
        .map_err(|error| error.to_string())
    }

    /// Load a grid saved by [`Terrain::to_ron`], resampling bilinearly when the
    /// file's resolution or world size differs from the current one. Resampling
    /// rather than rejecting is what lets `CELLS` be tuned later without
    /// orphaning levels.
    ///
    /// The resampling goes **through world space**, using the file's own
    /// `extent`: a sample lands where the file says it is in metres, not at the
    /// same fraction of the grid. Stretching a 320 m level over a 640 m world
    /// would double every slope silently — the ground would look right and walk
    /// wrong. A file covering less than the current world keeps its edge heights
    /// outward, which is what clamping the sample does.
    ///
    /// Rejects what it cannot honour: this file *is* the level, and a grid with
    /// a `NaN` in it poisons the collider (parry builds a heightfield that
    /// swallows anything standing on it) with no error anywhere. Heights that
    /// are merely out of band are clamped, not refused — a stray peak is a fixed
    /// level, not a lost one.
    pub fn apply_ron(&mut self, text: &str) -> Result<(), String> {
        let file: TerrainFile = ron::from_str(text).map_err(|error| error.to_string())?;
        if file.points < 2 || file.heights.len() != file.points * file.points {
            return Err("terrain file is malformed (points/heights mismatch)".into());
        }
        if !file.extent.is_finite() || file.extent <= 0.0 {
            return Err(format!("terrain file has a bad extent ({})", file.extent));
        }
        if let Some(bad) = file.heights.iter().find(|height| !height.is_finite()) {
            return Err(format!("terrain file holds a non-finite height ({bad})"));
        }
        let file_cells = file.points - 1;
        let file_kinds = decode_kinds(&file.kinds, file_cells)?;
        // Exact bits mean the authored and runtime grids share coordinates;
        // any numerical difference requires the world-space resample below.
        if file.points == self.points && file.extent.to_bits() == self.extent.to_bits() {
            self.heights = file.heights;
            self.kinds = file_kinds;
        } else {
            let source_last = (file.points - 1) as f32;
            let source_index =
                |metres: f32| ((metres / file.extent + 0.5) * source_last).clamp(0.0, source_last);
            self.heights = (0..self.heights.len())
                .map(|idx| {
                    let xz = grid_xz(self.points, self.extent, idx);
                    sample_bilinear(
                        &file.heights,
                        file.points,
                        source_index(xz.x),
                        source_index(xz.y),
                    )
                })
                .collect();
            // Nearest cell, not bilinear: there is no value between `Rock` and
            // `TallGrass` to interpolate to. Copying the height path here would
            // have invented kinds nobody painted the first time `CELLS` changed.
            let cells = self.cells();
            let last = (file_cells - 1) as f32;
            let source_cell =
                |metres: f32| ((metres / file.extent + 0.5) * file_cells as f32).clamp(0.0, last);
            self.kinds = (0..cells * cells)
                .map(|idx| {
                    let xz = self.cell_centre_xz(idx / cells, idx % cells);
                    let row = bounded_grid_index(source_cell(xz.x), file_cells - 1);
                    let col = bounded_grid_index(source_cell(xz.y), file_cells - 1);
                    file_kinds[row * file_cells + col]
                })
                .collect();
        }
        for height in &mut self.heights {
            *height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
        self.relief_revision = self.relief_revision.wrapping_add(1);
        Ok(())
    }
}

/// Run-length encode the semantic layer, cell-major.
pub(super) fn encode_kinds(kinds: &[TerrainKind]) -> Vec<KindRun> {
    let mut runs: Vec<KindRun> = Vec::new();
    for &kind in kinds {
        match runs.last_mut() {
            Some(run) if run.1 == kind => run.0 += 1,
            _ => runs.push(KindRun(1, kind)),
        }
    }
    runs
}

/// Expand what [`encode_kinds`] wrote, for a grid of `cells × cells`.
///
/// An empty list is not an error: it is how a level saved before this layer
/// existed reads, and the honest expansion of "no semantic layer" is a level of
/// the default kind. Anything else must add up exactly — a truncated run list
/// would otherwise offset every cell after the gap, sliding the whole map's
/// meaning sideways by a silent amount.
pub(super) fn decode_kinds(runs: &[KindRun], cells: usize) -> Result<Vec<TerrainKind>, String> {
    let expected = cells * cells;
    if runs.is_empty() {
        return Ok(vec![TerrainKind::default(); expected]);
    }
    let total: u64 = runs.iter().map(|run| u64::from(run.0)).sum();
    if total != expected as u64 {
        return Err(format!(
            "terrain file's semantic layer covers {total} cells, expected {expected}"
        ));
    }
    let mut kinds = Vec::with_capacity(expected);
    for run in runs {
        kinds.extend(std::iter::repeat_n(run.1, run.0 as usize));
    }
    Ok(kinds)
}
