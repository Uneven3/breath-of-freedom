//! The terrain: the ground as **editable data** — a height grid that is the
//! single source of truth the physics collider and the visual mesh both derive
//! from.
//!
//! Data lives here in `world` (data-in-the-world). The flat-shaded visual is
//! generated in [`crate::visuals::terrain`], and the in-engine sculpt tool lives
//! in [`crate::editor`]. One grid → two representations, kept in sync by change
//! detection: mutating [`Terrain`] re-triggers both the collider rebuild and the
//! mesh rebuild.
//!
//! **The grid owns *how* it changes.** Every brush the editor offers is a method
//! here; the editor only decides *where and when* one fires. That split is what
//! keeps a new brush from needing a new system — it is a closure handed to
//! [`Terrain::brush_stroke`].

use std::path::Path;

use avian3d::prelude::*;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::Surface;
use super::forest::hash_u32;
use super::layout::WORLD_SIZE;
use crate::asset_pipeline::schema::SurfaceKind;

/// Grid cells per side; the heightfield has `CELLS + 1` points per side. Sized
/// so the brush covers enough vertices to sculpt smooth domes (a coarser grid
/// gives spiky tents) while the whole-terrain rebuild each edit stays cheap.
/// Pushing this much higher is the point where per-edit *partial* rebuilds
/// (chunking) start to matter — deferred for now.
const CELLS: usize = 128;

/// Each scene names its own heightmap in `crate::scene::SCENES`. **That file is
/// the level**: the editor writes it and [`setup_terrain`] loads it on entry, so
/// a scene starts on the ground the last session shaped — and sculpting one
/// scene cannot disturb another. Paths are relative to the working directory,
/// like `assets/`.
pub fn terrain_file(state: &State<crate::scene::AppState>) -> Option<&'static str> {
    crate::scene::current_scene(state).map(|def| def.terrain_file)
}

/// Heights are clamped to this band. Not a design limit — a guard so a stuck
/// mouse button cannot push the ground somewhere the camera and the physics
/// solver stop making sense.
const MIN_HEIGHT: f32 = -60.0;
const MAX_HEIGHT: f32 = 120.0;

/// How much a raise stroke relaxes *itself*, per metre of height it just added.
///
/// This is the whole reason sculpting feels soft instead of spiky. A pure raise
/// applies `delta * falloff` to the same points every frame, so holding the
/// button integrates the falloff curve into a tent with a sharp apex. Bleeding a
/// little smoothing into each application keeps the dome round while it grows,
/// which is what "one continuous stroke" is supposed to look like.
const RELAX_PER_METRE: f32 = 6.0;
/// Ceiling on that per-application relax, so a big single step cannot flatten
/// the area it was meant to lift.
const MAX_RELAX_PER_STEP: f32 = 0.3;

/// World size of one value-noise cell, in metres. Bigger than the 2.5 m grid
/// spacing on purpose: per-vertex randomness reads as salt-and-pepper, while
/// interpolating over ~12 m reads as terrain.
const NOISE_CELL: f32 = 12.0;

/// The ground as a height grid. Authoritative data: both the collider and the
/// visual mesh are derived from it. Row-major `points × points`, where the row
/// indexes X and the column indexes Z (parry heightfield convention).
#[derive(Component, Debug, Clone)]
pub struct Terrain {
    /// Heights in world units, row-major (`row * points + col`).
    heights: Vec<f32>,
    /// Points per side (`CELLS + 1`).
    points: usize,
    /// World size spanned on each of X and Z.
    extent: f32,
}

/// The on-disk shape of a level. Resolution and extent travel with the heights
/// so a file authored before a `CELLS` or `WORLD_SIZE` change still loads —
/// [`Terrain::apply_ron`] resamples in world space instead of rejecting.
#[derive(Serialize, Deserialize)]
struct TerrainFile {
    points: usize,
    extent: f32,
    heights: Vec<f32>,
}

impl Terrain {
    /// A flat grid at `y = 0`, matching the graybox floor it replaces.
    fn flat() -> Self {
        let points = CELLS + 1;
        Self {
            heights: vec![0.0; points * points],
            points,
            extent: WORLD_SIZE,
        }
    }

