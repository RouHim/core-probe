use std::collections::BTreeMap;
use std::time::Duration;

use iced::widget::{
    button, center, column, container, mouse_area, opaque, row, stack, text, text_input, Space,
};
use iced::{Color, Element, Length, Padding};

use crate::gui::{ErrorCategory, Message, ModalContent, ModalCoreResult};
use crate::gui_theme;
use crate::mce_monitor::MceError;

/// Public entry point for the modal overlay. Callers pass the base UI element,
/// the test-result content, and the current theme flag. The backdrop dismiss
/// behaviour and card layout are fully encapsulated here.
pub fn modal_overlay_view<'a>(
    base: Element<'a, Message>,
    content: &'a ModalContent,
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
                            r: 0.05,
                            g: 0.06,
                            b: 0.07,
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

fn build_result_card<'a>(content: &'a ModalContent, is_dark: bool) -> Element<'a, Message> {
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

    let has_unstable = !content.unstable_cores.is_empty();

    let title_str = match (content.interrupted, has_unstable) {
        (true, true) => "TEST INTERRUPTED - UNSTABLE CORES FOUND",
        (true, false) => "TEST INTERRUPTED",
        (false, true) => "TEST COMPLETE - UNSTABLE CORES FOUND",
        (false, false) => "TEST COMPLETE",
    };
    let title = text(title_str).size(20).color(text_primary);

    let mut body = column![title].spacing(12).padding(24).width(Length::Fill);

    // ── Summary badge bar ──
    if has_unstable {
        let bios_indices: Vec<u32> = content
            .unstable_cores
            .iter()
            .map(|c| c.bios_index)
            .collect();
        body = body.push(build_summary_bar(
            content.unstable_cores.len(),
            content.stable_core_indices.len(),
            &bios_indices,
            is_dark,
        ));

        // ── Group unstable cores by CCD ──
        let mut ccd_groups: BTreeMap<u32, Vec<&ModalCoreResult>> = BTreeMap::new();
        for c in &content.unstable_cores {
            ccd_groups.entry(c.ccd_index).or_default().push(c);
        }
        for (ccd_idx, cores) in &ccd_groups {
            body = body.push(build_ccd_group(*ccd_idx, cores, is_dark));
        }
    }

    // ── Stable core chips ──
    if !content.stable_core_indices.is_empty() {
        body = body.push(build_stable_chips(&content.stable_core_indices, is_dark));
    }

    // ── QR + next steps ──
    let qr_widget = crate::gui_qr::qr_code_view(&content.qr_content, is_dark, 5.0);
    let qr_container = container(qr_widget).width(Length::Shrink);
    let steps = build_next_steps(is_dark);
    let qr_row: Element<'a, Message> = row![qr_container, Space::new().width(16), steps]
        .align_y(iced::Alignment::Start)
        .into();
    body = body.push(qr_row);

    // ── System MCE events (informational, never attributed to a core) ──
    if !content.system_mce_errors.is_empty() {
        body = body.push(build_system_mce_section(
            &content.system_mce_errors,
            is_dark,
        ));
    }

    // ── Footer stats ──
    body = body.push(build_modal_footer(
        content.total_duration,
        content.iterations_completed,
        is_dark,
    ));

    // ── Buttons ──
    let close_btn: Element<'a, Message> = button(text("Close").size(14))
        .on_press(Message::DismissModal)
        .padding(Padding::from([6, 16]))
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
        .padding(Padding::from([6, 16]))
        .style(move |_theme, _status| button::Style {
            background: Some(
                if is_dark {
                    iced::Color::from_rgb(0.18, 0.35, 0.15)
                } else {
                    iced::Color::from_rgb(0.2, 0.45, 0.18)
                }
                .into(),
            ),
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
        .max_width(580)
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

// ─────────────────────────────────────────────────────────────────────────────
// Helper widgets
// ─────────────────────────────────────────────────────────────────────────────

/// Coloured summary bar: "● N UNSTABLE    ○ N STABLE"
/// Coloured summary bar: "● N UNSTABLE [bios, list]    ○ N STABLE"
fn build_summary_bar<'a>(
    unstable_count: usize,
    stable_count: usize,
    bios_indices: &[u32],
    is_dark: bool,
) -> Element<'a, Message> {
    let (unstable_color, stable_color) = if is_dark {
        (gui_theme::DARK_ERROR_BORDER, gui_theme::DARK_CHIP_BG)
    } else {
        (gui_theme::LIGHT_ERROR_BORDER, gui_theme::LIGHT_CHIP_BG)
    };
    let text_color = if is_dark {
        gui_theme::DARK_TEXT_PRIMARY
    } else {
        gui_theme::LIGHT_TEXT_PRIMARY
    };

    let comma_list: String = bios_indices
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");

    let inline_input: Element<'a, Message> = text_input("", &comma_list)
        .size(12)
        .font(iced::font::Font::MONOSPACE)
        .width(Length::Shrink)
        .padding(Padding::from([2, 4]))
        .style(move |_theme, _status| text_input::Style {
            background: Color::TRANSPARENT.into(),
            border: iced::Border::default(),
            icon: Color::TRANSPARENT,
            placeholder: Default::default(),
            value: text_color,
            selection: Default::default(),
        })
        .into();

    row![
        text("\u{25CF}").color(unstable_color).size(14),
        Space::new().width(4),
        text(format!("{unstable_count} UNSTABLE"))
            .color(text_color)
            .size(14)
            .font(iced::font::Font::MONOSPACE),
        Space::new().width(4),
        text("[").color(text_color).size(13),
        inline_input,
        text("]").color(text_color).size(13),
        Space::new().width(12),
        text("\u{25CB}").color(stable_color).size(14),
        Space::new().width(4),
        text(format!("{stable_count} STABLE"))
            .color(text_color)
            .size(14)
            .font(iced::font::Font::MONOSPACE),
    ]
    .into()
}

