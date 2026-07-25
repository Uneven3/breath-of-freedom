//! The decorative grass meadow: a dense field of authored tufts that sway in a
//! shared world-space wind.
//!
//! Pure presentation — the tufts carry no collider and no simulation meaning;
//! the floor beneath already reports `Surface(Grass)` for footstep audio, so the
//! grass never needs to be *known* by the simulation. This mirrors the tree
//! split: placement and look live here in `visuals`, never in `world`.

use bevy::prelude::*;

use crate::asset_pipeline::materials::AuthoredVisualRoot;
use crate::world::forest::{hash_u32, hash_unit};

/// Center of the meadow field in world XZ. The playable area sits north of the
/// origin, so the field is offset in Z; kept explicit so the asymmetry reads as
/// intentional rather than a stray literal.
const MEADOW_CENTER: Vec2 = Vec2::new(0.0, 6.0);

/// Density presets cycled by the F8 stress toggle. Index 0 is exactly what the
/// meadow spawns at startup, so [`GrassStressState::default`] (tier 0) always
/// describes what is actually on screen — the resource never lies about reality.
const GRASS_TIERS: [(usize, f32, &str); 3] = [
    (2400, 35.0, "2,400 briznas (Pradera BOTW - 38k tris budget)"),
    (5000, 60.0, "5,000 briznas (Océano BOTW)"),
    (800, 15.0, "800 briznas (Normal)"),
];

// --- Wind tuning ---------------------------------------------------------
// One shared world-space field, so the whole meadow reads as a single gust
// rolling across it rather than per-tuft noise.

/// Rate the wind *direction* drifts, in rad/s (a slow rotation).
const WIND_TURN_RATE: f32 = 0.05;
/// Gust envelope: how often gusts swell, and how sharply they peak.
const GUST_RATE: f32 = 0.12;
const GUST_SHARPNESS: f32 = 2.5;
/// Always-on micro-jitter, so grass is never dead-still between gusts.
const MICRO_RATE: f32 = 1.2;
const MICRO_AMPLITUDE: f32 = 0.015;
/// Spatial frequency of the travelling wave across the field.
const WAVE_SPATIAL_FREQ: f32 = 0.20;
/// Wave travel speed: a base plus a gust-driven boost.
const WAVE_BASE_SPEED: f32 = 3.0;
const WAVE_GUST_SPEED: f32 = 2.0;
/// Peak blade tilt at full gust, in radians.
const BEND_AMPLITUDE: f32 = 0.18;
/// Mix of the fundamental wave and its second harmonic (a touch of chop).
const WAVE_FUNDAMENTAL: f32 = 0.85;
const WAVE_HARMONIC: f32 = 0.15;

/// Which density preset the meadow is currently showing. Mutated only by the
/// F8 stress toggle; tier 0 is the startup meadow.
#[derive(Resource, Default)]
pub(super) struct GrassStressState {
    tier: u8,
}

#[derive(Component)]
pub(super) struct GrassTuft;

/// The tuft's authored yaw, kept separate so the per-frame wind tilt can be
/// composed on top of it without accumulating drift.
#[derive(Component, Clone, Copy)]
pub(super) struct GrassTuftBaseRotation(Quat);

/// Startup: lay down the initial meadow at the default (tier 0) density.
pub(super) fn spawn_meadow(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    stress: Res<GrassStressState>,
    scene: Res<State<crate::scene::AppState>>,
) {
    let (count, radius, _) = GRASS_TIERS[stress.tier as usize];
    spawn_grass_density(&mut commands, &asset_server, count, radius, *scene.get());
}

