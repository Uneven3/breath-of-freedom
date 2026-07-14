//! Static world geometry — the graybox test course.
//!
//! Every piece is an Avian `RigidBody::Static` + `Collider`. Markers (`Stairs`,
//! `Ladder`) and their sensor volumes are added when their services land.

use avian3d::prelude::*;
use bevy::prelude::*;

/// Authored uniform straight stair segment. Curved stairs are composed from
/// adjacent one-step segments with independently oriented trigger volumes.
#[derive(Component, Debug, Clone)]
pub struct Stairs {
    pub base: Vec3,
    pub top: Vec3,
    pub step_count: i32,
    pub step_depth: f32,
    pub step_rise: f32,
    pub trigger_center: Vec3,
    pub trigger_half_extents: Vec3,
    pub trigger_rotation: Quat,
}

/// Authored ladder marker.
#[derive(Component, Debug, Clone)]
pub struct Ladder {
    pub bottom: Vec3,
    pub top: Vec3,
    /// Where the controlled body's center is held while attached.
    pub body_anchor: Vec3,
    /// Horizontal normal pointing away from the supporting wall.
    pub outward_normal: Vec3,
    pub trigger_center: Vec3,
    pub trigger_half_extents: Vec3,
}

/// Marks world geometry that supports a ladder but must not start wall-climb.
/// Ledge sensing still sees it for Mantle and Vault.
#[derive(Component)]
pub struct NonClimbable;

impl Ladder {
    pub fn contains(&self, p: Vec3) -> bool {
        let d = (p - self.trigger_center).abs();
        d.x <= self.trigger_half_extents.x
            && d.y <= self.trigger_half_extents.y
            && d.z <= self.trigger_half_extents.z
    }
}

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_world);
    }
}

struct BoxSpec {
    position: Vec3,
    dimensions: Vec3,
    rotation: Quat,
    color: Color,
}

struct StairSegmentSpec<'a> {
    name: &'a str,
    base: Vec3,
    axis: Vec3,
    step_count: i32,
    step_depth: f32,
    step_rise: f32,
    width: f32,
    color: Color,
}

/// Spawn a static axis-aligned box with full-size `dims` centred at `pos`.
fn spawn_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
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

fn spawn_oriented_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
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
            MeshMaterial3d(materials.add(spec.color)),
            Transform::from_translation(spec.position).with_rotation(spec.rotation),
            RigidBody::Static,
            Collider::cuboid(spec.dimensions.x, spec.dimensions.y, spec.dimensions.z),
        ))
        .id()
}

fn horizontal_rotation(axis: Vec3) -> Quat {
    let yaw = axis.z.atan2(axis.x);
    Quat::from_rotation_y(-yaw)
}

