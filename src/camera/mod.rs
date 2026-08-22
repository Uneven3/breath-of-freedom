//! Orbit follow camera.
//!
//! Presentation-only camera: follows the local actor using the control
//! orientation published by Input, with a landing dip and collision spring arm.

use avian3d::prelude::*;
use bevy::core_pipeline::prepass::{DepthPrepass, NormalPrepass};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::prelude::*;

use crate::input::frame::ControlOrientation;
use crate::visuals::PlayerVisual;
use crate::world::day_night::{TimeOfDay, atmosphere_color};
use bof_domain::movement::Player;
use bof_domain::movement::state::LocomotionState;

use bof_domain::combat::state::{AIM_PIVOT_HEIGHT, AIM_SHOULDER_OFFSET};

mod crosshair;
mod data;
mod freecam;

pub use data::{CameraControl, CameraRig, CameraShake, Crosshair, CrosshairRing, LookSensitivity};
pub(crate) use freecam::RADIUS_MODIFIER;

const SPRING_LENGTH: f32 = 6.5;
/// Free-orbit pivot height, camera feel only. While aiming the pivot blends
/// down to Combat's `AIM_PIVOT_HEIGHT` (§20: the aim line is simulation) so
/// at full blend the crosshair ray and the aim ray are the same line.
const LENS_HEIGHT: f32 = 1.5;
/// Aim mode (bow drawn): tighter boom, shoulder offset shared with Combat
/// (see `LENS_HEIGHT`), and how fast the camera blends in/out of it.
const AIM_SPRING_LENGTH: f32 = 3.6;
const AIM_BLEND_PER_SEC: f32 = 10.0;
/// Height over the lock-on target the camera looks at (torso). Matches Combat's
/// `LOCK_AIM_TARGET_HEIGHT` so the crosshair and the arrow converge on one point.
const LOCK_LOOK_HEIGHT: f32 = 0.9;
/// FOV eases toward its aim/draw target so firing (draw factor snapping to
/// zero) doesn't pop the lens.
const FOV_LERP_PER_SEC: f32 = 12.0;
const LANDING_DIP_INTENSITY: f32 = 0.5;
const LANDING_DIP_RECOVERY: f32 = 8.0;
const FOLLOW_LERP_Y: f32 = 15.0;
/// Keep the camera this far off a surface it would otherwise clip into.
const SPRING_MARGIN: f32 = 0.2;
const SPRING_PROBE_RADIUS: f32 = 0.2;
/// Fade out the player model if the camera is closer than this to prevent clipping.
const FIRST_PERSON_THRESHOLD: f32 = 0.8;

// Steepened to 40/80/0.7 on 2026-08-09 to paper over the meadow's own 64 m
// LOD edge reading as a hole in the ground — played, and it didn't work: a
// density wall parked on top of the camera reads as "fog placed on purpose
// to hide something," not depth. BOTW reserves real opacity for authored
// setpieces (sandstorms, Korok forest fog) and uses a barely-there veil for
// everything else, tied to the view arc rather than to any one LOD edge.
// Reverted 2026-08-09: gradual again, decoupled from the meadow edge. The
// hole itself stays open as its own debt — dressing terrain past 64 m, not
// another turn of this knob (see AHORA.md).
const FOG_START_METERS: f32 = 45.0;
const FOG_END_METERS: f32 = 240.0;
const FOG_MAX_ALPHA: f32 = 0.3;

const SHAKE_DECAY_PER_SEC: f32 = 2.2;
const SHAKE_MAX_OFFSET: f32 = 0.25;
const SHAKE_MAX_ROLL: f32 = 0.03;

pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CameraShake>();
        app.init_resource::<LookSensitivity>();
        app.add_systems(Startup, (spawn_camera, crosshair::spawn));
        // Orientation resolves in PreUpdate (see InputPlugin), so Update-time
        // camera systems always read the current frame's orientation.
        //
        // The orbit-follow systems and the freecam are gated by mode so exactly
        // one writer owns the camera transform each frame; the benchmark park
        // stays ungated and last, so it overrides whichever mode is active.
        app.add_systems(
            Update,
            (
                // F3 alterna órbita↔freecam, y eso sólo tiene sentido con un
                // jugador al que volver. El World Lab es freecam y nada más.
                freecam::toggle_camera_mode.run_if(crate::scene::in_game_mode),
                freecam::log_waypoint.run_if(freecam::in_freecam_mode),
                camera_landing_dip.run_if(freecam::in_orbit_mode),
                follow_player.run_if(freecam::in_orbit_mode),
                apply_camera_shake.run_if(freecam::in_orbit_mode),
                freecam::fly_freecam.run_if(freecam::in_freecam_mode),
                update_distance_fog,
                // Last, so it overrides follow/freecam and shake for the duration.
                park_scripted_camera,
                crosshair::toggle,
                apply_render_scale,
                apply_msaa,
                apply_flat_measure_view,
            )
                .chain(),
        );
    }
}

fn spawn_camera(
    mut commands: Commands,
    time_of_day: Res<TimeOfDay>,
    perf: Res<crate::perf::PerfToggles>,
    mode: Res<crate::scene::AppMode>,
) {
    // El World Lab no spawnea jugador, y `follow_player` es un `Single` sobre
    // él: en órbita la cámara se quedaría clavada en el origen. Vuela desde el
    // arranque, encuadrando el mapa desde arriba.
    let (camera_control, pose) = match *mode {
        crate::scene::AppMode::Editor => (CameraControl::authoring(), authoring_pose()),
        crate::scene::AppMode::Game => (
            CameraControl::default(),
            Transform::from_xyz(0.0, 3.0, 6.0).looking_at(Vec3::Y * 1.5, Vec3::Y),
        ),
    };
    commands.spawn((
        Name::new("CameraRig"),
        CameraRig::default(),
        // Camera mode and freecam look angles live on the camera entity, next to
        // the rest of its view state (`CameraRig`), not in a global resource.
        camera_control,
        Camera3d::default(),
        pose,
        soft_distance_fog(atmosphere_color(time_of_day.hours)),
        crate::perf::data::profile_msaa(perf.profile),
        // Ambos juntos, no sólo `DepthPrepass`: con sólo depth, Bevy bindea un
        // material bind group **vacío** para materiales opacos
        // (`is_depth_only_opaque_prepass`, bevy_pbr 0.19 prepass/mod.rs), y el
        // vertex de la pradera necesita `blade_records` —que vive en ese bind
        // group— para reconstruir la brizna. `NormalPrepass` fuerza el bind
        // group completo. Ver `visuals::grass_material` y `BOTWGrass.md`.
        DepthPrepass,
        NormalPrepass,
    ));
}

/// Dónde nace la cámara de autoría, derivado de sus propios ángulos: a
/// `AUTHORING_HEIGHT` sobre el origen y la distancia horizontal que hace que
/// `AUTHORING_PITCH` apunte justo ahí. Calcularlo —en vez de escribir un
/// `(0, 45, 53)` a mano— es lo que impide que tocar el ángulo deje la cámara
/// mirando al costado del mapa.
fn authoring_pose() -> Transform {
    let distance = data::AUTHORING_HEIGHT / data::AUTHORING_PITCH.abs().tan();
    Transform::from_xyz(0.0, data::AUTHORING_HEIGHT, distance).looking_at(Vec3::ZERO, Vec3::Y)
}

