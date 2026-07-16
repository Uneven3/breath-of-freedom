//! Game-feel feedback ("juice"): everything here is presentation reacting to
//! simulation, read-only over gameplay state plus its own ephemeral entities.
//!
//! Consumes `combat::motors::attack::HitImpactMessage` (Combat owns the type
//! and doesn't know we exist) and `LocomotionState` transitions:
//!
//! - **Hit flash**: the struck actor's capsule goes white for a beat.
//! - **Hit burst**: a small procedural white burst at the impact point.
//! - **Floating damage text**: world-anchored UI number, louder on crits.
//! - **Jelly**: squash & stretch on jump/landing for every actor visual.
//! - **Screen flash + camera trauma** when the *player* is the one hit.
//! - **Hitstop on criticals**: a ~90 ms global pause via
//!   `Time<Virtual>::relative_speed` — `Time<Fixed>` accumulates from
//!   virtual time, so the whole simulation freezes coherently and resumes
//!   with no drift; the countdown runs on `Time<Real>`. This is the
//!   "camera pause" of a sneakstrike, NOT the rejected flurry-rush slow-mo
//!   (a fixed, tiny, non-gameplay pause — nothing reads or fights it).

use bevy::prelude::*;

use crate::camera::CameraShake;
use crate::combat::motors::attack::HitImpactMessage;
use crate::movement::state::LocomotionState;
use crate::movement::{Actor, Player};
use crate::visuals::VisualOf;

const HIT_FLASH_SECS: f32 = 0.12;
const HIT_FLASH_COLOR: Color = Color::srgb(2.5, 2.5, 2.5);

const BURST_PARTICLES: usize = 8;
const BURST_SECS: f32 = 0.22;
const BURST_SPEED: f32 = 5.0;

const DAMAGE_TEXT_SECS: f32 = 0.8;
const DAMAGE_TEXT_RISE: f32 = 1.1;

const JELLY_JUMP_STRETCH: f32 = 0.28;
const JELLY_LAND_SQUASH: f32 = -0.24;
const JELLY_RECOVERY_PER_SEC: f32 = 9.0;

const HITSTOP_SECS: f32 = 0.09;
const PLAYER_HIT_TRAUMA: f32 = 0.55;
const SCREEN_FLASH_ALPHA: f32 = 0.3;
const SCREEN_FLASH_FADE_PER_SEC: f32 = 2.2;

/// Bow juice: camera trauma on release, scaled by draw charge.
const BOW_FIRE_TRAUMA_MIN: f32 = 0.08;
const BOW_FIRE_TRAUMA_MAX: f32 = 0.25;
/// Charged shot hitstop: brief freeze on a max-charge release.
const BOW_FULL_CHARGE_HITSTOP: f32 = 0.06;

pub struct JuicePlugin;

impl Plugin for JuicePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Hitstop>();
        app.add_systems(Startup, spawn_screen_flash);
        app.add_systems(
            Update,
            (
                flash_on_hit,
                expire_hit_flash,
                burst_on_hit,
                tick_burst_particles,
                damage_text_on_hit,
                tick_damage_text,
                attach_jelly,
                apply_jelly,
                player_hit_feedback,
                fade_screen_flash,
                hitstop_on_crit,
                tick_hitstop,
                bow_fire_feedback,
                animate_crosshair_charge,
            ),
        );
    }
}

// ---------------------------------------------------------------------------
// Hit flash
// ---------------------------------------------------------------------------

/// Present on a *visual* entity while its actor's hit flash lasts. While
/// attached, `visuals::tint_enemy_visual` leaves the material alone.
#[derive(Component)]
pub struct HitFlash {
    remaining: f32,
    original: Color,
}

type FlashableVisual<'a> = (
    Entity,
    &'a VisualOf,
    &'a MeshMaterial3d<StandardMaterial>,
    Option<&'a mut HitFlash>,
);

