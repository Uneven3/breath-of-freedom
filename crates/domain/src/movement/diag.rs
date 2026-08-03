use bevy_ecs::prelude::{Entity, Resource};
use bevy_math::Vec3;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum CastKind {
    Shape,
    Ray,
}

#[derive(Clone, Copy)]
pub struct CastRecord {
    pub entity: Entity,
    pub kind: CastKind,
    pub label: &'static str,
    pub origin: Vec3,
    pub dir: Vec3,
    pub max_dist: f32,
    pub hit: Option<(Vec3, Vec3)>,
}

const MAX_CAST_RECORDS: usize = 1024;

#[derive(Resource)]
pub struct CastTrace {
    pub enabled: bool,
    pub records: Vec<CastRecord>,
}

impl Default for CastTrace {
    fn default() -> Self {
        Self {
            enabled: false,
            records: Vec::with_capacity(MAX_CAST_RECORDS),
        }
    }
}

impl CastTrace {
    pub fn record_shape(
        &mut self,
        entity: Entity,
        label: &'static str,
        origin: Vec3,
        dir: Vec3,
        max_dist: f32,
        hit: Option<(Vec3, Vec3)>,
    ) {
        if self.enabled && self.records.len() < MAX_CAST_RECORDS {
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
        if self.enabled && self.records.len() < MAX_CAST_RECORDS {
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
