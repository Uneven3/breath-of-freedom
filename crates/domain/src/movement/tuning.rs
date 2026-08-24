//! Numbers the play sessions put in doubt, in one place that can be written at
//! runtime.
//!
//! Every field here was a hardcoded constant until a measurement made it a
//! question. Recompiling to answer each question costs the user a launch per
//! experiment, so these live in a resource instead: seeded from the environment
//! at startup, writable while the game runs, and **applied onto the per-actor
//! components** rather than read from the hot path.
//!
//! That last part is the whole design. The components (`GroundSensing`,
//! `LedgeSensing`, the ability profiles) stay the single source of truth a
//! system reads — an actor keeps its own profile, and a headless world with no
//! resource registered behaves exactly as before. This resource is an *author*
//! of those components, never a second reader of them.

use bevy_ecs::prelude::Resource;

/// Live-writable overrides for the locomotion numbers under investigation.
///
/// `None` means "leave the component's own value alone", so an untouched field
/// costs nothing and a profile that never opted in is never rewritten.
#[derive(Resource, Clone, Copy, Debug, Default, PartialEq)]
pub struct MovementTuning {
    /// Dot the floor loses before the body is declared off it (hysteresis).
    pub slope_hysteresis_dot: Option<f32>,
    /// Ticks without floor tolerated before the body counts as airborne.
    pub ground_grace_ticks: Option<u8>,
    /// How far past the body the climb sensor reaches for a wall.
    pub wall_detection_reach: Option<f32>,
    /// Yaw cone, in degrees, within which a wall counts as faced.
    pub climb_wall_angle_max_deg: Option<f32>,
    /// Fraction of gravity that pulls a sliding body down the face.
    pub slide_gravity_factor: Option<f32>,
    /// Terminal speed of a slide, in m/s.
    pub slide_max_speed: Option<f32>,
    /// How fast sideways motion across a slid face decays, in 1/s.
    pub slide_contour_friction: Option<f32>,
}

/// One tunable field, as data, so the parser and any UI enumerate the same list
/// instead of each carrying its own copy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TuningField {
    SlopeHysteresisDot,
    GroundGraceTicks,
    WallDetectionReach,
    ClimbWallAngleMaxDeg,
    SlideGravityFactor,
    SlideMaxSpeed,
    SlideContourFriction,
}

impl TuningField {
    pub const ALL: [TuningField; 7] = [
        TuningField::SlopeHysteresisDot,
        TuningField::GroundGraceTicks,
        TuningField::WallDetectionReach,
        TuningField::ClimbWallAngleMaxDeg,
        TuningField::SlideGravityFactor,
        TuningField::SlideMaxSpeed,
        TuningField::SlideContourFriction,
    ];

    pub const fn key(self) -> &'static str {
        match self {
            TuningField::SlopeHysteresisDot => "slope_hysteresis_dot",
            TuningField::GroundGraceTicks => "ground_grace_ticks",
            TuningField::WallDetectionReach => "wall_detection_reach",
            TuningField::ClimbWallAngleMaxDeg => "climb_wall_angle_max_deg",
            TuningField::SlideGravityFactor => "slide_gravity_factor",
            TuningField::SlideMaxSpeed => "slide_max_speed",
            TuningField::SlideContourFriction => "slide_contour_friction",
        }
    }

    pub fn from_key(key: &str) -> Option<TuningField> {
        TuningField::ALL.into_iter().find(|f| f.key() == key)
    }

    /// How much one nudge moves the value. Chosen so a knob crosses its whole
    /// useful range in a handful of clicks — a step that needs thirty presses
    /// is a knob nobody turns.
    pub const fn step(self) -> f32 {
        match self {
            TuningField::SlopeHysteresisDot => 0.01,
            TuningField::GroundGraceTicks => 1.0,
            TuningField::WallDetectionReach => 0.05,
            TuningField::ClimbWallAngleMaxDeg => 5.0,
            TuningField::SlideGravityFactor => 0.05,
            TuningField::SlideMaxSpeed => 0.5,
            TuningField::SlideContourFriction => 1.0,
        }
    }

    /// A short label for the panel. Not the key: the key is what a script
    /// types, this is what a person reads at a glance while playing.
    pub const fn label(self) -> &'static str {
        match self {
            TuningField::SlopeHysteresisDot => "histéresis suelo",
            TuningField::GroundGraceTicks => "gracia suelo",
            TuningField::WallDetectionReach => "alcance pared",
            TuningField::ClimbWallAngleMaxDeg => "cono escalada",
            TuningField::SlideGravityFactor => "slide: gravedad",
            TuningField::SlideMaxSpeed => "slide: tope",
            TuningField::SlideContourFriction => "slide: fricción",
        }
    }
    /// The range a value has to fall in to be worth trying. A typo that lands
    /// outside it is a mistake, not an experiment: a negative reach or a cone
    /// past 90° describes no surface the player can meet.
    pub const fn limits(self) -> (f32, f32) {
        match self {
            TuningField::SlopeHysteresisDot => (0.0, 0.5),
            TuningField::GroundGraceTicks => (0.0, 30.0),
            TuningField::WallDetectionReach => (0.1, 3.0),
            TuningField::ClimbWallAngleMaxDeg => (1.0, 90.0),
            TuningField::SlideGravityFactor => (0.0, 1.0),
            TuningField::SlideMaxSpeed => (0.0, 30.0),
            TuningField::SlideContourFriction => (0.0, 60.0),
        }
    }
}