fn flash_on_hit(
    mut commands: Commands,
    mut impacts: MessageReader<HitImpactMessage>,
    mut visuals: Query<FlashableVisual>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for impact in impacts.read() {
        for (visual, of, material_handle, flash) in &mut visuals {
            if of.0 != impact.target {
                continue;
            }
            match flash {
                Some(mut flash) => flash.remaining = HIT_FLASH_SECS,
                None => {
                    let Some(mut material) = materials.get_mut(&material_handle.0) else {
                        continue;
                    };
                    commands.entity(visual).insert(HitFlash {
                        remaining: HIT_FLASH_SECS,
                        original: material.base_color,
                    });
                    material.base_color = HIT_FLASH_COLOR;
                }
            }
        }
    }
}

fn expire_hit_flash(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut visuals: Query<(Entity, &MeshMaterial3d<StandardMaterial>, &mut HitFlash)>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for (visual, material_handle, mut flash) in &mut visuals {
        flash.remaining -= time.delta_secs();
        if flash.remaining > 0.0 {
            continue;
        }
        if let Some(mut material) = materials.get_mut(&material_handle.0) {
            material.base_color = flash.original;
        }
        commands.entity(visual).remove::<HitFlash>();
    }
}

// ---------------------------------------------------------------------------
// Hit burst (procedural white VFX)
// ---------------------------------------------------------------------------

#[derive(Component)]
struct BurstParticle {
    velocity: Vec3,
    remaining: f32,
}

fn burst_on_hit(
    mut commands: Commands,
    mut impacts: MessageReader<HitImpactMessage>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    for impact in impacts.read() {
        let mesh = meshes.add(Sphere::new(0.07));
        let material = materials.add(StandardMaterial {
            base_color: Color::srgb(3.0, 3.0, 3.0),
            unlit: true,
            ..default()
        });
        for i in 0..BURST_PARTICLES {
            // Deterministic golden-angle fan, tilted upward: reads as a spark
            // spray without an RNG.
            let angle = i as f32 * 2.399_963;
            let dir = Vec3::new(
                angle.cos() * 0.8,
                0.6 + (i as f32 * 0.61).fract() * 0.5,
                angle.sin() * 0.8,
            )
            .normalize_or_zero();
            commands.spawn((
                BurstParticle {
                    velocity: dir * BURST_SPEED,
                    remaining: BURST_SECS,
                },
                Name::new("HitBurstParticle"),
                Mesh3d(mesh.clone()),
                MeshMaterial3d(material.clone()),
                Transform::from_translation(impact.position),
            ));
        }
    }
}

