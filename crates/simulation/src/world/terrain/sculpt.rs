//! Autoría del relieve y de la semántica: los siete pinceles y su maquinaria.
//!
//! `Terrain` es dueño del **cómo** cambia la grilla —un método por pincel sobre
//! `brush_stroke`, que toma un *segmento*, porque un círculo es una cápsula de
//! largo cero—; `editor/` sólo decide **dónde y cuándo**. Un séptimo pincel es
//! un método acá y una fila allá, nunca un sistema nuevo.

use bevy_math::prelude::*;
use bof_domain::world::hash_u32;

use super::{
    MAX_HEIGHT, MAX_RELAX_PER_STEP, MIN_HEIGHT, NOISE_CELL, RELAX_PER_METRE, Terrain,
    bounded_grid_index, grid_xz, noise_coordinate,
};
use crate::world::terrain_kind::TerrainKind;

impl Terrain {
    /// Raise or lower the grid with smooth falloff, relaxing it as it moves so a
    /// held stroke grows a rounded dome rather than integrating into a tent.
    pub fn raise_area(&mut self, center: Vec2, radius: f32, delta: f32) {
        self.brush(center, radius, |_grid, _idx, falloff| delta * falloff);
        let relax = (delta.abs() * RELAX_PER_METRE).min(MAX_RELAX_PER_STEP);
        if relax > 0.0 {
            self.smooth_area(center, radius, relax);
        }
    }

    /// Corta la grilla con borde vertical y **sin relajarla**: el pincel de
    /// telón de fondo.
    ///
    /// **Lo que produce no es un lugar.** El terreno no se escala, y una cara
    /// empinada más alta que el alcance del mantle no se cruza de ninguna
    /// forma, así que esto autora silueta —cañón visto de lejos, pared que
    /// cierra el valle— y nunca terreno por donde el jugador vaya a pasar.
    /// Medido el 2026-08-23: acumula 45 m de tramo empinado a cualquier radio,
    /// contra 1,53 m de [`Terrain::raise_area`] al radio por defecto.
    ///
    /// Es el gemelo duro de [`Terrain::raise_area`], y existe porque el
    /// suavizado de aquélla es incondicional. El borde no tiene falloff a
    /// propósito: con el degradado, el radio grande reparte el corte entre
    /// puntos vecinos y la silueta se pierde.
    pub fn carve_area(&mut self, center: Vec2, radius: f32, delta: f32) {
        self.brush(
            center,
            radius,
            |_grid, _idx, falloff| {
                if falloff > 0.0 { delta } else { 0.0 }
            },
        );
    }

    /// Relax the grid around `center` toward each point's neighbour average, to
    /// erode the spikes a heavy raise leaves. `amount` in `[0, 1]` is how far to
    /// pull toward the average at full falloff. Reads the pre-stroke snapshot
    /// `brush` provides, so the pass has no directional bias.
    pub fn smooth_area(&mut self, center: Vec2, radius: f32, amount: f32) {
        let points = self.points;
        let spacing = self.extent / self.cells() as f32;
        self.brush(center, radius, |grid, idx, falloff| {
            (neighbour_average(grid, points, spacing, idx) - grid[idx]) * amount * falloff
        });
    }

    /// Pull the grid around `center` toward `target` height — mesas, clearings,
    /// campsites, anywhere the player needs flat ground to stand and build on.
    /// `amount` in `[0, 1]` is how far to travel per application.
    pub fn flatten_area(&mut self, center: Vec2, radius: f32, target: f32, amount: f32) {
        self.brush(center, radius, |grid, idx, falloff| {
            (target - grid[idx]) * amount * falloff
        });
    }

