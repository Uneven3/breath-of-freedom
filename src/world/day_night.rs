//! Day/night cycle. World owns `TimeOfDay` (simulation state — Enemies and
//! StatusEffects will read it); the sun/ambient presentation reads it in
//! `Update` and never writes back (§20).

use bevy::light::{CascadeShadowConfig, CascadeShadowConfigBuilder, DirectionalLightShadowMap};
use bevy::prelude::*;

pub use bof_simulation::world::day_night::{TimeOfDay, TimeOfDayRequest};

use crate::perf::PerfToggles;

/// Below this illuminance a directional light contributes nothing visible, so
/// its cascades are pure waste. Bevy gates shadow-map rendering on
/// `shadow_maps_enabled` alone — never on illuminance — so without this the
/// moon renders four full cascade passes over the whole forest at noon (and
/// the sun does the same at midnight).
const SHADOW_CASTING_LUX: f32 = 1.0;

fn casts_shadows(illuminance: f32) -> bool {
    illuminance >= SHADOW_CASTING_LUX
}

/// Pick one directional shadow owner. Around the horizon both stylized lights
/// intentionally overlap, but rendering two cascade sets there doubles the
/// largest shadow cost and produces competing shadow directions.
fn shadow_casters(palette: &LightingPalette) -> (bool, bool) {
    if palette.sun_illuminance >= palette.moon_illuminance {
        (casts_shadows(palette.sun_illuminance), false)
    } else {
        (false, casts_shadows(palette.moon_illuminance))
    }
}

/// Sun tilt off the east-west arc, so shadows never collapse to a line.
const SUN_ARC_TILT: f32 = 0.35;

/// Visible past this module: `visuals::grass` normalizes the sun's live
/// illuminance against this same reference instead of inventing its own scale.
pub(crate) const SUN_NOON_LUX: f32 = 10_000.0;
// Stylized rather than physical moonlight: night must remain navigable without
// flattening the much stronger daylight bands.
const MOON_LUX: f32 = 400.0;
// Ambient stays low on purpose: it fills shadowed faces, and too much of it
// flattens the form and material response until every surface merges.
pub(crate) const AMBIENT_DAY: f32 = 90.0;
const AMBIENT_NIGHT: f32 = 40.0;

const SUN_NOON_COLOR: Color = Color::srgb(1.0, 0.98, 0.92);
const SUN_DAWN_COLOR: Color = Color::srgb(1.0, 0.68, 0.42);
const SUN_DUSK_COLOR: Color = Color::srgb(1.0, 0.38, 0.2);
const MOON_COLOR: Color = Color::srgb(0.55, 0.65, 0.9);
const AMBIENT_DAY_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const AMBIENT_NIGHT_COLOR: Color = Color::srgb(0.38, 0.48, 0.78);
const AMBIENT_DAWN_COLOR: Color = Color::srgb(1.0, 0.65, 0.52);
const AMBIENT_DUSK_COLOR: Color = Color::srgb(0.9, 0.42, 0.52);

// Stylized sky gradient (BotW-like, not physical). Dawn leans coral and dusk
// leans magenta so the two horizon transitions have different identities.
const SKY_DAY: Color = Color::srgb(0.45, 0.68, 0.95);
const SKY_DAWN: Color = Color::srgb(0.95, 0.42, 0.38);
const SKY_DUSK: Color = Color::srgb(0.72, 0.2, 0.42);
const SKY_NIGHT: Color = Color::srgb(0.055, 0.075, 0.17);

const DAWN_START: f32 = 4.5;
const DAWN_PEAK: f32 = 6.0;
const DAWN_END: f32 = 7.75;
const DUSK_START: f32 = 16.25;
const DUSK_PEAK: f32 = 18.0;
const DUSK_END: f32 = 20.0;

/// How far out the visible sun/moon discs orbit (inside the default far
/// plane, outside the playable terrain).
///
/// The orbit is centred on the *camera*, not the world origin. A sun has to
/// behave as if at infinity: its apparent direction must not change as you
/// walk. Orbiting the origin gave it parallax — crossing 112 m of a 320 m map
/// swung the disc by ~14 degrees and pulled it from 420 m to ~308 m, so it
/// drifted and grew as you moved. On a Zelda-sized map that would be grotesque.
const DISC_ORBIT_RADIUS: f32 = 420.0;

