//! Player visual mesh, decoupled from the physics body.
//!
//! A standalone mesh entity interpolates toward the kinematic body each render
//! frame, which (a) smooths the 60 Hz fixed-step motion on high-refresh
//! displays and (b) dips −0.4 in Sneak so crouching reads visually even though
//! the collider, not the mesh, is what actually shrinks.

use bevy::prelude::*;

use crate::movement::state::LocomotionState;
use crate::movement::{Player, body};

const INTERPOLATION_SPEED: f32 = 20.0;
const SNEAK_Y_OFFSET: f32 = -0.4;

#[derive(Component)]
struct PlayerVisual;

pub struct VisualsPlugin;

impl Plugin for VisualsPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_visual);
        app.add_systems(Update, interpolate_visual);
    }
}

fn spawn_visual(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    commands.spawn((
        PlayerVisual,
        Name::new("PlayerVisual"),
        Mesh3d(meshes.add(Capsule3d::new(body::RADIUS, body::STAND_CAPSULE_LENGTH))),
        MeshMaterial3d(materials.add(Color::srgb(0.3, 0.6, 0.9))),
        Transform::from_xyz(0.0, 1.5, 0.0),
    ));
}

type BodyFilter = (With<Player>, Without<PlayerVisual>);

fn interpolate_visual(
    player: Single<(&Transform, &LocomotionState), BodyFilter>,
    mut visual: Single<&mut Transform, With<PlayerVisual>>,
    time: Res<Time>,
) {
    let (body, state) = *player;
    let t = (INTERPOLATION_SPEED * time.delta_secs()).clamp(0.0, 1.0);
    let offset = if *state == LocomotionState::Sneak {
        SNEAK_Y_OFFSET
    } else {
        0.0
    };

    // X/Z track the body directly; Y and rotation interpolate.
    let target_y = body.translation.y + offset;
    visual.translation.x = body.translation.x;
    visual.translation.z = body.translation.z;
    visual.translation.y += (target_y - visual.translation.y) * t;
    visual.rotation = visual.rotation.slerp(body.rotation, t);
}
