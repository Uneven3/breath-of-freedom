//! Day/night cycle. World owns `TimeOfDay` (simulation state — Enemies and
//! StatusEffects will read it); the sun/ambient presentation reads it in
//! `Update` and never writes back (§20).

use bevy::prelude::*;

/// One full in-game day per this many real minutes (BotW pacing).
const REAL_MINUTES_PER_GAME_DAY: f32 = 24.0;

/// Sun tilt off the east-west arc, so shadows never collapse to a line.
const SUN_ARC_TILT: f32 = 0.35;

const SUN_NOON_LUX: f32 = 10_000.0;
const MOON_LUX: f32 = 30.0;
// Ambient stays low on purpose: it fills shadowed faces, and too much of it
// flattens the toon bands until every surface merges.
const AMBIENT_DAY: f32 = 90.0;
const AMBIENT_NIGHT: f32 = 20.0;

const SUN_NOON_COLOR: Color = Color::srgb(1.0, 0.98, 0.92);
const SUN_HORIZON_COLOR: Color = Color::srgb(1.0, 0.55, 0.25);
const MOON_COLOR: Color = Color::srgb(0.55, 0.65, 0.9);
const AMBIENT_DAY_COLOR: Color = Color::srgb(1.0, 1.0, 1.0);
const AMBIENT_NIGHT_COLOR: Color = Color::srgb(0.45, 0.55, 0.85);

// Stylized sky gradient (BotW-like, not physical): the camera clear color
// blends across night → horizon glow → day, following the sun.
const SKY_DAY: Color = Color::srgb(0.45, 0.68, 0.95);
const SKY_HORIZON: Color = Color::srgb(0.95, 0.55, 0.3);
const SKY_NIGHT: Color = Color::srgb(0.04, 0.05, 0.12);

/// How far out the visible sun/moon discs orbit (inside the default far
/// plane, far outside the 100 m course).
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

/// Unit vector pointing *toward* the sun: east horizon at 06:00, zenith at
/// 12:00, west horizon at 18:00, below the horizon at night.
pub(crate) fn sun_direction(hours: f32) -> Vec3 {
    let angle = (hours / 24.0) * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
    Vec3::new(angle.cos(), angle.sin(), SUN_ARC_TILT).normalize()
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
            (With<Sun>, Without<SunDisc>, Without<MoonDisc>),
        >,
    >,
    mut discs: ParamSet<(
        Option<Single<(&mut Transform, &mut Visibility), With<SunDisc>>>,
        Option<Single<(&mut Transform, &mut Visibility), With<MoonDisc>>>,
    )>,
    mut ambient: ResMut<GlobalAmbientLight>,
    mut sky: ResMut<ClearColor>,
) {
    let Some(sun) = sun else {
        return;
    };
    let (mut transform, mut light) = sun.into_inner();

    let to_sun = sun_direction(tod.hours);
    let to_moon = Vec3::new(-to_sun.x, -to_sun.y, SUN_ARC_TILT).normalize();
    let elevation = to_sun.y;

    if elevation >= 0.0 {
        // Daytime: intensity and warmth follow the elevation; near the
        // horizon the light is dim and orange, at noon bright and white.
        let strength = elevation.clamp(0.0, 1.0).powf(0.6);
        transform.look_to(-to_sun, Vec3::Y);
        light.illuminance = MOON_LUX + (SUN_NOON_LUX - MOON_LUX) * strength;
        light.color = SUN_HORIZON_COLOR.mix(&SUN_NOON_COLOR, strength);
    } else {
        // Night: the same light acts as the moon, opposite the sun's arc.
        transform.look_to(-to_moon, Vec3::Y);
        light.illuminance = MOON_LUX;
        light.color = MOON_COLOR;
    }

    let daylight = elevation.clamp(0.0, 1.0).powf(0.6);
    ambient.brightness = AMBIENT_NIGHT + (AMBIENT_DAY - AMBIENT_NIGHT) * daylight;
    ambient.color = AMBIENT_NIGHT_COLOR.mix(&AMBIENT_DAY_COLOR, daylight);

    // Stylized sky: night → horizon glow → day, driven by sun elevation.
    // The glow window is ±0.25 of elevation around the horizon.
    let horizon_glow = (1.0 - (elevation.abs() / 0.25).min(1.0)).powi(2);
    let base = SKY_NIGHT.mix(&SKY_DAY, daylight);
    sky.0 = base.mix(&SKY_HORIZON, horizon_glow * 0.85);

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
}
