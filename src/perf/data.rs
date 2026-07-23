//! Benchmark knobs — pure data (§6, §19).
//!
//! `perf` is the only writer; each owner module *reads* what it needs and
//! applies it to what it owns (§7), the same direction as `AnimationDebug`.
//! Every knob is presentation-only: colliders, `TreeKind` and `FixedUpdate`
//! results are identical in every combination.

use bevy::prelude::*;

const PROFILE_ENV: &str = "BOF_PROFILE";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PerfProfile {
    #[default]
    Desktop,
    Mobile,
}

impl PerfProfile {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Desktop => "desktop",
            Self::Mobile => "mobile",
        }
    }

    pub const fn cascade_count(self) -> usize {
        match self {
            Self::Desktop => 4,
            Self::Mobile => 2,
        }
    }

    pub const fn msaa(self) -> Msaa {
        match self {
            Self::Desktop => Msaa::Off,
            Self::Mobile => Msaa::Sample4,
        }
    }

    pub const fn msaa_label(self) -> &'static str {
        match self {
            Self::Desktop => "off",
            Self::Mobile => "4x",
        }
    }
}

/// Distance culling steps for the forest. `None` = no distance cull (today's
/// behaviour); the rest hide tree visuals past that many metres.
pub const CULL_STEPS: [Option<f32>; 4] = [None, Some(100.0), Some(70.0), Some(45.0)];

/// How far a tree still casts into the shadow cascades. Index 0 is the shipped
/// default; `None` restores the unbounded behaviour so the budget can be
/// measured against it.
pub const SHADOW_CASTER_STEPS: [Option<f32>; 4] = [Some(60.0), Some(120.0), Some(30.0), None];

/// Far bound of the shadow cascades, in metres.
///
/// This is a *look* decision with a performance side effect, so it is a live
/// dial rather than a constant: past this distance nothing receives shadow, so
/// too small a value draws a lit ring around the player — a dark dome that
/// follows them. Too large spends texels on ground that reads flat anyway.
/// Only the distance is tunable at runtime; changing the cascade *count* live
/// desynchronises Bevy's per-cascade visibility bookkeeping and panics.
pub const SHADOW_DISTANCE_STEPS: [f32; 5] = [65.0, 100.0, 140.0, 200.0, 40.0];

/// Shadow map edge, in texels. Shadow cost inside dense foliage is fill-bound,
/// not vertex-bound, so this is the most direct lever on it.
pub const SHADOW_MAP_STEPS: [usize; 3] = [2048, 1024, 512];

/// Which knob the hub acts on.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PerfKnob {
    Vsync,
    Forest,
    Wireframe,
    Overdraw,
    SunShadows,
    MoonShadows,
    Cull,
    ShadowRange,
    ShadowDistance,
    ShadowMap,
    LeafShadows,
    TreeDetail,
}

impl PerfKnob {
    pub const ALL: [PerfKnob; 12] = [
        PerfKnob::Vsync,
        PerfKnob::Forest,
        PerfKnob::Wireframe,
        PerfKnob::Overdraw,
        PerfKnob::SunShadows,
        PerfKnob::MoonShadows,
        PerfKnob::Cull,
        PerfKnob::ShadowRange,
        PerfKnob::ShadowDistance,
        PerfKnob::ShadowMap,
        PerfKnob::LeafShadows,
        PerfKnob::TreeDetail,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PerfKnob::Vsync => "vsync",
            PerfKnob::Forest => "forest",
            PerfKnob::Wireframe => "wireframe",
            PerfKnob::Overdraw => "overdraw",
            PerfKnob::SunShadows => "sun-shadow",
            PerfKnob::MoonShadows => "moon-shadow",
            PerfKnob::Cull => "cull",
            PerfKnob::ShadowRange => "shadow-range",
            PerfKnob::ShadowDistance => "shadow-dist",
            PerfKnob::ShadowMap => "shadow-map",
            PerfKnob::LeafShadows => "leaf-shadows",
            PerfKnob::TreeDetail => "tree-detail",
        }
    }
}

