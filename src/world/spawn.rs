//! Spawn mechanism: how a described piece of geometry becomes entities.
//!
//! Level-agnostic — every function takes a spec and produces mesh + collider
//! (+ markers). The level itself lives in [`super::layout`].

use avian3d::prelude::*;
use bevy::prelude::*;

use super::{GameLayer, PRACTICE_TARGET_HP, PracticeTarget, Stairs};
use crate::visuals::ToonMaterial;
use crate::visuals::toon::toon_color;

pub(super) struct BoxSpec {
    pub position: Vec3,
    pub dimensions: Vec3,
    pub rotation: Quat,
    pub color: Color,
}

pub(super) struct StairSegmentSpec<'a> {
    pub name: &'a str,
    pub base: Vec3,
    pub axis: Vec3,
    pub step_count: i32,
    pub step_depth: f32,
    pub step_rise: f32,
    pub width: f32,
    pub color: Color,
}

/// Spawn a static axis-aligned box with full-size `dims` centred at `pos`.
pub(super) fn spawn_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ToonMaterial>,
    name: &str,
    pos: Vec3,
    dims: Vec3,
    color: Color,
) -> Entity {
    spawn_oriented_box(
        commands,
        meshes,
        materials,
        name,
        BoxSpec {
            position: pos,
            dimensions: dims,
            rotation: Quat::IDENTITY,
            color,
        },
    )
}

pub(super) fn spawn_oriented_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ToonMaterial>,
    name: &str,
    spec: BoxSpec,
) -> Entity {
    commands
        .spawn((
            Name::new(name.to_string()),
            Mesh3d(meshes.add(Cuboid::new(
                spec.dimensions.x,
                spec.dimensions.y,
                spec.dimensions.z,
            ))),
            MeshMaterial3d(materials.add(toon_color(spec.color))),
            Transform::from_translation(spec.position).with_rotation(spec.rotation),
            RigidBody::Static,
            Collider::cuboid(spec.dimensions.x, spec.dimensions.y, spec.dimensions.z),
        ))
        .id()
}

pub(super) fn horizontal_rotation(axis: Vec3) -> Quat {
    let yaw = axis.z.atan2(axis.x);
    Quat::from_rotation_y(-yaw)
}

const PRACTICE_TARGET_DIMS: Vec3 = Vec3::new(0.15, 1.1, 1.1);

/// A practice target: static geometry that lives on `GameLayer::Actor`, so
/// sword sweeps and arrows hit it while movement sensors (masked to
/// `Default`) stay blind to it — not climbable, not a mantle lip. Carries
/// `VisualOf` on itself so it flashes and shows damage numbers like any
/// struck actor.
pub(super) fn spawn_practice_target(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    toon: &mut Assets<ToonMaterial>,
    materials: &mut Assets<StandardMaterial>,
    name: &str,
    center: Vec3,
) {
    // Supporting post: ordinary world geometry (Default layer).
    let post_height = (center.y - PRACTICE_TARGET_DIMS.y * 0.5).max(0.1);
    spawn_box(
        commands,
        meshes,
        toon,
        &format!("{name}Post"),
        Vec3::new(center.x, post_height * 0.5, center.z),
        Vec3::new(0.12, post_height, 0.12),
        Color::srgb(0.35, 0.3, 0.25),
    );

    let target = commands
        .spawn((
            Name::new(name.to_string()),
            PracticeTarget,
            crate::health::Health::new(PRACTICE_TARGET_HP),
            Mesh3d(meshes.add(Cuboid::new(
                PRACTICE_TARGET_DIMS.x,
                PRACTICE_TARGET_DIMS.y,
                PRACTICE_TARGET_DIMS.z,
            ))),
            MeshMaterial3d(materials.add(Color::srgb(0.85, 0.25, 0.2))),
            Transform::from_translation(center),
            RigidBody::Static,
            Collider::cuboid(
                PRACTICE_TARGET_DIMS.x,
                PRACTICE_TARGET_DIMS.y,
                PRACTICE_TARGET_DIMS.z,
            ),
            CollisionLayers::new(GameLayer::Actor, LayerMask::ALL),
        ))
        .id();
    commands
        .entity(target)
        .insert(crate::visuals::VisualOf(target));
}

pub(super) fn spawn_stair_segment(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<ToonMaterial>,
    spec: StairSegmentSpec,
) {
    let axis = spec.axis.normalize_or_zero();
    if axis == Vec3::ZERO || spec.step_count <= 0 || spec.step_depth <= 0.0 || spec.step_rise <= 0.0
    {
        return;
    }

    let rotation = horizontal_rotation(axis);
    for i in 0..spec.step_count {
        let height = spec.step_rise * (i + 1) as f32;
        let center = spec.base + axis * (spec.step_depth * (i as f32 + 0.5));
        spawn_oriented_box(
            commands,
            meshes,
            materials,
            &format!("{}Step{i}", spec.name),
            BoxSpec {
                position: center.with_y(spec.base.y + height * 0.5),
                dimensions: Vec3::new(spec.step_depth, height, spec.width),
                rotation,
                color: spec.color,
            },
        );
    }

    let total_run = spec.step_count as f32 * spec.step_depth;
    let total_rise = spec.step_count as f32 * spec.step_rise;
    commands.spawn((
        Name::new(spec.name.to_string()),
        Stairs {
            base: spec.base,
            top: spec.base + axis * total_run + Vec3::Y * total_rise,
            step_count: spec.step_count,
            step_depth: spec.step_depth,
            step_rise: spec.step_rise,
            trigger_center: spec.base
                + axis * (total_run * 0.5)
                + Vec3::Y * (total_rise * 0.5 + 0.3),
            trigger_half_extents: Vec3::new(
                total_run * 0.5 + 0.2,
                total_rise * 0.5 + 0.3,
                spec.width * 0.5 + 0.1,
            ),
            trigger_rotation: rotation,
        },
        Transform::from_translation(spec.base),
    ));
}
