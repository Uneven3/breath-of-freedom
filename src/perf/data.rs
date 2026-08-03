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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profiles_map_to_the_validated_msaa_modes() {
        assert_eq!(profile_msaa(PerfProfile::Desktop), Msaa::Off);
        assert_eq!(profile_msaa(PerfProfile::Mobile), Msaa::Sample4);
    }
}