fn spawn_stair_segment(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
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

fn setup_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    // --- Lighting ---
    commands.spawn((
        Name::new("Sun"),
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

    let m = &mut meshes;
    let mat = &mut materials;
    let floor_c = Color::srgb(0.4, 0.45, 0.4);
    let prop_c = Color::srgb(0.55, 0.5, 0.45);
    let vault_c = Color::srgb(0.7, 0.5, 0.3);

    // --- Floor 50×1×50 at (0,-0.5,0) ---
    spawn_box(
        &mut commands,
        m,
        mat,
        "Floor",
        Vec3::new(0.0, -0.5, 0.0),
        Vec3::new(50.0, 1.0, 50.0),
        floor_c,
    );

    // --- Wall 10×4×1 at (0,2,-10) ---
    spawn_box(
        &mut commands,
        m,
        mat,
        "Wall",
        Vec3::new(0.0, 2.0, -10.0),
        Vec3::new(10.0, 4.0, 1.0),
        prop_c,
    );

    // --- Rock: sphere r=2 at (-10,1,-5) ---
    commands.spawn((
        Name::new("Rock"),
        Mesh3d(m.add(Sphere::new(2.0))),
        MeshMaterial3d(mat.add(prop_c)),
        Transform::from_xyz(-10.0, 1.0, -5.0),
        RigidBody::Static,
        Collider::sphere(2.0),
    ));

    // --- Tree: cylinder r=1 h=10 at (10,5,-5) ---
    commands.spawn((
        Name::new("Tree"),
        Mesh3d(m.add(Cylinder::new(1.0, 10.0))),
        MeshMaterial3d(mat.add(Color::srgb(0.4, 0.3, 0.2))),
        Transform::from_xyz(10.0, 5.0, -5.0),
        RigidBody::Static,
        Collider::cylinder(1.0, 10.0),
    ));

    // --- Slope: 8×0.3×4 at (10,1.37,0), rotated 20° about Z ---
    commands.spawn((
        Name::new("Slope"),
        Mesh3d(m.add(Cuboid::new(8.0, 0.3, 4.0))),
        MeshMaterial3d(mat.add(floor_c)),
        Transform::from_xyz(10.0, 1.37, 0.0)
            .with_rotation(Quat::from_rotation_z(20.0_f32.to_radians())),
        RigidBody::Static,
        Collider::cuboid(8.0, 0.3, 4.0),
    ));

    // --- Auto-vault obstacles ---
    spawn_box(
        &mut commands,
        m,
        mat,
        "AutoVaultSingleBlock",
        Vec3::new(0.0, 0.5, 4.0),
        Vec3::new(2.0, 1.0, 0.5),
        vault_c,
    );
    spawn_box(
        &mut commands,
        m,
        mat,
        "AutoVaultWideRail",
        Vec3::new(-3.0, 0.45, 7.0),
        Vec3::new(3.5, 0.9, 0.5),
        vault_c,
    );
    spawn_box(
        &mut commands,
        m,
        mat,
        "AutoVaultNarrowPost",
        Vec3::new(3.0, 0.55, 7.0),
        Vec3::new(0.8, 1.1, 0.5),
        vault_c,
    );
    spawn_box(
        &mut commands,
        m,
        mat,
        "AutoVaultTallBlocker",
        Vec3::new(0.0, 1.1, 10.5),
        Vec3::new(2.5, 2.2, 0.5),
        prop_c,
    );

    // --- Baseline stairs ---
    spawn_stair_segment(
        &mut commands,
        m,
        mat,
        StairSegmentSpec {
            name: "Stairs",
            base: Vec3::new(-5.0, 0.0, 0.0),
            axis: Vec3::NEG_X,
            step_count: 8,
            step_depth: 0.5,
            step_rise: 0.25,
            width: 3.0,
            color: floor_c,
        },
    );
    // Landing 4×2×3 at (-11,1,0)
    spawn_box(
        &mut commands,
        m,
        mat,
        "Landing",
        Vec3::new(-11.0, 1.0, 0.0),
        Vec3::new(4.0, 2.0, 3.0),
        floor_c,
    );

    // --- Long treads followed by a continuous slope ---
    let long_base = Vec3::new(16.0, 0.0, 10.0);
    let long_axis = Vec3::NEG_X;
    let long_count = 4;
    let long_depth = 1.2;
    let long_rise = 0.25;
    spawn_stair_segment(
        &mut commands,
        m,
        mat,
        StairSegmentSpec {
            name: "LongTreadStairs",
            base: long_base,
            axis: long_axis,
            step_count: long_count,
            step_depth: long_depth,
            step_rise: long_rise,
            width: 2.5,
            color: floor_c,
        },
    );
    let stair_top = long_base
        + long_axis * (long_count as f32 * long_depth)
        + Vec3::Y * (long_count as f32 * long_rise);
    let ramp_length = 5.0;
    let ramp_angle = 15.0_f32.to_radians();
    let ramp_rotation = Quat::from_rotation_z(std::f32::consts::PI - ramp_angle);
    let ramp_center = stair_top
        + long_axis * (ramp_length * 0.5 * ramp_angle.cos())
        + Vec3::Y * (ramp_length * 0.5 * ramp_angle.sin() - 0.15 * ramp_angle.cos());
    spawn_oriented_box(
        &mut commands,
        m,
        mat,
        "LongTreadExitSlope",
        BoxSpec {
            position: ramp_center,
            dimensions: Vec3::new(ramp_length, 0.3, 2.5),
            rotation: ramp_rotation,
            color: floor_c,
        },
    );

    // --- Short-tread stress course ---
    spawn_stair_segment(
        &mut commands,
        m,
        mat,
        StairSegmentSpec {
            name: "ShortTreadStairs",
            base: Vec3::new(8.0, 0.0, 16.0),
            axis: Vec3::NEG_Z,
            step_count: 10,
            step_depth: 0.3,
            step_rise: 0.18,
            width: 2.4,
            color: floor_c,
        },
    );

    // --- Curved castle stair: twelve independently oriented one-step segments. ---
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
            mat,
            StairSegmentSpec {
                name: &format!("CurvedStair{i}"),
                base,
                axis: chord,
                step_count: 1,
                step_depth: chord.length(),
                step_rise: arc_step_rise,
                width: 1.8,
                color: floor_c,
            },
        );
    }

    // --- Dedicated non-climbable ladder wall, separate from the climb wall. ---
    let ladder_x = 10.0;
    let ladder_wall_z = -10.0;
    let ladder_wall = spawn_box(
        &mut commands,
        m,
        mat,
        "LadderWall",
        Vec3::new(ladder_x, 2.0, ladder_wall_z),
        Vec3::new(4.0, 4.0, 1.0),
        prop_c,
    );
    commands.entity(ladder_wall).insert(NonClimbable);

    let ladder_surface_z = ladder_wall_z + 0.5;
    // Authored body centerline: surface + capsule radius + a small skin gap.
    let ladder_body_z = ladder_surface_z + 0.55;
    commands.spawn((
        Name::new("Ladder"),
        Mesh3d(m.add(Cuboid::new(0.8, 4.0, 0.1))),
        MeshMaterial3d(mat.add(Color::srgb(0.5, 0.35, 0.2))),
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