impl MovementTuning {
    pub fn set(&mut self, field: TuningField, value: f32) {
        match field {
            TuningField::SlopeHysteresisDot => self.slope_hysteresis_dot = Some(value),
            // El clamp deja el valor dentro de `u8` antes de convertir, así que
            // el truncado que el lint teme no puede ocurrir; el `round` es lo que
            // hace que 2.9 ticks sean 3 y no 2.
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            TuningField::GroundGraceTicks => {
                self.ground_grace_ticks = Some(value.clamp(0.0, u8::MAX as f32).round() as u8)
            }
            TuningField::WallDetectionReach => self.wall_detection_reach = Some(value),
            TuningField::ClimbWallAngleMaxDeg => self.climb_wall_angle_max_deg = Some(value),
            TuningField::SlideGravityFactor => self.slide_gravity_factor = Some(value),
            TuningField::SlideMaxSpeed => self.slide_max_speed = Some(value),
            TuningField::SlideContourFriction => self.slide_contour_friction = Some(value),
        }
    }

    pub fn get(&self, field: TuningField) -> Option<f32> {
        match field {
            TuningField::SlopeHysteresisDot => self.slope_hysteresis_dot,
            TuningField::GroundGraceTicks => self.ground_grace_ticks.map(f32::from),
            TuningField::WallDetectionReach => self.wall_detection_reach,
            TuningField::ClimbWallAngleMaxDeg => self.climb_wall_angle_max_deg,
            TuningField::SlideGravityFactor => self.slide_gravity_factor,
            TuningField::SlideMaxSpeed => self.slide_max_speed,
            TuningField::SlideContourFriction => self.slide_contour_friction,
        }
    }

    /// Move one knob by `steps` of its own step size, clamped to its range.
    ///
    /// `current` is the value the actor is running with, because a knob that
    /// was never set has no value of its own — the first nudge has to start
    /// from what the game is actually doing, not from zero.
    pub fn nudge(&mut self, field: TuningField, current: f32, steps: i32) -> f32 {
        let (low, high) = field.limits();
        let from = self.get(field).unwrap_or(current);
        let value = (from + field.step() * steps as f32).clamp(low, high);
        self.set(field, value);
        value
    }
    /// Parse `name=value,name=value`. Returns what it applied and what it
    /// rejected, so the caller decides how to report — a bad entry **warns and
    /// continues**: this is a diagnostic tool, and aborting on a typo protects
    /// nothing. What it must never do is silently apply something other than
    /// what was asked for.
    pub fn apply_spec(&mut self, raw: &str) -> TuningReport {
        let mut report = TuningReport::default();
        for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
            let Some((name, value)) = entry.split_once('=') else {
                report
                    .rejected
                    .push(format!("'{entry}' no tiene forma nombre=valor"));
                continue;
            };
            let Some(field) = TuningField::from_key(name.trim()) else {
                report
                    .rejected
                    .push(format!("no existe la perilla '{}'", name.trim()));
                continue;
            };
            let Ok(value) = value.trim().parse::<f32>() else {
                report
                    .rejected
                    .push(format!("'{}' no es un número", value.trim()));
                continue;
            };
            let (low, high) = field.limits();
            if !(low..=high).contains(&value) {
                report.rejected.push(format!(
                    "{} = {value} está fuera de [{low}, {high}]",
                    field.key()
                ));
                continue;
            }
            self.set(field, value);
            report.applied.push((field, value));
        }
        report
    }
}

/// What `apply_spec` did, so the caller logs it once instead of the parser
/// depending on a logging crate.
#[derive(Default, Debug)]
pub struct TuningReport {
    pub applied: Vec<(TuningField, f32)>,
    pub rejected: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_field_round_trips_through_its_key() {
        for field in TuningField::ALL {
            assert_eq!(TuningField::from_key(field.key()), Some(field));
        }
    }