/// Defaults are the shipped configuration, so a fresh launch measures what the
/// game actually is and every deviation is one deliberate click.
///
/// Two of them are not neutral: leaf shadows off and a 1024 px map. Measured
/// together they take sun-shadow cost from ~70% of the frame down to 2.74 ms —
/// the difference between 15 and 51 fps inside the forest. Leaving them off by
/// default meant every launch started from the expensive case and made the sun
/// look like something that had to be removed rather than budgeted.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PerfToggles {
    /// Fixed at launch because Bevy cannot safely resize cascade visibility
    /// bookkeeping in a running scene. Select with `BOF_PROFILE=mobile`.
    pub profile: PerfProfile,
    /// Indexes [`PerfKnob::ALL`].
    pub selected: usize,
    /// With vsync on, frame time quantises to the refresh rate and every
    /// measurement above it reads as the same "60". An A/B cannot size a win
    /// against a ceiling, so attribution runs need this off.
    pub vsync: bool,
    pub forest_visible: bool,
    /// Shows triangle density using Bevy's native line renderer. Diagnostic
    /// only: it adds a pass and must never contaminate benchmark samples.
    pub wireframe: bool,
    /// Replaces visible PBR handles with shared additive variants that preserve
    /// each source material's culling, so repeated fragments accumulate.
    /// Mutually exclusive with [`Self::wireframe`].
    pub overdraw: bool,
    pub sun_shadows: bool,
    pub moon_shadows: bool,
    /// Indexes [`CULL_STEPS`].
    pub cull_step: usize,
    /// Indexes [`SHADOW_CASTER_STEPS`].
    pub shadow_range_step: usize,
    /// Indexes [`SHADOW_DISTANCE_STEPS`].
    pub shadow_distance_step: usize,
    /// Indexes [`SHADOW_MAP_STEPS`].
    pub shadow_map_step: usize,
    /// Whether leaf meshes cast shadows. Trunks always do: they carry the
    /// tree's vertical structure, while the leaves contribute the dappling —
    /// which is exactly the alpha-tested, self-overlapping fill that makes the
    /// shadow passes expensive.
    pub leaf_shadows: bool,
    /// Off (default) renders the cheap graybox proxy; on swaps in the detailed
    /// Quaternius scene. A live A/B on what modeled foliage actually costs.
    pub tree_detail: bool,
}

impl Default for PerfToggles {
    fn default() -> Self {
        Self {
            profile: PerfProfile::Desktop,
            selected: 0,
            vsync: true,
            forest_visible: true,
            wireframe: false,
            overdraw: false,
            sun_shadows: true,
            moon_shadows: true,
            cull_step: 0,
            shadow_range_step: 0,
            shadow_distance_step: 0,
            shadow_map_step: 1,
            leaf_shadows: false,
            tree_detail: false,
        }
    }
}

impl PerfToggles {
    pub(crate) fn configured() -> Self {
        match std::env::var(PROFILE_ENV) {
            Ok(raw) => match parse_profile(&raw) {
                Ok(profile) => Self::for_profile(profile),
                Err(expected) => {
                    warn!("[perf] ignoring {PROFILE_ENV}={raw}: expected {expected}");
                    Self::default()
                }
            },
            Err(std::env::VarError::NotPresent) => Self::default(),
            Err(std::env::VarError::NotUnicode(_)) => {
                warn!("[perf] ignoring non-Unicode {PROFILE_ENV}: expected desktop or mobile");
                Self::default()
            }
        }
    }

    pub fn for_profile(profile: PerfProfile) -> Self {
        let mut toggles = Self {
            profile,
            ..default()
        };
        if profile == PerfProfile::Mobile {
            toggles.cull_step = 2;
            toggles.shadow_range_step = 2;
            toggles.shadow_distance_step = 4;
            toggles.shadow_map_step = 2;
        }
        toggles
    }

    pub fn cull_distance(&self) -> Option<f32> {
        CULL_STEPS
            .get(self.cull_step)
            .copied()
            .unwrap_or(CULL_STEPS[0])
    }

