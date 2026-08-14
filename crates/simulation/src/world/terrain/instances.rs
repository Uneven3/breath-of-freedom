//! The instance layer: placing and removing discrete props on the terrain.
//! `Terrain` owns *how* it changes — including the minimum spacing between
//! two props — the editor only decides *where and when*, same split as
//! `sculpt`. [`Terrain::apply_ron`] is a third, wholesale edit; it bumps
//! `instances_generation` instead, so presentation can tell "one prop was
//! added" from "everything may have moved".
//!
//! Deliberately outside [`super::TerrainSnapshot`]/[`Terrain::restore`]: undo
//! for this layer does not exist yet (`docs/MAP_EDITOR.md`), so Ctrl+Z cannot
//! take back a placement or a clear.

use bevy_math::prelude::*;
use bof_domain::props::{Instance, PropKind};

use super::Terrain;

/// Independent of any brush radius — a small brush must not cram props
/// tighter than a large one, it should just cover less ground.
const MIN_SPACING_M: f32 = 0.4;

impl Terrain {
    pub fn instances(&self) -> &[Instance] {
        &self.instances
    }

    pub fn instances_revision(&self) -> u32 {
        self.instances_revision
    }

    pub fn instances_generation(&self) -> u32 {
        self.instances_generation
    }

    /// Adds one instance unless it lands within [`MIN_SPACING_M`] of one
    /// already there; returns whether it did. Never takes a height — callers
    /// sample that from this same terrain at spawn time, never store it.
    pub fn place_instance(&mut self, kind: PropKind, xz: Vec2, yaw: f32, scale: f32) -> bool {
        let too_close = self
            .instances
            .iter()
            .any(|instance| instance.xz.distance_squared(xz) < MIN_SPACING_M * MIN_SPACING_M);
        if too_close {
            return false;
        }
        self.instances.push(Instance {
            kind,
            xz,
            yaw,
            scale,
        });
        self.instances_revision = self.instances_revision.wrapping_add(1);
        true
    }

    /// Removes every instance within `radius` of `centre`; returns whether
    /// anything actually was, same contract as [`Terrain::paint_area`].
    pub fn remove_instances_in_radius(&mut self, centre: Vec2, radius: f32) -> bool {
        let before = self.instances.len();
        let radius_sq = radius * radius;
        self.instances
            .retain(|instance| instance.xz.distance_squared(centre) > radius_sq);
        let removed = self.instances.len() != before;
        if removed {
            self.instances_revision = self.instances_revision.wrapping_add(1);
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placing_an_instance_grows_the_list_and_bumps_the_revision() {
        let mut terrain = Terrain::flat_for_test();
        let before = terrain.instances_revision();
        assert!(terrain.place_instance(PropKind::GrassA, Vec2::new(3.0, 4.0), 0.5, 1.0));
        assert_eq!(terrain.instances().len(), 1);
        assert_eq!(terrain.instances()[0].kind, PropKind::GrassA);
        assert_eq!(terrain.instances()[0].xz, Vec2::new(3.0, 4.0));
        assert!(terrain.instances_revision() != before);
    }

    #[test]
    fn placing_too_close_to_an_existing_instance_is_rejected() {
        // The grid owns this invariant, not whichever caller happens to
        // drive it — a future importer or test must not be able to stack
        // two props on the same spot just by skipping a check the editor
        // used to make on its own.
        let mut terrain = Terrain::flat_for_test();
        terrain.place_instance(PropKind::GrassA, Vec2::ZERO, 0.0, 1.0);
        let before = terrain.instances_revision();
        let placed = terrain.place_instance(PropKind::GrassB, Vec2::new(0.1, 0.0), 0.0, 1.0);
        assert!(!placed, "0.1 m is well inside the minimum spacing");
        assert_eq!(terrain.instances().len(), 1);
        assert_eq!(terrain.instances_revision(), before);
        // Far enough away still succeeds.
        assert!(terrain.place_instance(PropKind::GrassB, Vec2::new(5.0, 0.0), 0.0, 1.0));
        assert_eq!(terrain.instances().len(), 2);
    }

    #[test]
    fn removing_in_radius_only_takes_what_the_circle_covers() {
        let mut terrain = Terrain::flat_for_test();
        terrain.place_instance(PropKind::GrassA, Vec2::new(0.0, 0.0), 0.0, 1.0);
        terrain.place_instance(PropKind::GrassB, Vec2::new(20.0, 0.0), 0.0, 1.0);
        let removed = terrain.remove_instances_in_radius(Vec2::ZERO, 5.0);
        assert!(removed);
        assert_eq!(terrain.instances().len(), 1);
        assert_eq!(terrain.instances()[0].kind, PropKind::GrassB);
    }

    #[test]
    fn removing_where_nothing_stands_reports_no_change() {
        let mut terrain = Terrain::flat_for_test();
        terrain.place_instance(PropKind::GrassA, Vec2::new(50.0, 50.0), 0.0, 1.0);
        let before = terrain.instances_revision();
        let removed = terrain.remove_instances_in_radius(Vec2::ZERO, 5.0);
        assert!(!removed);
        assert_eq!(terrain.instances().len(), 1);
        assert_eq!(terrain.instances_revision(), before);
    }
}
