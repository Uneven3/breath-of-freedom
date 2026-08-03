//! The terrain: the ground as **editable data**, and the single source of truth
//! the physics collider, the visual mesh and the footstep audio all derive from.
//!
//! Two channels, which is what a map needs to be more than a shape:
//! - **relief** — the height grid, in metres, on the grid's corners.
//! - **meaning** — what each cell is *made of* ([`TerrainKind`]), on the quads.
//!
//! Data lives here in `world` (data-in-the-world). The flat-shaded visual is
//! generated in [`crate::visuals::terrain`], and the in-engine authoring tool
//! lives in [`crate::editor`]. One grid → several representations, kept in sync by
//! change detection: mutating [`Terrain`] re-triggers the mesh rebuild, and the
//! collider rebuild when the *relief* specifically moved
//! ([`Terrain::relief_revision`]).
//!
//! **The grid owns *how* it changes.** Every brush the editor offers is a method
//! here; the editor only decides *where and when* one fires. That split is what
//! keeps a new brush from needing a new system — it is a closure handed to
//! [`Terrain::brush_stroke`], or in the semantic layer's case
//! [`Terrain::paint_area`].

use std::path::Path;

use avian3d::prelude::*;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::forest::hash_u32;
use super::layout::WORLD_SIZE;
use super::terrain_kind::TerrainKind;

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

#[expect(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "the value is clamped to a finite non-negative grid bound before truncation"
)]
fn bounded_grid_index(value: f32, last: usize) -> usize {
    value.clamp(0.0, last as f32) as usize
}

#[expect(
    clippy::cast_possible_truncation,
    reason = "noise coordinates are finite authored world positions; signed bits intentionally wrap"
)]
fn noise_coordinate(value: f32) -> u32 {
    (value.floor() as i32).cast_unsigned()
}

/// The ground as editable data: a height grid plus what each cell is **made
/// of**. Authoritative: the collider and the visual mesh are both derived from
/// it. Row-major `points × points`, where the row indexes X and the column
/// indexes Z (parry heightfield convention).
///
/// Two channels on one grid, because they answer two different questions and are
/// sampled differently. Heights live on the `points × points` **corners** and
/// interpolate — the surface between them is a real slope. Kinds live on the
/// `cells × cells` **quads** and do not: a patch of ground is stone or it is
/// sand, and there is no meaningful halfway.
#[derive(Component, Debug, Clone)]
pub struct Terrain {
    /// Heights in world units, row-major (`row * points + col`).
    heights: Vec<f32>,
    /// What each cell is made of, row-major (`row * cells + col`).
    kinds: Vec<TerrainKind>,
    /// Bumped every time [`Terrain::heights`] actually changes.
    ///
    /// `Changed<Terrain>` cannot tell the two channels apart, and the physics
    /// collider is derived from the heights alone. Without this, painting a patch
    /// of sand would rebuild a 16k-point heightfield that is bit-identical to the
    /// one already there — the most expensive thing this module does, for nothing.
    relief_revision: u32,
    /// Points per side (`CELLS + 1`).
    points: usize,
    /// World size spanned on each of X and Z.
    extent: f32,
}

/// The on-disk shape of a level. Resolution and extent travel with the data so a
/// file authored before a `CELLS` or `WORLD_SIZE` change still loads —
/// [`Terrain::apply_ron`] resamples in world space instead of rejecting.
#[derive(Serialize, Deserialize)]
struct TerrainFile {
    points: usize,
    extent: f32,
    heights: Vec<f32>,
    /// The semantic layer, run-length encoded, cell-major.
    ///
    /// Run-length because a painted map is mostly uniform: a fresh level is a
    /// single run, and a level with a meadow on it is a few hundred. That keeps
    /// the one part of the file a human can actually read *readable* — you can
    /// see at a glance that a level is all soil except one patch — next to 16k
    /// heights that will never be anything but noise.
    ///
    /// `default` on purpose: levels saved before this layer existed have no
    /// field, and an empty list means "all the default kind" rather than an
    /// error. That is what keeps the terrain files already on disk loadable.
    #[serde(default)]
    kinds: Vec<KindRun>,
}