    /// Pull the ground under the segment `from → to` toward the straight slope
    /// between the two heights. The connectivity brush: it is how a mesa gets a
    /// walkable way up instead of a wall the player has to climb.
    pub fn ramp_area(
        &mut self,
        from: Vec2,
        from_height: f32,
        to: Vec2,
        to_height: f32,
        radius: f32,
        amount: f32,
    ) {
        let points = self.points;
        let extent = self.extent;
        let span = to - from;
        let length_squared = span.length_squared();
        self.brush_stroke(from, to, radius, |grid, idx, falloff| {
            let t = if length_squared > f32::EPSILON {
                ((grid_xz(points, extent, idx) - from).dot(span) / length_squared).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let target = from_height + (to_height - from_height) * t;
            (target - grid[idx]) * amount * falloff
        });
    }

    /// Add interpolated value noise around `center`, `amplitude` metres worth
    /// per application. Breaks up the artificial evenness a smoothed dome has;
    /// the noise pattern is deterministic per world position, so holding the
    /// button deepens the same wrinkles instead of boiling them.
    pub fn noise_area(&mut self, center: Vec2, radius: f32, amplitude: f32, seed: u32) {
        let points = self.points;
        let extent = self.extent;
        self.brush(center, radius, |_grid, idx, falloff| {
            let xz = grid_xz(points, extent, idx);
            value_noise(xz / NOISE_CELL, seed) * amplitude * falloff
        });
    }

    /// Quantise heights around `center` onto `step`-metre shelves. Terraces are
    /// what make a slope readable as a place — flat treads to stand on, honest
    /// risers between them — and they are the shape BOTW's mesas are built from.
    pub fn terrace_area(&mut self, center: Vec2, radius: f32, step: f32, amount: f32) {
        if step <= 0.0 {
            return;
        }
        self.brush(center, radius, |grid, idx, falloff| {
            let stepped = (grid[idx] / step).round() * step;
            (stepped - grid[idx]) * amount * falloff
        });
    }

    // ---- the semantic layer ------------------------------------------------

    /// Paint `kind` onto every cell whose centre falls within `radius` of
    /// `centre`. Returns whether anything actually changed.
    ///
    /// **No falloff, and no rate.** Both are meaningless here: there is no
    /// halfway between rock and sand to ease into, and painting the same cell
    /// twice cannot deepen it. A cell is either inside the circle or it is not,
    /// which makes a paint stroke idempotent — hold the button still and nothing
    /// keeps happening, unlike every sculpt brush.
    ///
    /// The consequence to know about: the edge of a painted patch is the cell
    /// grid, one metre at the current resolution. Fine detail is not available at
    /// this resolution and pretending otherwise with a soft brush would only
    /// hide it.
    pub fn paint_area(&mut self, centre: Vec2, radius: f32, kind: TerrainKind) -> bool {
        if radius <= 0.0 {
            return false;
        }
        let cells = self.cells();
        let mut changed = false;
        let (rows, cols) = self.cell_window(centre, radius);
        for row in rows {
            for col in cols.clone() {
                if self.cell_centre_xz(row, col).distance(centre) >= radius {
                    continue;
                }
                let idx = row * cells + col;
                if self.kinds[idx] != kind {
                    self.kinds[idx] = kind;
                    changed = true;
                }
            }
        }
        changed
    }

    /// The inclusive cell window a paint stroke of `radius` can reach. Same job
    /// as [`Terrain::window`] does for grid points, in cell space.
    pub(super) fn cell_window(
        &self,
        centre: Vec2,
        radius: f32,
    ) -> (
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>,
    ) {
        let cells = self.cells();
        let last = cells - 1;
        // Inverse of `cell_centre_xz`: world metres back to a fractional cell
        // index, measured from cell centres (hence the half-cell shift).
        let index = |v: f32| (v / self.extent + 0.5) * cells as f32 - 0.5;
        let clamp = |v: f32| bounded_grid_index(v, last);
        (
            clamp(index(centre.x - radius).floor())..=clamp(index(centre.x + radius).ceil()),
            clamp(index(centre.y - radius).floor())..=clamp(index(centre.y + radius).ceil()),
        )
    }

    // ---- brush plumbing ----------------------------------------------------

    /// Radial brush: a stroke whose two ends coincide.
    pub(super) fn brush(
        &mut self,
        center: Vec2,
        radius: f32,
        delta: impl Fn(&[f32], usize, f32) -> f32,
    ) {
        self.brush_stroke(center, center, radius, delta);
    }

    /// Shared brush traversal: for every grid point within `radius` of the
    /// segment `from → to`, add `delta(pre_stroke_snapshot, index, falloff)` to
    /// its height. The snapshot lets smoothing read unbiased neighbours while
    /// still writing in place; a plain raise just ignores it.
    ///
    /// Taking a *segment* rather than a point is what lets the ramp brush share
    /// this traversal with the radial ones — a capsule with zero length is a
    /// circle.
    pub(super) fn brush_stroke(
        &mut self,
        from: Vec2,
        to: Vec2,
        radius: f32,
        delta: impl Fn(&[f32], usize, f32) -> f32,
    ) {
        if radius <= 0.0 {
            return;
        }
        let snapshot = self.heights.clone();
        self.relief_revision = self.relief_revision.wrapping_add(1);
        // Only the grid window the stroke can reach. Walking all 16k points to
        // touch the ~100 under a 6 m brush was pure waste, and it ran every
        // frame of a stroke — twice, since `raise_area` relaxes as it lifts.
        let (rows, cols) = self.window(from, to, radius);
        for row in rows {
            for col in cols.clone() {
                let distance = distance_to_segment(self.point_xz(row, col), from, to);
                if distance >= radius {
                    continue;
                }
                // Smoothstep falloff: full strength at the center, zero at the rim.
                let t = 1.0 - distance / radius;
                let falloff = t * t * (3.0 - 2.0 * t);
                let idx = row * self.points + col;
                self.heights[idx] = (self.heights[idx] + delta(&snapshot, idx, falloff))
                    .clamp(MIN_HEIGHT, MAX_HEIGHT);
            }
        }
    }

    /// The inclusive grid window that a stroke of `radius` around the segment
    /// `from → to` can possibly touch, clamped to the grid. The brush's falloff
    /// still decides what actually moves; this only avoids visiting points that
    /// are provably out of reach.
    pub(super) fn window(
        &self,
        from: Vec2,
        to: Vec2,
        radius: f32,
    ) -> (
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>,
    ) {
        let last = self.points - 1;
        // Inverse of `point_xz`: world metres back to a fractional grid index.
        let index = |v: f32| (v / self.extent + 0.5) * last as f32;
        let low = from.min(to) - Vec2::splat(radius);
        let high = from.max(to) + Vec2::splat(radius);
        let clamp = |v: f32| bounded_grid_index(v, last);
        (
            clamp(index(low.x).floor())..=clamp(index(high.x).ceil()),
            clamp(index(low.y).floor())..=clamp(index(high.y).ceil()),
        )
    }

    // ---- persistence -------------------------------------------------------
}

/// How far the relax kernel reaches, in **metres** and not in grid steps: a
/// radius counted in steps shrinks with the cell, so on a finer grid the same
/// held stroke would spike where it used to dome. Rounded to whole steps, so
/// the reach lands on 3 m at a 1 m cell and on 2.5 m at a 0.5 m one.
const RELAX_REACH_METRES: f32 = 2.5;
/// Average of the grid points within `RELAX_REACH_METRES` of `idx`, along the
/// four axes. Falls back to the point itself when nothing is in reach.
pub(super) fn neighbour_average(grid: &[f32], points: usize, spacing: f32, idx: usize) -> f32 {
    // El cociente de dos distancias positivas y acotadas cabe siempre en
    // `usize`; el `max(1)` garantiza además que el kernel nunca sea vacío.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let reach = if spacing > 0.0 {
        ((RELAX_REACH_METRES / spacing).round() as usize).max(1)
    } else {
        1
    };
    let row = idx / points;
    let col = idx % points;
    let mut sum = 0.0;
    let mut count = 0.0;
    for step in 1..=reach {
        let candidates = [
            row.checked_sub(step).map(|nr| (nr, col)),
            (row + step < points).then_some((row + step, col)),
            col.checked_sub(step).map(|nc| (row, nc)),
            (col + step < points).then_some((row, col + step)),
        ];
        for (nr, nc) in candidates.into_iter().flatten() {
            sum += grid[nr * points + nc];
            count += 1.0;
        }
    }
    if count > 0.0 { sum / count } else { grid[idx] }
}
/// Distance from `point` to the segment `from → to`; the circle case falls out
/// when the segment has zero length.
fn distance_to_segment(point: Vec2, from: Vec2, to: Vec2) -> f32 {
    let span = to - from;
    let length_squared = span.length_squared();
    if length_squared <= f32::EPSILON {
        return point.distance(from);
    }
    let t = ((point - from).dot(span) / length_squared).clamp(0.0, 1.0);
    point.distance(from + span * t)
}