/// Scatter `count` tufts in a disc of `radius` around [`MEADOW_CENTER`], each a
/// GPU-instanced authored scene with a deterministic position, yaw and scale.
fn spawn_grass_density(
    commands: &mut Commands,
    asset_server: &AssetServer,
    count: usize,
    radius: f32,
    scene: crate::scene::AppState,
) {
    let tall_grass = asset_server.load("game/authored/props/prop_grass_tall_a.glb#Scene0");
    let card_grass = asset_server.load("game/authored/props/prop_grass_card_a.glb#Scene0");
    let flower_grass = asset_server.load("game/authored/props/prop_flower_wild_a.glb#Scene0");

    for i in 0..count {
        let hash = hash_u32(i as u32 ^ 0x6472_6173);
        let u1 = hash_unit(hash);
        let u2 = hash_unit(hash ^ 0x1234_5678);
        let u3 = hash_unit(hash ^ 0x8765_4321);

        let angle = u1 * std::f32::consts::TAU;
        let r = u2.sqrt() * radius;
        let x = MEADOW_CENTER.x + r * angle.cos();
        let z = MEADOW_CENTER.y + r * angle.sin();

        // BOTW Hybrid Architecture:
        // 0-10m: High-detail 3D geometry blades (prop_grass_tall_a)
        // 8m-45m: Interleaved 2D CardMeshes (prop_grass_card_a) for dense background foliage wall
        let cluster_noise = (x * 0.15).sin() * (z * 0.15).cos();
        let scene_handle = if cluster_noise > 0.70 {
            flower_grass.clone()
        } else if r < 10.0 || u3 > 0.40 {
            tall_grass.clone()
        } else {
            card_grass.clone()
        };

        let yaw = u3 * std::f32::consts::TAU;
        let scale = 0.90 + u1 * 0.30;
        let base_rotation = Quat::from_rotation_y(yaw);

        commands.spawn((
            DespawnOnExit(scene),
            Name::new(format!("GrassTuft_{i}")),
            GrassTuft,
            GrassTuftBaseRotation(base_rotation),
            bevy::light::NotShadowCaster,
            bevy::world_serialization::WorldAssetRoot(scene_handle),
            AuthoredVisualRoot,
            Transform::from_xyz(x, 0.0, z)
                .with_rotation(base_rotation)
                .with_scale(Vec3::splat(scale)),
        ));
    }
}

/// Tilt every tuft by the shared wind field this frame. Reads each tuft's fixed
/// world position for the wave phase, so the gust stays spatially coherent.
pub(super) fn animate_grass_wind(
    time: Res<Time>,
    mut tufts: Query<(&mut Transform, &GrassTuftBaseRotation), With<GrassTuft>>,
) {
    let t = time.elapsed_secs();

    let wind_angle = t * WIND_TURN_RATE;
    let wind_dir = Vec2::new(wind_angle.cos(), wind_angle.sin());

    let gust_cycle = (t * GUST_RATE).sin() * 0.5 + 0.5;
    let gust_intensity = gust_cycle.powf(GUST_SHARPNESS);

    let micro = (t * MICRO_RATE).sin() * MICRO_AMPLITUDE;

    for (mut transform, base_rot) in &mut tufts {
        let pos = transform.translation;

        let wave_pos = pos.x * wind_dir.x + pos.z * wind_dir.y;
        let wave_speed = WAVE_BASE_SPEED + gust_intensity * WAVE_GUST_SPEED;
        let wave_raw = (wave_pos * WAVE_SPATIAL_FREQ + t * wave_speed).sin();

        let wave = wave_raw * WAVE_FUNDAMENTAL + (wave_raw * 2.0).sin() * WAVE_HARMONIC;
        let bend = micro + wave * gust_intensity * BEND_AMPLITUDE;

        let pitch = wind_dir.y * bend;
        let roll = wind_dir.x * bend;

        let wind_tilt = Quat::from_euler(EulerRot::XYZ, pitch, 0.0, roll);
        transform.rotation = wind_tilt * base_rot.0;
    }
}

#[derive(Component)]
pub(super) struct TuftKind3D;

#[derive(Component)]
pub(super) struct TuftKindCard;

/// The camera the billboarded cards turn to face, disjoint from the tufts it
/// reads alongside.
type LodCameraQuery<'w, 's> = Query<
    'w,
    's,
    &'static Transform,
    (
        With<Camera3d>,
        Without<GrassTuft>,
        Without<crate::movement::Player>,
    ),
