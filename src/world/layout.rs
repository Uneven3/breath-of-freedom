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
use crate::scene::AppState;

// Graybox palette.
const FLOOR_MATERIAL: &str = "GroundGrass";
const PROP_MATERIAL: &str = "GrayboxProp";
const VAULT_MATERIAL: &str = "GrayboxVault";

/// What a row's `y` is measured from.
///
/// The ground stopped being flat, so an authored `y` had to pick a meaning.
/// It means **height above the terrain**, because that is what the piece is
/// for: a vault block is "knee high" wherever you meet it, and the traversal
/// motors measure obstacles against the ground you are standing on, not
/// against the world origin. On flat ground both anchors agree exactly, which
/// is why the tables did not have to change a single number.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Anchor {
    /// `y` is a height over the terrain sampled beneath the piece.
    Ground,
    /// `y` is absolute. For pieces too large to sit on any one sample: the
    /// perimeter walls span the whole world, so "the ground beneath them" is
    /// not a place — sampling one point would bury one end and float the other.
    World,
}

struct BoxRow {
    name: &'static str,
    pos: Vec3,
    dims: Vec3,
    material_key: &'static str,
    /// `NonClimbable` marker: ladder walls and containment perimeter.
    climbable: bool,
    anchor: Anchor,
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
        name: "NorthPerimeterWall",
        pos: Vec3::new(0.0, PERIMETER_HEIGHT * 0.5, -PERIMETER_HALF_EXTENT),
        dims: Vec3::new(WORLD_SIZE, PERIMETER_HEIGHT, PERIMETER_THICKNESS),
        material_key: PROP_MATERIAL,
        climbable: false,
        anchor: Anchor::World,
    },
    BoxRow {
        name: "SouthPerimeterWall",
        pos: Vec3::new(0.0, PERIMETER_HEIGHT * 0.5, PERIMETER_HALF_EXTENT),
        dims: Vec3::new(WORLD_SIZE, PERIMETER_HEIGHT, PERIMETER_THICKNESS),
        material_key: PROP_MATERIAL,
        climbable: false,
        anchor: Anchor::World,
    },
    BoxRow {
        name: "WestPerimeterWall",
        pos: Vec3::new(-PERIMETER_HALF_EXTENT, PERIMETER_HEIGHT * 0.5, 0.0),
        dims: Vec3::new(PERIMETER_THICKNESS, PERIMETER_HEIGHT, WORLD_SIZE),
        material_key: PROP_MATERIAL,
        climbable: false,
        anchor: Anchor::World,
    },
    BoxRow {
        name: "EastPerimeterWall",
        pos: Vec3::new(PERIMETER_HALF_EXTENT, PERIMETER_HEIGHT * 0.5, 0.0),
        dims: Vec3::new(PERIMETER_THICKNESS, PERIMETER_HEIGHT, WORLD_SIZE),
        material_key: PROP_MATERIAL,
        climbable: false,
        anchor: Anchor::World,
    },
    BoxRow {
        name: "Wall",
        pos: Vec3::new(0.0, 2.0, -10.0),
        dims: Vec3::new(10.0, 4.0, 1.0),
        material_key: PROP_MATERIAL,
        climbable: true,
        anchor: Anchor::Ground,
    },
    BoxRow {
        name: "AutoVaultSingleBlock",
        pos: Vec3::new(0.0, 0.5, 4.0),
        dims: Vec3::new(2.0, 1.0, 0.5),
        material_key: VAULT_MATERIAL,
        climbable: true,
        anchor: Anchor::Ground,
    },
    BoxRow {
        name: "AutoVaultWideRail",
        pos: Vec3::new(-3.0, 0.45, 7.0),
        dims: Vec3::new(3.5, 0.9, 0.5),
        material_key: VAULT_MATERIAL,
        climbable: true,
        anchor: Anchor::Ground,
    },
    BoxRow {
        name: "AutoVaultNarrowPost",
        pos: Vec3::new(3.0, 0.55, 7.0),
        dims: Vec3::new(0.8, 1.1, 0.5),
        material_key: VAULT_MATERIAL,
        climbable: true,
        anchor: Anchor::Ground,
    },
    BoxRow {
        name: "AutoVaultTallBlocker",
        pos: Vec3::new(0.0, 1.1, 10.5),
        dims: Vec3::new(2.5, 2.2, 0.5),
        material_key: PROP_MATERIAL,
        climbable: true,
        anchor: Anchor::Ground,
    },
    BoxRow {
        name: "Landing",
        pos: Vec3::new(-11.0, 1.0, 0.0),
        dims: Vec3::new(4.0, 2.0, 3.0),
        material_key: FLOOR_MATERIAL,
        climbable: true,
        anchor: Anchor::Ground,
    },
    BoxRow {
        name: "LadderWall",
        pos: Vec3::new(10.0, 2.0, -10.0),
        dims: Vec3::new(4.0, 4.0, 1.0),
        material_key: PROP_MATERIAL,
        climbable: false,
        anchor: Anchor::Ground,
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

/// The sky every walkable scene needs: sun, moon discs and ambient light. Split
/// from [`setup_graybox`] so the editor scene can have light to read shapes by
/// without inheriting a forest.
pub(super) fn setup_sky(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
) {
    let m = &mut meshes;
    let scene = *state.get();

    // --- Lighting: the day/night cycle drives this light every frame ---
    commands.spawn((
        DespawnOnExit(scene),
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
        DespawnOnExit(scene),
        Name::new("SunDisc"),
        super::day_night::SunDisc,
        bevy::light::NotShadowCaster,
        Mesh3d(m.add(Sphere::new(14.0))),
        MeshMaterial3d(palette.handle("Sun")),
        Transform::from_xyz(0.0, 400.0, 0.0),
    ));
    commands.spawn((
        DespawnOnExit(scene),
        Name::new("MoonDisc"),
        super::day_night::MoonDisc,
        bevy::light::NotShadowCaster,
        Mesh3d(m.add(Sphere::new(9.0))),
        MeshMaterial3d(palette.handle("Moon")),
        Transform::from_xyz(0.0, -400.0, 0.0),
        Visibility::Hidden,
    ));
}

/// The locomotion course: boxes, walls, stairs (straight, curved and derived),
/// the ladder and the ramps. One of the pieces a scene row can ask for
/// (`crate::scene`), so the traversal box can have it without the forest and
/// the sandbox can have none of it.
trait GroundHeight {
    fn sample_height(&self, world_xz: Vec2) -> Option<f32>;
}

impl GroundHeight for super::Terrain {
    fn sample_height(&self, world_xz: Vec2) -> Option<f32> {
        Some(self.height_at(world_xz))
    }
}

impl GroundHeight for super::TerrainAccess<'_, '_> {
    fn sample_height(&self, world_xz: Vec2) -> Option<f32> {
        self.height_at(world_xz)
    }
}

/// Lift an authored position onto the ground beneath it.
///
/// The authored `y` is a height *over* the terrain, so this adds the ground
/// height at the piece's own XZ. Flat ground samples 0 and the position comes
/// back unchanged, which is why sculpting a scene cannot silently move a course
/// that was authored before the terrain existed.
fn settle(pos: Vec3, anchor: Anchor, ground: Option<&impl GroundHeight>) -> Vec3 {
    match (anchor, ground) {
        (Anchor::Ground, Some(terrain)) => Vec3::new(
            pos.x,
            pos.y + terrain.sample_height(pos.xz()).unwrap_or(0.0),
            pos.z,
        ),
        _ => pos,
    }
}

pub(super) fn setup_course(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
    ground: super::TerrainAccess,
) {
    let m = &mut meshes;
    let scene = *state.get();
    let ground = Some(&ground);

    // --- Declarative tables ---
    for row in BOXES {
        let entity = spawn_box(
            &mut commands,
            m,
            &palette,
            row.name,
            settle(row.pos, row.anchor, ground),
            row.dims,
            row.material_key,
            scene,
        );
        if !row.climbable {
            commands.entity(entity).insert(NonClimbable);
        }
    }
    // --- Rock: sphere r=2 at (-10,1,-5) ---
    commands.spawn((
        DespawnOnExit(scene),
        Name::new("Rock"),
        Mesh3d(m.add(Sphere::new(2.0))),
        MeshMaterial3d(palette.handle(PROP_MATERIAL)),
        Transform::from_translation(settle(Vec3::new(-10.0, 1.0, -5.0), Anchor::Ground, ground)),
        RigidBody::Static,
        Collider::sphere(2.0),
    ));

    // --- Tree: cylinder r=1 h=10 at (10,5,-5) ---
    commands.spawn((
        DespawnOnExit(scene),
        Name::new("Tree"),
        Mesh3d(m.add(Cylinder::new(1.0, 10.0))),
        MeshMaterial3d(palette.handle("TreeTrunk")),
        Transform::from_translation(settle(Vec3::new(10.0, 5.0, -5.0), Anchor::Ground, ground)),
        RigidBody::Static,
        Collider::cylinder(1.0, 10.0),
    ));

    // --- Slope: 8×0.3×4 at (10,1.37,0), rotated 20° about Z ---
    commands.spawn((
        DespawnOnExit(scene),
        Name::new("Slope"),
        Mesh3d(m.add(Cuboid::new(8.0, 0.3, 4.0))),
        MeshMaterial3d(palette.handle(FLOOR_MATERIAL)),
        Transform::from_translation(settle(Vec3::new(10.0, 1.37, 0.0), Anchor::Ground, ground))
            .with_rotation(Quat::from_rotation_z(20.0_f32.to_radians())),
        RigidBody::Static,
        Collider::cuboid(8.0, 0.3, 4.0),
    ));

    // --- Ladder on its (non-climbable) wall — the wall is a `BOXES` row. ---
    let ladder_x = 10.0;
    let ladder_wall_z = -10.0;
    let ladder_surface_z = ladder_wall_z + 0.5;
    // Authored body centerline: surface + capsule radius + a small skin gap.
    let ladder_body_z = ladder_surface_z + 0.55;
    // The ladder is five authored points that have to keep agreeing with each
    // other *and* with the wall they hang on, so the whole assembly takes one
    // lift — sampling each point separately would tilt the climb against a wall
    // that moved as a block.
    let ladder_lift = ground
        .and_then(|terrain| terrain.sample_height(Vec2::new(ladder_x, ladder_wall_z)))
        .unwrap_or(0.0);
    let rung = |y: f32, z: f32| Vec3::new(ladder_x, y + ladder_lift, z);
    commands.spawn((
        DespawnOnExit(scene),
        Name::new("Ladder"),
        Mesh3d(m.add(Cuboid::new(0.8, 4.0, 0.1))),
        MeshMaterial3d(palette.handle("Ladder")),
        Transform::from_translation(rung(2.0, ladder_surface_z + 0.05)),
        Ladder {
            bottom: rung(0.0, ladder_body_z),
            top: rung(4.0, ladder_body_z),
            body_anchor: rung(0.0, ladder_body_z),
            outward_normal: Vec3::Z,
            trigger_center: rung(2.0, ladder_body_z),
            trigger_half_extents: Vec3::new(0.7, 2.0, 0.65),
        },
    ));
}

/// Stairs: the straight tables, the curved castle arc, and the ramp derived from
/// the long-tread flight. Its own piece so a box can test stair traversal
/// without the rest of the course around it.
pub(super) fn setup_stairs(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
    ground: super::TerrainAccess,
) {
    let m = &mut meshes;
    let scene = *state.get();
    let ground = Some(&ground);

    for row in STAIRS {
        spawn_stair_segment(
            &mut commands,
            m,
            &palette,
            StairSegmentSpec {
                name: row.name,
                // A flight of stairs is one designed shape: it takes a single
                // lift from its base, so the steps keep their rise relative to
                // each other instead of following every bump underneath.
                base: settle(row.base, Anchor::Ground, ground),
                axis: row.axis,
                step_count: row.step_count,
                step_depth: row.step_depth,
                step_rise: row.step_rise,
                width: row.width,
                material_key: FLOOR_MATERIAL,
            },
            scene,
        );
    }

    // --- Derived geometry: exit ramp continuing the long-tread stairs ---
    let long = &STAIRS[1];
    let stair_top = settle(long.base, Anchor::Ground, ground)
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
        scene,
    );

    // --- Derived geometry: curved castle stair — twelve independently
    // oriented one-step segments. ---
    let arc_center = settle(Vec3::new(-13.0, 0.0, 13.0), Anchor::Ground, ground);
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
            scene,
        );
    }
}