/// Applies the `RenderScale` knob to the one camera (§7: the owner of the
/// entity applies the knob).
///
/// A shrunk viewport rather than a scaled render target: it needs no offscreen
/// image and no upscale pass, and because both axes shrink by the same factor
/// the aspect ratio — and so the framing — is untouched. What changes is only
/// the number of fragments, which is exactly the variable the resolution sweep
/// isolates. The image landing in a corner of the window is the honest tell
/// that a diagnostic dial is on.
///
/// Runs every frame rather than on `perf.is_changed()`, because resizing the
/// window also invalidates the viewport, and a stale one clips the view to a
/// rectangle that no longer matches the surface.
fn apply_render_scale(
    perf: Res<crate::perf::PerfToggles>,
    window: Single<&Window, With<bevy::window::PrimaryWindow>>,
    mut camera: Single<&mut Camera, With<CameraRig>>,
) {
    let scale = perf.render_scale();
    let wanted = (scale < 1.0).then(|| {
        (window.physical_size().as_vec2() * scale)
            .as_uvec2()
            .max(UVec2::ONE)
    });
    // `Viewport` has no `PartialEq`, so the comparison is on the one field this
    // system writes; position is always the origin and depth is always default.
    if camera.viewport.as_ref().map(|v| v.physical_size) != wanted {
        camera.viewport = wanted.map(|physical_size| bevy::camera::Viewport {
            physical_position: UVec2::ZERO,
            physical_size,
            ..default()
        });
    }
}

/// Applies the `Msaa` knob to the one camera (§7, same as the render scale).
///
/// **Changing this rebuilds render pipelines**, because the sample count is part
/// of a pipeline's key — so the first frames after a click stutter while they
/// recompile. That is not a reason to keep it at launch-time only: it is a
/// reason for the benchmark to prime every configuration before measuring,
/// which it already does. Outside a run the hitch is one click's worth.
///
/// Gated on `is_changed` rather than run every frame like the viewport: nothing
/// else invalidates it, and writing the component unconditionally would mark it
/// changed every frame for every observer downstream.
fn apply_msaa(perf: Res<crate::perf::PerfToggles>, mut camera: Single<&mut Msaa, With<CameraRig>>) {
    if !perf.is_changed() {
        return;
    }
    let wanted = crate::perf::data::msaa_for_samples(perf.msaa_samples());
    if **camera != wanted {
        **camera = wanted;
    }
}

/// Deja pasar los colores exactos cuando la pradera está en modo de medición
/// (§7, igual que las dos perillas de arriba: el dueño de la cámara aplica).
///
/// **Sin esto la vista de medición mide otra cosa.** El shader escribe el color
/// de la paleta y después el frame pasa por dos etapas que lo cambian antes de
/// llegar al PNG: el tonemapping, que comprime el rango, y el dithering, que le
/// suma ruido de ±1/255 justamente para que las rampas no se vean en bandas. Las
/// dos son correctas para una imagen y fatales para un histograma — contar
/// píxeles de un color conocido deja de ser exacto si el color ya no es el que
/// se escribió. Es la misma clase de error que este trabajo vino a cerrar:
/// precisión real sobre un blanco equivocado.
///
/// Las vistas de *ver* no lo tocan: ahí el objetivo es que el campo siga
/// pareciéndose al juego.
fn apply_flat_measure_view(
    perf: Res<crate::perf::PerfToggles>,
    mut camera: Single<(&mut Tonemapping, &mut DebandDither), With<CameraRig>>,
) {
    if !perf.is_changed() {
        return;
    }
    let flat =
        crate::visuals::grass_debug::GrassDebugView::from_step(perf.grass_debug_step()).is_flat();
    let (tonemapping, dither) = &mut *camera;
    let wanted_tonemapping = if flat {
        Tonemapping::None
    } else {
        Tonemapping::default()
    };
    if **tonemapping != wanted_tonemapping {
        **tonemapping = wanted_tonemapping;
    }
    let wanted_dither = if flat {
        DebandDither::Disabled
    } else {
        DebandDither::Enabled
    };
    if **dither != wanted_dither {
        **dither = wanted_dither;
    }
}