/// Read-only seam for terrain queries outside the terrain owner.
///
/// The current world has one grid, but consumers ask this service for samples
/// or entity membership instead of encoding that cardinality with
/// `Query::single`. A future chunk lookup therefore changes here, not in every
/// player, movement, editor and presentation system.
#[derive(SystemParam)]
pub struct TerrainAccess<'w, 's> {
    terrains: Query<'w, 's, (Entity, &'static Terrain)>,
    changed: Query<'w, 's, (Entity, &'static Terrain), Changed<Terrain>>,
}

impl TerrainAccess<'_, '_> {
    pub fn height_at(&self, world_xz: Vec2) -> Option<f32> {
        self.primary().map(|terrain| terrain.height_at(world_xz))
    }

    pub fn kind_at(&self, world_xz: Vec2) -> Option<TerrainKind> {
        self.primary().map(|terrain| terrain.kind_at(world_xz))
    }

    pub fn slope_deg_at(&self, world_xz: Vec2) -> Option<f32> {
        self.primary().map(|terrain| terrain.slope_deg_at(world_xz))
    }

    pub fn contains(&self, entity: Entity) -> bool {
        self.terrains.contains(entity)
    }

    pub fn primary(&self) -> Option<&Terrain> {
        self.terrains.single().ok().map(|(_, terrain)| terrain)
    }

    pub fn changed(&self) -> impl Iterator<Item = (Entity, &Terrain)> {
        self.changed.iter()
    }
}

/// `(how many cells, of what)`. Serialises as `(1234, Soil)`.
#[derive(Serialize, Deserialize)]
struct KindRun(u32, TerrainKind);

/// A copy of both channels, for the editor's undo history.
///
/// Opaque on purpose: only [`Terrain::restore`] can put one back, so history
/// cannot invent a grid the terrain never had. It carries the semantic layer too
/// — undo has to take back a paint stroke as readily as a sculpt one, and a
/// snapshot of half the data would silently resurrect the other half.
#[derive(Clone, Debug, PartialEq)]
pub struct TerrainSnapshot {
    heights: Vec<f32>,
    kinds: Vec<TerrainKind>,
}

impl Terrain {
    /// A flat grid at `y = 0`, matching the graybox floor it replaces.
    fn flat() -> Self {
        let points = CELLS + 1;
        Self {
            heights: vec![0.0; points * points],
            kinds: vec![TerrainKind::default(); CELLS * CELLS],
            relief_revision: 0,
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

    /// How many times the height grid has changed. Only meaningful compared
    /// against a previously observed value.
    pub fn relief_revision(&self) -> u32 {
        self.relief_revision
    }

    /// Cells per side — one fewer than [`Terrain::points`], since a cell is the
    /// quad *between* four corners.
    pub fn cells(&self) -> usize {
        self.points - 1
    }

    pub fn height(&self, row: usize, col: usize) -> f32 {
        self.heights[row * self.points + col]
    }

    /// What cell `(row, col)` is made of.
    pub fn kind(&self, row: usize, col: usize) -> TerrainKind {
        self.kinds[row * self.cells() + col]
    }

    /// What the ground is made of at an arbitrary world XZ.
    ///
    /// Nearest cell, never interpolated: the answer is "stone" or "sand", and
    /// there is no blend of the two to return. Read every tick by the ground
    /// probe, so it stays a couple of divisions and an index.
    pub fn kind_at(&self, xz: Vec2) -> TerrainKind {
        let (row, col) = self.cell_at(xz);
        self.kind(row, col)
    }

    /// The cell containing a world XZ, clamped to the grid.
    fn cell_at(&self, xz: Vec2) -> (usize, usize) {
        let cells = self.cells();
        let last = cells - 1;
        let index = |v: f32| bounded_grid_index((v / self.extent + 0.5) * cells as f32, last);
        (index(xz.x), index(xz.y))
    }

    /// World XZ of the *centre* of cell `(row, col)` — half a cell past its
    /// low corner, which is the point a paint brush measures its radius against.
    fn cell_centre_xz(&self, row: usize, col: usize) -> Vec2 {
        let spacing = self.extent / self.cells() as f32;
        self.point_xz(row, col) + Vec2::splat(spacing * 0.5)
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
    /// the visual mesh cut each cell into two flat triangles; a bilinear patch
    /// bulges above that surface inside the quad — by half a metre on 2.5 m cells
    /// with real relief. Lifting a body onto the bulge is how the player ended up
    /// hovering above sculpted ground.
    ///
    /// **Which diagonal is not ours to choose** — see [`Terrain::to_collider`].
    pub fn height_at(&self, xz: Vec2) -> f32 {
        let last = (self.points - 1) as f32;
        let fx = ((xz.x / self.extent + 0.5) * last).clamp(0.0, last);
        let fz = ((xz.y / self.extent + 0.5) * last).clamp(0.0, last);
        let row = bounded_grid_index(fx.floor(), self.points - 2);
        let col = bounded_grid_index(fz.floor(), self.points - 2);
        // Position inside the cell, both in `[0, 1]`.
        let u = fx - row as f32;
        let v = fz - col as f32;
        let corner = |dr: usize, dc: usize| self.height(row + dr, col + dc);
        // The shared diagonal runs where `u + v == 1`, so that sum picks the
        // triangle; each expression is the plane through its three corners.
        if u + v <= 1.0 {
            // The triangle holding the low corner: (0,0), (1,0), (0,1).
            corner(0, 0) + (corner(1, 0) - corner(0, 0)) * u + (corner(0, 1) - corner(0, 0)) * v
        } else {
            // The triangle holding the high corner: (1,1), (0,1), (1,0).
            corner(1, 1)
                + (corner(0, 1) - corner(1, 1)) * (1.0 - u)
                + (corner(1, 0) - corner(1, 1)) * (1.0 - v)
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
    ///
    /// **This function is why the cell diagonal is not ours to choose.** Which way a
    /// quad is cut into two triangles is parry's decision, and everything that reads
    /// this grid has to follow it.
    ///
    /// A quad's four corners are only coplanar on flat or uniformly sloping ground.
    /// Anywhere the relief twists, the two possible diagonals describe two *different
    /// surfaces*, and the collider is authoritative because it is the one bodies
    /// stand on.
    ///
    /// `parry3d` cuts along the **anti-diagonal** — from `(row, col+1)` to
    /// `(row+1, col)`. Its `HeightField::triangles_at` picks the main diagonal only
    /// for cells flagged `ZIGZAG_SUBDIVISION`, and the default cell status is empty
    /// (`heightfield3.rs`: `Array2::repeat(.., HeightFieldCellStatus::default())`),
    /// so no cell is ever flagged. Nothing in our code path sets it, and reaching it
    /// would need `parry` as a direct dependency.
    ///
    /// Found by playing: the collider visibly disagreed with the ground "where the
    /// terrain is very uneven", which is exactly the shape of this bug. Measured
    /// before the fix on hard-sculpted ground: **0.33 m at worst, and 36% of sampled
    /// points off by more than a centimetre**. It cost nothing on the flat graybox
    /// floor this code was written against, which is why it survived this long — and
    /// why the test that guards it now sweeps a grid instead of six points.
    fn to_collider(&self) -> Collider {
        let rows: Vec<Vec<f32>> = (0..self.points)
            .map(|row| (0..self.points).map(|col| self.height(row, col)).collect())
            .collect();
        Collider::heightfield(rows, Vec3::new(self.extent, 1.0, self.extent))
    }

    /// A copy of both channels, for the editor's undo history.
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
    /// grid, 2.5 m at the current resolution. Fine detail is not available at
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
    fn cell_window(
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
    fn window(
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
fn encode_kinds(kinds: &[TerrainKind]) -> Vec<KindRun> {
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
fn decode_kinds(runs: &[KindRun], cells: usize) -> Result<Vec<TerrainKind>, String> {
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
    let row = idx / points;
    let col = idx % points;
    let mut sum = 0.0;
    let mut count = 0.0;
    let neighbours = [
        row.checked_sub(1).map(|nr| (nr, col)),
        (row + 1 < points).then_some((row + 1, col)),
        col.checked_sub(1).map(|nc| (row, nc)),
        (col + 1 < points).then_some((row, col + 1)),
    ];
    for (nr, nc) in neighbours.into_iter().flatten() {
        sum += grid[nr * points + nc];
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
        let x = noise_coordinate(cell.x + dx);
        let y = noise_coordinate(cell.y + dy);
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
    let r0 = bounded_grid_index(row.floor(), last);
    let c0 = bounded_grid_index(col.floor(), last);
    let r1 = (r0 + 1).min(last);
    let c1 = (c0 + 1).min(last);
    let fr = row - r0 as f32;
    let fc = col - c0 as f32;
    let top = heights[r0 * points + c0].lerp(heights[r0 * points + c1], fc);
    let bottom = heights[r1 * points + c0].lerp(heights[r1 * points + c1], fc);
    top.lerp(bottom, fr)
}

/// Tracks which version of the relief the collider on this entity was built
/// from, so the rebuild can tell a sculpt edit from a paint edit.
#[derive(Component)]
pub(super) struct ColliderRevision(u32);

/// Rebuild the heightfield collider whenever the **relief** changes. Reacts to
/// `Changed`, which also fires the frame the terrain spawns — rebuilding once
/// more then is harmless.
///
/// `Changed<Terrain>` is necessary but not sufficient: the semantic layer shares
/// the component and changes far more often than the heights do while painting,
/// and this rebuild is the single most expensive thing an edit can trigger. The
/// revision check is what keeps painting off the physics path entirely.
pub(super) fn rebuild_terrain_collider(
    mut terrain: Query<(&Terrain, &mut Collider, &mut ColliderRevision), Changed<Terrain>>,
) {
    for (terrain, mut collider, mut built) in &mut terrain {
        if built.0 == terrain.relief_revision() {
            continue;
        }
        built.0 = terrain.relief_revision();
        *collider = terrain.to_collider();
    }
}

/// Spawn the single terrain entity: the height grid plus its derived collider.
/// Static world geometry, so it stays on the `Default` collision layer (no
/// `CollisionLayers`) where ledge sensing can see it.
///
/// No `Surface` component, unlike every other piece of ground. One component
/// could only name one surface for all 320 m of terrain, which is exactly the
/// thing the semantic layer exists to stop being true; the ground probe reads
/// [`Terrain::kind_at`] at the contact point instead. Leaving a stale `Surface`
/// on the entity would have given the probe two answers to choose from.
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
    let revision = ColliderRevision(terrain.relief_revision());
    commands.spawn((
        DespawnOnExit(*state.get()),
        Name::new("Terrain"),
        terrain,
        collider,
        revision,
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
        Transform::default(),
    ));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    fn read_access(terrain: TerrainAccess) -> (Option<f32>, Option<TerrainKind>, usize) {
        (
            terrain.height_at(Vec2::ZERO),
            terrain.kind_at(Vec2::ZERO),
            terrain.changed().count(),
        )
    }

    #[test]
    fn terrain_access_is_the_single_seam_for_sampling_the_current_grid() {
        let mut world = World::new();
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 8.0, 3.0);
        terrain.paint_area(Vec2::ZERO, 8.0, TerrainKind::Rock);
        world.spawn(terrain);

        let (height, kind, changed) = world.run_system_once(read_access).unwrap();
        assert!(height.is_some_and(|height| height > 0.0));
        assert_eq!(kind, Some(TerrainKind::Rock));
        assert_eq!(changed, 1);
    }

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
        let inside = bounded_grid_index((radius / spacing).floor(), terrain.points()) - 1;
        let outside = bounded_grid_index((radius / spacing).ceil(), terrain.points()) + 1;
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

        // The cell centre sits exactly on the shared diagonal, so the surface
        // there is the height of that diagonal's endpoints — and bilinear would
        // say 5.0, the average of all four corners, which is the bug this guards.
        //
        // Which endpoints those are depends on which diagonal parry cuts, and
        // this assertion used to say `0.0`: it had the *other* diagonal baked in,
        // and went on passing while the collider and the mesh described two
        // different surfaces. So it now checks the invariant it actually cares
        // about — the sample is on a triangle, not on the bulge — and gets the
        // diagonal from the collider instead of from an assumption.
        let centre = (terrain.point_xz(row, col) + terrain.point_xz(row + 1, col + 1)) * 0.5;
        let sampled = terrain.height_at(centre);

        let collider = terrain.to_collider();
        let ray =
            avian3d::parry::query::Ray::new(Vec3::new(centre.x, 500.0, centre.y), Vec3::NEG_Y);
        let hit = collider
            .shape_scaled()
            .cast_local_ray(&ray, 1000.0, true)
            .expect("the ray must hit the terrain");
        let on_collider = 500.0 - hit;

        assert!(
            (sampled - on_collider).abs() < 0.01,
            "the grid says {sampled} where the collider says {on_collider}"
        );
        assert!(
            (sampled - 5.0).abs() > 1.0,
            "{sampled} is the bilinear average — the sample left the triangle"
        );
    }

    #[test]
    fn the_collider_surface_matches_the_grid_everywhere() {
        // The wireframe looked offset from the ground. This settles it without
        // interpreting a screenshot: shoot a ray straight down at the actual
        // collider and compare where it lands with what `height_at` (the surface
        // the mesh is built from) claims. Any mapping mistake — transposed axes,
        // a shifted origin, the wrong diagonal — shows up here as a mismatch.
        //
        // **Swept, not sampled.** This test used to check six hand-picked points
        // and it passed for days while a third of the terrain was wrong: cutting
        // the cell along the other diagonal than parry does is invisible on even
        // ground and grows with the twist of each quad, so six points on gentle
        // relief hit none of it. Hence: hard-sculpted ground, and a grid of
        // samples deliberately offset from the cell centres so they land all over
        // the inside of the quads, where the two triangulations differ most.
        // See `Terrain::to_collider`.
        let mut terrain = Terrain::flat();
        terrain.raise_area(Vec2::ZERO, 60.0, 12.0);
        terrain.noise_area(Vec2::ZERO, 150.0, 6.0, 7);
        terrain.terrace_area(Vec2::new(30.0, 30.0), 40.0, 2.0, 1.0);
        let collider = terrain.to_collider();
        let shape = collider.shape_scaled();

        let mut worst = 0.0_f32;
        let mut worst_at = Vec2::ZERO;
        let mut checked = 0;
        for i in 0..60 {
            for j in 0..60 {
                // 2.51 m step against 2.5 m cells: the sample drifts across the
                // cell instead of landing on the same spot of every one.
                let xz = Vec2::new(-75.0 + i as f32 * 2.51, -75.0 + j as f32 * 2.51);
                let ray =
                    avian3d::parry::query::Ray::new(Vec3::new(xz.x, 500.0, xz.y), Vec3::NEG_Y);
                let Some(hit) = shape.cast_local_ray(&ray, 1000.0, true) else {
                    continue;
                };
                checked += 1;
                let error = (500.0 - hit - terrain.height_at(xz)).abs();
                if error > worst {
                    worst = error;
                    worst_at = xz;
                }
            }
        }
        assert!(checked > 3000, "the sweep hit only {checked} points");
        assert!(
            worst < 0.01,
            "collider and grid disagree by {worst} m at {worst_at:?}"
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
        let stale = TerrainSnapshot {
            heights: vec![0.0; 4],
            kinds: vec![TerrainKind::default(); 1],
        };
        assert!(!terrain.restore(&stale));
    }

    #[test]
    fn a_snapshot_carries_the_semantic_layer_too() {
        // Undo has to take back a paint stroke as readily as a sculpt one. A
        // snapshot of only the heights would restore the shape and leave the
        // painted meaning behind — the level would come back half undone.
        let mut terrain = Terrain::flat();
        let before = terrain.snapshot();
        assert!(terrain.paint_area(Vec2::ZERO, 20.0, TerrainKind::Rock));
        assert_ne!(terrain.snapshot(), before);
        assert!(terrain.restore(&before));
        assert_eq!(terrain.kind_at(Vec2::ZERO), TerrainKind::Soil);
    }

    #[test]
    fn painting_covers_the_radius_and_nothing_past_it() {
        let mut terrain = Terrain::flat();
        let radius = 20.0;
        assert!(terrain.paint_area(Vec2::ZERO, radius, TerrainKind::Rock));
        assert_eq!(terrain.kind_at(Vec2::ZERO), TerrainKind::Rock);
        // A cell centre just inside the circle is painted; well outside is not.
        let spacing = WORLD_SIZE / CELLS as f32;
        assert_eq!(
            terrain.kind_at(Vec2::new(radius - spacing * 1.5, 0.0)),
            TerrainKind::Rock
        );
        assert_eq!(
            terrain.kind_at(Vec2::new(radius + spacing * 1.5, 0.0)),
            TerrainKind::Soil
        );
    }

    #[test]
    fn painting_the_same_patch_twice_changes_nothing() {
        // Paint has no rate and no falloff, so a held button must be idempotent.
        // The editor relies on this to know whether a stroke is worth an undo
        // step: a brush that always reports a change files empty history.
        let mut terrain = Terrain::flat();
        assert!(terrain.paint_area(Vec2::ZERO, 15.0, TerrainKind::Sand));
        assert!(!terrain.paint_area(Vec2::ZERO, 15.0, TerrainKind::Sand));
    }

    #[test]
    fn painting_at_the_world_edge_stays_inside_the_grid() {
        // Same guard the sculpt window needs: the cell window is clamped, so a
        // stroke centred on the corner must not index past the end.
        let mut terrain = Terrain::flat();
        let edge = WORLD_SIZE / 2.0;
        terrain.paint_area(Vec2::new(edge, edge), 30.0, TerrainKind::Rock);
        terrain.paint_area(Vec2::new(-edge, -edge), 30.0, TerrainKind::Sand);
        let last = terrain.cells() - 1;
        assert_eq!(terrain.kind(last, last), TerrainKind::Rock);
        assert_eq!(terrain.kind(0, 0), TerrainKind::Sand);
    }

    #[test]
    fn painting_never_touches_the_relief_revision() {
        // The guard that keeps painting off the physics path: the collider is
        // derived from the heights alone, and rebuilding it is the most expensive
        // thing an edit can trigger. A paint stroke must be invisible to it.
        let mut terrain = Terrain::flat();
        let before = terrain.relief_revision();
        assert!(terrain.paint_area(Vec2::ZERO, 20.0, TerrainKind::TallGrass));
        assert_eq!(
            terrain.relief_revision(),
            before,
            "painting moved no ground"
        );

        terrain.raise_area(Vec2::ZERO, 20.0, 1.0);
        assert_ne!(
            terrain.relief_revision(),
            before,
            "sculpting did move ground"
        );
    }

    #[test]
    fn undoing_a_paint_stroke_does_not_rebuild_the_collider() {
        // The same guard on the way back. `restore` writes both channels, so it
        // has to check whether the relief actually differs rather than assume it.
        let mut terrain = Terrain::flat();
        let before = terrain.snapshot();
        terrain.paint_area(Vec2::ZERO, 20.0, TerrainKind::Rock);
        let revision = terrain.relief_revision();
        assert!(terrain.restore(&before));
        assert_eq!(terrain.relief_revision(), revision);

        // ...but undoing a sculpt stroke must.
        terrain.raise_area(Vec2::ZERO, 20.0, 2.0);
        let sculpted = terrain.relief_revision();
        assert!(terrain.restore(&before));
        assert_ne!(terrain.relief_revision(), sculpted);
    }

    #[test]
    fn the_semantic_layer_round_trips_through_disk() {
        let mut terrain = Terrain::flat();
        terrain.paint_area(Vec2::ZERO, 25.0, TerrainKind::TallGrass);
        terrain.paint_area(Vec2::new(60.0, -40.0), 12.0, TerrainKind::Rock);
        let expected = terrain.snapshot();
        let text = terrain.to_ron().expect("serialises");
        let mut loaded = Terrain::flat();
        loaded.apply_ron(&text).expect("loads");
        assert_eq!(loaded.snapshot(), expected);
    }

    #[test]
    fn a_level_saved_before_the_semantic_layer_still_loads() {
        // The terrain files already on disk have no `kinds` field. They are the
        // levels of the sessions before this one, and losing them to a format
        // change is exactly what the file-is-the-level rule forbids.
        //
        // Both load paths, because `apply_ron` branches on whether the file's
        // resolution matches the current one and the real files take the branch a
        // small hand-written fixture does not: `sandbox.ron` on disk today is
        // `points: 129, extent: 320.0` with no `kinds`.
        let resampled = "(\n    points: 3,\n    extent: 320.0,\n    heights: [0.0, 1.0, 2.0, \
                         3.0, 4.0, 5.0, 6.0, 7.0, 8.0],\n)\n";
        let native = format!(
            "(points: {}, extent: {WORLD_SIZE:?}, heights: {:?})",
            CELLS + 1,
            vec![0.0f32; (CELLS + 1) * (CELLS + 1)]
        );
        for text in [resampled.to_string(), native] {
            let mut terrain = Terrain::flat();
            terrain.paint_area(Vec2::ZERO, 40.0, TerrainKind::Rock);
            terrain
                .apply_ron(&text)
                .expect("a file without kinds loads");
            // And it loads as unpainted, rather than leaving whatever the previous
            // level had painted in place: the file is the level, all of it.
            assert_eq!(terrain.kind_at(Vec2::ZERO), TerrainKind::Soil);
        }
    }

    #[test]
    fn a_truncated_semantic_layer_is_refused() {
        // Run-length encoding's failure mode: runs that do not add up would
        // offset every cell after the gap, sliding the whole map's meaning
        // sideways. Silently accepting that is worse than not loading.
        let text = "(\n    points: 3,\n    extent: 320.0,\n    heights: [0.0, 0.0, 0.0, \
                    0.0, 0.0, 0.0, 0.0, 0.0, 0.0],\n    kinds: [(2, Rock)],\n)\n";
        let mut terrain = Terrain::flat();
        let error = terrain
            .apply_ron(text)
            .expect_err("a short run list must be refused");
        assert!(error.contains("semantic layer"), "unhelpful error: {error}");
    }

    #[test]
    fn run_length_encoding_survives_every_pattern_it_can_meet() {
        // Encode/decode is the one place a bug corrupts a level silently, so it
        // is pinned on the shapes that actually occur: uniform (a fresh level),
        // one patch (a painted one), and alternating (the worst case for runs).
        let cells = 8;
        let uniform = vec![TerrainKind::Soil; cells * cells];
        let mut patch = uniform.clone();
        for cell in patch.iter_mut().skip(10).take(7) {
            *cell = TerrainKind::TallGrass;
        }
        let striped: Vec<TerrainKind> = (0..cells * cells)
            .map(|i| TerrainKind::ALL[i % TerrainKind::ALL.len()])
            .collect();
        for original in [uniform, patch, striped] {
            let runs = encode_kinds(&original);
            assert_eq!(decode_kinds(&runs, cells).expect("decodes"), original);
        }
        // And the point of the encoding: a fresh level is a single run.
        assert_eq!(encode_kinds(&vec![TerrainKind::Soil; 4096]).len(), 1);
    }

    #[test]
    fn a_semantic_layer_from_another_resolution_is_resampled_without_inventing_kinds() {
        // Heights resample bilinearly; kinds must not. Interpolating them would
        // produce values between `Rock` and `TallGrass` — which is to say kinds
        // nobody painted, appearing the first time `CELLS` changes.
        let coarse = TerrainFile {
            points: 3,
            extent: WORLD_SIZE,
            heights: vec![0.0; 9],
            // 2×2 cells: rock along the low-x row, sand along the high-x row.
            kinds: vec![KindRun(2, TerrainKind::Rock), KindRun(2, TerrainKind::Sand)],
        };
        let text = ron::ser::to_string(&coarse).expect("serialises");
        let mut terrain = Terrain::flat();
        terrain.apply_ron(&text).expect("loads");
        let quarter = WORLD_SIZE / 4.0;
        assert_eq!(terrain.kind_at(Vec2::new(-quarter, 0.0)), TerrainKind::Rock);
        assert_eq!(terrain.kind_at(Vec2::new(quarter, 0.0)), TerrainKind::Sand);
        // Every cell holds a kind that was actually in the file.
        for row in 0..terrain.cells() {
            for col in 0..terrain.cells() {
                let kind = terrain.kind(row, col);
                assert!(
                    kind == TerrainKind::Rock || kind == TerrainKind::Sand,
                    "resampling invented {kind:?} at ({row}, {col})"
                );
            }
        }
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
            kinds: Vec::new(),
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
            kinds: Vec::new(),
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
        let heights = terrain.snapshot().heights;
        let peak = heights.iter().cloned().fold(f32::MIN, f32::max);
        let pit = heights.iter().cloned().fold(f32::MAX, f32::min);
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