/// A CCD group container with header and failure cards.
fn build_ccd_group<'a>(
    ccd_index: u32,
    cores: &[&'a ModalCoreResult],
    is_dark: bool,
) -> Element<'a, Message> {
    let header_color = if is_dark {
        gui_theme::DARK_SECTION_HEADER
    } else {
        gui_theme::LIGHT_SECTION_HEADER
    };
    let header = text(format!("CCD{ccd_index}")).size(12).color(header_color);

    let ccd_bg = if is_dark {
        gui_theme::DARK_CCD_BG
    } else {
        gui_theme::LIGHT_CCD_BG
    };
    let ccd_border = if is_dark {
        gui_theme::DARK_CCD_BORDER
    } else {
        gui_theme::LIGHT_CCD_BORDER
    };

    let mut cards_col = column![header].spacing(6);
    for core in cores {
        cards_col = cards_col.push(build_failure_card(core, is_dark));
    }

    container(cards_col)
        .padding(12)
        .style(move |_theme| container::Style {
            background: Some(ccd_bg.into()),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: ccd_border,
            },
            ..container::Style::default()
        })
        .into()
}

/// A single failure card with accent-coloured left border and error-type badge.
fn build_failure_card<'a>(core: &'a ModalCoreResult, is_dark: bool) -> Element<'a, Message> {
    let (accent, card_bg, badge_bg, badge_text) = match (core.error_category, is_dark) {
        (ErrorCategory::Mprime, true) => (
            gui_theme::DARK_ERROR_BORDER,
            gui_theme::DARK_ERROR_BG,
            gui_theme::DARK_ERROR_BADGE_BG,
            gui_theme::DARK_ERROR_BADGE_TEXT,
        ),
        (ErrorCategory::Mprime, false) => (
            gui_theme::LIGHT_ERROR_BORDER,
            gui_theme::LIGHT_ERROR_BG,
            gui_theme::LIGHT_ERROR_BADGE_BG,
            gui_theme::LIGHT_ERROR_BADGE_TEXT,
        ),
        (ErrorCategory::MceOnly, true) => (
            gui_theme::DARK_MCE_BORDER,
            gui_theme::DARK_MCE_BG,
            gui_theme::DARK_MCE_BADGE_BG,
            gui_theme::DARK_MCE_BADGE_TEXT,
        ),
        (ErrorCategory::MceOnly, false) => (
            gui_theme::LIGHT_MCE_BORDER,
            gui_theme::LIGHT_MCE_BG,
            gui_theme::LIGHT_MCE_BADGE_BG,
            gui_theme::LIGHT_MCE_BADGE_TEXT,
        ),
    };

    let text_primary = if is_dark {
        gui_theme::DARK_TEXT_PRIMARY
    } else {
        gui_theme::LIGHT_TEXT_PRIMARY
    };
    let text_muted = if is_dark {
        gui_theme::DARK_TEXT_MUTED
    } else {
        gui_theme::LIGHT_TEXT_MUTED
    };

    let core_label = text(format!("Core {}", core.bios_index))
        .size(13)
        .color(text_primary);
    let separator = text("\u{00B7}").size(13).color(text_muted); // ·
    let badge = container(text(&core.error_summary).size(11).color(badge_text))
        .padding(Padding::from([2, 8]))
        .style(move |_theme| container::Style {
            background: Some(badge_bg.into()),
            border: iced::Border {
                radius: 10.0.into(),
                ..Default::default()
            },
            ..container::Style::default()
        });

    // 3 px accent stripe matching the inner row height (13 px text + 8*2 padding ≈ 32 px)
    let accent_bar = container(row![])
        .width(3)
        .height(Length::Fixed(32.0))
        .style(move |_theme| container::Style {
            background: Some(accent.into()),
            ..container::Style::default()
        });

    container(row![
        accent_bar,
        row![
            core_label,
            Space::new().width(6),
            separator,
            Space::new().width(6),
            badge
        ]
        .padding(Padding::from([8, 12]))
        .align_y(iced::Alignment::Center),
    ])
    .style(move |_theme| container::Style {
        background: Some(card_bg.into()),
        border: iced::Border {
            radius: 6.0.into(),
            ..Default::default()
        },
        ..container::Style::default()
    })
    .into()
}

