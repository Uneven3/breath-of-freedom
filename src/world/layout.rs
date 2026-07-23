//! The graybox level, as data.
//!
//! Declarative tables describe every simple piece (boxes, targets, straight
//! stairs); only genuinely derived geometry (curved stair arc, exit ramp
//! trigonometry) remains code. Growing the map means editing tables here —
//! and this file is the seam a future asset-file loader (RON/GLTF scene)
//! replaces without touching `spawn` or the world types.

use avian3d::prelude::*;
use bevy::prelude::*;

use super::spawn::{
    BoxSpec, StairSegmentSpec, spawn_box, spawn_oriented_box, spawn_practice_target,
    spawn_stair_segment,
};
use super::{Ladder, NonClimbable};
use crate::asset_pipeline::{MaterialPalette, SpatialCatalog};

// Graybox palette.
const FLOOR_MATERIAL: &str = "GrayboxFloor";
const PROP_MATERIAL: &str = "GrayboxProp";
const VAULT_MATERIAL: &str = "GrayboxVault";

struct BoxRow {
    name: &'static str,
    pos: Vec3,
    dims: Vec3,
    material_key: &'static str,
    /// `NonClimbable` marker: ladder walls and containment perimeter.
    climbable: bool,
}

pub const WORLD_SIZE: f32 = 320.0;
const PERIMETER_HALF_EXTENT: f32 = WORLD_SIZE * 0.5 - 0.5;
const PERIMETER_HEIGHT: f32 = 12.0;
const PERIMETER_THICKNESS: f32 = 1.0;

/// Every axis-aligned box in the course. The perimeter is intentionally
/// non-climbable and taller than the ledge traversal range, so autonomous
/// graybox actors stay in course.
const BOXES: &[BoxRow] = &[
    BoxRow {
        name: "Floor",
        pos: Vec3::new(0.0, -0.5, 0.0),
        dims: Vec3::new(WORLD_SIZE, 1.0, WORLD_SIZE),
        material_key: FLOOR_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "NorthPerimeterWall",
        pos: Vec3::new(0.0, PERIMETER_HEIGHT * 0.5, -PERIMETER_HALF_EXTENT),
        dims: Vec3::new(WORLD_SIZE, PERIMETER_HEIGHT, PERIMETER_THICKNESS),
        material_key: PROP_MATERIAL,
        climbable: false,
    },
    BoxRow {
        name: "SouthPerimeterWall",
        pos: Vec3::new(0.0, PERIMETER_HEIGHT * 0.5, PERIMETER_HALF_EXTENT),
        dims: Vec3::new(WORLD_SIZE, PERIMETER_HEIGHT, PERIMETER_THICKNESS),
        material_key: PROP_MATERIAL,
        climbable: false,
    },
    BoxRow {
        name: "WestPerimeterWall",
        pos: Vec3::new(-PERIMETER_HALF_EXTENT, PERIMETER_HEIGHT * 0.5, 0.0),
        dims: Vec3::new(PERIMETER_THICKNESS, PERIMETER_HEIGHT, WORLD_SIZE),
        material_key: PROP_MATERIAL,
        climbable: false,
    },
    BoxRow {
        name: "EastPerimeterWall",
        pos: Vec3::new(PERIMETER_HALF_EXTENT, PERIMETER_HEIGHT * 0.5, 0.0),
        dims: Vec3::new(PERIMETER_THICKNESS, PERIMETER_HEIGHT, WORLD_SIZE),
        material_key: PROP_MATERIAL,
        climbable: false,
    },
    BoxRow {
        name: "Wall",
        pos: Vec3::new(0.0, 2.0, -10.0),
        dims: Vec3::new(10.0, 4.0, 1.0),
        material_key: PROP_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "AutoVaultSingleBlock",
        pos: Vec3::new(0.0, 0.5, 4.0),
        dims: Vec3::new(2.0, 1.0, 0.5),
        material_key: VAULT_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "AutoVaultWideRail",
        pos: Vec3::new(-3.0, 0.45, 7.0),
        dims: Vec3::new(3.5, 0.9, 0.5),
        material_key: VAULT_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "AutoVaultNarrowPost",
        pos: Vec3::new(3.0, 0.55, 7.0),
        dims: Vec3::new(0.8, 1.1, 0.5),
        material_key: VAULT_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "AutoVaultTallBlocker",
        pos: Vec3::new(0.0, 1.1, 10.5),
        dims: Vec3::new(2.5, 2.2, 0.5),
        material_key: PROP_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "Landing",
        pos: Vec3::new(-11.0, 1.0, 0.0),
        dims: Vec3::new(4.0, 2.0, 3.0),
        material_key: FLOOR_MATERIAL,
        climbable: true,
    },
    BoxRow {
        name: "LadderWall",
        pos: Vec3::new(10.0, 2.0, -10.0),
        dims: Vec3::new(4.0, 4.0, 1.0),
        material_key: PROP_MATERIAL,
        climbable: false,
    },
];

