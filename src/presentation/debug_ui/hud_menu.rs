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

use bevy::prelude::*;

use crate::debug::channel::HudSectionToggle;
use crate::debug::snapshot::{HudVisibility, SectionId};
use crate::input::ModalInputFocusRequest;

use super::style::{
    BORDER, PANEL, ROW, TEXT_BRIGHT, TEXT_MUTED, body_font, heading_font, row_node, section_title,
};

#[derive(Resource, Default)]
pub(super) struct HudMenuState {
    open: bool,
}

#[derive(Component)]
pub(super) struct HudMenuRoot;

#[derive(Component)]
pub(super) struct SectionButton(SectionId);

#[derive(Component)]
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
                    row.spawn((
                        MenuCloseButton,
                        Button,
                        Node {
                            padding: UiRect::axes(Val::Px(12.0), Val::Px(5.0)),
                            border_radius: BorderRadius::all(Val::Px(4.0)),
                            ..default()
                        },
                        BackgroundColor(ROW),
                    ))
                    .with_child((
                        Text::new("F2"),
                        body_font(),
                        TextColor(TEXT_MUTED),
                    ));
                });
            section_title(
                panel,
                "Grupos",
                "Qué contextos dibuja el overlay en pantalla. El log guarda todo igual.",
            );
            for section in SectionId::ALL {
                panel
                    .spawn((
                        SectionButton(section),
                        Button,
                        row_node(),
                        BackgroundColor(ROW),
                    ))
                    .with_children(|row| {
                        row.spawn((
                            Text::new(section.title()),
                            body_font(),
                            TextColor(TEXT_BRIGHT),
                        ));
                        row.spawn((
                            SectionStateText(section),
                            Text::new("—"),
                            body_font(),
                            TextColor(TEXT_MUTED),
                        ));
                    });
            }
        });
}

/// The one key. Everything else inside is a click.
pub(super) fn toggle_hud_menu(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<HudMenuState>,
    root: Single<Entity, With<HudMenuRoot>>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
) {
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

pub(super) fn handle_hud_menu_clicks(
    mut state: ResMut<HudMenuState>,
    root: Single<Entity, With<HudMenuRoot>>,
    sections: Query<(&Interaction, &SectionButton), Changed<Interaction>>,
    close: Query<&Interaction, (Changed<Interaction>, With<MenuCloseButton>)>,
    mut focus: MessageWriter<ModalInputFocusRequest>,
    mut toggle: MessageWriter<HudSectionToggle>,
) {
    if !state.open {
        return;
    }
    let pressed = |interaction: &Interaction| *interaction == Interaction::Pressed;

    for (interaction, section) in &sections {
        if pressed(interaction) {
            toggle.write(HudSectionToggle(section.0));
        }
    }
    if close.iter().any(pressed) {
        set_open(&mut state, false, *root, &mut focus);
    }
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
