use iced::widget::{button, center, column, container, mouse_area, opaque, row, stack, text};
use iced::{Color, Element, Length};

use crate::gui::Message;
use crate::gui_theme;
use crate::updater::ReleaseInfo;

// ---------------------------------------------------------------------------
// Phase & state
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppUpdatePhase {
    /// Showing the release notes and Update/Skip buttons.
    Prompt,
    /// Downloading + installing in progress.
    Updating,
    /// Download + install succeeded, waiting to restart.
    Completed,
    /// Download or install failed.
    Failed,
}

#[derive(Debug, Clone)]
pub struct AppUpdateState {
    pub release: ReleaseInfo,
    pub phase: AppUpdatePhase,
    pub status_message: Option<String>,
    pub spinner_tick: usize,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

/// Render the update-available modal over `base`.
/// Uses the same `stack! + opaque + mouse_area + center` pattern as `gui_modal`.
pub fn render_app_update_modal<'a>(
    base: impl Into<Element<'a, Message>>,
    state: &'a AppUpdateState,
    is_dark: bool,
) -> Element<'a, Message> {
    let card = build_update_card(state, is_dark);

    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(card)).style(|_theme| {
                iced::widget::container::Style {
                    background: Some(
                        Color {
                            a: 0.8,
                            r: 0.05,
                            g: 0.06,
                            b: 0.07,
                        }
                        .into(),
                    ),
                    ..iced::widget::container::Style::default()
                }
            }))
            .on_press(Message::CloseAppUpdateModal)
        )
    ]
    .into()
}

// ---------------------------------------------------------------------------
// Card builder
// ---------------------------------------------------------------------------

const SPINNER_CHARS: &[&str] = &["◐", "◓", "◑", "◒"];

fn build_update_card<'a>(state: &'a AppUpdateState, is_dark: bool) -> Element<'a, Message> {
    let (bg, border, text_primary, text_muted, text_bright) = if is_dark {
        (
            gui_theme::DARK_UPDATE_MODAL_BG,
            gui_theme::DARK_UPDATE_MODAL_BORDER,
            gui_theme::DARK_TEXT_PRIMARY,
            gui_theme::DARK_TEXT_MUTED,
            gui_theme::DARK_TEXT_PRIMARY,
        )
    } else {
        (
            gui_theme::LIGHT_UPDATE_MODAL_BG,
            gui_theme::LIGHT_UPDATE_MODAL_BORDER,
            gui_theme::LIGHT_TEXT_PRIMARY,
            gui_theme::LIGHT_TEXT_MUTED,
            gui_theme::LIGHT_TEXT_PRIMARY,
        )
    };

    let inner: Element<'_, Message> = match state.phase {
        AppUpdatePhase::Prompt => {
            build_prompt_content(&state.release, text_primary, text_muted, is_dark)
        }
        AppUpdatePhase::Updating => build_updating_content(state.spinner_tick, text_bright),
        AppUpdatePhase::Completed => build_completed_content(is_dark),
        AppUpdatePhase::Failed => build_failed_content(
            state.status_message.as_deref().unwrap_or("Unknown error"),
            is_dark,
        ),
    };

    container(inner)
        .padding(24)
        .width(Length::Fixed(520.0))
        .style(move |_theme| iced::widget::container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                color: border,
                width: 1.0,
                radius: 8.0.into(),
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Prompt phase
// ---------------------------------------------------------------------------

fn build_prompt_content<'a>(
    release: &'a ReleaseInfo,
    text_primary: Color,
    text_muted: Color,
    is_dark: bool,
) -> Element<'a, Message> {
    let title = text(format!("Update available: v{}", release.version))
        .size(20)
        .color(text_primary)
        .font(iced::font::Font::MONOSPACE);

    let body = text(&release.body).size(14).color(text_muted);

    let notes_scroll = column![body].spacing(4).height(Length::Fixed(160.0));

    let button_text = if is_dark {
        // Near-black on DARK_ACCENT (#5ab8e6): ≈ 8.4:1 contrast
        Color::from_rgb(
            0x10 as f32 / 255.0,
            0x14 as f32 / 255.0,
            0x18 as f32 / 255.0,
        )
    } else {
        // Off-white on LIGHT_ACCENT (#25668d): ≈ 5.6:1 contrast
        gui_theme::DARK_TEXT_PRIMARY
    };

    let update_btn = button(text("Update").color(button_text))
        .on_press(Message::StartAppUpdate)
        .padding(iced::Padding::from([6, 18]))
        .style(move |_theme, _status| iced::widget::button::Style {
            background: Some(
                if is_dark {
                    gui_theme::DARK_ACCENT
                } else {
                    gui_theme::LIGHT_ACCENT
                }
                .into(),
            ),
            text_color: button_text,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let skip_btn = button(text("Skip").color(text_muted))
        .on_press(Message::CloseAppUpdateModal)
        .padding(iced::Padding::from([6, 18]))
        .style(move |_theme, _status| iced::widget::button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: text_muted,
            border: iced::Border {
                radius: 6.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    column![title, notes_scroll, row![update_btn, skip_btn].spacing(10),]
        .spacing(16)
        .into()
}

// ---------------------------------------------------------------------------
// Updating phase
// ---------------------------------------------------------------------------

fn build_updating_content<'a>(spinner_tick: usize, text_bright: Color) -> Element<'a, Message> {
    let spinner_char = SPINNER_CHARS[spinner_tick % SPINNER_CHARS.len()];

    let spinner = text(spinner_char).size(32).color(text_bright);
    let label = text("Downloading and installing...")
        .size(16)
        .color(text_bright);

    column![center(spinner), center(label),]
        .spacing(12)
        .width(Length::Fill)
        .into()
}

// ---------------------------------------------------------------------------
// Completed phase
// ---------------------------------------------------------------------------

fn build_completed_content<'a>(is_dark: bool) -> Element<'a, Message> {
    let success_color = if is_dark {
        Color::from_rgb(0.3, 0.8, 0.3)
    } else {
        Color::from_rgb(0.15, 0.65, 0.15)
    };

    let checkmark = text("✓").size(32).color(success_color);
    let label = text("Update complete. Restarting...")
        .size(16)
        .color(success_color);

    column![center(checkmark), center(label),]
        .spacing(12)
        .into()
}

// ---------------------------------------------------------------------------
// Failed phase
// ---------------------------------------------------------------------------

fn build_failed_content<'a>(message: &'a str, is_dark: bool) -> Element<'a, Message> {
    let error_color = if is_dark {
        Color::from_rgb(0.9, 0.25, 0.25)
    } else {
        Color::from_rgb(0.8, 0.1, 0.1)
    };

    let cross = text("✗").size(32).color(error_color);
    let label = text(message).size(14).color(error_color);

    let close_btn = button(text("Close"))
        .on_press(Message::CloseAppUpdateModal)
        .padding(iced::Padding::from([6, 18]))
        .style(move |_theme, _status| iced::widget::button::Style {
            background: Some(Color::TRANSPARENT.into()),
            text_color: error_color,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    column![center(cross), center(label), center(close_btn),]
        .spacing(12)
        .into()
}
