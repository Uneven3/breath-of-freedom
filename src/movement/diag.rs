//! Diagnostic capture for the sensor suite.
//!
//! Services record every spatial cast they actually issue into `CastTrace`
//! (when F2 or F4 enables capture), so the debug gizmos draw ground truth instead of a
//! re-computation that could drift from the real sensors. Recording is a
//! no-op while disabled (`DebugPlugin` toggles it, key F2).

use bevy::prelude::*;

/// The spatial-query API used for a recorded sensor probe.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Shape,
    Ray,
}

/// One spatial query as issued by a service this fixed tick.
#[derive(Clone, Copy)]
pub struct CastRecord {
    pub entity: Entity,
    pub kind: CastKind,
    /// Which sensor issued it (e.g. "ground_probe", "ledge_fwd_3").
    pub label: &'static str,
    pub origin: Vec3,
    pub dir: Vec3,
    pub max_dist: f32,
    /// Hit point and world-space normal, if anything was hit.
    pub hit: Option<(Vec3, Vec3)>,
}

/// Per-fixed-tick log of sensor casts. Cleared at the start of `SenseWorld`,
/// drawn by `debug.rs` every render frame until the next tick refills it.
#[derive(Resource, Default)]
pub struct CastTrace {
    pub enabled: bool,
    pub records: Vec<CastRecord>,
}

impl CastTrace {
    /// Record a shape cast; no-op while capture is disabled.
    pub fn record_shape(
        &mut self,
        entity: Entity,
        label: &'static str,
        origin: Vec3,
        dir: Vec3,
        max_dist: f32,
        hit: Option<(Vec3, Vec3)>,
    ) {
        if self.enabled {
            self.records.push(CastRecord {
                entity,
                kind: CastKind::Shape,
                label,
                origin,
                dir,
                max_dist,
                hit,
            });
        }
    }

    pub fn record_ray(
        &mut self,
        entity: Entity,
        label: &'static str,
        origin: Vec3,
        dir: Vec3,
        max_dist: f32,
        hit: Option<(Vec3, Vec3)>,
    ) {
        if self.enabled {
            self.records.push(CastRecord {
                entity,
                kind: CastKind::Ray,
                label,
                origin,
                dir,
                max_dist,
                hit,
            });
        }
    }
}

/// Clears the trace right before the services sense the world.
pub fn clear_cast_trace(mut trace: ResMut<CastTrace>) {
    trace.records.clear();
}