    /// A flat grid for tests outside this module (the editor's history tests
    /// need a terrain, and only `world` may construct one).
    #[cfg(test)]
    pub(crate) fn flat_for_test() -> Self {
        Self::flat()
    }

    pub fn points(&self) -> usize {
        self.points
    }

    pub fn height(&self, row: usize, col: usize) -> f32 {
        self.heights[row * self.points + col]
    }

    /// World XZ of grid point `(row, col)`, independent of its height. The one
    /// mapping the collider and the visual mesh must agree on, so they never
    /// drift: parry centers the heightfield on the entity origin, spanning
    /// `[-extent/2, extent/2]`.
    fn point_xz(&self, row: usize, col: usize) -> Vec2 {
        grid_xz(self.points, self.extent, row * self.points + col)
    }

    /// Full world position of grid point `(row, col)`, height included.
    pub fn point_world_pos(&self, row: usize, col: usize) -> Vec3 {
        let xz = self.point_xz(row, col);
        Vec3::new(xz.x, self.height(row, col), xz.y)
    }

    /// Ground height at an arbitrary world XZ — **on the actual surface**, not
    /// on a smooth approximation of it.
    ///
    /// Anything *placed* in the world needs this the moment the ground stops
    /// being flat: a spawn point authored as a constant `y` puts an actor
    /// underground the first time someone sculpts a hill over it, and a
    /// heightfield is a one-sided surface — from below it does not catch you,
    /// so you fall forever.
    ///
    /// It samples the **triangle**, not a bilinear patch. Both the collider and
    /// the visual mesh cut each cell along the `(row, col) → (row+1, col+1)`
    /// diagonal into two flat triangles; a bilinear patch bulges above that
    /// surface inside the quad — by half a metre on 2.5 m cells with real
    /// relief. Lifting a body onto the bulge is how the player ended up
    /// hovering above sculpted ground.
    pub fn height_at(&self, xz: Vec2) -> f32 {
        let last = (self.points - 1) as f32;
        let fx = ((xz.x / self.extent + 0.5) * last).clamp(0.0, last);
        let fz = ((xz.y / self.extent + 0.5) * last).clamp(0.0, last);
        let row = (fx.floor() as usize).min(self.points - 2);
        let col = (fz.floor() as usize).min(self.points - 2);
        // Position inside the cell, both in `[0, 1]`.
        let u = fx - row as f32;
        let v = fz - col as f32;
        let corner = |dr: usize, dc: usize| self.height(row + dr, col + dc);
        // The shared diagonal runs where `u == v`, so the sign of `u - v` picks
        // the triangle; each expression is the plane through its three corners.
        if u >= v {
            corner(0, 0) + (corner(1, 0) - corner(0, 0)) * u + (corner(1, 1) - corner(1, 0)) * v
        } else {
            corner(0, 0) + (corner(1, 1) - corner(0, 1)) * u + (corner(0, 1) - corner(0, 0)) * v
        }
    }

    /// Calculate terrain slope angle in degrees at a world coordinate position (x, z).
    pub fn slope_deg_at(&self, xz: Vec2) -> f32 {
        let step = 0.5;
        let h0 = self.height_at(xz);
        let h_east = self.height_at(xz + Vec2::X * step);
        let h_north = self.height_at(xz + Vec2::Y * step);
        let dx = (h_east - h0) / step;
        let dz = (h_north - h0) / step;
        (dx * dx + dz * dz).sqrt().atan().to_degrees()
    }

    /// The heightfield collider derived from the grid. `scale` maps parry's unit
    /// rectangle to the world extent; heights pass through at `scale.y = 1`.
    fn to_collider(&self) -> Collider {
        let rows: Vec<Vec<f32>> = (0..self.points)
            .map(|row| (0..self.points).map(|col| self.height(row, col)).collect())
            .collect();
        Collider::heightfield(rows, Vec3::new(self.extent, 1.0, self.extent))
    }

