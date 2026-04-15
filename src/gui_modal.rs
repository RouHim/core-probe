use std::time::Duration;

use iced::widget::{
    button, center, column, container, mouse_area, opaque, row, stack, text, Space,
};
use iced::{Color, Element, Length};

use crate::gui::{Message, ModalContent};
use crate::gui_theme;

/// Public entry point for the modal overlay. Callers pass the base UI element,
/// the test-result content, and the current theme flag. The backdrop dismiss
/// behaviour and card layout are fully encapsulated here.
pub fn modal_overlay_view<'a>(
    base: Element<'a, Message>,
    content: &ModalContent,
    is_dark: bool,
) -> Element<'a, Message> {
    let card = build_result_card(content, is_dark);
    modal(base, card, Message::DismissModal)
}

// ---------------------------------------------------------------------------
// Iced 0.14 modal helper (official example pattern)
// ---------------------------------------------------------------------------

fn modal<'a>(
    base: impl Into<Element<'a, Message>>,
    content: impl Into<Element<'a, Message>>,
    on_blur: Message,
) -> Element<'a, Message> {
    stack![
        base.into(),
        opaque(
            mouse_area(center(opaque(content)).style(|_theme| {
                container::Style {
                    background: Some(
                        Color {
                            a: 0.8,
                            ..Color::BLACK
                        }
                        .into(),
                    ),
                    ..container::Style::default()
                }
            }))
            .on_press(on_blur)
        )
    ]
    .into()
}

// ---------------------------------------------------------------------------
// Result card
// ---------------------------------------------------------------------------

fn build_result_card<'a>(content: &ModalContent, is_dark: bool) -> Element<'a, Message> {
    let card_bg = if is_dark {
        gui_theme::DARK_BG_SECONDARY
    } else {
        gui_theme::LIGHT_BG_SECONDARY
    };
    let card_border = if is_dark {
        gui_theme::DARK_CARD_BORDER
    } else {
        gui_theme::LIGHT_CARD_BORDER
    };
    let text_primary = if is_dark {
        gui_theme::DARK_TEXT_PRIMARY
    } else {
        gui_theme::LIGHT_TEXT_PRIMARY
    };
    let text_secondary = if is_dark {
        gui_theme::DARK_TEXT_SECONDARY
    } else {
        gui_theme::LIGHT_TEXT_SECONDARY
    };
    let btn_bg = if is_dark {
        gui_theme::DARK_BUTTON_BG
    } else {
        gui_theme::LIGHT_BUTTON_BG
    };
    let btn_text = if is_dark {
        gui_theme::DARK_BUTTON_TEXT
    } else {
        gui_theme::LIGHT_BUTTON_TEXT
    };
    let section_header_color = if is_dark {
        gui_theme::DARK_BADGE_PBO_TEXT
    } else {
        gui_theme::LIGHT_BADGE_PBO_TEXT
    };

    let has_unstable = !content.unstable_cores.is_empty();

    let title_str = if has_unstable {
        "TEST COMPLETE \u{2014} UNSTABLE CORES FOUND"
    } else {
        "TEST COMPLETE"
    };
    let title = text(title_str).size(20).color(text_primary);

    let mut body = column![title].spacing(12).padding(24).width(Length::Fill);

    if has_unstable {
        let header: Element<'a, Message> = text("Unstable Cores:")
            .size(14)
            .color(section_header_color)
            .into();
        body = body.push(header);

        for c in &content.unstable_cores {
            let line: Element<'a, Message> = text(format!(
                "\u{2022} Core {} (CCD{}) \u{2014} {}",
                c.bios_index, c.ccd_index, c.error_summary
            ))
            .size(13)
            .color(text_primary)
            .into();
            body = body.push(line);
        }
    }

    let stable_list = if content.stable_core_indices.is_empty() {
        "None".to_string()
    } else {
        content
            .stable_core_indices
            .iter()
            .map(|i| i.to_string())
            .collect::<Vec<_>>()
            .join(", ")
    };
    let stable_line: Element<'a, Message> = text(format!("Stable: {stable_list}"))
        .size(13)
        .color(text_secondary)
        .into();
    body = body.push(stable_line);

    let duration_line: Element<'a, Message> = text(format!(
        "Duration: {} | Iterations: {}",
        format_duration(content.total_duration),
        content.iterations_completed
    ))
    .size(13)
    .color(text_secondary)
    .into();
    body = body.push(duration_line);

    let qr_placeholder: Element<'a, Message> =
        container(text("[ QR Code ]").size(14).color(text_secondary))
            .width(Length::Fixed(180.0))
            .height(Length::Fixed(180.0))
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(
                    if is_dark {
                        gui_theme::DARK_BG_TERTIARY
                    } else {
                        gui_theme::LIGHT_BG_TERTIARY
                    }
                    .into(),
                ),
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: card_border,
                },
                ..container::Style::default()
            })
            .into();
    body = body.push(qr_placeholder);

    let instruction: Element<'a, Message> =
        text("Scan QR code with phone before rebooting to BIOS")
            .size(12)
            .color(text_secondary)
            .into();
    body = body.push(instruction);

    let close_btn: Element<'a, Message> = button(text("Close").size(14))
        .on_press(Message::DismissModal)
        .padding(iced::Padding::from([6, 16]))
        .style(move |_theme, _status| button::Style {
            background: Some(btn_bg.into()),
            text_color: btn_text,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

    let reboot_btn: Element<'a, Message> = button(text("Reboot to BIOS").size(14))
        .on_press(Message::RebootToFirmware)
        .padding(iced::Padding::from([6, 16]))
        .style(|_theme, _status| button::Style {
            background: Some(iced::Color::from_rgb(0.18, 0.35, 0.15).into()),
            text_color: iced::Color::WHITE,
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        })
        .into();

    let button_row: Element<'a, Message> =
        row![close_btn, Space::new().width(Length::Fill), reboot_btn]
            .width(Length::Fill)
            .into();
    body = body.push(button_row);

    container(body)
        .max_width(500)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(card_bg.into()),
            border: iced::Border {
                radius: 8.0.into(),
                width: 1.0,
                color: card_border,
            },
            ..container::Style::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Duration formatting
// ---------------------------------------------------------------------------

fn format_duration(d: Duration) -> String {
    let total_secs = d.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_zero_duration_when_formatting_then_shows_seconds() {
        assert_eq!(format_duration(Duration::from_secs(0)), "0s");
    }

    #[test]
    fn given_seconds_only_when_formatting_then_shows_seconds() {
        assert_eq!(format_duration(Duration::from_secs(45)), "45s");
    }

    #[test]
    fn given_minutes_and_seconds_when_formatting_then_shows_both() {
        assert_eq!(format_duration(Duration::from_secs(125)), "2m 5s");
    }

    #[test]
    fn given_hours_and_minutes_when_formatting_then_shows_hm() {
        assert_eq!(format_duration(Duration::from_secs(3660)), "1h 1m");
    }

    #[test]
    fn given_exact_hour_when_formatting_then_shows_zero_minutes() {
        assert_eq!(format_duration(Duration::from_secs(3600)), "1h 0m");
    }
}
