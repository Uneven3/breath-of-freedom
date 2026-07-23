//! Benchmark overlay: the only on-screen sign that a run is happening.
//!
//! It exists separately from the hub panel because the panel *closes* when a
//! run starts — a modal is extra draw work and holds the pointer, neither of
//! which belongs in the frame times being recorded. Without this the sequence
//! ran for 33 seconds with no feedback at all and ended in silence.
//!
//! Three things it must say, in order of how much they save you:
//!
//! 1. **Spoiled** — the current step has already been invalidated by movement.
//!    Seeing that at second 8 lets you abandon and restart instead of holding
//!    still for another 25 seconds to earn a table full of `INVALID`.
//! 2. **Progress** — which step, which phase, seconds left.
//! 3. **Done** — with how many steps survived, so the log is worth opening.
//!
//! Cost is one text node, constant across every step, so it cannot bias the
//! A/B it reports on.

use bevy::prelude::*;

use crate::debug::MaterialReportNotice;
use crate::perf::{Benchmark, Flythrough, PerfToggles};

/// How long the completion notice stays up after a run ends.
const NOTICE_SECS: f32 = 12.0;

const RUNNING: Color = Color::srgb(0.25, 0.82, 0.67);
const SPOILED: Color = Color::srgb(0.95, 0.55, 0.25);
const DONE: Color = Color::srgb(0.75, 0.85, 0.95);

#[derive(Component)]
pub(super) struct BenchmarkOverlay;

#[derive(Component)]
pub(super) struct OverdrawLegend;

pub(super) fn spawn_overlay(mut commands: Commands) {
    commands.spawn((
        BenchmarkOverlay,
        Text::new(""),
        TextFont {
            font_size: FontSize::Px(20.0),
            ..default()
        },
        TextColor(RUNNING),
        Node {
            position_type: PositionType::Absolute,
            top: Val::Px(16.0),
            right: Val::Px(16.0),
            max_width: Val::Percent(50.0),
            ..default()
        },
        // Above the HUD, below the modal panel.
        GlobalZIndex(110),
        Visibility::Hidden,
    ));

    commands
        .spawn((
            OverdrawLegend,
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(16.0),
                bottom: Val::Px(16.0),
                flex_direction: FlexDirection::Column,
                row_gap: Val::Px(6.0),
                padding: UiRect::all(Val::Px(10.0)),
                border_radius: BorderRadius::all(Val::Px(6.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.025, 0.01, 0.01, 0.9)),
            GlobalZIndex(110),
            Visibility::Hidden,
        ))
        .with_children(|legend| {
            legend.spawn((
                Text::new("OVERDRAW · capas por píxel"),
                TextFont {
                    font_size: FontSize::Px(16.0),
                    ..default()
                },
                TextColor(Color::srgb(1.0, 0.82, 0.78)),
            ));
            legend.spawn((
                Text::new("Mira áreas grandes y persistentes, no puntos pequeños."),
                TextFont {
                    font_size: FontSize::Px(12.0),
                    ..default()
                },
                TextColor(Color::srgb(0.82, 0.68, 0.66)),
            ));
            legend
                .spawn(Node {
                    column_gap: Val::Px(5.0),
                    ..default()
                })
                .with_children(|scale| {
                    for (label, color) in [
                        ("BIEN  1-2", Color::srgb(0.18, 0.018, 0.01)),
                        ("MEDIO  3-5", Color::srgb(0.42, 0.025, 0.012)),
                        ("MALO  6-9", Color::srgb(0.72, 0.035, 0.015)),
                        ("CRÍTICO  10+", Color::srgb(1.0, 0.06, 0.025)),
                    ] {
                        scale
                            .spawn((
                                Node {
                                    padding: UiRect::axes(Val::Px(8.0), Val::Px(5.0)),
                                    border_radius: BorderRadius::all(Val::Px(3.0)),
                                    ..default()
                                },
                                BackgroundColor(color),
                            ))
                            .with_child((
                                Text::new(label),
                                TextFont {
                                    font_size: FontSize::Px(11.0),
                                    ..default()
                                },
                                TextColor(Color::WHITE),
                            ));
                    }
                });
        });
}

pub(super) fn update_overdraw_legend(
    perf: Res<PerfToggles>,
    mut legend: Single<&mut Visibility, With<OverdrawLegend>>,
) {
    if perf.is_changed() {
        **legend = if perf.overdraw {
            Visibility::Inherited
        } else {
            Visibility::Hidden
        };
    }
}

type OverlayQuery = (
    &'static mut Text,
    &'static mut TextColor,
    &'static mut Visibility,
);

pub(super) fn update_overlay(
    benchmark: Res<Benchmark>,
    flythrough: Res<Flythrough>,
    materials: Res<MaterialReportNotice>,
    time: Res<Time<Real>>,
    overlay: Single<OverlayQuery, With<BenchmarkOverlay>>,
) {
    let (mut text, mut color, mut visibility) = overlay.into_inner();

    // Running states first (they own the camera), then the lingering notices.
    if let Some(status) = benchmark.status() {
        let spoiled = benchmark.current_step_spoiled();
        text.0 = if spoiled {
            format!("BENCHMARK  {status}\nTE MOVISTE — este paso queda inválido")
        } else {
            format!("BENCHMARK  {status}\nno te muevas")
        };
        *color = TextColor(if spoiled { SPOILED } else { RUNNING });
        *visibility = Visibility::Inherited;
        return;
    }

    // The flythrough moves the camera on purpose, so there is no "don't move".
    if let Some(status) = flythrough.status() {
        text.0 = format!("FLYTHROUGH  {status}");
        *color = TextColor(RUNNING);
        *visibility = Visibility::Inherited;
        return;
    }

    // The material breakdown is instantaneous, so its only feedback is this
    // notice; shown above the run-finished ones as the most recent action.
    if let Some(summary) = &materials.summary
        && time.elapsed_secs() - materials.at < NOTICE_SECS
    {
        text.0 = summary.clone();
        *color = TextColor(DONE);
        *visibility = Visibility::Inherited;
        return;
    }

    if let Some(finished) = &benchmark.finished
        && time.elapsed_secs() - finished.at < NOTICE_SECS
    {
        if let Some(reason) = finished.aborted {
            text.0 = format!("BENCHMARK ABORTADO\n{reason} — configuración restaurada");
            *color = TextColor(SPOILED);
        } else {
            text.0 = format!(
                "BENCHMARK TERMINADO\n{}/{} pasos válidos — tabla en el log",
                finished.valid, finished.total
            );
            *color = TextColor(DONE);
        }
        *visibility = Visibility::Inherited;
        return;
    }

    if let Some(finished) = &flythrough.finished
        && time.elapsed_secs() - finished.at < NOTICE_SECS
    {
        if let Some(reason) = finished.aborted {
            text.0 = format!("FLYTHROUGH ABORTADO\n{reason} — configuración restaurada");
            *color = TextColor(SPOILED);
        } else {
            text.0 = format!(
                "FLYTHROUGH TERMINADO\n{} tramos medidos — tabla en el log",
                finished.legs
            );
            *color = TextColor(DONE);
        }
        *visibility = Visibility::Inherited;
        return;
    }

    *visibility = Visibility::Hidden;
}