/// Interpolated value noise in `[-1, 1]`, deterministic per position. Built on
/// the same scatter hash the forest and the grass meadow use, so the project
/// keeps one source of pseudo-randomness instead of pulling in a noise crate.
fn value_noise(p: Vec2, seed: u32) -> f32 {
    let cell = p.floor();
    let f = p - cell;
    // Smoothstep the interpolation weights so cell borders do not show as creases.
    let w = Vec2::new(f.x * f.x * (3.0 - 2.0 * f.x), f.y * f.y * (3.0 - 2.0 * f.y));
    let corner = |dx: f32, dy: f32| {
        let x = noise_coordinate(cell.x + dx);
        let y = noise_coordinate(cell.y + dy);
        let hash = hash_u32(x.wrapping_mul(0x9e37_79b9) ^ hash_u32(y ^ seed));
        hash as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let top = corner(0.0, 0.0).lerp(corner(1.0, 0.0), w.x);
    let bottom = corner(0.0, 1.0).lerp(corner(1.0, 1.0), w.x);
    top.lerp(bottom, w.y)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A grid where every point is 0 except a spike of 1 at the centre.
    fn spiked(points: usize) -> Vec<f32> {
        let mut grid = vec![0.0; points * points];
        grid[points / 2 * points + points / 2] = 1.0;
        grid
    }

    /// The kernel reaches the same **distance** whatever the spacing.
    ///
    /// This is what stops the brush from getting sharper as the grid gets
    /// finer: a relax radius counted in grid steps shrinks with the cell, so
    /// the same held stroke that makes a dome today would make a spike on a
    /// finer grid — the failure that fixed the resolution here in the first
    /// place.
    #[test]
    fn the_relax_kernel_covers_the_same_metres_at_any_spacing() {
        // 2.5 m of reach: one step at 2.5 m spacing, five at 0.5 m.
        let coarse = spiked(9);
        let fine = spiked(41);
        let coarse_centre = 9 / 2 * 9 + 9 / 2;
        let fine_centre = 41 / 2 * 41 + 41 / 2;

        // With the spike at the centre, its own neighbours average the ring
        // around it: the count of contributing points is what changes, and the
        // reach in metres is what must not.
        let coarse_reach = ((RELAX_REACH_METRES / 2.5).round() as usize).max(1);
        let fine_reach = ((RELAX_REACH_METRES / 0.5).round() as usize).max(1);
        assert_eq!(coarse_reach, 1);
        assert_eq!(fine_reach, 5);
        assert!(
            (coarse_reach as f32 * 2.5 - fine_reach as f32 * 0.5).abs() < 1e-5,
            "los dos kernels deben cubrir los mismos metros"
        );

        // And both actually see the spike from one cell away in world terms.
        let coarse_neighbour = coarse_centre - 1;
        let fine_neighbour = fine_centre - 5;
        assert!(neighbour_average(&coarse, 9, 2.5, coarse_neighbour) > 0.0);
        assert!(neighbour_average(&fine, 41, 0.5, fine_neighbour) > 0.0);
    }

    /// At the 2.5 m spacing it was written against, the metre-based kernel must
    /// be the four-neighbour average it replaced, or the brush numbers moved.
    #[test]
    fn at_two_and_a_half_metres_the_kernel_is_the_old_four_neighbour_average() {
        let grid = spiked(9);
        let centre = 9 / 2 * 9 + 9 / 2;
        // The four points around the spike each see it once out of four.
        assert!((neighbour_average(&grid, 9, 2.5, centre - 1) - 0.25).abs() < 1e-6);
        // The spike itself sees only zeros.
        assert!(neighbour_average(&grid, 9, 2.5, centre).abs() < 1e-6);
    }

    /// A degenerate spacing must not divide by zero or reach nowhere.
    #[test]
    fn a_zero_spacing_still_averages_something() {
        let grid = spiked(9);
        let centre = 9 / 2 * 9 + 9 / 2;
        assert!((neighbour_average(&grid, 9, 0.0, centre - 1) - 0.25).abs() < 1e-6);
    }
}
