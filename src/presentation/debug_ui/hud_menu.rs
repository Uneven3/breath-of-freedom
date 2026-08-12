//! The F2 readout menu: which real-time HUD groups the on-screen overlay draws.
//!
//! The overlay used to dump every snapshot section at once — a wall of text
//! over the game. This menu turns each context group (perf, vitals, locomotion,
//! contact, combat, toggles) into a toggle, so the player keeps only the
//! readouts they are watching right now.
//!
//! Read-only over `debug` state (§20): it renders [`HudVisibility`] and emits
//! [`HudSectionToggle`]; `debug` validates and applies. It is a separate modal
//! from the F1 hub — a docked panel rather than a full-screen dim — so the two
//! can coexist and it sits beside the overlay it controls.

use bevy::feathers::controls::FeathersButton;
use bevy::feathers::theme::ThemedText;
use bevy::prelude::*;
use bevy::ui_widgets::Activate;

use crate::debug::channel::HudSectionToggle;
use crate::debug::snapshot::{HudVisibility, SectionId};
use crate::input::ModalInputFocusRequest;
use crate::perf::{Benchmark, Flythrough};

use super::style::{BORDER, PANEL, TEXT_BRIGHT, heading_font, section_title};

#[derive(Resource, Default)]
pub(super) struct HudMenuState {
    open: bool,
}

#[derive(Component)]
pub(super) struct HudMenuRoot;

#[derive(Component)]
pub(super) struct SectionButton(SectionId);

// `Clone, Default`: vive dentro de un `bsn!` (etiqueta de un botón de
// Feathers) — ver la nota en `PerfKnob`/`SectionId` sobre por qué el
// default nunca sobrevive al spawn.
#[derive(Component, Clone, Default)]
pub(super) struct SectionStateText(SectionId);

#[derive(Component)]
pub(super) struct MenuCloseButton;

pub(super) fn menu_is_open(state: Res<HudMenuState>) -> bool {
    state.open
}

pub(super) fn spawn_hud_menu(mut commands: Commands) {
    commands
        .spawn((
            HudMenuRoot,
            Name::new("HudReadoutMenu"),
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(8.0),
                right: Val::Px(8.0),
                width: Val::Px(268.0),
                max_height: Val::Percent(94.0),
                flex_direction: FlexDirection::Column,
                padding: UiRect::all(Val::Px(14.0)),
                row_gap: Val::Px(6.0),
                border: UiRect::all(Val::Px(1.0)),
                border_radius: BorderRadius::all(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(PANEL),
            BorderColor::all(BORDER),
            // Above the F1 hub (z 120) so it reads even if both are open.
            GlobalZIndex(130),
        ))
        .with_children(|panel| {
            panel
                .spawn(Node {
                    width: Val::Percent(100.0),
                    justify_content: JustifyContent::SpaceBetween,
                    align_items: AlignItems::Center,
                    ..default()
                })
                .with_children(|row| {
                    row.spawn((
                        Text::new("Readouts"),
                        heading_font(),
                        TextColor(TEXT_BRIGHT),
                    ));
                    row.spawn(MenuCloseButton).apply_scene(bsn! {
                        @FeathersButton
                        on(activate_menu_close)
                        Children [ (Text("F2") ThemedText TextFont { font_size: FontSize::Px(15.0) }) ]
                    });
                });
            section_title(
                panel,
                "Grupos",
                "Qué contextos dibuja el overlay en pantalla. El log guarda todo igual.",
            );
            for section in SectionId::ALL {
                let title = section.title();
                panel.spawn(SectionButton(section)).apply_scene(bsn! {
                    @FeathersButton
                    Node {
                        width: Val::Percent(100.0),
                        justify_content: JustifyContent::SpaceBetween,
                        column_gap: Val::Px(12.0),
                    }
                    on(activate_section)
                    Children [
                        (
                            Text({title})
                            ThemedText
                            TextFont { font_size: FontSize::Px(15.0) }
                        ),
                        (
                            Text("—")
                            ThemedText
                            TextFont { font_size: FontSize::Px(15.0) }
                            SectionStateText({section})
                        ),
                    ]
                });
            }
        });
}

/// The one key. Everything else inside is a click.
pub(super) fn toggle_hud_menu(
    keys: Res<ButtonInput<KeyCode>>,
    benchmark: Res<Benchmark>,
    flythrough: Res<Flythrough>,
    mut state: ResMut<HudMenuState>,
    root: Single<Entity, With<HudMenuRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    if benchmark.is_running() || flythrough.is_running() {
        set_open(&mut state, false, *root, &mut focus);
        return;
    }
    if !keys.just_pressed(KeyCode::F2) {
        return;
    }
    let wanted = !state.open;
    set_open(&mut state, wanted, *root, &mut focus);
}

fn set_open(
    state: &mut HudMenuState,
    open: bool,
    root: Entity,
    focus: &mut MessageWriter<ModalInputFocusRequest>,
) {
    if state.open == open {
        return;
    }
    state.open = open;
    focus.write(if open {
        ModalInputFocusRequest::Acquire(root)
    } else {
        ModalInputFocusRequest::Release(root)
    });
}

// Un observer por tipo de botón, no por instancia — `bevy_ui_widgets::Button`
// no llena `Interaction` (hallazgo del spike de F1, ver `AHORA.md`), así que
// cada `FeathersButton` se engancha con `on(...)` en vez de una query
// centralizada como la que tenía este archivo hasta el 2026-08-12.
fn activate_section(
    activate: On<Activate>,
    sections: Query<&SectionButton>,
    mut toggle: MessageWriter<HudSectionToggle>,
) {
    if let Ok(section) = sections.get(activate.entity) {
        toggle.write(HudSectionToggle(section.0));
    }
}

fn activate_menu_close(
    _activate: On<Activate>,
    mut state: ResMut<HudMenuState>,
    root: Single<Entity, With<HudMenuRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
    set_open(&mut state, false, *root, &mut focus);
}

pub(super) fn sync_hud_menu_visibility(
    state: Res<HudMenuState>,
    mut root: Single<&mut Node, With<HudMenuRoot>>,
) {
    let wanted = if state.open {
        Display::Flex
    } else {
        Display::None
    };
    if root.display != wanted {
        root.display = wanted;
    }
}

pub(super) fn sync_hud_menu_labels(
    visibility: Res<HudVisibility>,
    mut labels: Query<(&mut Text, &SectionStateText)>,
) {
    for (mut text, section) in &mut labels {
        text.0 = if visibility.is_visible(section.0) {
            "ON".to_string()
        } else {
            "off".to_string()
        };
    }
}
