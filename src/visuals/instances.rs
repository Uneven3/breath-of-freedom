//! Presentation for the editor's instance layer: a placed [`PropKind`]
//! resolves to its authored `.glb` scene via `bsn!`/[`SceneComponent`] — no
//! proxy tier, no swap machinery like `forest.rs` needs for trees, because a
//! placed prop has exactly one representation.
//!
//! Runs in every scene unconditionally, same as `terrain::sync_terrain_visual`
//! — saved instances are level content, not an editing-mode preview. Only
//! placing/clearing them (`editor::instances`) is gated to scenes that
//! declare terrain authoring.

use bevy::prelude::*;
use bof_domain::scene::SceneScoped;

use crate::asset_pipeline::materials::AuthoredVisualRoot;
use crate::world::{PropKind, Terrain};

/// Marks an entity spawned from one row of [`Terrain::instances`].
#[derive(Component, Default, Clone)]
pub(super) struct InstanceVisual;

/// A unit struct: which prop travels through [`PropVisualProps`] at spawn
/// time, not as a stored field — [`PropKind`] has no meaningful default.
#[derive(SceneComponent, Default, Clone)]
#[scene(PropVisualProps)]
struct PropVisual;

struct PropVisualProps {
    kind: PropKind,
}

impl Default for PropVisualProps {
    fn default() -> Self {
        Self {
            kind: PropKind::ALL[0],
        }
    }
}

impl PropVisual {
    fn scene(props: PropVisualProps) -> impl Scene {
        bsn! { WorldAssetRoot({glb_path_for(props.kind)}) }
    }
}

/// Hand-written, not routed through `visuals::catalog`'s `AppearanceKey` —
/// that catalog serves the tree proxy/detail swap this module deliberately
/// skips, and `bsn!` already resolves a string straight to a `Handle`.
fn glb_path_for(kind: PropKind) -> &'static str {
    match kind {
        PropKind::GrassA => "game/authored/props/prop_grass_a.glb#Scene0",
        PropKind::GrassB => "game/authored/props/prop_grass_b.glb#Scene0",
        PropKind::GrassC => "game/authored/props/prop_grass_c.glb#Scene0",
        PropKind::GrassDryA => "game/authored/props/prop_grass_dry_a.glb#Scene0",
        PropKind::GrassTallA => "game/authored/props/prop_grass_tall_a.glb#Scene0",
        PropKind::GrassVeryShortA => "game/authored/props/prop_grass_very_short_a.glb#Scene0",
    }
}

/// What was last synced: which terrain, which wholesale-replace generation,
/// which revision, and how many instances it held.
type SyncedState = (Entity, u32, u32, usize);

/// Resync visuals from [`Terrain::instances`] when the layer actually
/// changed — gated on `instances_revision`, not the generic `Changed<Terrain>`
/// every sculpt or paint stroke also fires.
///
/// The hot path is incremental, not "despawn everything": while an author
/// drags the scatter brush, `place_instance` only ever appends, so spawning
/// just the new tail is what keeps a session from flickering every already-
/// placed prop on every frame. That shortcut is only safe when
/// `instances_generation` also matches the last sync — a `Ctrl+L` reload can
/// grow the list too, but every entry in it may have moved, and `retain`
/// (an RMB clear) does not say which rows it dropped either. Both fall back
/// to the simple despawn-and-respawn-everything this first cut was scoped
/// for.
pub(super) fn sync_instance_visuals(
    mut commands: Commands,
    terrain: Query<(Entity, &Terrain)>,
    existing: Query<Entity, With<InstanceVisual>>,
    mut last_synced: Local<Option<SyncedState>>,
) {
    let Ok((terrain_entity, terrain)) = terrain.single() else {
        return;
    };
    let len = terrain.instances().len();
    let current: SyncedState = (
        terrain_entity,
        terrain.instances_generation(),
        terrain.instances_revision(),
        len,
    );
    if *last_synced == Some(current) {
        return;
    }
    let append_only = last_synced.is_some_and(|(entity, generation, _, previous_len)| {
        entity == terrain_entity
            && generation == terrain.instances_generation()
            && previous_len <= len
    });
    let start = if append_only {
        last_synced.map_or(0, |(.., previous_len)| previous_len)
    } else {
        for entity in &existing {
            commands.entity(entity).despawn();
        }
        0
    };
    *last_synced = Some(current);
    for instance in &terrain.instances()[start..] {
        let translation = Vec3::new(instance.xz.x, terrain.height_at(instance.xz), instance.xz.y);
        let rotation = Quat::from_rotation_y(instance.yaw);
        let scale = Vec3::splat(instance.scale);
        // Chained, not inside the `bsn!` — `AuthoredVisualRoot` has no
        // `Default`/`Clone`/`FromTemplate`. Without it, `apply_authored_lod`
        // and `remap_authored_materials` (`asset_pipeline::materials`) never
        // find this entity as an ancestor, so every LOD mesh in the `.glb`
        // renders stacked at once instead of one by distance.
        commands
            .queue_spawn_scene(bsn! {
                @PropVisual { @kind: {instance.kind} }
                InstanceVisual
                SceneScoped
                Transform { translation: {translation}, rotation: {rotation}, scale: {scale} }
            })
            .insert(AuthoredVisualRoot);
    }
}

/// Keeps every placed prop sitting on the ground as relief is sculpted.
/// Deliberately ungated: a `relief_revision`-gated version left a real gap —
/// an instance whose `.glb` is still loading when relief changes has no
/// `Transform` yet to correct, and resolves later with the stale height
/// already baked in, past the point the gate considered that change seen.
pub(super) fn follow_relief_changes(
    terrain: Query<&Terrain>,
    mut visuals: Query<&mut Transform, With<InstanceVisual>>,
) {
    let Ok(terrain) = terrain.single() else {
        return;
    };
    for mut transform in &mut visuals {
        let xz = transform.translation.xz();
        transform.translation.y = terrain.height_at(xz);
    }
}
