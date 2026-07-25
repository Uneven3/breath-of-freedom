//! The terrain: the ground as **editable data** — a height grid that is the
//! single source of truth the physics collider and the visual mesh both derive
//! from.
//!
//! Data lives here in `world` (data-in-the-world). The flat-shaded visual is
//! generated in [`crate::visuals::terrain`], and the in-engine sculpt tool will
//! live in `editor`. One grid → two representations, kept in sync by change
//! detection: mutating [`Terrain`] re-triggers both the collider rebuild and the
//! mesh rebuild.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::Surface;
use super::layout::WORLD_SIZE;
use crate::asset_pipeline::schema::SurfaceKind;

/// Grid cells per side; the heightfield has `CELLS + 1` points per side. Sized
/// so the brush covers enough vertices to sculpt smooth domes (a coarser grid
/// gives spiky tents) while the whole-terrain rebuild each edit stays cheap.
/// Pushing this much higher is the point where per-edit *partial* rebuilds
/// (chunking) start to matter — deferred for now.
const CELLS: usize = 128;

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
        let last = (self.points - 1) as f32;
        Vec2::new(
            (row as f32 / last - 0.5) * self.extent,
            (col as f32 / last - 0.5) * self.extent,
        )
    }

    /// Full world position of grid point `(row, col)`, height included.
    pub fn point_world_pos(&self, row: usize, col: usize) -> Vec3 {
        let xz = self.point_xz(row, col);
        Vec3::new(xz.x, self.height(row, col), xz.y)
    }

    /// The heightfield collider derived from the grid. `scale` maps parry's unit
    /// rectangle to the world extent; heights pass through at `scale.y = 1`.
    fn to_collider(&self) -> Collider {
        let rows: Vec<Vec<f32>> = (0..self.points)
            .map(|row| (0..self.points).map(|col| self.height(row, col)).collect())
            .collect();
        Collider::heightfield(rows, Vec3::new(self.extent, 1.0, self.extent))
    }

    /// Raise (or lower, with negative `delta`) the grid around `center` in world
    /// XZ, with a smooth falloff to `radius`. The sculpt brush's one edit: the
    /// editor decides *where and when*, the grid owns *how* it changes. Mutating
    /// through `&mut Terrain` flags `Changed`, which regenerates collider + mesh.
    pub fn raise_area(&mut self, center: Vec2, radius: f32, delta: f32) {
        self.brush(center, radius, |_grid, _idx, falloff| delta * falloff);
    }

    /// Relax the grid around `center` toward each point's neighbour average, to
    /// erode the spikes a heavy raise leaves. `amount` in `[0, 1]` is how far to
    /// pull toward the average at full falloff. Reads the pre-stroke snapshot
    /// `brush` provides, so the pass has no directional bias.
    pub fn smooth_area(&mut self, center: Vec2, radius: f32, amount: f32) {
        let points = self.points;
        self.brush(center, radius, |grid, idx, falloff| {
            let row = idx / points;
            let col = idx % points;
            let mut sum = 0.0;
            let mut count = 0.0;
            for (dr, dc) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                let nr = row as i32 + dr;
                let nc = col as i32 + dc;
                if nr < 0 || nc < 0 || nr >= points as i32 || nc >= points as i32 {
                    continue;
                }
                sum += grid[nr as usize * points + nc as usize];
                count += 1.0;
            }
            let average = if count > 0.0 { sum / count } else { grid[idx] };
            (average - grid[idx]) * amount * falloff
        });
    }

    /// Shared brush traversal: for every grid point within `radius` of `center`,
    /// add `delta(pre_stroke_snapshot, index, falloff)` to its height. The
    /// snapshot lets smoothing read unbiased neighbours while still writing in
    /// place; a plain raise just ignores it.
    fn brush(&mut self, center: Vec2, radius: f32, delta: impl Fn(&[f32], usize, f32) -> f32) {
        if radius <= 0.0 {
            return;
        }
        let snapshot = self.heights.clone();
        for row in 0..self.points {
            for col in 0..self.points {
                let distance = self.point_xz(row, col).distance(center);
                if distance >= radius {
                    continue;
                }
                // Smoothstep falloff: full strength at the center, zero at the rim.
                let t = 1.0 - distance / radius;
                let falloff = t * t * (3.0 - 2.0 * t);
                let idx = row * self.points + col;
                self.heights[idx] += delta(&snapshot, idx, falloff);
            }
        }
    }
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
pub(super) fn setup_terrain(mut commands: Commands) {
    let terrain = Terrain::flat();
    let collider = terrain.to_collider();
    commands.spawn((
        Name::new("Terrain"),
        terrain,
        collider,
        // A heightfield is a thin, one-sided surface (unlike the old solid floor
        // box), so a kinematic body can penetrate it at cell edges or on slopes.
        // avian's documented remedy for thin colliders: a margin that gives the
        // surface virtual thickness for stable depenetration. Tunable.
        CollisionMargin(0.1),
        RigidBody::Static,
        Surface(SurfaceKind::Grass),
        Transform::default(),
    ));
}