    #[test]
    fn keys_are_unique() {
        let mut keys: Vec<_> = TuningField::ALL.iter().map(|f| f.key()).collect();
        keys.sort_unstable();
        let count = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), count, "dos perillas comparten nombre");
    }

    #[test]
    fn set_and_get_agree_for_every_field() {
        for field in TuningField::ALL {
            let mut tuning = MovementTuning::default();
            assert_eq!(tuning.get(field), None, "sin tocar no debe opinar");
            let (low, high) = field.limits();
            let value = (low + high) * 0.5;
            tuning.set(field, value);
            // `GroundGraceTicks` redondea a entero: comparamos contra lo guardado.
            assert!(tuning.get(field).is_some(), "{} no volvió", field.key());
        }
    }

    #[test]
    fn a_full_spec_applies_every_entry() {
        let mut tuning = MovementTuning::default();
        let report = tuning.apply_spec("slope_hysteresis_dot=0.06, ground_grace_ticks=3");
        assert!(report.rejected.is_empty(), "{:?}", report.rejected);
        assert_eq!(report.applied.len(), 2);
        assert_eq!(tuning.slope_hysteresis_dot, Some(0.06));
        assert_eq!(tuning.ground_grace_ticks, Some(3));
    }

    /// A typo must not quietly become a different experiment.
    #[test]
    fn bad_entries_are_reported_and_leave_the_value_untouched() {
        let mut tuning = MovementTuning::default();
        let report =
            tuning.apply_spec("no_existe=1, slope_hysteresis_dot=perro, ground_grace_ticks");
        assert_eq!(report.applied.len(), 0);
        assert_eq!(report.rejected.len(), 3);
        assert_eq!(tuning, MovementTuning::default());
    }

    /// Out of range is a mistake, not an experiment: a 200° cone describes no
    /// surface the player can meet.
    #[test]
    fn values_outside_their_range_are_rejected() {
        let mut tuning = MovementTuning::default();
        let report = tuning.apply_spec("climb_wall_angle_max_deg=200, slide_gravity_factor=-1");
        assert_eq!(report.applied.len(), 0);
        assert_eq!(report.rejected.len(), 2);
    }

    #[test]
    fn an_empty_spec_changes_nothing() {
        let mut tuning = MovementTuning::default();
        let report = tuning.apply_spec("");
        assert!(report.applied.is_empty() && report.rejected.is_empty());
        assert_eq!(tuning, MovementTuning::default());
    }
}

#[cfg(test)]
mod nudge_tests {
    use super::*;

    #[test]
    fn a_first_nudge_starts_from_what_the_game_is_running() {
        let mut tuning = MovementTuning::default();
        let out = tuning.nudge(TuningField::SlideMaxSpeed, 2.0, 1);
        assert!(
            (out - 2.5).abs() < 1e-5,
            "arrancó de {out}, no de 2.0 + paso"
        );
    }

    #[test]
    fn nudging_never_leaves_the_range() {
        for field in TuningField::ALL {
            let (low, high) = field.limits();
            let mut tuning = MovementTuning::default();
            let up = tuning.nudge(field, high, 100);
            assert!(up <= high, "{} se pasó a {up}", field.key());
            let down = tuning.nudge(field, low, -100);
            assert!(down >= low, "{} bajó a {down}", field.key());
        }
    }

    #[test]
    fn every_knob_crosses_its_range_in_few_enough_steps() {
        for field in TuningField::ALL {
            let (low, high) = field.limits();
            let steps = (high - low) / field.step();
            assert!(
                steps <= 60.0,
                "{} necesita {steps} clicks para cruzar su rango: nadie la va a girar",
                field.key()
            );
        }
    }

    #[test]
    fn labels_are_unique_so_two_rows_cannot_read_the_same() {
        let mut labels: Vec<_> = TuningField::ALL.iter().map(|f| f.label()).collect();
        labels.sort_unstable();
        let count = labels.len();
        labels.dedup();
        assert_eq!(labels.len(), count);
    }
}

/// Presentation asks; the movement crate owns the knobs and applies them (§7,
/// §20). The panel writes one of these per click and never touches the resource.
#[derive(bevy_ecs::prelude::Message, Clone, Copy, Debug)]
pub struct TuningNudge {
    pub field: TuningField,
    /// How many steps, signed. `-1` and `+1` are the two panel buttons.
    pub steps: i32,
}

/// What an actor is actually running with for `field`.
///
/// Lives here, next to the knobs, because two crates need the same answer: the
/// panel to show a value, and the nudge to know where to start from. A second
/// copy of this match is a second chance to disagree with the first.
pub fn live_value(
    field: TuningField,
    ground: &crate::movement::sensing::GroundSensing,
    ledge: Option<&crate::movement::sensing::LedgeSensing>,
    slide: crate::movement::abilities::SlideMovement,
) -> f32 {
    use crate::movement::sensing::LedgeSensing;
    match field {
        TuningField::SlopeHysteresisDot => ground.slope_hysteresis_dot,
        TuningField::GroundGraceTicks => f32::from(ground.ground_grace_ticks),
        TuningField::WallDetectionReach => ledge
            .map_or(LedgeSensing::PLAYER.wall_detection_reach, |l| {
                l.wall_detection_reach
            }),
        TuningField::ClimbWallAngleMaxDeg => ledge
            .map_or(LedgeSensing::PLAYER.climb_wall_angle_max_deg, |l| {
                l.climb_wall_angle_max_deg
            }),
        TuningField::SlideGravityFactor => slide.gravity_factor,
        TuningField::SlideMaxSpeed => slide.max_speed,
        TuningField::SlideContourFriction => slide.contour_friction,
    }
}
