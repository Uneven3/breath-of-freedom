//! Deterministic forest layout and its gameplay collision profiles.

use bevy::prelude::*;

use super::spawn::{TreeSpec, spawn_tree};
use crate::asset_pipeline::{SpatialCatalog, SpatialProfileKey};

const CLEARING_RADIUS: f32 = 42.0;
const GRID_STEP: f32 = 11.5;
const GRID_RADIUS: i32 = 12;
const DENSITY_PERCENT: u32 = 31;
const PATH_HALF_WIDTH: f32 = 5.0;

/// Semantic tree identity. Physics is authored independently here;
/// presentation maps this value to the active asset library.
#[derive(Component, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeKind {
    Common1,
    Common2,
    Common3,
    Common4,
    Common5,
    Pine1,
    Pine2,
    Pine3,
    Pine4,
    Pine5,
    Twisted1,
    Twisted2,
    Twisted3,
    Twisted4,
    Twisted5,
}

#[derive(Debug, Clone, Copy)]
struct ForestTreeRow {
    kind: TreeKind,
    position: Vec3,
    yaw: f32,
    trunk_radius: f32,
    trunk_height: f32,
}

fn hash_u32(mut value: u32) -> u32 {
    value ^= value >> 16;
    value = value.wrapping_mul(0x7feb_352d);
    value ^= value >> 15;
    value = value.wrapping_mul(0x846c_a68b);
    value ^ (value >> 16)
}

fn hash_unit(value: u32) -> f32 {
    hash_u32(value) as f32 / u32::MAX as f32
}

fn forest_kind(hash: u32) -> TreeKind {
    let variant = (hash % 5) as u8;
    match (hash >> 8) % 20 {
        0..=10 => match variant {
            0 => TreeKind::Common1,
            1 => TreeKind::Common2,
            2 => TreeKind::Common3,
            3 => TreeKind::Common4,
            _ => TreeKind::Common5,
        },
        11..=17 => match variant {
            0 => TreeKind::Pine1,
            1 => TreeKind::Pine2,
            2 => TreeKind::Pine3,
            3 => TreeKind::Pine4,
            _ => TreeKind::Pine5,
        },
        _ => match variant {
            0 => TreeKind::Twisted1,
            1 => TreeKind::Twisted2,
            2 => TreeKind::Twisted3,
            3 => TreeKind::Twisted4,
            _ => TreeKind::Twisted5,
        },
    }
}

fn tree_collider(kind: TreeKind) -> (f32, f32) {
    match kind {
        TreeKind::Common1
        | TreeKind::Common2
        | TreeKind::Common3
        | TreeKind::Common4
        | TreeKind::Common5 => (0.48, 4.8),
        TreeKind::Pine1 | TreeKind::Pine2 | TreeKind::Pine3 | TreeKind::Pine4 | TreeKind::Pine5 => {
            (0.44, 4.4)
        }
        TreeKind::Twisted1
        | TreeKind::Twisted2
        | TreeKind::Twisted3
        | TreeKind::Twisted4
        | TreeKind::Twisted5 => (0.72, 6.2),
    }
}

fn forest_layout() -> Vec<ForestTreeRow> {
    let mut rows = Vec::new();
    let mut cell_index = 0_u32;
    for z in -GRID_RADIUS..=GRID_RADIUS {
        for x in -GRID_RADIUS..=GRID_RADIUS {
            let hash = hash_u32(cell_index.wrapping_add(0x5eed_1234));
            cell_index += 1;
            if hash % 100 >= DENSITY_PERCENT {
                continue;
            }

            let jitter_x = (hash_unit(hash ^ 0x94d0_49bb) - 0.5) * GRID_STEP * 0.58;
            let jitter_z = (hash_unit(hash ^ 0x369d_ea0f) - 0.5) * GRID_STEP * 0.58;
            let position = Vec3::new(
                x as f32 * GRID_STEP + jitter_x,
                0.0,
                z as f32 * GRID_STEP + jitter_z,
            );
            if position.xz().length() < CLEARING_RADIUS || position.x.abs() < PATH_HALF_WIDTH {
                continue;
            }

            let kind = forest_kind(hash);
            let (trunk_radius, trunk_height) = tree_collider(kind);
            rows.push(ForestTreeRow {
                kind,
                position,
                yaw: hash_unit(hash ^ 0x68bc_21eb) * std::f32::consts::TAU,
                trunk_radius,
                trunk_height,
            });
        }
    }
    rows
}

fn authored_tree_collider(kind: TreeKind, spatial: &SpatialCatalog) -> Option<(f32, f32)> {
    let profile = match kind {
        TreeKind::Pine1 => SpatialProfileKey("tree_pine_trunk"),
        _ => return None,
    };
    spatial
        .cylinder(profile, "UCY_Trunk")
        .map(|collider| (collider.radius, collider.height))
}

pub(super) fn spawn_forest(commands: &mut Commands, spatial: &SpatialCatalog) {
    for (index, row) in forest_layout().into_iter().enumerate() {
        let (trunk_radius, trunk_height) = authored_tree_collider(row.kind, spatial)
            .unwrap_or((row.trunk_radius, row.trunk_height));
        spawn_tree(
            commands,
            format!("ForestTree{index:03}"),
            TreeSpec {
                kind: row.kind,
                position: row.position,
                yaw: row.yaw,
                trunk_radius,
                trunk_height,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forest_is_dense_but_preserves_the_course_and_path() {
        let trees = forest_layout();
        assert!(
            (140..=210).contains(&trees.len()),
            "unexpected forest density: {}",
            trees.len()
        );
        for tree in trees {
            assert!(tree.position.xz().length() >= CLEARING_RADIUS);
            assert!(tree.position.x.abs() >= PATH_HALF_WIDTH);
            assert!(tree.trunk_radius > 0.0);
            assert!(tree.trunk_height > 0.0);
        }
    }

    #[test]
    fn pine_one_uses_the_authored_spatial_profile() {
        let spatial = SpatialCatalog::default();
        let collider = authored_tree_collider(TreeKind::Pine1, &spatial).unwrap();
        assert!((collider.0 - 0.44).abs() < 0.001);
        assert!((collider.1 - 4.4).abs() < 0.001);
        assert!(authored_tree_collider(TreeKind::Pine2, &spatial).is_none());
    }
}
