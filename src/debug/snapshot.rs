//! What debug reports, as pure data (§6, §19).
//!
//! One snapshot, two sinks: [`super::hud`] renders it to the screen and
//! [`super::console`] writes it to the log. Producers fill their own section
//! and never format for a particular sink — that is the whole point of the
//! split. The console is the channel that carries hard data out of a playtest;
//! the HUD is what the player reads while judging feeling. Both must show the
//! same numbers, and the only way to guarantee that is to have them read the
//! same values instead of formatting twice.

use bevy::prelude::*;

/// One labelled value in a section. The label is owned rather than `&'static`
/// because some come from runtime data — GPU pass names arrive as strings from
/// the render diagnostics.
#[derive(Clone, PartialEq)]
pub struct Field {
    pub label: String,
    pub value: String,
    /// Continuous values churn every frame. Change-triggered console output
    /// skips them, or a drifting sensor float would emit a line per frame and
    /// bury the discrete transitions that channel exists to surface.
    pub volatile: bool,
}

impl Field {
    /// A discrete value: worth a log line when it changes.
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            volatile: false,
        }
    }

    /// A continuous value: shown on screen and in periodic/on-demand dumps,
    /// never a reason to emit on its own.
    pub fn volatile(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            volatile: true,
            ..Self::new(label, value)
        }
    }

    pub fn flag(label: impl Into<String>, value: bool) -> Self {
        Self::new(label, if value { "ON" } else { "off" })
    }
}

/// Fixed slots, so the report's order never depends on system execution order.
/// Producers write into their own slot and stay ignorant of each other (§7).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SectionId {
    Perf,
    Scene,
    Vitals,
    Locomotion,
    Contact,
    Combat,
    Mount,
    Toggles,
}

impl SectionId {
    pub const COUNT: usize = 8;
    pub const ALL: [SectionId; Self::COUNT] = [
        SectionId::Perf,
        SectionId::Scene,
        SectionId::Vitals,
        SectionId::Locomotion,
        SectionId::Contact,
        SectionId::Combat,
        SectionId::Mount,
        SectionId::Toggles,
    ];

    pub fn title(self) -> &'static str {
        match self {
            SectionId::Perf => "perf",
            SectionId::Scene => "scene",
            SectionId::Vitals => "vitals",
            SectionId::Locomotion => "locomotion",
            SectionId::Contact => "contact",
            SectionId::Combat => "combat",
            SectionId::Mount => "mount",
            SectionId::Toggles => "toggles",
        }
    }

    fn index(self) -> usize {
        match self {
            SectionId::Perf => 0,
            SectionId::Scene => 1,
            SectionId::Vitals => 2,
            SectionId::Locomotion => 3,
            SectionId::Contact => 4,
            SectionId::Combat => 5,
            SectionId::Mount => 6,
            SectionId::Toggles => 7,
        }
    }
}

#[derive(Clone, PartialEq, Default)]
pub struct Section {
    pub fields: Vec<Field>,
}

/// The whole debug picture. Sections absent from the snapshot simply do not
/// render — a producer that has nothing to say this frame (no player alive,
/// diagnostics not ready) leaves its slot empty rather than reporting zeros.
#[derive(Resource, Default)]
pub struct DebugSnapshot {
    sections: [Option<Section>; SectionId::COUNT],
}

impl DebugSnapshot {
    pub fn set(&mut self, id: SectionId, fields: Vec<Field>) {
        self.sections[id.index()] = Some(Section { fields });
    }

    pub fn get(&self, id: SectionId) -> Option<&Section> {
        self.sections[id.index()].as_ref()
    }

    /// Drops a section so it stops rendering — for a producer whose subject is
    /// absent this frame (no horse spawned), so it never lingers stale. The
    /// mirror of `set`: "nothing to report" rather than reporting the last value.
    pub fn clear(&mut self, id: SectionId) {
        self.sections[id.index()] = None;
    }

    /// One section rendered as `title: label=value  label=value`.
    pub fn line(&self, id: SectionId) -> Option<String> {
        self.render(id, false)
    }

    /// The same section with continuous values dropped — what change detection
    /// compares, so only a discrete transition can trigger a log line.
    pub fn stable_line(&self, id: SectionId) -> Option<String> {
        self.render(id, true)
    }

    fn render(&self, id: SectionId, stable_only: bool) -> Option<String> {
        let section = self.get(id)?;
        let body = section
            .fields
            .iter()
            .filter(|field| !(stable_only && field.volatile))
            .map(|field| format!("{}={}", field.label, field.value))
            .collect::<Vec<_>>();
        if body.is_empty() {
            return None;
        }
        Some(format!("{}: {}", id.title(), body.join("  ")))
    }

    pub fn lines(&self) -> Vec<String> {
        SectionId::ALL
            .into_iter()
            .filter_map(|id| self.line(id))
            .collect()
    }

    /// HUD lines for the sections the readout menu currently shows. The console
    /// and the full `P` dump ignore visibility — this only trims the always-on
    /// on-screen overlay so it stops being a wall of text (see [`HudVisibility`]).
    pub fn visible_lines(&self, visibility: &HudVisibility) -> Vec<String> {
        SectionId::ALL
            .into_iter()
            .filter(|id| visibility.is_visible(*id))
            .filter_map(|id| self.line(id))
            .collect()
    }
}

/// Which HUD sections the on-screen overlay draws. Toggled from the F2 readout
/// menu; `debug` owns it and applies the toggles (§7). The console and full
/// dumps stay complete — the log remains the whole record regardless of what
/// the screen is trimmed to.
#[derive(Resource)]
pub struct HudVisibility {
    visible: [bool; SectionId::COUNT],
}