    /// A copy of the raw grid, for the editor's undo history. Opaque on purpose:
    /// only [`Terrain::restore`] can put one back, so history cannot invent a
    /// grid the terrain never had.
    pub fn snapshot(&self) -> Vec<f32> {
        self.heights.clone()
    }

    /// Put a [`Terrain::snapshot`] back. Ignores a snapshot of the wrong size
    /// (a stale history across a resolution change) rather than panicking.
    pub fn restore(&mut self, snapshot: &[f32]) -> bool {
        if snapshot.len() != self.heights.len() {
            return false;
        }
        self.heights.copy_from_slice(snapshot);
        true
    }

    // ---- brushes: the vocabulary the sculpt tool draws with ----------------

    /// Raise (or lower, with negative `delta`) the grid around `center`, with a
    /// smooth falloff to `radius`.
    ///
    /// The stroke relaxes itself as it lifts (see [`RELAX_PER_METRE`]) — without
    /// that, holding the button builds a tent with a sharp apex instead of a
    /// dome, which is exactly how this brush used to feel.
    pub fn raise_area(&mut self, center: Vec2, radius: f32, delta: f32) {
        self.brush(center, radius, |_grid, _idx, falloff| delta * falloff);
        let relax = (delta.abs() * RELAX_PER_METRE).min(MAX_RELAX_PER_STEP);
        if relax > 0.0 {
            self.smooth_area(center, radius, relax);
        }
    }

