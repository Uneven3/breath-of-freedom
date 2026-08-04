//! Lectura del terreno: alturas, pendientes, y qué significa cada celda.
//!
//! Todo lo que consulta la grilla sin cambiarla. Es la mitad que consume el
//! resto de la simulación —`TerrainAccess` la enruta— mientras que esculpir
//! vive en [`super::sculpt`] y guardar en [`super::persist`].

use bevy_math::prelude::*;

use super::{Terrain, bounded_grid_index, grid_xz};
use crate::world::terrain_kind::TerrainKind;

impl Terrain {
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
    pub(super) fn cell_at(&self, xz: Vec2) -> (usize, usize) {
        let cells = self.cells();
        let last = cells - 1;
        let index = |v: f32| bounded_grid_index((v / self.extent + 0.5) * cells as f32, last);
        (index(xz.x), index(xz.y))
    }

    /// World XZ of the *centre* of cell `(row, col)` — half a cell past its
    /// low corner, which is the point a paint brush measures its radius against.
    pub(super) fn cell_centre_xz(&self, row: usize, col: usize) -> Vec2 {
        let spacing = self.extent / self.cells() as f32;
        self.point_xz(row, col) + Vec2::splat(spacing * 0.5)
    }

    /// World XZ of grid point `(row, col)`, independent of its height. The one
    /// mapping the collider and the visual mesh must agree on, so they never
    /// drift: parry centers the heightfield on the entity origin, spanning
    /// `[-extent/2, extent/2]`.
    pub(super) fn point_xz(&self, row: usize, col: usize) -> Vec2 {
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
}