    pub fn shadow_caster_range(&self) -> Option<f32> {
        SHADOW_CASTER_STEPS
            .get(self.shadow_range_step)
            .copied()
            .unwrap_or(SHADOW_CASTER_STEPS[0])
    }

    pub fn shadow_distance(&self) -> f32 {
        SHADOW_DISTANCE_STEPS
            .get(self.shadow_distance_step)
            .copied()
            .unwrap_or(SHADOW_DISTANCE_STEPS[0])
    }

    pub fn shadow_map_size(&self) -> usize {
        SHADOW_MAP_STEPS
            .get(self.shadow_map_step)
            .copied()
            .unwrap_or(SHADOW_MAP_STEPS[0])
    }

    /// Steps the currently selected knob.
    pub fn step_selected(&mut self) {
        match PerfKnob::ALL[self.selected % PerfKnob::ALL.len()] {
            PerfKnob::Vsync => self.vsync = !self.vsync,
            PerfKnob::Forest => self.forest_visible = !self.forest_visible,
            PerfKnob::Wireframe => {
                self.wireframe = !self.wireframe;
                self.overdraw &= !self.wireframe;
            }
            PerfKnob::Overdraw => {
                self.overdraw = !self.overdraw;
                self.wireframe &= !self.overdraw;
            }
            PerfKnob::SunShadows => self.sun_shadows = !self.sun_shadows,
            PerfKnob::MoonShadows => self.moon_shadows = !self.moon_shadows,
            PerfKnob::Cull => self.cull_step = (self.cull_step + 1) % CULL_STEPS.len(),
            PerfKnob::ShadowRange => {
                self.shadow_range_step = (self.shadow_range_step + 1) % SHADOW_CASTER_STEPS.len()
            }
            PerfKnob::ShadowDistance => {
                self.shadow_distance_step =
                    (self.shadow_distance_step + 1) % SHADOW_DISTANCE_STEPS.len()
            }
            PerfKnob::ShadowMap => {
                self.shadow_map_step = (self.shadow_map_step + 1) % SHADOW_MAP_STEPS.len()
            }
            PerfKnob::LeafShadows => self.leaf_shadows = !self.leaf_shadows,
            PerfKnob::TreeDetail => self.tree_detail = !self.tree_detail,
        }
    }

    /// The knob currently pointed at.
    pub fn selected_knob(&self) -> PerfKnob {
        PerfKnob::ALL[self.selected % PerfKnob::ALL.len()]
    }

    pub fn set_selected(&mut self, knob: PerfKnob) {
        if let Some(index) = PerfKnob::ALL.iter().position(|entry| *entry == knob) {
            self.selected = index;
        }
    }

    /// A knob's value with no decoration. Single definition — the HUD, the
    /// console and the keypress log all render from this.
    pub fn knob_value(&self, knob: PerfKnob) -> String {
        match knob {
            PerfKnob::Vsync => on_off(self.vsync),
            PerfKnob::Forest => on_off(self.forest_visible),
            PerfKnob::Wireframe => on_off(self.wireframe),
            PerfKnob::Overdraw => on_off(self.overdraw),
            PerfKnob::SunShadows => on_off(self.sun_shadows),
            PerfKnob::MoonShadows => on_off(self.moon_shadows),
            PerfKnob::Cull => match self.cull_distance() {
                Some(d) => format!("{d:.0}m"),
                None => "off".to_string(),
            },
            PerfKnob::ShadowRange => match self.shadow_caster_range() {
                Some(d) => format!("{d:.0}m"),
                None => "sin limite".to_string(),
            },
            PerfKnob::ShadowDistance => format!("{:.0}m", self.shadow_distance()),
            PerfKnob::ShadowMap => format!("{}", self.shadow_map_size()),
            PerfKnob::LeafShadows => on_off(self.leaf_shadows),
            PerfKnob::TreeDetail => on_off(self.tree_detail),
        }
    }

    /// `>forest:ON` when selected, ` forest:ON` otherwise.
    pub fn knob_text(&self, knob: PerfKnob) -> String {
        let cursor = if self.selected_knob() == knob {
            '>'
        } else {
            ' '
        };
        format!("{cursor}{}:{}", knob.label(), self.knob_value(knob))
    }
}

