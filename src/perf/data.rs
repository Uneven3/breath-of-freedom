use bevy::prelude::Msaa;

pub use bof_domain::perf::*;

const PROFILE_ENV: &str = "BOF_PROFILE";

pub fn configured_toggles() -> PerfToggles {
    match std::env::var(PROFILE_ENV) {
        Ok(raw) => match parse_profile(&raw) {
            Ok(profile) => PerfToggles::for_profile(profile),
            Err(expected) => {
                bevy::log::warn!("[perf] ignoring {PROFILE_ENV}={raw}: expected {expected}");
                PerfToggles::default()
            }
        },
        Err(std::env::VarError::NotPresent) => PerfToggles::default(),
        Err(std::env::VarError::NotUnicode(_)) => {
            bevy::log::warn!(
                "[perf] ignoring non-Unicode {PROFILE_ENV}: expected desktop or mobile"
            );
            PerfToggles::default()
        }
    }
}

pub const fn profile_msaa(profile: PerfProfile) -> Msaa {
    match profile {
        PerfProfile::Desktop => Msaa::Off,
        PerfProfile::Mobile => Msaa::Sample4,
    }
}

/// Translates the `Msaa` knob's sample count into Bevy's component.
///
/// Lives here and not in `bof_domain` for the reason §7 gives: the knob is data
/// —a sample count— and mapping it onto a rendering type is presentation's job.
/// An unknown count falls back to off rather than panicking; the steps table is
/// the only caller and a test pins that every one of its entries maps.
pub const fn msaa_for_samples(samples: u32) -> Msaa {
    match samples {
        2 => Msaa::Sample2,
        4 => Msaa::Sample4,
        8 => Msaa::Sample8,
        _ => Msaa::Off,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_map_to_the_validated_msaa_modes() {
        assert_eq!(profile_msaa(PerfProfile::Desktop), Msaa::Off);
        assert_eq!(profile_msaa(PerfProfile::Mobile), Msaa::Sample4);
    }

    /// Every step of the knob has to name a mode Bevy knows, or a click would
    /// silently land on "off" and the sweep would measure the same thing twice.
    #[test]
    fn every_msaa_step_maps_to_a_real_mode() {
        for samples in MSAA_STEPS {
            let mode = msaa_for_samples(samples);
            if samples == 1 {
                assert_eq!(mode, Msaa::Off);
            } else {
                assert_ne!(mode, Msaa::Off, "{samples}x fell back to off");
                assert_eq!(mode.samples(), samples, "{samples}x mapped to another mode");
            }
        }
    }

    /// The launch profile and the knob's baseline have to agree, or the
    /// benchmark's first row would not be the shipped configuration.
    #[test]
    fn the_knobs_baseline_is_what_each_profile_ships() {
        for profile in [PerfProfile::Desktop, PerfProfile::Mobile] {
            let toggles = PerfToggles::for_profile(profile);
            assert_eq!(
                msaa_for_samples(toggles.msaa_samples()),
                profile_msaa(profile),
                "profile {} disagrees with its own knob step",
                profile.label()
            );
        }
    }
}