fn soft_distance_fog(color: Color) -> DistanceFog {
    DistanceFog {
        color: color.with_alpha(FOG_MAX_ALPHA),
        falloff: FogFalloff::Linear {
            start: FOG_START_METERS,
            end: FOG_END_METERS,
        },
        ..default()
    }
}

/// The fog meets the same horizon color as `ClearColor`, so dawn, dusk and
/// night transition without a visible seam or a permanently blue veil.
fn update_distance_fog(
    time_of_day: Res<TimeOfDay>,
    mut fog: Single<&mut DistanceFog, With<CameraRig>>,
) {
    fog.color = atmosphere_color(time_of_day.hours).with_alpha(FOG_MAX_ALPHA);
}

/// Add a downward dip when the player lands (Fall → Walk/Sprint).
///
/// Intentionally scoped to `Player`, not `Actor`: the camera always follows
/// the one local human-controlled entity, which stays singular even once
/// other `Actor`-tagged entities (NPCs, remote players) exist — see
/// `docs/ARCHITECTURE.md`.
fn camera_landing_dip(
    player: Single<&LocomotionState, With<Player>>,
    mut rig: Single<&mut CameraRig>,
    mut prev: Local<Option<LocomotionState>>,
) {
    let current = **player;
    if let Some(old) = *prev
        && old == LocomotionState::Fall
        && (current == LocomotionState::Walk || current == LocomotionState::Sprint)
    {
        rig.current_dip += LANDING_DIP_INTENSITY;
    }
    *prev = Some(current);
}

type FollowFilter = (With<Player>, Without<CameraRig>);

/// While a scripted measurement runs, the camera holds the pose `perf` publishes
/// instead of following (a benchmark vantage or the flythrough's interpolated
/// pose). The camera reads the one reconciled seam and stays ignorant of which
/// run produced it, so new producers plug in without touching this (§2).
fn park_scripted_camera(
    pose: Res<crate::perf::ScriptedCameraPose>,
    mut cam: Single<&mut Transform, With<CameraRig>>,
) {
    let Some((position, facing)) = pose.0 else {
        return;
    };
    cam.translation = position;
    cam.look_to(facing, Vec3::Y);
}