/// Practice targets: the destructible things combat is judged against.
pub(super) fn setup_targets(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
    ground: super::TerrainAccess,
) {
    let scene = *state.get();
    let ground = Some(&ground);
    for (name, center) in PRACTICE_TARGETS {
        spawn_practice_target(
            &mut commands,
            &mut meshes,
            &mut materials,
            &palette,
            name,
            settle(*center, Anchor::Ground, ground),
            scene,
        );
    }
}

/// Ground items near spawn: a lootable weapon plus auto-collected stacks.
pub(super) fn setup_pickups(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    palette: Res<MaterialPalette>,
    state: Res<State<AppState>>,
    ground: super::TerrainAccess,
) {
    let scene = *state.get();
    let ground = Some(&ground);
    for row in PICKUPS {
        crate::inventory::spawn_world_item(
            &mut commands,
            &mut meshes,
            &palette,
            row.name,
            settle(row.pos, Anchor::Ground, ground),
            row.stack,
            row.mode,
            scene,
        );
    }
}

/// The deterministic forest. Its own piece, so the world scene can have trees
/// while the traversal box stays readable.
pub(super) fn setup_forest(
    mut commands: Commands,
    spatial: Res<SpatialCatalog>,
    state: Res<State<AppState>>,
) {
    super::forest::spawn_forest(&mut commands, &spatial, *state.get());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flat_ground_leaves_every_authored_position_untouched() {
        // The tables were authored when the floor was flat, and not one number
        // changed when `y` started meaning "over the terrain". That only holds
        // if a flat sample is a no-op — otherwise this refactor silently moved
        // a course that took a long time to calibrate.
        let flat = super::super::Terrain::flat_for_test();
        for row in BOXES {
            assert_eq!(
                settle(row.pos, row.anchor, Some(&flat)),
                row.pos,
                "{} moved on flat ground",
                row.name
            );
        }
    }

    #[test]
    fn a_piece_rides_the_ground_it_stands_on() {
        let mut terrain = super::super::Terrain::flat_for_test();
        terrain.raise_area(Vec2::ZERO, 25.0, 6.0);
        let authored = Vec3::new(0.0, 0.5, 4.0);
        let settled = settle(authored, Anchor::Ground, Some(&terrain));
        let ground = terrain.height_at(authored.xz());

        assert!(ground > 0.5, "the test hill must actually lift this spot");
        assert_eq!(settled.xz(), authored.xz(), "only height may change");
        assert!(
            (settled.y - (authored.y + ground)).abs() < 0.001,
            "a knee-high block must stay knee high over the hill it sits on"
        );
    }

    #[test]
    fn the_perimeter_never_rides_the_terrain() {
        // Walls that span the whole world have no single ground beneath them:
        // one sample would bury one end and float the other.
        let mut terrain = super::super::Terrain::flat_for_test();
        terrain.raise_area(Vec2::ZERO, 40.0, 9.0);
        for row in BOXES.iter().filter(|row| row.anchor == Anchor::World) {
            assert_eq!(settle(row.pos, row.anchor, Some(&terrain)), row.pos);
        }
        assert!(
            BOXES.iter().any(|row| row.anchor == Anchor::World),
            "the perimeter rows must still be anchored to the world"
        );
    }

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