impl Default for HudVisibility {
    fn default() -> Self {
        // Lean default: only locomotion and the health/stamina readout (Vitals).
        // Everything else — perf, contact, combat, mount, the toggle mirror — is
        // opt-in from the F2 menu.
        let mut visible = [false; SectionId::COUNT];
        for id in [SectionId::Vitals, SectionId::Locomotion] {
            visible[id.index()] = true;
        }
        Self { visible }
    }
}

impl HudVisibility {
    pub fn is_visible(&self, id: SectionId) -> bool {
        self.visible[id.index()]
    }

    /// Flips one section and returns its new state, for the log line.
    pub fn toggle(&mut self, id: SectionId) -> bool {
        let slot = &mut self.visible[id.index()];
        *slot = !*slot;
        *slot
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DebugSnapshot {
        let mut snapshot = DebugSnapshot::default();
        snapshot.set(SectionId::Vitals, vec![Field::new("hp", "30/30")]);
        snapshot.set(SectionId::Perf, vec![Field::new("fps", "28.4")]);
        snapshot
    }

    /// `index()` and `ALL` are hand-kept in sync (the whole slot scheme relies
    /// on it); this fails the moment a new `SectionId` breaks either.
    #[test]
    fn section_ids_index_uniquely_and_cover_all() {
        assert_eq!(SectionId::ALL.len(), SectionId::COUNT);
        for (position, id) in SectionId::ALL.into_iter().enumerate() {
            assert_eq!(id.index(), position, "{id:?} index must match ALL order");
        }
    }

    /// A producer with no subject this frame clears its slot; the section then
    /// stops rendering instead of showing the last value it had.
    #[test]
    fn clearing_a_section_stops_it_rendering() {
        let mut snapshot = sample();
        assert!(snapshot.line(SectionId::Vitals).is_some());
        snapshot.clear(SectionId::Vitals);
        assert!(snapshot.line(SectionId::Vitals).is_none());
    }

    /// The reason the snapshot exists: HUD and console must never be able to
    /// disagree, so both sinks read the same rendered values.
    #[test]
    fn both_sinks_render_the_same_values() {
        let snapshot = sample();
        let lines = snapshot.lines();

        assert_eq!(lines, vec!["perf: fps=28.4", "vitals: hp=30/30"]);
        for id in [SectionId::Perf, SectionId::Vitals] {
            assert!(lines.contains(&snapshot.line(id).unwrap()));
        }
    }

    /// Producers run in whatever order the scheduler picks; the report must not.
    #[test]
    fn section_order_is_fixed_regardless_of_write_order() {
        let mut reversed = DebugSnapshot::default();
        reversed.set(SectionId::Perf, vec![Field::new("fps", "28.4")]);
        reversed.set(SectionId::Vitals, vec![Field::new("hp", "30/30")]);

        assert_eq!(sample().lines(), reversed.lines());
    }

    #[test]
    fn an_unset_section_renders_nothing_rather_than_zeros() {
        let snapshot = sample();
        assert!(snapshot.line(SectionId::Combat).is_none());
        assert_eq!(snapshot.lines().len(), 2);
    }

    /// The bug this guards: a continuous value in a change-triggered section
    /// emits a line every frame. `stable_line` must not see it at all.
    #[test]
    fn volatile_fields_are_invisible_to_change_detection() {
        let mut snapshot = DebugSnapshot::default();
        snapshot.set(
            SectionId::Locomotion,
            vec![
                Field::new("state", "Walk"),
                Field::volatile("ascend_dot", "0.006"),
            ],
        );
        let before = snapshot.stable_line(SectionId::Locomotion);

        snapshot.set(
            SectionId::Locomotion,
            vec![
                Field::new("state", "Walk"),
                Field::volatile("ascend_dot", "-0.009"),
            ],
        );

        assert_eq!(before, snapshot.stable_line(SectionId::Locomotion));
        assert!(
            snapshot
                .line(SectionId::Locomotion)
                .is_some_and(|line| line.contains("ascend_dot")),
            "still shown on screen and in full dumps"
        );
    }

    /// A discrete change must still get through.
    #[test]
    fn a_discrete_change_still_triggers() {
        let mut snapshot = DebugSnapshot::default();
        snapshot.set(SectionId::Locomotion, vec![Field::new("state", "Walk")]);
        let before = snapshot.stable_line(SectionId::Locomotion);
        snapshot.set(SectionId::Locomotion, vec![Field::new("state", "Fall")]);
        assert_ne!(before, snapshot.stable_line(SectionId::Locomotion));
    }

    /// The declutter contract: the overlay only shows the sections the readout
    /// menu left on, while the full `lines()` dump (and the console) keep them all.
    #[test]
    fn hud_visibility_trims_the_overlay_but_not_the_full_dump() {
        let snapshot = sample(); // Perf + Vitals populated.
        let mut visibility = HudVisibility::default();
        // Default shows Vitals but not Perf (Locomotion has no data here).
        assert_eq!(
            snapshot.visible_lines(&visibility),
            vec!["vitals: hp=30/30"]
        );

        visibility.toggle(SectionId::Perf); // opt Perf in
        assert_eq!(
            snapshot.visible_lines(&visibility),
            vec!["perf: fps=28.4", "vitals: hp=30/30"]
        );
        assert_eq!(
            snapshot.lines().len(),
            2,
            "hiding a section on screen must not touch the full dump"
        );
    }

    /// A section made only of continuous values must stay silent, not emit a
    /// bare title.
    #[test]
    fn an_all_volatile_section_has_no_stable_line() {
        let mut snapshot = DebugSnapshot::default();
        snapshot.set(SectionId::Perf, vec![Field::volatile("fps", "58.3")]);
        assert!(snapshot.stable_line(SectionId::Perf).is_none());
        assert!(snapshot.line(SectionId::Perf).is_some());
    }
}
