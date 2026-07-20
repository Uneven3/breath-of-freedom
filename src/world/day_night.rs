//! Day/night cycle. World owns `TimeOfDay` (simulation state — Enemies and
//! StatusEffects will read it); the sun/ambient presentation reads it in
//! `Update` and never writes back (§20).

use bevy::prelude::*;

/// One full in-game day per this many real minutes (BotW pacing).
const REAL_MINUTES_PER_GAME_DAY: f32 = 24.0;

/// Sun tilt off the east-west arc, so shadows never collapse to a line.
const SUN_ARC_TILT: f32 = 0.35;

const SUN_NOON_LUX: f32 = 10_000.0;
// Stylized rather than physical moonlight: night must remain navigable without
// flattening the much stronger daylight bands.
const MOON_LUX: f32 = 400.0;
// Ambient stays low on purpose: it fills shadowed faces, and too much of it
// flattens the toon bands until every surface merges.
const AMBIENT_DAY: f32 = 90.0;
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
const DISC_ORBIT_RADIUS: f32 = 420.0;

/// Simulation clock: `hours` in `0.0..24.0`, advanced on the fixed step.
/// `speed` is a debug affordance (F9 fast-forward); 1.0 in normal play.
#[derive(Resource)]
pub struct TimeOfDay {
    pub hours: f32,
    pub speed: f32,
}

impl Default for TimeOfDay {
    fn default() -> Self {
        Self {
            hours: 9.0,
            speed: 1.0,
        }
    }
}

/// Marker for the directional light the cycle drives.
#[derive(Component)]
pub struct Sun;

/// Presentation-only directional moonlight. Keeping it separate from the sun
/// lets both fade across the horizon without rotating one light by 180 degrees.
#[derive(Component)]
pub(super) struct MoonLight;

/// Marker for the visible sun disc (unlit sphere on the sun's arc).
#[derive(Component)]
pub struct SunDisc;

/// Marker for the visible moon disc, opposite the sun.
#[derive(Component)]
pub struct MoonDisc;

pub(super) fn advance_time(time: Res<Time>, mut tod: ResMut<TimeOfDay>) {
    let game_hours_per_real_second = 24.0 / (REAL_MINUTES_PER_GAME_DAY * 60.0);
    tod.hours =
        (tod.hours + time.delta_secs() * game_hours_per_real_second * tod.speed).rem_euclid(24.0);
}

pub(super) fn setup_moon_light(mut commands: Commands) {
    commands.spawn((
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
    mut discs: ParamSet<(
        Option<Single<(&mut Transform, &mut Visibility), With<SunDisc>>>,
        Option<Single<(&mut Transform, &mut Visibility), With<MoonDisc>>>,
    )>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sky: ResMut<ClearColor>,
) {
    let to_sun = sun_direction(tod.hours);
    let to_moon = Vec3::new(-to_sun.x, -to_sun.y, SUN_ARC_TILT).normalize();
    let elevation = to_sun.y;
    let palette = lighting_palette(tod.hours, elevation);

    if let Some(sun) = sun {
        let (mut transform, mut light) = sun.into_inner();
        transform.look_to(-to_sun, Vec3::Y);
        light.illuminance = palette.sun_illuminance;
        light.color = palette.sun_color;
    }
    if let Some(moon) = moon {
        let (mut transform, mut light) = moon.into_inner();
        transform.look_to(-to_moon, Vec3::Y);
        light.illuminance = palette.moon_illuminance;
        light.color = MOON_COLOR;
    }

    ambient.brightness = palette.ambient_brightness;
    ambient.color = palette.ambient_color;
    sky.0 = palette.sky_color;

    // Visible discs ride their arcs; each hides below the horizon.
    if let Some(disc) = discs.p0().as_mut() {
        let (transform, visibility) = &mut **disc;
        transform.translation = to_sun * DISC_ORBIT_RADIUS;
        **visibility = if elevation > -0.05 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
    if let Some(disc) = discs.p1().as_mut() {
        let (transform, visibility) = &mut **disc;
        transform.translation = to_moon * DISC_ORBIT_RADIUS;
        **visibility = if to_moon.y > -0.05 {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::system::RunSystemOnce;

    #[test]
    fn sun_hits_the_cardinal_hours() {
        assert!(sun_direction(6.0).y.abs() < 1e-4, "06:00 is sunrise");
        assert!(sun_direction(12.0).y > 0.9, "12:00 is near zenith");
        assert!(sun_direction(18.0).y.abs() < 1e-4, "18:00 is sunset");
        assert!(sun_direction(0.0).y < -0.9, "00:00 is deep night");
    }

    #[test]
    fn time_advances_and_wraps_at_midnight() {
        let mut world = World::new();
        world.init_resource::<Time>();
        world
            .resource_mut::<Time>()
            .advance_by(std::time::Duration::from_secs_f32(1.0));
        world.insert_resource(TimeOfDay {
            hours: 23.99,
            speed: 1.0,
        });

        world.run_system_once(advance_time).unwrap();

        let tod = world.resource::<TimeOfDay>();
        let expected_step = 24.0 / (REAL_MINUTES_PER_GAME_DAY * 60.0);
        assert!(tod.hours < expected_step + 1e-4, "must wrap past 24:00");

        // Debug fast-forward multiplies the same step.
        world.insert_resource(TimeOfDay {
            hours: 0.0,
            speed: 60.0,
        });
        world.run_system_once(advance_time).unwrap();
        let tod = world.resource::<TimeOfDay>();
        assert!((tod.hours - expected_step * 60.0).abs() < 1e-4);
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