fn tick_burst_particles(
    mut commands: Commands,
    time: Res<Time>,
    mut particles: Query<(Entity, &mut BurstParticle, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (entity, mut particle, mut transform) in &mut particles {
        particle.remaining -= dt;
        if particle.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        transform.translation += particle.velocity * dt;
        // Decelerate and shrink out.
        particle.velocity *= 1.0 - (6.0 * dt).min(1.0);
        transform.scale = Vec3::splat((particle.remaining / BURST_SECS).max(0.05));
    }
}

// ---------------------------------------------------------------------------
// Floating damage text
// ---------------------------------------------------------------------------

#[derive(Component)]
struct DamageText {
    world: Vec3,
    remaining: f32,
}

fn damage_text_on_hit(mut commands: Commands, mut impacts: MessageReader<HitImpactMessage>) {
    for impact in impacts.read() {
        let (size, color) = if impact.critical {
            (30.0, Color::srgb(1.0, 0.85, 0.2))
        } else {
            (20.0, Color::WHITE)
        };
        commands.spawn((
            DamageText {
                world: impact.position + Vec3::Y * 1.2,
                remaining: DAMAGE_TEXT_SECS,
            },
            Name::new("DamageText"),
            Text::new(format!("{:.0}", impact.damage)),
            TextFont {
                font_size: FontSize::Px(size),
                ..default()
            },
            TextColor(color),
            Node {
                position_type: PositionType::Absolute,
                ..default()
            },
        ));
    }
}

fn tick_damage_text(
    mut commands: Commands,
    time: Res<Time<Real>>,
    camera: Single<(&Camera, &GlobalTransform), With<Camera3d>>,
    mut texts: Query<(Entity, &mut DamageText, &mut Node, &mut TextColor)>,
) {
    let (camera, camera_tf) = *camera;
    let dt = time.delta_secs();
    for (entity, mut text, mut node, mut color) in &mut texts {
        text.remaining -= dt;
        if text.remaining <= 0.0 {
            commands.entity(entity).despawn();
            continue;
        }
        let progress = 1.0 - text.remaining / DAMAGE_TEXT_SECS;
        let world = text.world + Vec3::Y * (DAMAGE_TEXT_RISE * progress);
        match camera.world_to_viewport(camera_tf, world) {
            Ok(screen) => {
                node.left = Val::Px(screen.x);
                node.top = Val::Px(screen.y);
                color.0 = color
                    .0
                    .with_alpha((text.remaining / DAMAGE_TEXT_SECS).min(1.0));
            }
            Err(_) => {
                // Behind the camera: just let it finish silently off-screen.
                node.left = Val::Px(-1000.0);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Jelly (squash & stretch on jump/landing)
// ---------------------------------------------------------------------------

/// Per-visual squash amount: positive stretches (jump), negative squashes
/// (landing), eased back to zero. Volume-ish preserving: Y gets `amount`,
/// XZ get `-amount * 0.6`.
#[derive(Component)]
pub struct Jelly {
    amount: f32,
    last_state: LocomotionState,
}

fn attach_jelly(
    mut commands: Commands,
    visuals: Query<(Entity, &VisualOf), Without<Jelly>>,
    actors: Query<&LocomotionState, With<Actor>>,
) {
    for (visual, of) in &visuals {
        if let Ok(state) = actors.get(of.0) {
            commands.entity(visual).insert(Jelly {
                amount: 0.0,
                last_state: *state,
            });
        }
    }
}

fn is_airborne(state: LocomotionState) -> bool {
    matches!(
        state,
        LocomotionState::Jump
            | LocomotionState::Fall
            | LocomotionState::Glide
            | LocomotionState::WallJump
            | LocomotionState::EdgeLeap
    )
}

fn jelly_impulse(from: LocomotionState, to: LocomotionState) -> Option<f32> {
    if !is_airborne(from) && matches!(to, LocomotionState::Jump) {
        return Some(JELLY_JUMP_STRETCH);
    }
    if is_airborne(from) && !is_airborne(to) {
        return Some(JELLY_LAND_SQUASH);
    }
    None
}

fn apply_jelly(
    time: Res<Time>,
    actors: Query<&LocomotionState, With<Actor>>,
    mut visuals: Query<(&VisualOf, &mut Jelly, &mut Transform)>,
) {
    let dt = time.delta_secs();
    for (of, mut jelly, mut transform) in &mut visuals {
        let Ok(state) = actors.get(of.0) else {
            continue;
        };
        if *state != jelly.last_state {
            if let Some(impulse) = jelly_impulse(jelly.last_state, *state) {
                jelly.amount = impulse;
            }
            jelly.last_state = *state;
        }
        jelly.amount *= 1.0 - (JELLY_RECOVERY_PER_SEC * dt).min(1.0);
        transform.scale = Vec3::new(
            1.0 - jelly.amount * 0.6,
            1.0 + jelly.amount,
            1.0 - jelly.amount * 0.6,
        );
    }
}

// ---------------------------------------------------------------------------
// Player-received feedback: screen flash + camera trauma
// ---------------------------------------------------------------------------

#[derive(Component)]
struct ScreenFlash {
    alpha: f32,
}

fn spawn_screen_flash(mut commands: Commands) {
    commands.spawn((
        ScreenFlash { alpha: 0.0 },
        Name::new("ScreenFlash"),
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        },
        BackgroundColor(Color::NONE),
        // Above the debug HUD, below nothing that matters.
        GlobalZIndex(50),
    ));
}

/// The wired half of "getting hit feels bad": fires when the *player* is the
/// impact target. Exercised for real once enemies attack (`enemies-combat`).
fn player_hit_feedback(
    mut impacts: MessageReader<HitImpactMessage>,
    player: Query<(), With<Player>>,
    mut flash: Single<&mut ScreenFlash>,
    mut shake: ResMut<CameraShake>,
) {
    for impact in impacts.read() {
        if player.get(impact.target).is_ok() {
            flash.alpha = SCREEN_FLASH_ALPHA;
            shake.add_trauma(PLAYER_HIT_TRAUMA);
        }
    }
}

fn fade_screen_flash(
    time: Res<Time<Real>>,
    mut flash: Single<(&mut ScreenFlash, &mut BackgroundColor)>,
) {
    let (flash, background) = &mut *flash;
    flash.alpha = (flash.alpha - SCREEN_FLASH_FADE_PER_SEC * time.delta_secs()).max(0.0);
    background.0 = Color::WHITE.with_alpha(flash.alpha);
}

// ---------------------------------------------------------------------------
// Hitstop
// ---------------------------------------------------------------------------

/// Remaining hitstop, counted in *real* seconds (virtual time is the thing
/// being paused).
#[derive(Resource, Default)]
pub struct Hitstop(f32);

fn hitstop_on_crit(mut impacts: MessageReader<HitImpactMessage>, mut hitstop: ResMut<Hitstop>) {
    for impact in impacts.read() {
        if impact.critical {
            hitstop.0 = HITSTOP_SECS;
        }
    }
}

fn tick_hitstop(
    real: Res<Time<Real>>,
    mut hitstop: ResMut<Hitstop>,
    mut virtual_time: ResMut<Time<Virtual>>,
) {
    if hitstop.0 > 0.0 {
        hitstop.0 -= real.delta_secs();
        virtual_time.set_relative_speed(if hitstop.0 > 0.0 { 0.0 } else { 1.0 });
    }
}

// ---------------------------------------------------------------------------
// Bow juice: camera kick on release, crosshair contraction while charging
// ---------------------------------------------------------------------------

fn bow_fire_feedback(
    player: Query<&crate::combat::motors::aim::DrawStrength, With<Player>>,
    mut shake: ResMut<CameraShake>,
    mut hitstop: ResMut<Hitstop>,
    mut prev_factor: Local<f32>,
) {
    let Ok(draw) = player.single() else {
        return;
    };
    if draw.just_fired {
        // Trauma proportional to how charged the shot was.
        let trauma = BOW_FIRE_TRAUMA_MIN
            + (BOW_FIRE_TRAUMA_MAX - BOW_FIRE_TRAUMA_MIN) * *prev_factor;
        shake.add_trauma(trauma);
        if *prev_factor > 0.95 {
            hitstop.0 = BOW_FULL_CHARGE_HITSTOP;
        }
    }
    *prev_factor = draw.factor;
}

#[allow(clippy::type_complexity)]
fn animate_crosshair_charge(
    player: Single<&crate::combat::motors::aim::DrawStrength, With<Player>>,
    rig: Single<&crate::camera::CameraRig>,
    mut ring: Single<&mut Node, With<crate::camera::CrosshairRing>>,
) {
    let draw = *player;
    let rig = *rig;
    let node = &mut **ring;
    if rig.aim_blend < 0.5 {
        return;
    }
    // Circle starts at 64px spread when uncharged, shrinks to 12px when fully charged.
    // If not actively charging/draw factor is 0.0, it stays at 64px.
    let size = 64.0 - 52.0 * draw.factor;
    let half = size / 2.0;
    node.width = Val::Px(size);
    node.height = Val::Px(size);
    node.margin = UiRect {
        left: Val::Px(-half),
        top: Val::Px(-half),
        ..default()
    };
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jelly_stretches_on_jump_and_squashes_on_landing() {
        assert_eq!(
            jelly_impulse(LocomotionState::Walk, LocomotionState::Jump),
            Some(JELLY_JUMP_STRETCH)
        );
        assert_eq!(
            jelly_impulse(LocomotionState::Fall, LocomotionState::Walk),
            Some(JELLY_LAND_SQUASH)
        );
        assert_eq!(
            jelly_impulse(LocomotionState::Glide, LocomotionState::Sneak),
            Some(JELLY_LAND_SQUASH)
        );
    }

    #[test]
    fn jelly_ignores_transitions_that_are_not_takeoff_or_landing() {
        assert_eq!(
            jelly_impulse(LocomotionState::Walk, LocomotionState::Sprint),
            None
        );
        assert_eq!(
            jelly_impulse(LocomotionState::Jump, LocomotionState::Fall),
            None,
            "the jump arc turning into a fall is not a landing"
        );
        assert_eq!(
            jelly_impulse(LocomotionState::Walk, LocomotionState::Climb),
            None
        );
    }
}
