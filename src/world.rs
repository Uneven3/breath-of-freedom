//! Static world geometry — the graybox test course.
//!
//! Every piece is an Avian `RigidBody::Static` + `Collider`. Markers (`Stairs`,
//! `Ladder`) and their sensor volumes are added when their services land.

use avian3d::prelude::*;
use bevy::prelude::*;

/// Authored stair marker. The trigger is a plain AABB region (`StairsService`
/// does an AABB overlap test) plus the step geometry the motor needs — no
/// physics sensor required.
#[derive(Component, Debug, Clone)]
pub struct Stairs {
    pub base: Vec3,
    pub top: Vec3,
    pub step_count: i32,
    pub step_depth: f32,
    pub step_rise: f32,
    pub trigger_center: Vec3,
    pub trigger_half_extents: Vec3,
}

impl Stairs {
    pub fn contains(&self, p: Vec3) -> bool {
        let d = (p - self.trigger_center).abs();
        d.x <= self.trigger_half_extents.x
            && d.y <= self.trigger_half_extents.y
            && d.z <= self.trigger_half_extents.z
    }
}

/// Authored ladder marker.
#[derive(Component, Debug, Clone)]
pub struct Ladder {
    pub bottom: Vec3,
    pub top: Vec3,
    pub trigger_center: Vec3,
    pub trigger_half_extents: Vec3,
}

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

/// Spawn a static axis-aligned box with full-size `dims` centred at `pos`.
fn spawn_box(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    name: &str,
    pos: Vec3,
    dims: Vec3,
    color: Color,
) {
    commands.spawn((
        Name::new(name.to_string()),
        Mesh3d(meshes.add(Cuboid::new(dims.x, dims.y, dims.z))),
        MeshMaterial3d(materials.add(color)),
        Transform::from_translation(pos),
        RigidBody::Static,
        Collider::cuboid(dims.x, dims.y, dims.z),
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
    spawn_box(&mut commands, m, mat, "Floor", Vec3::new(0.0, -0.5, 0.0), Vec3::new(50.0, 1.0, 50.0), floor_c);

    // --- Wall 10×4×1 at (0,2,-10) ---
    spawn_box(&mut commands, m, mat, "Wall", Vec3::new(0.0, 2.0, -10.0), Vec3::new(10.0, 4.0, 1.0), prop_c);

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
    spawn_box(&mut commands, m, mat, "AutoVaultSingleBlock", Vec3::new(0.0, 0.5, 4.0), Vec3::new(2.0, 1.0, 0.5), vault_c);
    spawn_box(&mut commands, m, mat, "AutoVaultWideRail", Vec3::new(-3.0, 0.45, 7.0), Vec3::new(3.5, 0.9, 0.5), vault_c);
    spawn_box(&mut commands, m, mat, "AutoVaultNarrowPost", Vec3::new(3.0, 0.55, 7.0), Vec3::new(0.8, 1.1, 0.5), vault_c);
    spawn_box(&mut commands, m, mat, "AutoVaultTallBlocker", Vec3::new(0.0, 1.1, 10.5), Vec3::new(2.5, 2.2, 0.5), prop_c);

    // --- Stairs: 8 steps, depth 0.5, rise 0.25, base near x=-5 going to x=-9 ---
    let step_count = 8;
    let step_depth = 0.5;
    let step_rise = 0.25;
    let width = 3.0;
    for i in 0..step_count {
        let height = step_rise * (i + 1) as f32;
        let x = -5.25 - step_depth * i as f32;
        spawn_box(
            &mut commands,
            m,
            mat,
            &format!("Step{i}"),
            Vec3::new(x, height * 0.5, 0.0),
            Vec3::new(step_depth, height, width),
            floor_c,
        );
    }
    // Landing 4×2×3 at (-11,1,0)
    spawn_box(&mut commands, m, mat, "Landing", Vec3::new(-11.0, 1.0, 0.0), Vec3::new(4.0, 2.0, 3.0), floor_c);

    // Stairs marker (trigger region + step geometry).
    commands.spawn((
        Name::new("Stairs"),
        Stairs {
            base: Vec3::new(-5.0, 0.0, 0.0),
            top: Vec3::new(-9.0, 2.0, 0.0),
            step_count,
            step_depth,
            step_rise,
            trigger_center: Vec3::new(-7.0, 1.3, 0.0),
            trigger_half_extents: Vec3::new(2.2, 1.3, 1.6),
        },
        Transform::from_xyz(-7.0, 1.3, 0.0),
    ));

    // --- Ladder against the front face of the Wall (z = -10, front at z ≈ -9.5) ---
    // Added so LadderMotor can be exercised.
    let ladder_x = 0.0;
    let ladder_z = -9.4;
    commands.spawn((
        Name::new("Ladder"),
        Mesh3d(m.add(Cuboid::new(0.8, 4.0, 0.1))),
        MeshMaterial3d(mat.add(Color::srgb(0.5, 0.35, 0.2))),
        Transform::from_xyz(ladder_x, 2.0, ladder_z),
        Ladder {
            bottom: Vec3::new(ladder_x, 0.0, ladder_z),
            top: Vec3::new(ladder_x, 4.0, ladder_z),
            trigger_center: Vec3::new(ladder_x, 2.0, ladder_z + 0.3),
            trigger_half_extents: Vec3::new(0.6, 2.0, 0.6),
        },
    ));
}
