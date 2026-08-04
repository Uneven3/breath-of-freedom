//! Presentation adapter for inventory items placed in the world.

use bevy::prelude::*;

use crate::asset_pipeline::MaterialPalette;
use crate::inventory::data::WorldItem;
use crate::inventory::{ItemKind, ItemStack, PickupMode};
use bof_simulation::interaction::{Interactable, InteractionKind};

const INTERACT_PICKUP_RANGE: f32 = 2.5;

/// Spawns the disposable graybox representation around simulation-owned data.
#[allow(clippy::too_many_arguments)]
pub fn spawn_world_item(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    palette: &MaterialPalette,
    name: &str,
    position: Vec3,
    stack: ItemStack,
    mode: PickupMode,
    scene: crate::scene::AppState,
) -> Entity {
    let (dims, material_key) = match stack.kind {
        ItemKind::Weapon(_) => (Vec3::new(0.15, 0.7, 0.15), "PickupWeapon"),
        ItemKind::Material(_) => (Vec3::new(0.3, 0.3, 0.3), "PickupMaterial"),
        ItemKind::Food { .. } => (Vec3::new(0.25, 0.25, 0.25), "PickupFood"),
    };
    let mut item = commands.spawn((
        DespawnOnExit(scene),
        Name::new(name.to_string()),
        WorldItem { stack, mode },
        Mesh3d(meshes.add(Cuboid::new(dims.x, dims.y, dims.z))),
        MeshMaterial3d(palette.handle(material_key)),
        Transform::from_translation(position),
    ));
    if mode == PickupMode::Interact {
        item.insert(Interactable {
            kind: InteractionKind::Pickup,
            range: INTERACT_PICKUP_RANGE,
        });
    }
    item.id()
}