/// Marker for the directional light the cycle drives.
#[derive(Component)]
pub struct Sun;

/// Presentation-only directional moonlight. Keeping it separate from the sun
/// lets both fade across the horizon without rotating one light by 180 degrees.
///
/// `pub(crate)`, not `pub(super)`: both this and `Sun` carry `DirectionalLight`
/// and spawn together in every scene (`world::mod`), so any other module that
/// queries by `DirectionalLight` alone needs this to disambiguate — see the
/// `Without<MoonLight>` fix in `visuals::grass::track_meadow_focus`.
#[derive(Component)]
pub(crate) struct MoonLight;

/// Marker for the visible sun disc (unlit sphere on the sun's arc).
#[derive(Component)]
pub struct SunDisc;

/// Marker for the visible moon disc, opposite the sun.
#[derive(Component)]
pub struct MoonDisc;

pub(super) fn setup_moon_light(mut commands: Commands, state: Res<State<crate::scene::AppState>>) {
    commands.spawn((
        DespawnOnExit(*state.get()),
        Name::new("MoonLight"),
        MoonLight,
        DirectionalLight {
            color: MOON_COLOR,
            illuminance: MOON_LUX,
            shadow_maps_enabled: true,
            ..default()
        },
        Transform::default(),
    ));
}

/// Unit vector pointing *toward* the sun: east horizon at 06:00, zenith at
/// 12:00, west horizon at 18:00, below the horizon at night.
pub(crate) fn sun_direction(hours: f32) -> Vec3 {
    let angle = (hours / 24.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    Vec3::new(angle.cos(), angle.sin(), SUN_ARC_TILT).normalize()
}

#[derive(Clone, Copy)]
struct LightingPalette {
    sun_illuminance: f32,
    sun_color: Color,
    moon_illuminance: f32,
    ambient_brightness: f32,
    ambient_color: Color,
    sky_color: Color,
}

fn smoothstep(start: f32, end: f32, value: f32) -> f32 {
    let t = ((value - start) / (end - start)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn phase_weight(hours: f32, start: f32, peak: f32, end: f32) -> f32 {
    let hours = hours.rem_euclid(24.0);
    if hours <= peak {
        smoothstep(start, peak, hours)
    } else {
        1.0 - smoothstep(peak, end, hours)
    }
}

fn lighting_palette(hours: f32, elevation: f32) -> LightingPalette {
    let hours = hours.rem_euclid(24.0);
    let sun_strength = smoothstep(0.0, 1.0, elevation).powf(0.6);
    let dawn = phase_weight(hours, DAWN_START, DAWN_PEAK, DAWN_END);
    let dusk = phase_weight(hours, DUSK_START, DUSK_PEAK, DUSK_END);
    let sun_visibility = smoothstep(-0.08, 0.08, elevation);
    let moon_visibility = smoothstep(-0.08, 0.08, -elevation);

    // Ambient and sky start changing before the sun crosses the horizon. This
    // avoids an abrupt black-to-day transition while the directional light
    // continues to follow the actual sun/moon arc.
    let daylight = smoothstep(-0.08, 0.25, elevation);
    let horizon_color = if hours < 12.0 {
        SUN_DAWN_COLOR
    } else {
        SUN_DUSK_COLOR
    };
    let sun_color = horizon_color.mix(&SUN_NOON_COLOR, sun_strength);
    let sun_illuminance = sun_visibility * (MOON_LUX + (SUN_NOON_LUX - MOON_LUX) * sun_strength);
    let moon_illuminance = moon_visibility * MOON_LUX;

    let mut ambient_color = AMBIENT_NIGHT_COLOR.mix(&AMBIENT_DAY_COLOR, daylight);
    ambient_color = ambient_color.mix(&AMBIENT_DAWN_COLOR, dawn * 0.35);
    ambient_color = ambient_color.mix(&AMBIENT_DUSK_COLOR, dusk * 0.4);

    let mut sky_color = SKY_NIGHT.mix(&SKY_DAY, daylight);
    sky_color = sky_color.mix(&SKY_DAWN, dawn * 0.9);
    sky_color = sky_color.mix(&SKY_DUSK, dusk * 0.9);

    LightingPalette {
        sun_illuminance,
        sun_color,
        moon_illuminance,
        ambient_brightness: AMBIENT_NIGHT + (AMBIENT_DAY - AMBIENT_NIGHT) * daylight,
        ambient_color,
        sky_color,
    }
}

/// Environment color at a given time. Camera-owned effects read this instead
/// of duplicating the day/night palette or reaching into world presentation
/// entities.
pub(crate) fn atmosphere_color(hours: f32) -> Color {
    lighting_palette(hours, sun_direction(hours).y).sky_color
}

/// Presentation: place and tint the sun (or moon, when the sun is below the
/// horizon), blend the ambient light, paint the stylized sky, and move the
/// visible discs. Reads `TimeOfDay`, writes only what it owns.
#[allow(clippy::type_complexity)]
pub(super) fn apply_sun(
    tod: Res<TimeOfDay>,
    sun: Option<
        Single<
            (&mut Transform, &mut DirectionalLight),
            (
                With<Sun>,
                Without<MoonLight>,
                Without<SunDisc>,
                Without<MoonDisc>,
            ),
        >,
    >,
    moon: Option<
        Single<
            (&mut Transform, &mut DirectionalLight),
            (
                With<MoonLight>,
                Without<Sun>,
                Without<SunDisc>,
                Without<MoonDisc>,
            ),
        >,
    >,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sky: ResMut<ClearColor>,
    perf: Res<PerfToggles>,
) {
    let to_sun = sun_direction(tod.hours);
    let to_moon = Vec3::new(-to_sun.x, -to_sun.y, SUN_ARC_TILT).normalize();
    let elevation = to_sun.y;
    let palette = lighting_palette(tod.hours, elevation);
    let (sun_casts_shadows, moon_casts_shadows) = shadow_casters(&palette);

    if let Some(sun) = sun {
        let (mut transform, mut light) = sun.into_inner();
        transform.look_to(-to_sun, Vec3::Y);
        light.illuminance = palette.sun_illuminance;
        light.color = palette.sun_color;
        light.shadow_maps_enabled = perf.sun_shadows && sun_casts_shadows;
    }
    if let Some(moon) = moon {
        let (mut transform, mut light) = moon.into_inner();
        transform.look_to(-to_moon, Vec3::Y);
        light.illuminance = palette.moon_illuminance;
        light.color = MOON_COLOR;
        light.shadow_maps_enabled = perf.moon_shadows && moon_casts_shadows;
    }

    ambient.brightness = palette.ambient_brightness;
    ambient.color = palette.ambient_color;
    sky.0 = palette.sky_color;
}

/// Rides the visible discs around the viewer and hides each below the horizon.
///
/// Separate from [`apply_sun`] because it answers a different question: that
/// one decides what the light *is*, this one decides where the body appears.
#[allow(clippy::type_complexity)]
pub(super) fn place_sky_discs(
    tod: Res<TimeOfDay>,
    camera: Option<Single<&GlobalTransform, With<Camera3d>>>,
    mut discs: ParamSet<(
        Option<Single<(&mut Transform, &mut Visibility), With<SunDisc>>>,
        Option<Single<(&mut Transform, &mut Visibility), With<MoonDisc>>>,
    )>,
) {
    let to_sun = sun_direction(tod.hours);
    let to_moon = Vec3::new(-to_sun.x, -to_sun.y, SUN_ARC_TILT).normalize();
    let eye = camera
        .map(|camera| camera.translation())
        .unwrap_or(Vec3::ZERO);

    if let Some(disc) = discs.p0().as_mut() {
        let (transform, visibility) = &mut **disc;
        transform.translation = eye + to_sun * DISC_ORBIT_RADIUS;
        **visibility = if to_sun.y > -0.05 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(disc) = discs.p1().as_mut() {
        let (transform, visibility) = &mut **disc;
        transform.translation = eye + to_moon * DISC_ORBIT_RADIUS;
        **visibility = if to_moon.y > -0.05 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

/// Shadow cost inside dense foliage is fill-bound, so the map edge is the most
/// direct lever there is on it — halving it quarters the texels rendered.
pub(super) fn apply_shadow_map_size(
    perf: Res<PerfToggles>,
    mut shadow_map: ResMut<DirectionalLightShadowMap>,
) {
    let wanted = perf.shadow_map_size();
    if shadow_map.size != wanted {
        shadow_map.size = wanted;
        info!("[world] shadow map: {wanted}px");
    }
}

type DirectionalLightFilter = Or<(With<Sun>, With<MoonLight>)>;

/// Env var that sets the cascade count for an attribution run, e.g.
/// `BOF_CASCADES=2 cargo run --release`.
const CASCADE_ENV: &str = "BOF_CASCADES";

/// Reads an explicit override or the launch profile once at startup.
/// Deliberately *not* a runtime knob:
/// changing the count while the app runs desynchronises Bevy's per-cascade
/// visibility bookkeeping (`check_dir_light_mesh_visibility` sizes its queues
/// from the frusta) and panics with an out-of-bounds index. A debug affordance
/// that can crash the game is worse than no affordance (§9), so this costs one
/// relaunch per value instead.
fn configured_cascades(profile: crate::perf::PerfProfile) -> (usize, &'static str) {
    match std::env::var(CASCADE_ENV) {
        Ok(raw) => match parse_cascade_count(&raw) {
            Ok(count) => (count, CASCADE_ENV),
            Err(expected) => {
                warn!("[world] ignoring {CASCADE_ENV}={raw}: expected {expected}");
                (profile.cascade_count(), "launch profile")
            }
        },
        Err(std::env::VarError::NotPresent) => (profile.cascade_count(), "launch profile"),
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("[world] ignoring non-Unicode {CASCADE_ENV}: expected 1..=4");
            (profile.cascade_count(), "launch profile")
        }
    }
}

fn parse_cascade_count(raw: &str) -> Result<usize, &'static str> {
    match raw.parse::<usize>() {
        Ok(count @ 1..=4) => Ok(count),
        Ok(_) | Err(_) => Err("an integer from 1 to 4"),
    }
}

/// Applies the cascade configuration. The count is fixed at launch; the far
/// distance is a live dial, because how close the shadowed disc ends is
/// something you judge by looking, not by reading a table.
///
/// Rebuilding on distance change is safe — the cascade *count* is what Bevy's
/// per-cascade visibility bookkeeping is sized from, and that never moves here.
pub(super) fn apply_cascade_config(
    mut commands: Commands,
    perf: Res<PerfToggles>,
    lights: Query<Entity, DirectionalLightFilter>,
    mut applied: Local<Option<f32>>,
) {
    let distance = perf.shadow_distance();
    if *applied == Some(distance) || lights.is_empty() {
        return;
    }
    let first_run = applied.is_none();
    *applied = Some(distance);
    let (count, cascade_source) = configured_cascades(perf.profile);
    // Bevy defaults to 150 m, which spends cascade texels on ground no tree
    // casts onto any more (`visuals::foliage` budgets casters by distance).
    // Shrinking the covered area concentrates the same shadow map on what is
    // actually near, so this buys resolution as well as time.
    let config: CascadeShadowConfig = CascadeShadowConfigBuilder {
        num_cascades: count,
        maximum_distance: distance,
        ..default()
    }
    .into();
    for light in &lights {
        commands.entity(light).try_insert(config.clone());
    }
    if first_run {
        info!("[world] directional shadow cascades: {count} ({cascade_source})");
    }
    info!("[world] shadow distance: {distance:.0}m");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cascade_parser_rejects_malformed_and_out_of_range_values() {
        assert_eq!(parse_cascade_count("2"), Ok(2));
        assert_eq!(parse_cascade_count("0"), Err("an integer from 1 to 4"));
        assert_eq!(parse_cascade_count("five"), Err("an integer from 1 to 4"));
    }

    /// The stylized horizon blend makes both lights visible for a short time,
    /// but only the dominant one may own cascades. This sweep protects the
    /// transition, not just the obvious noon and midnight samples.
    #[test]
    fn at_most_one_directional_light_casts_shadows_throughout_the_day() {
        for minute in 0..24 * 60 {
            let hours = minute as f32 / 60.0;
            let palette = lighting_palette(hours, sun_direction(hours).y);
            let (sun, moon) = shadow_casters(&palette);
            assert!(!(sun && moon), "two shadow owners at {hours:05.2}h");
        }

        for (hours, sun_expected, moon_expected) in [(12.0_f32, true, false), (0.0, false, true)] {
            let palette = lighting_palette(hours, sun_direction(hours).y);
            let (sun, moon) = shadow_casters(&palette);
            assert_eq!(sun, sun_expected, "sun at {hours:05.2}h");
            assert_eq!(moon, moon_expected, "moon at {hours:05.2}h");
        }
    }

    /// Both knobs must be able to force shadows off regardless of the clock,
    /// otherwise the A/B in `docs/AHORA.md` cannot isolate shadow cost.
    #[test]
    fn the_benchmark_knob_can_veto_a_lit_light() {
        let noon = lighting_palette(12.0, sun_direction(12.0).y);
        let midnight = lighting_palette(0.0, sun_direction(0.0).y);
        let veto = PerfToggles {
            sun_shadows: false,
            moon_shadows: false,
            ..default()
        };
        let (sun_casts_shadows, _) = shadow_casters(&noon);
        let (_, moon_casts_shadows) = shadow_casters(&midnight);
        assert!(sun_casts_shadows);
        assert!(moon_casts_shadows);
        assert!(!(veto.sun_shadows && sun_casts_shadows));
        assert!(!(veto.moon_shadows && moon_casts_shadows));
    }

    #[test]
    fn sun_hits_the_cardinal_hours() {
        assert!(sun_direction(6.0).y.abs() < 1e-4, "06:00 is sunrise");
        assert!(sun_direction(12.0).y > 0.9, "12:00 is near zenith");
        assert!(sun_direction(18.0).y.abs() < 1e-4, "18:00 is sunset");
        assert!(sun_direction(0.0).y < -0.9, "00:00 is deep night");
    }

    #[test]
    fn twilight_phase_weights_are_smooth_and_distinct() {
        assert_eq!(
            phase_weight(DAWN_START, DAWN_START, DAWN_PEAK, DAWN_END),
            0.0
        );
        assert_eq!(
            phase_weight(DAWN_PEAK, DAWN_START, DAWN_PEAK, DAWN_END),
            1.0
        );
        assert_eq!(phase_weight(DAWN_END, DAWN_START, DAWN_PEAK, DAWN_END), 0.0);
        assert_eq!(
            phase_weight(DUSK_PEAK, DUSK_START, DUSK_PEAK, DUSK_END),
            1.0
        );

        let dawn = lighting_palette(DAWN_PEAK, sun_direction(DAWN_PEAK).y)
            .sky_color
            .to_srgba();
        let dusk = lighting_palette(DUSK_PEAK, sun_direction(DUSK_PEAK).y)
            .sky_color
            .to_srgba();
        assert!(dawn.green > dusk.green, "dawn must read warmer/coral");
        assert!(dusk.blue > dawn.blue, "dusk must read more magenta");
    }

    #[test]
    fn deep_night_keeps_readable_cool_light() {
        let midnight = lighting_palette(0.0, sun_direction(0.0).y);
        let moon = MOON_COLOR.to_srgba();
        let sky = midnight.sky_color.to_srgba();

        assert_eq!(midnight.sun_illuminance, 0.0);
        assert_eq!(midnight.moon_illuminance, MOON_LUX);
        assert!(midnight.ambient_brightness >= AMBIENT_NIGHT);
        assert!(moon.blue > moon.red, "moonlight must stay cool");
        assert!(sky.blue > sky.red, "night sky must stay cool");
    }

    #[test]
    fn palette_is_continuous_at_horizons_and_midnight() {
        for hour in [6.0_f32, 18.0, 24.0] {
            let before_hour = hour - 0.001;
            let after_hour = (hour + 0.001).rem_euclid(24.0);
            let before = lighting_palette(before_hour, sun_direction(before_hour).y);
            let after = lighting_palette(after_hour, sun_direction(after_hour).y);
            let before_directional = before.sun_illuminance + before.moon_illuminance;
            let after_directional = after.sun_illuminance + after.moon_illuminance;

            assert!((before_directional - after_directional).abs() < 5.0);
            assert!((before.ambient_brightness - after.ambient_brightness).abs() < 0.1);
            assert!(color_distance(before.sky_color, after.sky_color) < 0.01);
        }
    }

    fn color_distance(a: Color, b: Color) -> f32 {
        let a = a.to_srgba();
        let b = b.to_srgba();
        Vec3::new(a.red - b.red, a.green - b.green, a.blue - b.blue).length()
    }
}
