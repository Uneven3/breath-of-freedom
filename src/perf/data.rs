use bevy::prelude::Msaa;

pub use bof_domain::perf::*;

const PROFILE_ENV: &str = "BOF_PROFILE";
const KNOBS_ENV: &str = "BOF_KNOBS";

pub fn configured_toggles() -> PerfToggles {
    let mut toggles = match std::env::var(PROFILE_ENV) {
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
    };
    if let Ok(raw) = std::env::var(KNOBS_ENV) {
        apply_configured_knobs(&mut toggles, &raw);
    }
    toggles
}

/// `BOF_KNOBS="grass-view=5,msaa=1"` — las perillas del hub, desde el arranque.
///
/// Genérico sobre [`PerfKnob::label`] y no una variable por perilla: la que
/// hacía falta hoy era la vista de pasto, y la que va a hacer falta mañana es
/// otra. Con esto, `BOF_SHOT` puede fotografiar cualquier configuración sin
/// pedirle al usuario que la ponga con el mouse — que era el único camino y
/// costaba una sesión suya por experimento.
///
/// Un nombre que no existe **avisa y sigue**: es una herramienta de diagnóstico
/// y abortar el arranque por un typo no protege nada. Lo que no puede pasar es
/// que se aplique en silencio algo distinto de lo que se pidió.
fn apply_configured_knobs(toggles: &mut PerfToggles, raw: &str) {
    for entry in raw.split(',').map(str::trim).filter(|e| !e.is_empty()) {
        let Some((name, value)) = entry.split_once('=') else {
            bevy::log::warn!("[perf] {KNOBS_ENV}: '{entry}' no tiene forma nombre=paso");
            continue;
        };
        let name = name.trim();
        let Some(knob) = PerfKnob::ALL.into_iter().find(|knob| knob.label() == name) else {
            bevy::log::warn!(
                "[perf] {KNOBS_ENV}: no existe la perilla '{name}'; hay: {}",
                PerfKnob::ALL
                    .iter()
                    .map(|knob| knob.label())
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            continue;
        };
        let Ok(step) = value.trim().parse::<usize>() else {
            bevy::log::warn!("[perf] {KNOBS_ENV}: '{value}' no es un número de paso");
            continue;
        };
        toggles.set_knob_step(knob, step);
        bevy::log::info!(
            "[perf] {KNOBS_ENV}: {}",
            toggles.knob_text(knob).trim_start()
        );
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