fn parse_profile(raw: &str) -> Result<PerfProfile, &'static str> {
    match raw {
        "desktop" => Ok(PerfProfile::Desktop),
        "mobile" => Ok(PerfProfile::Mobile),
        _ => Err("desktop or mobile"),
    }
}

fn on_off(value: bool) -> String {
    if value { "ON" } else { "off" }.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_reproduce_the_shipped_build() {
        let toggles = PerfToggles::default();
        assert_eq!(toggles.profile, PerfProfile::Desktop);
        assert_eq!(toggles.profile.cascade_count(), 4);
        assert_eq!(toggles.profile.msaa(), Msaa::Off);
        assert!(toggles.forest_visible);
        assert!(!toggles.wireframe);
        assert!(!toggles.overdraw);
        assert!(toggles.sun_shadows);
        assert!(toggles.moon_shadows);
        assert_eq!(toggles.cull_distance(), None);
        assert_eq!(
            toggles.shadow_caster_range(),
            Some(60.0),
            "the budget ships on; `None` exists to measure against it"
        );
        // Measured, not preferred: with these two the sun costs 2.74 ms
        // instead of most of the frame.
        assert!(!toggles.leaf_shadows);
        assert_eq!(toggles.shadow_map_size(), 1024);
    }

    #[test]
    fn mobile_profile_is_one_coherent_launch_configuration() {
        let toggles = PerfToggles::for_profile(PerfProfile::Mobile);

        assert_eq!(toggles.profile.cascade_count(), 2);
        assert_eq!(toggles.profile.msaa(), Msaa::Sample4);
        assert_eq!(toggles.cull_distance(), Some(70.0));
        assert_eq!(toggles.shadow_caster_range(), Some(30.0));
        assert_eq!(toggles.shadow_distance(), 40.0);
        assert_eq!(toggles.shadow_map_size(), 512);
        assert!(!toggles.leaf_shadows);
        assert!(!toggles.tree_detail);
    }

    #[test]
    fn every_knob_steps_and_returns_to_its_baseline() {
        for (index, knob) in PerfKnob::ALL.iter().enumerate() {
            let mut toggles = PerfToggles {
                selected: index,
                ..default()
            };
            let baseline = toggles.knob_text(*knob);

            // One step must always change the value...
            toggles.step_selected();
            assert_ne!(toggles.knob_text(*knob), baseline, "{}", knob.label());

            // ...and the cycle must close, so an A/B can be repeated exactly.
            // 60 = lcm(2 bool, 4 cull, 4 shadow-range, 5 shadow-dist, 3 shadow-map).
            for _ in 1..60 {
                toggles.step_selected();
            }
            assert_eq!(toggles.knob_text(*knob), baseline, "{}", knob.label());
        }
    }

    #[test]
    fn stepping_one_knob_never_moves_another() {
        let mut toggles = PerfToggles::default();
        let untouched: Vec<String> = PerfKnob::ALL[1..]
            .iter()
            .map(|knob| toggles.knob_text(*knob))
            .collect();

        toggles.step_selected(); // selected == 0 == Forest

        for (knob, before) in PerfKnob::ALL[1..].iter().zip(untouched) {
            assert_eq!(toggles.knob_text(*knob), before, "{}", knob.label());
        }
    }

    #[test]
    fn diagnostic_views_are_mutually_exclusive() {
        let mut toggles = PerfToggles::default();

        toggles.set_selected(PerfKnob::Wireframe);
        toggles.step_selected();
        assert!(toggles.wireframe);
        assert!(!toggles.overdraw);

        toggles.set_selected(PerfKnob::Overdraw);
        toggles.step_selected();
        assert!(!toggles.wireframe);
        assert!(toggles.overdraw);
    }

    #[test]
    fn profile_parser_distinguishes_valid_and_malformed_values() {
        assert_eq!(parse_profile("desktop"), Ok(PerfProfile::Desktop));
        assert_eq!(parse_profile("mobile"), Ok(PerfProfile::Mobile));
        assert_eq!(parse_profile("phone"), Err("desktop or mobile"));
    }
}