/// Intentionally scoped to `Player`, not `Actor` — see `camera_landing_dip`.
#[allow(clippy::type_complexity)]
fn follow_player(
    player: Single<
        (
            Entity,
            &Transform,
            &ControlOrientation,
            Option<&bof_domain::combat::state::CombatState>,
            Option<&bof_domain::combat::state::DrawStrength>,
            Option<&bof_domain::movement::facing::FacingSource>,
        ),
        FollowFilter,
    >,
    // Lock-on target positions. `Without<CameraRig>` keeps this read disjoint
    // from the camera's `&mut Transform` below.
    targets: Query<&Transform, Without<CameraRig>>,
    mut cam: Single<(&mut Transform, &mut CameraRig, &mut Projection)>,
    spatial: SpatialQuery,
    time: Res<Time>,
    mut player_vis: Query<&mut Visibility, With<PlayerVisual>>,
) {
    let (player_entity, player_transform, orientation, combat_state, draw_strength, facing) =
        *player;
    let (cam_transform, rig, proj) = &mut *cam;
    let body = player_transform.translation;
    let dt = time.delta_secs();

    // Adjust perspective camera field-of-view dynamically to simulate focus and weight.
    if let Projection::Perspective(ref mut persp) = **proj {
        let default_fov = std::f32::consts::FRAC_PI_4; // ~45 deg half fov (90 deg full)
        let draw_factor = draw_strength.map_or(0.0, |d| d.factor);
        let target_fov = default_fov - (0.12 * rig.aim_blend) - (0.16 * draw_factor);
        persp.fov.smooth_nudge(&target_fov, FOV_LERP_PER_SEC, dt);
    }

    // Recover the landing dip, then smooth the pivot Y (handles stairs/steps).
    rig.current_dip.smooth_nudge(&0.0, LANDING_DIP_RECOVERY, dt);

    // Blend toward the aim camera while the bow is drawn.
    let aiming = matches!(
        combat_state,
        Some(bof_domain::combat::state::CombatState::Aiming)
    );
    let aim_target = if aiming { 1.0 } else { 0.0 };
    rig.aim_blend
        .smooth_nudge(&aim_target, AIM_BLEND_PER_SEC, dt);

    // Blend the pivot down to Combat's aim pivot while aiming, so the
    // crosshair ray coincides with the arrow's aim ray at full blend.
    let blended_lens_height = lerp(LENS_HEIGHT, AIM_PIVOT_HEIGHT, rig.aim_blend);

    // Dynamically cap lens height if there is a low ceiling directly above the player center.
    // This prevents the camera pivot from clipping inside low ceilings or stairs.
    let mut effective_lens_height = blended_lens_height;
    let up_dir = Dir3::Y;
    let filter = SpatialQueryFilter::from_excluded_entities([player_entity]);
    if let Some(hit) = spatial.cast_ray(body, up_dir, blended_lens_height, true, &filter) {
        effective_lens_height = (hit.distance - SPRING_MARGIN).max(0.2);
    }

    let target_y = body.y + effective_lens_height - rig.current_dip;
    if rig.smoothed_y.is_nan() {
        rig.smoothed_y = target_y;
    } else {
        rig.smoothed_y.smooth_nudge(&target_y, FOLLOW_LERP_Y, dt);
    }
    // Lock-on framing: when the player is locked on, ease the camera yaw onto
    // the direction to the target so it stays framed. `lock_yaw` is remembered
    // so releasing eases back out instead of snapping to wherever the mouse
    // drifted. Pitch stays player-controlled.
    let lock_target_pos = match facing {
        Some(bof_domain::movement::facing::FacingSource::LockOn(target)) => {
            targets.get(*target).ok().map(|t| t.translation)
        }
        _ => None,
    };
    if let Some(target_pos) = lock_target_pos {
        let to = target_pos - body;
        rig.lock_yaw = (-to.x).atan2(-to.z);
    }
    rig.lock_blend.smooth_nudge(
        &(if lock_target_pos.is_some() { 1.0 } else { 0.0 }),
        AIM_BLEND_PER_SEC,
        dt,
    );
    let free_rot =
        Quat::from_rotation_y(orientation.yaw) * Quat::from_rotation_x(orientation.pitch);
    let lock_rot = Quat::from_rotation_y(rig.lock_yaw) * Quat::from_rotation_x(orientation.pitch);
    let rot = free_rot.slerp(lock_rot, rig.lock_blend);
    let dir = rot * Vec3::Z;

    // Aim mode: shift the pivot over the right shoulder so the body doesn't
    // block the shot line.
    let shoulder = rot * Vec3::X * (AIM_SHOULDER_OFFSET * rig.aim_blend);
    let pivot = Vec3::new(body.x, rig.smoothed_y, body.z) + shoulder;

    // Spring arm: pull the camera in if the boom would clip world geometry.
    let base_length = lerp(SPRING_LENGTH, AIM_SPRING_LENGTH, rig.aim_blend);
    let mut length = base_length;
    if let Ok(boom_dir) = Dir3::new(dir)
        && let Some(hit) = spatial.cast_shape(
            &Collider::sphere(SPRING_PROBE_RADIUS),
            pivot,
            Quat::IDENTITY,
            boom_dir,
            &ShapeCastConfig::from_max_distance(base_length),
            &filter,
        )
    {
        length = (hit.distance - SPRING_MARGIN).clamp(0.0, base_length);
    }

    // Hide the player visual if the camera is zoomed too close to prevent rendering internal faces.
    if let Ok(mut vis) = player_vis.single_mut() {
        if length < FIRST_PERSON_THRESHOLD {
            *vis = Visibility::Hidden;
        } else {
            *vis = Visibility::Inherited;
        }
    }

    cam_transform.translation = pivot + dir * length;
    // Set camera rotation directly to ControlOrientation rot. This is mathematically
    // identical to look_at(pivot) but avoids NaN / freeze singularities when length == 0.0.
    cam_transform.rotation = rot;

    // Lock-on: point the camera straight at the target from its final position,
    // so the enemy sits under the screen-center crosshair. Yaw-only framing
    // leaves the target off-center — the boom sits behind, above, and offset
    // from the body, so `body -> target` is not `camera -> target` (parallax).
    if let Some(target_pos) = lock_target_pos
        && rig.lock_blend > 0.001
        && let Ok(look_dir) =
            Dir3::new(target_pos + Vec3::Y * LOCK_LOOK_HEIGHT - cam_transform.translation)
    {
        let look_rot = Transform::IDENTITY.looking_to(look_dir, Dir3::Y).rotation;
        cam_transform.rotation = rot.slerp(look_rot, rig.lock_blend);
    }
}