    /// Relax the grid around `center` toward each point's neighbour average, to
    /// erode the spikes a heavy raise leaves. `amount` in `[0, 1]` is how far to
    /// pull toward the average at full falloff. Reads the pre-stroke snapshot
    /// `brush` provides, so the pass has no directional bias.
    pub fn smooth_area(&mut self, center: Vec2, radius: f32, amount: f32) {
        let points = self.points;
        self.brush(center, radius, |grid, idx, falloff| {
            (neighbour_average(grid, points, idx) - grid[idx]) * amount * falloff
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

    // ---- brush plumbing ----------------------------------------------------

    /// Radial brush: a stroke whose two ends coincide.
    fn brush(&mut self, center: Vec2, radius: f32, delta: impl Fn(&[f32], usize, f32) -> f32) {
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
    fn brush_stroke(
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
    fn window(
        &self,
        from: Vec2,
        to: Vec2,
        radius: f32,
    ) -> (
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>,
    ) {
        let last = (self.points - 1) as f32;
        // Inverse of `point_xz`: world metres back to a fractional grid index.
        let index = |v: f32| (v / self.extent + 0.5) * last;
        let low = from.min(to) - Vec2::splat(radius);
        let high = from.max(to) + Vec2::splat(radius);
        let clamp = |v: f32| v.clamp(0.0, last) as usize;
        (
            clamp(index(low.x).floor())..=clamp(index(high.x).ceil()),
            clamp(index(low.y).floor())..=clamp(index(high.y).ceil()),
        )
    }

    // ---- persistence -------------------------------------------------------

    /// Serialise the grid to RON. Resolution and extent ride along so a file
    /// outlives a `CELLS` change.
    ///
    /// Pretty-printed **except** for arrays: the header fields stay readable
    /// while the 16k heights remain one line instead of 16k. RON over JSON is a
    /// project-wide choice (it is Bevy's idiomatic data format and it takes
    /// comments) — it buys little for a numeric matrix like this one, but it is
    /// the format the *authored* files that follow will want.
    pub fn to_ron(&self) -> Result<String, String> {
        let config = ron::ser::PrettyConfig::default().compact_arrays(true);
        ron::ser::to_string_pretty(
            &TerrainFile {
                points: self.points,
                extent: self.extent,
                heights: self.heights.clone(),
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
        if file.points == self.points && file.extent == self.extent {
            self.heights = file.heights;
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
        }
        for height in &mut self.heights {
            *height = height.clamp(MIN_HEIGHT, MAX_HEIGHT);
        }
        Ok(())
    }
}

/// World XZ of a grid index, without needing the terrain itself — brush closures
/// only get the flat height slice, so they reconstruct positions through this.
fn grid_xz(points: usize, extent: f32, idx: usize) -> Vec2 {
    let last = (points - 1) as f32;
    Vec2::new(
        ((idx / points) as f32 / last - 0.5) * extent,
        ((idx % points) as f32 / last - 0.5) * extent,
    )
}

/// Average of a point's four orthogonal neighbours, falling back to the point
/// itself at the grid edge.
fn neighbour_average(grid: &[f32], points: usize, idx: usize) -> f32 {
    let row = (idx / points) as i32;
    let col = (idx % points) as i32;
    let mut sum = 0.0;
    let mut count = 0.0;
    for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
        let nr = row + dr;
        let nc = col + dc;
        if nr < 0 || nc < 0 || nr >= points as i32 || nc >= points as i32 {
            continue;
        }
        sum += grid[nr as usize * points + nc as usize];
        count += 1.0;
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
        let x = (cell.x + dx) as i32 as u32;
        let y = (cell.y + dy) as i32 as u32;
        let hash = hash_u32(x.wrapping_mul(0x9e37_79b9) ^ hash_u32(y ^ seed));
        hash as f32 / u32::MAX as f32 * 2.0 - 1.0
    };
    let top = corner(0.0, 0.0).lerp(corner(1.0, 0.0), w.x);
    let bottom = corner(0.0, 1.0).lerp(corner(1.0, 1.0), w.x);
    top.lerp(bottom, w.y)
}

/// Bilinear sample of a square height grid at fractional grid coordinates.
fn sample_bilinear(heights: &[f32], points: usize, row: f32, col: f32) -> f32 {
    let last = points - 1;
    let r0 = (row.floor() as usize).min(last);
    let c0 = (col.floor() as usize).min(last);
    let r1 = (r0 + 1).min(last);
    let c1 = (c0 + 1).min(last);
    let fr = row - r0 as f32;
    let fc = col - c0 as f32;
    let top = heights[r0 * points + c0].lerp(heights[r0 * points + c1], fc);
    let bottom = heights[r1 * points + c0].lerp(heights[r1 * points + c1], fc);
    top.lerp(bottom, fr)
}

/// Rebuild the heightfield collider whenever the grid changes (sculpting). Like
/// the mesh in `visuals::terrain`, this reacts to `Changed`, which also fires
/// the frame the terrain spawns — rebuilding once more then is harmless.
pub(super) fn rebuild_terrain_collider(
    mut terrain: Query<(&Terrain, &mut Collider), Changed<Terrain>>,
) {
    for (terrain, mut collider) in &mut terrain {
        *collider = terrain.to_collider();
    }
}

/// Spawn the single terrain entity: the height grid plus its derived collider.
/// Static world geometry, so it stays on the `Default` collision layer (no
/// `CollisionLayers`) where ledge sensing can see it. Carries `Surface` so the
/// footstep audio seam keeps working, exactly as the old floor did.
///
/// The saved level, if there is one, *is* the starting ground — a missing file
/// is the normal first-run case, not an error.
pub(super) fn setup_terrain(mut commands: Commands, state: Res<State<crate::scene::AppState>>) {
    let mut terrain = Terrain::flat();
    if let Some(file) = terrain_file(&state)
        && Path::new(file).exists()
    {
        match std::fs::read_to_string(file)
            .map_err(|error| error.to_string())
            .and_then(|text| terrain.apply_ron(&text))
        {
            Ok(()) => info!("[world] terrain loaded from {file}"),
            Err(error) => warn!("[world] terrain load failed ({error}); starting flat"),
        }
    }
    let collider = terrain.to_collider();
    commands.spawn((
        DespawnOnExit(*state.get()),
        Name::new("Terrain"),
        terrain,
        collider,
        // A margin gives this thin, one-sided surface virtual thickness for
        // stable depenetration — but it also lifts the effective surface by its
        // own size, and 0.1 m of that is **visible**: the body rests a hand's
        // width above the mesh you can see, and the debug wireframe draws the
        // inflated shape floating over the ground. Kept at the smallest value
        // that still helps the solver; the actual anti-penetration job belongs
        // to `movement::services::ground::lift_actors_out_of_terrain`, which
        // corrects against the real surface instead of hiding under a cushion.
        CollisionMargin(0.02),
        RigidBody::Static,
        Surface(SurfaceKind::Grass),
        Transform::default(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Height at the grid point nearest the world origin.
    fn center_height(terrain: &Terrain) -> f32 {
        let mid = terrain.points() / 2;
        terrain.height(mid, mid)
    }

    #[test]
    fn raising_lifts_the_centre_more_than_the_rim() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 2.0);
        let mid = terrain.points() / 2;
        let centre = terrain.height(mid, mid);
        let near_rim = terrain.height(mid + 7, mid);
        assert!(centre > near_rim, "{centre} should exceed {near_rim}");
        assert!(near_rim > 0.0, "the falloff should still reach the rim");
    }

    #[test]
    fn a_held_raise_stays_rounder_than_the_raw_falloff() {
        // The feel fix, stated as a test: integrating many small applications
        // must not sharpen the apex relative to its surroundings. A pure
        // `delta * falloff` stack does; the self-relaxing stroke does not.
        let mut sculpted = Terrain::flat();
        for _ in 0..120 {
            sculpted.raise_area(Vec2::ZERO, 15.0, 0.025);
        }
        let mut raw = Terrain::flat();
        for _ in 0..120 {
            raw.brush(Vec2::ZERO, 15.0, |_, _, falloff| 0.025 * falloff);
        }
        let mid = sculpted.points() / 2;
        // Ratio of apex to a point a third of the way out: lower means rounder.
        let sculpted_ratio = sculpted.height(mid, mid) / sculpted.height(mid, mid + 2);
        let raw_ratio = raw.height(mid, mid) / raw.height(mid, mid + 2);
        assert!(
            sculpted_ratio < raw_ratio,
            "self-relaxing stroke ({sculpted_ratio}) should be rounder than raw ({raw_ratio})"
        );
    }

    #[test]
    fn the_brush_window_does_not_clip_the_reach_of_the_radius() {
        // Visiting only the window the stroke can reach is an optimisation; it
        // must not change *what* a stroke touches. Points just inside the radius
        // still move, points just outside still do not.
        let mut terrain = Terrain::flat();
        let radius = 20.0;
        terrain.raise_area(Vec2::ZERO, radius, 3.0);
        let mid = terrain.points() / 2;
        let spacing = WORLD_SIZE / CELLS as f32;
        let inside = (radius / spacing).floor() as usize - 1;
        let outside = (radius / spacing).ceil() as usize + 1;
        assert!(
            terrain.height(mid + inside, mid) > 0.0,
            "a point inside the radius should have moved"
        );
        assert_eq!(
            terrain.height(mid + outside, mid),
            0.0,
            "a point outside the radius must stay put"
        );
    }

    #[test]
    fn a_stroke_at_the_world_edge_stays_inside_the_grid() {
        // The window is clamped, so a brush hanging off the corner must neither
        // panic nor wrap around to the opposite edge.
        let mut terrain = Terrain::flat();
        let corner = Vec2::splat(WORLD_SIZE * 0.5);
        terrain.raise_area(corner, 30.0, 4.0);
        let last = terrain.points() - 1;
        assert!(terrain.height(last, last) > 0.0, "the corner should lift");
        assert_eq!(
            terrain.height(0, 0),
            0.0,
            "the opposite corner must be untouched"
        );
    }

    #[test]
    fn smoothing_erodes_a_spike() {
        let mut terrain = Terrain::flat();
        terrain.brush(Vec2::ZERO, 4.0, |_, _, falloff| 10.0 * falloff);
        let before = center_height(&terrain);
        terrain.smooth_area(Vec2::ZERO, 20.0, 0.5);
        assert!(center_height(&terrain) < before);
    }

    #[test]
    fn flattening_converges_on_the_target() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 5.0);
        for _ in 0..40 {
            terrain.flatten_area(Vec2::ZERO, 20.0, 3.0, 0.5);
        }
        assert!((center_height(&terrain) - 3.0).abs() < 0.05);
    }

    #[test]
    fn a_ramp_interpolates_between_its_two_ends() {
        let mut terrain = Terrain::flat();
        let from = Vec2::new(-30.0, 0.0);
        let to = Vec2::new(30.0, 0.0);
        for _ in 0..60 {
            terrain.ramp_area(from, 0.0, to, 12.0, 8.0, 0.5);
        }
        let mid = terrain.points() / 2;
        // Sample along +X (the row axis) at the ramp's midpoint and near its top.
        let middle = terrain.height(mid, mid);
        let high = terrain.height(mid + 11, mid);
        assert!(
            (middle - 6.0).abs() < 1.0,
            "midpoint {middle} should sit near half the rise"
        );
        assert!(high > middle, "{high} should be above {middle}");
    }

    #[test]
    fn noise_is_deterministic_and_two_sided() {
        let mut a = Terrain::flat();
        let mut b = Terrain::flat();
        a.noise_area(Vec2::ZERO, 40.0, 2.0, 7);
        b.noise_area(Vec2::ZERO, 40.0, 2.0, 7);
        assert_eq!(a.snapshot(), b.snapshot());
        let mid = a.points() / 2;
        let window: Vec<f32> = (mid - 6..mid + 6).map(|row| a.height(row, mid)).collect();
        assert!(
            window.iter().any(|h| *h > 0.0) && window.iter().any(|h| *h < 0.0),
            "noise should push both up and down, got {window:?}"
        );
    }

    #[test]
    fn a_different_seed_gives_a_different_pattern() {
        let mut a = Terrain::flat();
        let mut b = Terrain::flat();
        a.noise_area(Vec2::ZERO, 40.0, 2.0, 1);
        b.noise_area(Vec2::ZERO, 40.0, 2.0, 2);
        assert_ne!(a.snapshot(), b.snapshot());
    }

    #[test]
    fn terracing_snaps_heights_onto_shelves() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 40.0, 7.3);
        for _ in 0..60 {
            terrain.terrace_area(Vec2::ZERO, 40.0, 2.0, 0.5);
        }
        let height = center_height(&terrain);
        let remainder = (height / 2.0 - (height / 2.0).round()).abs();
        assert!(remainder < 0.05, "{height} should sit on a 2 m shelf");
    }

    #[test]
    fn heights_stay_inside_the_guard_band() {
        let mut terrain = Terrain::flat();
        for _ in 0..200 {
            terrain.raise_area(Vec2::ZERO, 10.0, 5.0);
        }
        assert!(center_height(&terrain) <= MAX_HEIGHT);
    }

    #[test]
    fn height_at_lands_on_the_grid_points_it_passes_through() {
        let mut terrain = Terrain::flat();
        terrain.noise_area(Vec2::ZERO, 60.0, 4.0, 11);
        // Sampling exactly at a grid point must return that point's height —
        // true of any correct interpolation, and the cheapest way to catch an
        // off-by-one in the cell lookup.
        for (row, col) in [(40, 40), (41, 40), (40, 41), (64, 64)] {
            let xz = terrain.point_xz(row, col);
            let sampled = terrain.height_at(xz);
            let expected = terrain.height(row, col);
            assert!(
                (sampled - expected).abs() < 0.001,
                "at ({row},{col}): sampled {sampled}, grid says {expected}"
            );
        }
    }

    #[test]
    fn height_at_stays_on_the_triangles_not_above_them() {
        // The hovering bug: a bilinear patch bulges above the triangulated
        // surface inside a quad. Build a saddle — the worst case, where the two
        // disagree most — and check the sample sits on the triangle.
        let mut terrain = Terrain::flat();
        let points = terrain.points;
        let (row, col) = (60, 60);
        terrain.heights[row * points + col] = 0.0;
        terrain.heights[(row + 1) * points + col + 1] = 0.0;
        terrain.heights[(row + 1) * points + col] = 10.0;
        terrain.heights[row * points + col + 1] = 10.0;

        // Cell centre sits exactly on the shared diagonal, whose endpoints are
        // both 0.0 — so the surface there is 0.0, while bilinear would say 5.0.
        let a = terrain.point_xz(row, col);
        let c = terrain.point_xz(row + 1, col + 1);
        let centre = (a + c) * 0.5;
        let sampled = terrain.height_at(centre);
        assert!(
            sampled.abs() < 0.1,
            "on the diagonal the surface is 0, got {sampled} (bilinear would say 5)"
        );
    }

    #[test]
    fn the_collider_surface_matches_the_visual_mesh() {
        // The wireframe looked offset from the ground. This settles it without
        // interpreting a screenshot: shoot a ray straight down at the actual
        // collider and compare where it lands with what `height_at` (the mesh's
        // own surface) claims. Any mapping mistake — transposed axes, a shifted
        // origin, the wrong diagonal — shows up here as a mismatch.
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::new(-40.0, 20.0), 35.0, 9.0);
        terrain.noise_area(Vec2::ZERO, 120.0, 3.0, 5);
        let collider = terrain.to_collider();
        let shape = collider.shape_scaled();

        let mut worst = 0.0_f32;
        let mut worst_at = Vec2::ZERO;
        for (x, z) in [
            (0.0, 0.0),
            (-40.0, 20.0),
            (40.0, -20.0),
            (17.3, 61.9),
            (-93.4, -12.7),
            (120.0, 120.0),
        ] {
            let xz = Vec2::new(x, z);
            let expected = terrain.height_at(xz);
            let ray = avian3d::parry::query::Ray::new(Vec3::new(x, 500.0, z), Vec3::NEG_Y);
            let hit = shape
                .cast_local_ray(&ray, 1000.0, true)
                .expect("the ray must hit the terrain");
            let collider_height = 500.0 - hit;
            let error = (collider_height - expected).abs();
            if error > worst {
                worst = error;
                worst_at = xz;
            }
        }
        assert!(
            worst < 0.01,
            "collider and mesh disagree by {worst} m at {worst_at:?}"
        );
    }

    #[test]
    fn a_snapshot_restores_the_grid_it_came_from() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 4.0);
        let snapshot = terrain.snapshot();
        terrain.raise_area(Vec2::new(20.0, 20.0), 20.0, -6.0);
        assert_ne!(terrain.snapshot(), snapshot);
        assert!(terrain.restore(&snapshot));
        assert_eq!(terrain.snapshot(), snapshot);
    }