/// Archery practice targets, east of the course, facing spawn.
const PRACTICE_TARGETS: &[(&str, Vec3)] = &[
    ("PracticeTargetNear", Vec3::new(14.0, 1.6, -2.0)),
    ("PracticeTargetHigh", Vec3::new(17.0, 2.4, 3.0)),
    ("PracticeTargetFar", Vec3::new(24.0, 1.4, 10.0)),
];

struct PickupRow {
    name: &'static str,
    pos: Vec3,
    stack: crate::inventory::ItemStack,
    mode: crate::inventory::PickupMode,
}

/// Graybox inventory checkpoint: one lootable weapon (Interact) plus a
/// couple of auto-collected stacks, all close to spawn.
const PICKUPS: &[PickupRow] = &[
    PickupRow {
        name: "SpareClub",
        pos: Vec3::new(-4.0, 0.5, 3.0),
        stack: crate::inventory::ItemStack {
            kind: crate::inventory::ItemKind::Weapon(crate::inventory::WeaponItem::LOOTABLE_CLUB),
            quantity: 1,
        },
        mode: crate::inventory::PickupMode::Interact,
    },
    PickupRow {
        name: "WoodPile",
        pos: Vec3::new(-4.0, 0.3, 5.0),
        stack: crate::inventory::ItemStack {
            kind: crate::inventory::ItemKind::Material(crate::inventory::MaterialKind::Wood),
            quantity: 3,
        },
        mode: crate::inventory::PickupMode::Auto,
    },
    PickupRow {
        name: "Apple",
        pos: Vec3::new(-4.0, 0.3, 6.5),
        stack: crate::inventory::ItemStack {
            kind: crate::inventory::ItemKind::Food {
                label: "Apple",
                heal: 25.0,
            },
            quantity: 1,
        },
        mode: crate::inventory::PickupMode::Auto,
    },
];

struct StairRow {
    name: &'static str,
    base: Vec3,
    axis: Vec3,
    step_count: i32,
    step_depth: f32,
    step_rise: f32,
    width: f32,
}

/// Straight stair segments: baseline, long-tread, and short-tread stress.
const STAIRS: &[StairRow] = &[
    StairRow {
        name: "Stairs",
        base: Vec3::new(-5.0, 0.0, 0.0),
        axis: Vec3::NEG_X,
        step_count: 8,
        step_depth: 0.5,
        step_rise: 0.25,
        width: 3.0,
    },
    StairRow {
        name: "LongTreadStairs",
        base: Vec3::new(16.0, 0.0, 10.0),
        axis: Vec3::NEG_X,
        step_count: 4,
        step_depth: 1.2,
        step_rise: 0.25,
        width: 2.5,
    },
    StairRow {
        name: "ShortTreadStairs",
        base: Vec3::new(8.0, 0.0, 16.0),
        axis: Vec3::NEG_Z,
        step_count: 10,
        step_depth: 0.3,
        step_rise: 0.18,
        width: 2.4,
    },
];