/// Runs after `follow_player` (which fully rewrites the camera transform each
/// frame, so the shake offset never accumulates): jitter position and roll by
/// `trauma²` using cheap incommensurate sines as noise.
fn apply_camera_shake(
    real: Res<Time<Real>>,
    mut shake: ResMut<CameraShake>,
    mut cam: Single<&mut Transform, With<CameraRig>>,
) {
    if shake.trauma <= 0.0 {
        return;
    }
    shake.trauma = (shake.trauma - SHAKE_DECAY_PER_SEC * real.delta_secs()).max(0.0);
    let strength = shake.trauma * shake.trauma;
    if strength <= f32::EPSILON {
        return;
    }
    let t = real.elapsed_secs();
    let offset = Vec3::new((t * 61.7).sin(), (t * 73.3).sin(), (t * 53.9).sin())
        * (SHAKE_MAX_OFFSET * strength);
    let roll = (t * 97.1).sin() * SHAKE_MAX_ROLL * strength;
    cam.translation += offset;
    cam.rotation *= Quat::from_rotation_z(roll);
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// El bug del 2026-08-22: la cámara abría a 3 m mirando al horizonte, o sea
    /// dentro del terreno. Se fija lo que importa —alto y mirando abajo— y que
    /// la pose coincida con el ángulo que `fly_freecam` va a reconstruir; si se
    /// separan, el primer movimiento del mouse tira el encuadre.
    #[test]
    fn the_authoring_camera_opens_above_the_map_looking_down() {
        let pose = authoring_pose();
        assert!(
            pose.translation.y > 20.0,
            "abrió a {} m: cualquier relieve la deja bajo tierra",
            pose.translation.y
        );
        let (_, pitch, _) = pose.rotation.to_euler(EulerRot::YXZ);
        assert!(
            pitch < 0.0,
            "la cámara de autoría tiene que mirar hacia abajo"
        );
        assert!(
            (pitch - data::AUTHORING_PITCH).abs() < 0.01,
            "la pose ({pitch}) y el ángulo sembrado ({}) se separaron",
            data::AUTHORING_PITCH
        );
    }

    /// Distant and gradual (2026-08-09, reverted): clear right next to the
    /// player, ramping past the camera's own view arc rather than resolving
    /// on top of any one LOD edge, and never a fully opaque wall.
    #[test]
    fn distance_fog_stays_clear_up_close_and_never_fully_opaque() {
        let fog = soft_distance_fog(Color::srgb(0.4, 0.6, 0.8));

        let FogFalloff::Linear { start, end } = fog.falloff else {
            panic!("soft atmosphere must use the predictable linear falloff");
        };
        assert!(start >= 40.0, "near gameplay must remain completely clear");
        assert!(
            end - start >= 100.0,
            "the ramp must stay gradual, not a wall"
        );
        assert!(
            fog.color.to_srgba().alpha < 1.0,
            "a veil, however strong, must stay translucent"
        );
    }
}