    #[test]
    fn a_snapshot_of_the_wrong_size_is_refused() {
        let mut terrain = Terrain::flat();
        assert!(!terrain.restore(&[0.0; 4]));
    }

    #[test]
    fn a_saved_grid_round_trips() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 25.0, 6.0);
        terrain.noise_area(Vec2::ZERO, 40.0, 1.5, 3);
        let expected = terrain.snapshot();
        let text = terrain.to_ron().expect("serialises");
        let mut loaded = Terrain::flat();
        loaded.apply_ron(&text).expect("loads");
        assert_eq!(loaded.snapshot(), expected);
    }

    #[test]
    fn a_grid_saved_at_another_resolution_is_resampled() {
        // A file authored before a `CELLS` change must still load: half the
        // resolution, same world, so the shape survives even though the sample
        // count does not.
        let coarse = TerrainFile {
            points: 5,
            extent: WORLD_SIZE,
            heights: (0..25).map(|i| (i / 5) as f32).collect(),
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("loads");
        assert_eq!(terrain.points(), CELLS + 1);
        let last = CELLS;
        assert!((terrain.height(0, 0) - 0.0).abs() < 0.001);
        assert!((terrain.height(last, 0) - 4.0).abs() < 0.001);
        // Interpolated, not snapped: the middle row lands between the shelves.
        assert!((terrain.height(last / 2, 0) - 2.0).abs() < 0.05);
    }

    #[test]
    fn a_hand_written_file_loads() {
        // The shape a human (or a migration script) would type: pretty fields,
        // compact array, trailing commas. Pins the on-disk format so a change to
        // the serializer's config cannot silently orphan authored levels.
        let text = "(\n    points: 3,\n    extent: 320.0,\n    heights: [0.0, 1.0, 2.0, \
                    3.0, 4.0, 5.0, 6.0, 7.0, 8.0],\n)\n";
        let mut terrain = Terrain::flat();
        terrain.apply_ron(text).expect("hand-written RON loads");
        assert!((terrain.height(0, 0) - 0.0).abs() < 0.001);
        assert!((terrain.height(CELLS, CELLS) - 8.0).abs() < 0.001);
    }

    #[test]
    fn a_grid_saved_for_a_smaller_world_keeps_its_metres() {
        // The file's `extent` is not decoration: a level authored for a 160 m
        // world must land on the middle 160 m of a 320 m one, at its authored
        // heights. Stretching it to fill the world instead would halve every
        // slope — ground that looks authored and walks wrong.
        let half = WORLD_SIZE / 2.0;
        let coarse = TerrainFile {
            points: 3,
            extent: half,
            heights: vec![0.0, 0.0, 0.0, 10.0, 10.0, 10.0, 20.0, 20.0, 20.0],
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("loads");
        // The file spans x ∈ [-80, 80] with height 0 → 20 across it.
        assert!((terrain.height_at(Vec2::new(-half / 2.0, 0.0)) - 0.0).abs() < 0.05);
        assert!((terrain.height_at(Vec2::ZERO) - 10.0).abs() < 0.05);
        assert!((terrain.height_at(Vec2::new(half / 2.0, 0.0)) - 20.0).abs() < 0.05);
        // Outside the file's world the edge height carries on, rather than the
        // level being stretched to reach the corner.
        assert!((terrain.height_at(Vec2::new(WORLD_SIZE / 2.0, 0.0)) - 20.0).abs() < 0.05);
    }

    #[test]
    fn a_file_with_a_non_finite_height_is_refused() {
        // A NaN reaches parry as a heightfield vertex and quietly breaks every
        // contact against it, so it has to die at the door.
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 3.0);
        let expected = terrain.snapshot();
        let poisoned = "(points: 2, extent: 320.0, heights: [0.0, NaN, 0.0, 0.0])";
        assert!(terrain.apply_ron(poisoned).is_err());
        assert_eq!(terrain.snapshot(), expected);
        assert!(
            terrain
                .apply_ron("(points: 2, extent: 0.0, heights: [0.0, 0.0, 0.0, 0.0])")
                .is_err()
        );
        assert_eq!(terrain.snapshot(), expected);
    }

    #[test]
    fn a_loaded_height_outside_the_guard_band_is_clamped() {
        let mut terrain = Terrain::flat();
        let text = format!(
            "(points: 2, extent: 320.0, heights: [{}, {}, {}, {}])",
            MAX_HEIGHT * 10.0,
            MAX_HEIGHT * 10.0,
            MIN_HEIGHT * 10.0,
            MIN_HEIGHT * 10.0
        );
        terrain.apply_ron(&text).expect("loads");
        let peak = terrain.snapshot().iter().cloned().fold(f32::MIN, f32::max);
        let pit = terrain.snapshot().iter().cloned().fold(f32::MAX, f32::min);
        assert!(peak <= MAX_HEIGHT, "{peak} should be capped");
        assert!(pit >= MIN_HEIGHT, "{pit} should be floored");
    }

    #[test]
    fn a_malformed_file_is_rejected_without_touching_the_grid() {
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 20.0, 3.0);
        let expected = terrain.snapshot();
        // Parses as RON but lies about its own dimensions.
        assert!(
            terrain
                .apply_ron("(points: 4, extent: 10.0, heights: [1.0])")
                .is_err()
        );
        assert!(terrain.apply_ron("not ron at all").is_err());
        assert_eq!(terrain.snapshot(), expected);
    }

    #[test]
    fn slope_deg_at_calculates_flat_and_inclined_slopes() {
        let flat_terrain = Terrain::flat();
        assert_eq!(flat_terrain.slope_deg_at(Vec2::ZERO), 0.0);

        let mut sloped_terrain = Terrain::flat();
        sloped_terrain.ramp_area(Vec2::ZERO, 0.0, Vec2::new(10.0, 0.0), 10.0, 5.0, 1.0);
        let slope = sloped_terrain.slope_deg_at(Vec2::new(5.0, 0.0));
        assert!(slope > 0.0, "slope on ramp should be positive, got {slope}");
    }
}