pub(super) fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    spatial: Res<SpatialCatalog>,
) {
    let m = &mut meshes;

    // --- Lighting: the day/night cycle drives this light every frame ---
    commands.spawn((
        Name::new("Sun"),
        super::day_night::Sun,
        DirectionalLight {
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::from_xyz(5.0, 10.0, 5.0).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    commands.insert_resource(GlobalAmbientLight {
        brightness: 200.0,
        ..default()
    });

    // Visible sun/moon discs: unlit spheres the cycle moves along their
    // arcs, so the light source reads as a body in the sky.
    commands.spawn((
        Name::new("SunDisc"),
        super::day_night::SunDisc,
        bevy::light::NotShadowCaster,
        Mesh3d(m.add(Sphere::new(14.0))),
        MeshMaterial3d(palette.handle("Sun")),
        Transform::from_xyz(0.0, 400.0, 0.0),
    ));
    commands.spawn((
        Name::new("MoonDisc"),
        super::day_night::MoonDisc,
        bevy::light::NotShadowCaster,
        Mesh3d(m.add(Sphere::new(9.0))),
        MeshMaterial3d(palette.handle("Moon")),
        Transform::from_xyz(0.0, -400.0, 0.0),
        Visibility::Hidden,
    ));

    // --- Declarative tables ---
    for row in BOXES {
        let entity = spawn_box(
            &mut commands,
            m,
            &palette,
            row.name,
            row.pos,
            row.dims,
            row.material_key,
        );
        if !row.climbable {
            commands.entity(entity).insert(NonClimbable);
        }
    }
    for (name, center) in PRACTICE_TARGETS {
        spawn_practice_target(&mut commands, m, &palette, name, *center);
    }
    for row in PICKUPS {
        crate::inventory::spawn_world_item(
            &mut commands,
            m,
            &palette,
            row.name,
            row.pos,
            row.stack,
            row.mode,
        );
    }
    for row in STAIRS {
        spawn_stair_segment(
            &mut commands,
            m,
            &palette,
            StairSegmentSpec {
                name: row.name,
                base: row.base,
                axis: row.axis,
                step_count: row.step_count,
                step_depth: row.step_depth,
                step_rise: row.step_rise,
                width: row.width,
                material_key: FLOOR_MATERIAL,
            },
        );
    }
    super::forest::spawn_forest(&mut commands, &spatial);

    // --- Rock: sphere r=2 at (-10,1,-5) ---
    commands.spawn((
        Name::new("Rock"),
        Mesh3d(m.add(Sphere::new(2.0))),
        MeshMaterial3d(palette.handle(PROP_MATERIAL)),
        Transform::from_xyz(-10.0, 1.0, -5.0),
        RigidBody::Static,
        Collider::sphere(2.0),
    ));

    // --- Tree: cylinder r=1 h=10 at (10,5,-5) ---
    commands.spawn((
        Name::new("Tree"),
        Mesh3d(m.add(Cylinder::new(1.0, 10.0))),
        MeshMaterial3d(palette.handle("TreeTrunk")),
        Transform::from_xyz(10.0, 5.0, -5.0),
        RigidBody::Static,
        Collider::cylinder(1.0, 10.0),
    ));

    // --- Slope: 8×0.3×4 at (10,1.37,0), rotated 20° about Z ---
    commands.spawn((
        Name::new("Slope"),
        Mesh3d(m.add(Cuboid::new(8.0, 0.3, 4.0))),
        MeshMaterial3d(palette.handle(FLOOR_MATERIAL)),
        Transform::from_xyz(10.0, 1.37, 0.0)
            .with_rotation(Quat::from_rotation_z(20.0_f32.to_radians())),
        RigidBody::Static,
        Collider::cuboid(8.0, 0.3, 4.0),
    ));

    // --- Derived geometry: exit ramp continuing the long-tread stairs ---
    let long = &STAIRS[1];
    let stair_top = long.base
        + long.axis * (long.step_count as f32 * long.step_depth)
        + Vec3::Y * (long.step_count as f32 * long.step_rise);
    let ramp_length = 5.0;
    let ramp_angle = 15.0_f32.to_radians();
    let ramp_rotation = Quat::from_rotation_z(std::f32::consts::PI - ramp_angle);
    let ramp_center = stair_top
        + long.axis * (ramp_length * 0.5 * ramp_angle.cos())
        + Vec3::Y * (ramp_length * 0.5 * ramp_angle.sin() - 0.15 * ramp_angle.cos());
    spawn_oriented_box(
        &mut commands,
        m,
        "LongTreadExitSlope",
        BoxSpec {
            position: ramp_center,
            dimensions: Vec3::new(ramp_length, 0.3, 2.5),
            rotation: ramp_rotation,
            material: palette.handle(FLOOR_MATERIAL),
        },
    );

    // --- Derived geometry: curved castle stair — twelve independently
    // oriented one-step segments. ---
    let arc_center = Vec3::new(-13.0, 0.0, 13.0);
    let arc_radius = 3.0;
    let arc_steps = 12;
    let arc_step_rise = 0.2;
    let arc_start = -std::f32::consts::FRAC_PI_2;
    let arc_delta = std::f32::consts::PI / arc_steps as f32;
    for i in 0..arc_steps {
        let start = arc_start + arc_delta * i as f32;
        let end = start + arc_delta;
        let base = arc_center
            + Vec3::new(arc_radius * start.cos(), 0.0, arc_radius * start.sin())
            + Vec3::Y * (arc_step_rise * i as f32);
        let next = arc_center + Vec3::new(arc_radius * end.cos(), 0.0, arc_radius * end.sin());
        let chord = next - base.with_y(0.0);
        spawn_stair_segment(
            &mut commands,
            m,
            &palette,
            StairSegmentSpec {
                name: &format!("CurvedStair{i}"),
                base,
                axis: chord,
                step_count: 1,
                step_depth: chord.length(),
                step_rise: arc_step_rise,
                width: 1.8,
                material_key: FLOOR_MATERIAL,
            },
        );
    }

    // --- Ladder on its (non-climbable) wall — the wall is a `BOXES` row. ---
    let ladder_x = 10.0;
    let ladder_wall_z = -10.0;
    let ladder_surface_z = ladder_wall_z + 0.5;
    // Authored body centerline: surface + capsule radius + a small skin gap.
    let ladder_body_z = ladder_surface_z + 0.55;
    commands.spawn((
        Name::new("Ladder"),
        Mesh3d(m.add(Cuboid::new(0.8, 4.0, 0.1))),
        MeshMaterial3d(palette.handle("Ladder")),
        Transform::from_xyz(ladder_x, 2.0, ladder_surface_z + 0.05),
        Ladder {
            bottom: Vec3::new(ladder_x, 0.0, ladder_body_z),
            top: Vec3::new(ladder_x, 4.0, ladder_body_z),
            body_anchor: Vec3::new(ladder_x, 0.0, ladder_body_z),
            outward_normal: Vec3::Z,
            trigger_center: Vec3::new(ladder_x, 2.0, ladder_body_z),
            trigger_half_extents: Vec3::new(0.7, 2.0, 0.65),
        },
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_layout_row_has_a_unique_name_and_positive_dimensions() {
        let mut names: Vec<&str> = BOXES.iter().map(|row| row.name).collect();
        names.extend(PRACTICE_TARGETS.iter().map(|(name, _)| *name));
        names.extend(STAIRS.iter().map(|row| row.name));
        names.extend(PICKUPS.iter().map(|row| row.name));
        let total = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), total, "duplicate names in the layout tables");

        for row in BOXES {
            assert!(
                row.dims.cmpgt(Vec3::ZERO).all(),
                "{} has non-positive dimensions",
                row.name
            );
        }
        for row in STAIRS {
            assert!(
                row.step_count > 0
                    && row.step_depth > 0.0
                    && row.step_rise > 0.0
                    && row.width > 0.0,
                "{} has degenerate stair parameters",
                row.name
            );
        }
    }
}