>;

/// Every tuft plus which representation it currently wears.
type LodTuftQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static mut Transform,
        Option<&'static TuftKind3D>,
        Option<&'static TuftKindCard>,
    ),
    (With<GrassTuft>, Without<crate::movement::Player>),
>;

/// Dynamic LOD and vertical scale growth (BOTW anti-pop technique): evaluates
/// player distance to every grass tuft in real-time, swaps between 3D grass
/// (< 8 m) and 2D CardMesh (>= 8 m), and scales Y smoothly.
pub(super) fn update_grass_dynamic_lod(
    mut commands: Commands,
    asset_server: Res<AssetServer>,
    player_query: Query<&Transform, With<crate::movement::Player>>,
    camera_query: LodCameraQuery,
    mut tufts: LodTuftQuery,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let player_pos = player_transform.translation;
    let camera_transform = camera_query.single().ok();

    for (entity, mut transform, is_3d, is_card) in &mut tufts {
        let pos = transform.translation;
        let dist = Vec2::new(pos.x - player_pos.x, pos.z - player_pos.z).length();

        // Dynamic 3D Grass vs 2D CardMesh swap based on real-time player distance (8.0m BOTW threshold)
        if dist <= 8.0 {
            if is_card.is_some() || (is_3d.is_none() && is_card.is_none()) {
                let scene_handle =
                    asset_server.load("game/authored/props/prop_grass_tall_a.glb#Scene0");
                commands
                    .entity(entity)
                    .insert(bevy::world_serialization::WorldAssetRoot(scene_handle))
                    .insert(TuftKind3D)
                    .remove::<TuftKindCard>();
            }
        } else if is_3d.is_some() || (is_3d.is_none() && is_card.is_none()) {
            let scene_handle =
                asset_server.load("game/authored/props/prop_grass_card_a.glb#Scene0");
            commands
                .entity(entity)
                .insert(bevy::world_serialization::WorldAssetRoot(scene_handle))
                .insert(TuftKindCard)
                .remove::<TuftKind3D>();
        }

        // Camera-facing billboarding for 2D Single Quad CardMeshes
        if is_card.is_some()
            && let Some(cam_t) = camera_transform
        {
            let cam_pos = cam_t.translation;
            let dx = cam_pos.x - pos.x;
            let dz = cam_pos.z - pos.z;
            if dx.abs() > 0.001 || dz.abs() > 0.001 {
                let yaw = dx.atan2(dz);
                transform.rotation = Quat::from_rotation_y(yaw);
            }
        }

        // BOTW Anti-pop vertical scale growth: Scale_Y grows smoothly out of the ground
        let target_scale_y = if dist < 8.0 {
            1.0
        } else if dist > 35.0 {
            0.0
        } else {
            1.0 - (dist - 8.0) / 27.0
        };

        transform.scale.y = target_scale_y;
    }
}

/// F8: cycle to the next density preset, replacing the whole field. A profiling
/// aid, so `GrassStressState` and the tuft count on screen stay in lockstep.
pub(super) fn handle_grass_stress_toggle(
    mut commands: Commands,
    keys: Res<ButtonInput<KeyCode>>,
    asset_server: Res<AssetServer>,
    mut state: ResMut<GrassStressState>,
    scene: Res<State<crate::scene::AppState>>,
    tufts: Query<Entity, With<GrassTuft>>,
) {
    if !keys.just_pressed(KeyCode::F8) {
        return;
    }
    for entity in &tufts {
        commands.entity(entity).despawn();
    }
    state.tier = (state.tier + 1) % GRASS_TIERS.len() as u8;
    let (count, radius, label) = GRASS_TIERS[state.tier as usize];
    info!("[grass-stress] density → {label} (count: {count}, radius: {radius}m)");
    spawn_grass_density(&mut commands, &asset_server, count, radius, *scene.get());
}