/// Stable core pills row.
fn build_stable_chips<'a>(indices: &[u32], is_dark: bool) -> Element<'a, Message> {
    let chip_bg = if is_dark {
        gui_theme::DARK_CHIP_BG
    } else {
        gui_theme::LIGHT_CHIP_BG
    };
    let chip_text = if is_dark {
        gui_theme::DARK_CHIP_TEXT
    } else {
        gui_theme::LIGHT_CHIP_TEXT
    };

    let section_header = if is_dark {
        gui_theme::DARK_SECTION_HEADER
    } else {
        gui_theme::LIGHT_SECTION_HEADER
    };

    let header = text(format!("Stable Cores ({})", indices.len()))
        .size(12)
        .color(section_header);

    let mut chips_row = row![].spacing(4);
    for idx in indices {
        let chip = container(text(format!("{idx}")).size(12).color(chip_text))
            .padding(Padding::from([2, 8]))
            .style(move |_theme| container::Style {
                background: Some(chip_bg.into()),
                border: iced::Border {
                    radius: 10.0.into(),
                    ..Default::default()
                },
                ..container::Style::default()
            });
        chips_row = chips_row.push(chip);
    }

    column![header, chips_row].spacing(4).into()
}

/// Compact informational section for system-level MCE events (data fabric,
/// memory controller, shared L3, ...). These never fail a core; the section
/// is rendered only when such events were collected during the run.
fn build_system_mce_section<'a>(errors: &[MceError], is_dark: bool) -> Element<'a, Message> {
    let section_bg = if is_dark {
        gui_theme::DARK_MCE_BG
    } else {
        gui_theme::LIGHT_MCE_BG
    };
    let section_border = if is_dark {
        gui_theme::DARK_MCE_BORDER
    } else {
        gui_theme::LIGHT_MCE_BORDER
    };
    let text_muted = if is_dark {
        gui_theme::DARK_TEXT_MUTED
    } else {
        gui_theme::LIGHT_TEXT_MUTED
    };

    let header = text("System MCE events (not attributed to any core)")
        .size(12)
        .color(text_muted);

    let mut rows = column![].spacing(2);
    for error in errors {
        let bank = error
            .bank
            .map_or_else(|| "?".to_string(), |bank| bank.to_string());
        let label = match error.error_type {
            crate::mce_monitor::MceErrorType::MachineCheck => "Machine Check",
            crate::mce_monitor::MceErrorType::HardwareError => "Hardware Error",
            crate::mce_monitor::MceErrorType::EdacCorrectable => "EDAC correctable",
            crate::mce_monitor::MceErrorType::EdacUncorrectable => "EDAC uncorrectable",
            crate::mce_monitor::MceErrorType::Unknown => "Unknown",
        };
        rows = rows.push(
            text(format!(
                "Bank {bank} \u{00B7} {label} \u{00B7} {}",
                error.timestamp
            ))
            .size(11)
            .color(text_muted),
        );
    }

    container(column![header, rows].spacing(4))
        .padding(10)
        .width(Length::Fill)
        .style(move |_theme| container::Style {
            background: Some(section_bg.into()),
            border: iced::Border {
                radius: 6.0.into(),
                width: 1.0,
                color: section_border,
            },
            ..container::Style::default()
        })
        .into()
}

/// Numbered next-steps guide rendered beside the QR code.
fn build_next_steps<'a>(is_dark: bool) -> Element<'a, Message> {
    let text_color = if is_dark {
        gui_theme::DARK_TEXT_SECONDARY
    } else {
        gui_theme::LIGHT_TEXT_SECONDARY
    };

    let steps = [
        "Note unstable core numbers above",
        "Reboot to BIOS \u{2192} Curve Optimizer",
        "Increase CO offset for listed cores",
        "Re-run core-probe to verify",
    ];

    let header = text("Next steps:").size(13).color(text_color);
    let mut list = column![header].spacing(3);
    for (i, step) in steps.iter().enumerate() {
        let line = text(format!("{}. {step}", i + 1))
            .size(12)
            .color(text_color);
        list = list.push(line);
    }

    list.into()
}

/// Duration + iterations footer line.
fn build_modal_footer<'a>(
    duration: Duration,
    iterations: u32,
    is_dark: bool,
) -> Element<'a, Message> {
    let color = if is_dark {
        gui_theme::DARK_TEXT_MUTED
    } else {
        gui_theme::LIGHT_TEXT_MUTED
    };
    text(format!(
        "Duration: {} \u{00B7} Iterations: {iterations}",
        format_duration(duration)
    ))
    .size(12)
    .color(color)
    .font(iced::font::Font::MONOSPACE)
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
