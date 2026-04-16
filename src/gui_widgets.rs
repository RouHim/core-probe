use std::collections::BTreeMap;

use iced::widget::grid;
use iced::widget::tooltip::Position as TooltipPosition;
use iced::widget::{
    button, column, container, pick_list, progress_bar, row, scrollable, text, text_input, tooltip,
    Space,
};
use iced::{Element, Length, Padding};
use iced_anim::spring::Motion;
use iced_anim::AnimationBuilder;

use crate::coordinator::CoreStatus;
use crate::cpu_topology::CpuTopology;
use crate::error_parser::{MprimeError, MprimeErrorType};
use crate::gui::{
    self, ConfigField, CoreResultInfo, LogEntry, Message, PerCoreProgress, TestConfig, TestProgress,
};
use crate::gui_events::LogLevel;
use crate::gui_theme;
use crate::mprime_config::StressTestMode;
use crate::uefi_reader::{PboLimits, UefiSettings};

// ---------------------------------------------------------------------------
// StressTestMode Display + pick_list support
// ---------------------------------------------------------------------------

const STRESS_MODE_OPTIONS: &[StressTestMode] = &[
    StressTestMode::SSE,
    StressTestMode::AVX,
    StressTestMode::AVX2,
    StressTestMode::AVX512,
];

impl std::fmt::Display for StressTestMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::SSE => write!(f, "SSE"),
            Self::AVX => write!(f, "AVX"),
            Self::AVX2 => write!(f, "AVX2"),
            Self::AVX512 => write!(f, "AVX512"),
            Self::Custom { .. } => write!(f, "Custom"),
        }
    }
}

// ---------------------------------------------------------------------------
// header_view
// ---------------------------------------------------------------------------

pub fn header_view<'a>(
    topology: &'a CpuTopology,
    uefi: &Option<UefiSettings>,
    is_dark: bool,
) -> Element<'a, Message> {
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
    let header_bg = if is_dark {
        gui_theme::DARK_HEADER_BG
    } else {
        gui_theme::LIGHT_HEADER_BG
    };

    // Left section: CPU model + core/thread count badge
    let core_thread_badge = format!(
        "{}C/{}T",
        topology.physical_core_count, topology.logical_cpu_count
    );
    let left = column![
        text(&topology.model_name).size(20).color(text_primary),
        text(core_thread_badge).size(14).color(text_secondary),
    ]
    .spacing(4)
    .width(Length::FillPortion(3));

    // Right section: AGESA + PBO badge + limits
    let right = match uefi {
        Some(settings) => build_uefi_section(settings, is_dark),
        None => column![text("UEFI: Unavailable").size(13).color(text_secondary)]
            .spacing(4)
            .width(Length::FillPortion(2)),
    };

    let theme_label = if is_dark { "\u{263e}" } else { "\u{2600}" };
    let theme_btn = button(text(theme_label).size(18))
        .on_press(Message::ThemeToggle)
        .padding(Padding::from([4, 8]));

    let content = row![left, right, theme_btn]
        .spacing(16)
        .padding(Padding::from(12))
        .align_y(iced::Alignment::Center);

    container(content)
        .width(Length::Fill)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(header_bg.into()),
            ..Default::default()
        })
        .into()
}

fn build_uefi_section<'a>(
    settings: &UefiSettings,
    is_dark: bool,
) -> iced::widget::Column<'a, Message> {
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
    let pbo_badge_bg = if is_dark {
        gui_theme::DARK_BADGE_PBO_BG
    } else {
        gui_theme::LIGHT_BADGE_PBO_BG
    };
    let pbo_badge_text = if is_dark {
        gui_theme::DARK_BADGE_PBO_TEXT
    } else {
        gui_theme::LIGHT_BADGE_PBO_TEXT
    };

    let mut col = column![].spacing(4).width(Length::FillPortion(2));

    // AGESA version
    let agesa_label = match &settings.agesa_version {
        Some(ver) => format!("AGESA: {ver}"),
        None => "AGESA: N/A".to_string(),
    };
    col = col.push(text(agesa_label).size(13).color(text_secondary));

    // PBO badge
    let pbo_text_val = classify_pbo_badge(settings.pbo_status.as_deref());

    let (badge_bg, badge_fg) = match pbo_text_val {
        "PBO: ENABLED" => (pbo_badge_bg, pbo_badge_text),
        "PBO: DISABLED" => (
            iced::Color::from_rgb(0.4, 0.1, 0.1),
            iced::Color::from_rgb(1.0, 0.7, 0.7),
        ),
        "PBO: AUTO" => (
            if is_dark {
                iced::Color::from_rgb(0.16, 0.18, 0.24)
            } else {
                iced::Color::from_rgb(0.88, 0.90, 0.94)
            },
            if is_dark {
                iced::Color::from_rgb(0.72, 0.78, 0.92)
            } else {
                iced::Color::from_rgb(0.35, 0.40, 0.52)
            },
        ),
        _ => (
            if is_dark {
                gui_theme::DARK_BG_TERTIARY
            } else {
                gui_theme::LIGHT_BG_TERTIARY
            },
            text_primary,
        ),
    };

    let pbo_badge = container(text(pbo_text_val).size(12).color(badge_fg))
        .padding(Padding::from([2, 8]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(badge_bg.into()),
            border: iced::Border {
                radius: 4.0.into(),
                ..Default::default()
            },
            ..Default::default()
        });

    let hint = pbo_tooltip_hint(pbo_text_val);
    let pbo_with_tooltip = tooltip(pbo_badge, text(hint).size(12), TooltipPosition::Bottom)
        .gap(4)
        .style(move |theme: &iced::Theme| {
            let _ = theme;
            container::Style {
                background: Some(
                    if is_dark {
                        iced::Color::from_rgb(0.15, 0.15, 0.18)
                    } else {
                        iced::Color::from_rgb(0.97, 0.97, 0.97)
                    }
                    .into(),
                ),
                border: iced::Border {
                    radius: 4.0.into(),
                    width: 1.0,
                    color: if is_dark {
                        iced::Color::from_rgb(0.3, 0.3, 0.35)
                    } else {
                        iced::Color::from_rgb(0.8, 0.8, 0.8)
                    },
                },
                ..Default::default()
            }
        });
    col = col.push(pbo_with_tooltip);

    // PBO limits if available
    if let Some(limits) = &settings.pbo_limits {
        col = col.push(build_limits_row(limits, text_secondary));
    }

    col
}

fn classify_pbo_badge(status: Option<&str>) -> &'static str {
    let Some(raw) = status else {
        return "PBO: UNKNOWN";
    };
    let upper = raw.to_ascii_uppercase();
    if upper.contains("DISABLED") {
        "PBO: DISABLED"
    } else if upper.contains("AUTO") {
        "PBO: AUTO"
    } else if upper.contains("ENABLED")
        || upper.contains("ADVANCED")
        || upper.contains("MOTHERBOARD")
        || upper.contains("MANUAL")
        || upper.contains("ECO")
    {
        "PBO: ENABLED"
    } else {
        "PBO: UNKNOWN"
    }
}

fn pbo_tooltip_hint(badge_label: &str) -> &'static str {
    match badge_label {
        "PBO: ENABLED" => "PBO is actively enabled — cores boost beyond stock limits.\nStability test results are most meaningful in this mode.",
        "PBO: DISABLED" => "PBO is disabled — cores run at stock frequencies.\nStability issues are unlikely; test results may be less informative.",
        "PBO: AUTO" => "PBO is set to BIOS default — the motherboard decides\nwhether to enable boosting. Actual behavior varies by vendor.",
        _ => "PBO status could not be determined from UEFI settings.",
    }
}

fn build_limits_row<'a>(limits: &PboLimits, color: iced::Color) -> Element<'a, Message> {
    let mut parts = Vec::new();
    if let Some(ppt) = &limits.ppt_limit {
        parts.push(format!("PPT:{ppt}"));
    }
    if let Some(tdc) = &limits.tdc_limit {
        parts.push(format!("TDC:{tdc}"));
    }
    if let Some(edc) = &limits.edc_limit {
        parts.push(format!("EDC:{edc}"));
    }
    let label = if parts.is_empty() {
        "Limits: N/A".to_string()
    } else {
        parts.join(" / ")
    };
    text(label).size(12).color(color).into()
}

// ---------------------------------------------------------------------------
// format_error_summary
// ---------------------------------------------------------------------------

pub fn format_error_summary(errors: &[MprimeError]) -> Option<String> {
    let first = errors.first()?;
    let label = match first.error_type {
        MprimeErrorType::RoundoffError => "ROUNDOFF",
        MprimeErrorType::HardwareFailure => "HW FAIL",
        MprimeErrorType::FatalError => "FATAL",
        MprimeErrorType::IllegalSumout => "SUMOUT",
        MprimeErrorType::SumMismatch => "SUM ERR",
        MprimeErrorType::TortureTestFailed => "TORTURE FAIL",
        MprimeErrorType::TortureTestSummaryError => "ERRORS",
        MprimeErrorType::PossibleHardwareFailure => "HW FAIL?",
        MprimeErrorType::Unknown => "ERROR",
    };
    match first.fft_size {
        Some(fft) => Some(format!("{label} @ {fft}K")),
        None => Some(label.to_string()),
    }
}

// ---------------------------------------------------------------------------
// core_tile_view
// ---------------------------------------------------------------------------

pub struct CoreTileData<'a> {
    pub bios_index: u32,
    pub physical_core_id: u32,
    pub status: &'a CoreStatus,
    pub co_offset: Option<i32>,
    pub per_core_progress: Option<&'a PerCoreProgress>,
    pub error_summary: Option<String>,
    pub is_dark: bool,
    pub greyed_out: bool,
    pub amd_generation: crate::co_tier::AmdGeneration,
}

fn format_time_mm_ss(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

fn co_badge<'a>(
    label: String,
    bg: iced::Color,
    border: iced::Color,
    fg: iced::Color,
) -> Element<'a, Message> {
    container(text(label).size(12).color(fg))
        .padding(Padding::from([2, 8]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: border,
            },
            ..Default::default()
        })
        .into()
}

pub fn core_tile_view<'a>(data: &CoreTileData<'a>) -> Element<'a, Message> {
    if data.greyed_out {
        let bg = gui_theme::greyed_bg_color(data.is_dark);
        let fg = gui_theme::greyed_text_color(data.is_dark);
        let secondary_color = gui_theme::greyed_text_color(data.is_dark);

        let (icon, label) = match data.status {
            CoreStatus::Passed => ("\u{2713}", "STABLE"),
            CoreStatus::Failed => ("\u{2717}", "UNSTABLE"),
            CoreStatus::Testing => ("\u{25b6}", "TESTING"),
            CoreStatus::Skipped => ("\u{2298}", "SKIPPED"),
            CoreStatus::Idle => ("\u{25cb}", "IDLE"),
            CoreStatus::Interrupted => ("\u{26a0}", "INTERRUPTED"),
        };

        let mut top_row = row![
            text(format!("Core {}", data.bios_index)).size(22).color(fg),
            Space::new().width(Length::Fill)
        ]
        .align_y(iced::Alignment::Center);
        if let Some(offset) = data.co_offset {
            let badge = co_badge(
                format!("CO: {offset}"),
                if data.is_dark {
                    gui_theme::DARK_BG_TERTIARY
                } else {
                    gui_theme::LIGHT_BG_TERTIARY
                },
                if data.is_dark {
                    gui_theme::DARK_CARD_BORDER
                } else {
                    gui_theme::LIGHT_CARD_BORDER
                },
                secondary_color,
            );
            top_row = top_row.push(badge);
        }

        let phys_label = text(format!("Core ID {}", data.physical_core_id))
            .size(11)
            .color(secondary_color);

        let status_row: Element<'a, Message> = if *data.status == CoreStatus::Idle {
            Space::new().height(Length::Fixed(20.0)).into()
        } else {
            container(
                row![
                    text(icon).size(18).color(fg),
                    text(label).size(13).color(fg)
                ]
                .spacing(6)
                .align_y(iced::Alignment::Center),
            )
            .width(Length::Fill)
            .center_x(Length::Fill)
            .into()
        };

        let mut col = column![top_row, phys_label].spacing(4);

        let progress_section: Element<'a, Message> =
            column![Space::new().height(Length::Fixed(6.0)), text(" ").size(12)]
                .spacing(4)
                .into();
        col = col.push(progress_section);

        col = col.push(Space::new().height(Length::Fixed(14.0)));

        col = col.push(status_row);

        let border_color = gui_theme::status_border_color(data.status, data.is_dark);
        let border_width = if *data.status == CoreStatus::Failed {
            2.0
        } else {
            1.0
        };

        return container(col)
            .width(Length::Fill)
            .padding(Padding::from([10, 12]))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(bg.into()),
                border: iced::Border {
                    radius: 6.0.into(),
                    width: border_width,
                    color: border_color,
                },
                ..Default::default()
            })
            .into();
    }

    let bg = gui_theme::status_bg_color(data.status, data.is_dark);
    let fg = gui_theme::status_text_color(data.status, data.is_dark);
    let secondary_color = if data.is_dark {
        gui_theme::DARK_TEXT_SECONDARY
    } else {
        gui_theme::LIGHT_TEXT_SECONDARY
    };
    let border_color = gui_theme::status_border_color(data.status, data.is_dark);

    let ratio: f32 = if *data.status == CoreStatus::Testing {
        if let Some(pc) = data.per_core_progress {
            if pc.duration_secs > 0 {
                (pc.elapsed_secs as f32 / pc.duration_secs as f32).clamp(0.0, 1.0)
            } else {
                0.0
            }
        } else {
            0.0
        }
    } else {
        0.0
    };

    let time_str = if *data.status == CoreStatus::Testing {
        if let Some(pc) = data.per_core_progress {
            format!(
                "{} / {}",
                format_time_mm_ss(pc.elapsed_secs),
                format_time_mm_ss(pc.duration_secs)
            )
        } else {
            String::new()
        }
    } else {
        String::new()
    };

    let border_width: f32 = if *data.status == CoreStatus::Failed {
        2.0
    } else {
        1.0
    };

    let (icon, label) = match data.status {
        CoreStatus::Passed => ("\u{2713}", "STABLE"),
        CoreStatus::Failed => ("\u{2717}", "UNSTABLE"),
        CoreStatus::Testing => ("\u{25b6}", "TESTING"),
        CoreStatus::Skipped => ("\u{2298}", "SKIPPED"),
        CoreStatus::Idle => ("\u{25cb}", "IDLE"),
        CoreStatus::Interrupted => ("\u{26a0}", "INTERRUPTED"),
    };

    let bios_index = data.bios_index;
    let physical_core_id = data.physical_core_id;
    let co_offset = data.co_offset;
    let is_dark = data.is_dark;
    let amd_generation = data.amd_generation;
    let status = data.status.clone();
    AnimationBuilder::new(
        ((bg, fg, border_color, secondary_color), ratio),
        move |((bg, fg, border_color, secondary_color), ratio)| {
            let mut top_row = row![
                text(format!("Core {}", bios_index)).size(22).color(fg),
                Space::new().width(Length::Fill)
            ]
            .align_y(iced::Alignment::Center);
            if let Some(offset) = co_offset {
                let tier = crate::co_tier::classify_co(offset, amd_generation);
                let badge = co_badge(
                    format!("CO: {offset}"),
                    crate::gui_theme::co_tier_badge_background(&tier, is_dark),
                    crate::gui_theme::co_tier_badge_border(&tier, is_dark),
                    crate::gui_theme::co_tier_color(&tier, is_dark),
                );
                top_row = top_row.push(badge);
            }

            let phys_label = text(format!("Core ID {}", physical_core_id))
                .size(11)
                .color(secondary_color);

            let status_row: Element<'_, Message> = if status == CoreStatus::Idle {
                Space::new().height(Length::Fixed(20.0)).into()
            } else {
                container(
                    row![
                        text(icon).size(18).color(fg),
                        text(label).size(13).color(fg)
                    ]
                    .spacing(6)
                    .align_y(iced::Alignment::Center),
                )
                .width(Length::Fill)
                .center_x(Length::Fill)
                .into()
            };

            let mut col = column![top_row, phys_label].spacing(4);

            let progress_section: Element<'_, Message> = if status == CoreStatus::Testing {
                if !time_str.is_empty() {
                    column![
                        container(progress_bar(0.0..=1.0, ratio)).height(Length::Fixed(6.0)),
                        text(time_str.clone()).size(12).color(secondary_color)
                    ]
                    .spacing(4)
                    .into()
                } else {
                    column![Space::new().height(Length::Fixed(6.0)), text(" ").size(12)]
                        .spacing(4)
                        .into()
                }
            } else {
                column![Space::new().height(Length::Fixed(6.0)), text(" ").size(12)]
                    .spacing(4)
                    .into()
            };
            col = col.push(progress_section);

            col = col.push(Space::new().height(Length::Fixed(14.0)));

            col = col.push(status_row);

            container(col)
                .width(Length::Fill)
                .padding(Padding::from([10, 12]))
                .style(move |_theme: &iced::Theme| container::Style {
                    background: Some(bg.into()),
                    border: iced::Border {
                        radius: 6.0.into(),
                        width: border_width,
                        color: border_color,
                    },
                    ..Default::default()
                })
                .into()
        },
    )
    .animates_layout(false)
    .animation(Motion::SMOOTH)
    .into()
}

// ---------------------------------------------------------------------------
// group_cores_by_ccd
// ---------------------------------------------------------------------------

pub fn group_cores_by_ccd(core_map: &BTreeMap<u32, Vec<u32>>) -> Vec<(String, Vec<u32>)> {
    let core_ids: Vec<u32> = core_map.keys().copied().collect();
    if core_ids.is_empty() {
        return vec![];
    }

    let mut groups: Vec<(String, Vec<u32>)> = Vec::new();
    let mut current_group: Vec<u32> = vec![core_ids[0]];

    for window in core_ids.windows(2) {
        let prev = window[0];
        let curr = window[1];
        if curr - prev > 1 {
            // Gap detected — start new CCD group
            let label = format!("CCD{}", groups.len());
            groups.push((label, std::mem::take(&mut current_group)));
        }
        current_group.push(curr);
    }

    // Push final group
    let label = format!("CCD{}", groups.len());
    groups.push((label, current_group));

    groups
}

// ---------------------------------------------------------------------------
// topology_grid_view
// ---------------------------------------------------------------------------

#[allow(clippy::too_many_arguments)]
pub fn topology_grid_view<'a>(
    topology: &'a CpuTopology,
    statuses: &'a BTreeMap<u32, CoreStatus>,
    _progress: &TestProgress,
    uefi: &Option<UefiSettings>,
    is_dark: bool,
    selected_cores: &Option<Vec<u32>>,
    core_progress: &'a BTreeMap<u32, PerCoreProgress>,
    core_results: &'a BTreeMap<u32, CoreResultInfo>,
) -> Element<'a, Message> {
    let text_primary = if is_dark {
        gui_theme::DARK_TEXT_PRIMARY
    } else {
        gui_theme::LIGHT_TEXT_PRIMARY
    };
    let bg_secondary = if is_dark {
        gui_theme::DARK_BG_SECONDARY
    } else {
        gui_theme::LIGHT_BG_SECONDARY
    };

    let ccd_groups = group_cores_by_ccd(&topology.core_map);
    let amd_generation = crate::co_tier::detect_generation(&topology.model_name);
    let mut main_col = column![].spacing(12);

    for (ccd_label, core_ids) in ccd_groups {
        let label = text(ccd_label).size(14).color(text_primary);

        let mut grid = grid::Grid::new()
            .fluid(200.0)
            .height(grid::Sizing::EvenlyDistribute(Length::Shrink))
            .spacing(8.0);

        for core_id in &core_ids {
            let status = statuses.get(core_id).unwrap_or(&CoreStatus::Idle);

            let co_offset = uefi
                .as_ref()
                .and_then(|u| u.curve_optimizer_offsets.as_ref())
                .and_then(|m| {
                    let bios_idx = topology.bios_index(*core_id)?;
                    m.get(&bios_idx)
                })
                .copied();

            let greyed_out = selected_cores
                .as_ref()
                .is_some_and(|cores| !cores.contains(core_id));

            let bios_idx = topology.bios_index(*core_id).unwrap_or(*core_id);

            let per_core = core_progress.get(core_id);
            let error_summary = core_results
                .get(core_id)
                .and_then(|info| format_error_summary(&info.mprime_errors));

            let tile_data = CoreTileData {
                bios_index: bios_idx,
                physical_core_id: *core_id,
                status,
                co_offset,
                per_core_progress: per_core,
                error_summary,
                is_dark,
                greyed_out,
                amd_generation,
            };

            grid = grid.push(core_tile_view(&tile_data));
        }

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

        let grid_container = container(grid)
            .width(Length::Fill)
            .padding(Padding::from(10))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(ccd_bg.into()),
                border: iced::Border {
                    radius: 8.0.into(),
                    width: 1.0,
                    color: ccd_border,
                },
                ..Default::default()
            });

        let ccd_section = column![label, grid_container].spacing(6);
        main_col = main_col.push(ccd_section);
    }

    let topo_border = if is_dark {
        gui_theme::DARK_CARD_BORDER
    } else {
        gui_theme::LIGHT_CARD_BORDER
    };

    container(main_col)
        .width(Length::FillPortion(3))
        .padding(Padding::from(8))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg_secondary.into()),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: topo_border,
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// format_estimated_runtime
// ---------------------------------------------------------------------------

fn format_estimated_runtime(config: &TestConfig, topology: Option<&CpuTopology>) -> String {
    let core_count = estimate_core_count(config, topology);
    if core_count == 0 {
        return String::new();
    }
    let per_core_secs = gui::parse_duration(&config.duration).as_secs();
    let total_secs = core_count as u64 * per_core_secs * config.iterations as u64;
    format_duration_label(total_secs)
}

fn estimate_core_count(config: &TestConfig, topology: Option<&CpuTopology>) -> usize {
    let trimmed = config.cores.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
        return topology.map_or(0, |t| t.core_map.len());
    }
    let mut seen = std::collections::BTreeSet::new();
    for token in trimmed.split(',') {
        let token = token.trim();
        if !token.is_empty() {
            seen.insert(token);
        }
    }
    seen.len()
}

fn format_duration_label(total_secs: u64) -> String {
    if total_secs == 0 {
        return String::new();
    }
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600).div_ceil(60);
    if hours > 0 && minutes > 0 {
        format!("~{hours}h {minutes}m")
    } else if hours > 0 {
        format!("~{hours}h")
    } else {
        format!("~{minutes}m")
    }
}

// ---------------------------------------------------------------------------
// config_panel_view
// ---------------------------------------------------------------------------

pub fn config_panel_view<'a>(
    config: &'a TestConfig,
    test_running: bool,
    is_dark: bool,
    topology: Option<&CpuTopology>,
) -> Element<'a, Message> {
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
    let bg = if is_dark {
        gui_theme::DARK_BG_SECONDARY
    } else {
        gui_theme::LIGHT_BG_SECONDARY
    };

    // Duration input
    let duration_label = text("Duration").size(13).color(text_secondary);
    let mut duration_input = text_input("6m", &config.duration);
    if !test_running {
        duration_input =
            duration_input.on_input(|s| Message::ConfigChanged(ConfigField::Duration(s)));
    }

    // Iterations input
    let iterations_label = text("Iterations").size(13).color(text_secondary);
    let iterations_str = config.iterations.to_string();
    let mut iterations_input = text_input("3", &iterations_str);
    if !test_running {
        iterations_input = iterations_input.on_input(|s| {
            let n = s.parse::<u32>().unwrap_or(config.iterations);
            Message::ConfigChanged(ConfigField::Iterations(n))
        });
    }

    // Mode pick_list
    let mode_label = text("Mode").size(13).color(text_secondary);
    let mode_picker: Element<'a, Message> = if test_running {
        // Show read-only text when running
        text(config.mode.to_string())
            .size(14)
            .color(text_primary)
            .into()
    } else {
        pick_list(STRESS_MODE_OPTIONS, Some(config.mode), |m| {
            Message::ConfigChanged(ConfigField::Mode(m))
        })
        .into()
    };

    // Cores input
    let cores_label = tooltip(
        text("Cores").size(13).color(text_secondary),
        text("Leave empty or type \"all\" to test every core.\nTo test specific cores, enter comma-separated IDs: 0,1,5,8")
            .size(12),
        TooltipPosition::Top,
    )
    .gap(4)
    .style(move |_theme: &iced::Theme| container::Style {
        background: Some(
            if is_dark {
                iced::Color::from_rgb(0.15, 0.15, 0.18)
            } else {
                iced::Color::from_rgb(0.97, 0.97, 0.97)
            }
            .into(),
        ),
        border: iced::Border {
            radius: 4.0.into(),
            width: 1.0,
            color: if is_dark {
                iced::Color::from_rgb(0.3, 0.3, 0.35)
            } else {
                iced::Color::from_rgb(0.8, 0.8, 0.8)
            },
        },
        ..Default::default()
    });
    let mut cores_input = text_input("all", &config.cores);
    if !test_running {
        cores_input = cores_input.on_input(|s| Message::ConfigChanged(ConfigField::Cores(s)));
    }

    // Start / Stop button
    let action_button: Element<'a, Message> = if test_running {
        button(text("\u{25a0} Stop Test").size(14))
            .on_press(Message::StopTest)
            .style(|_theme, _status| button::Style {
                background: Some(iced::Color::from_rgb(0.6, 0.15, 0.15).into()),
                text_color: iced::Color::WHITE,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    } else {
        button(text("\u{25b6} Start Test").size(14))
            .on_press(Message::StartTest)
            .style(|_theme, _status| button::Style {
                background: Some(iced::Color::from_rgb(0.18, 0.35, 0.15).into()),
                text_color: iced::Color::WHITE,
                border: iced::Border {
                    radius: 4.0.into(),
                    ..Default::default()
                },
                ..Default::default()
            })
            .into()
    };

    let panel = column![
        text("Configuration").size(16).color(text_primary),
        Space::new().height(8),
        duration_label,
        duration_input,
        Space::new().height(4),
        iterations_label,
        iterations_input,
        Space::new().height(4),
        mode_label,
        mode_picker,
        Space::new().height(4),
        cores_label,
        cores_input,
        Space::new().height(12),
        {
            let estimation = format_estimated_runtime(config, topology);
            if estimation.is_empty() {
                row![action_button]
                    .align_y(iced::Alignment::Center)
                    .spacing(8)
            } else {
                row![
                    action_button,
                    text(estimation).size(13).color(text_secondary),
                ]
                .align_y(iced::Alignment::Center)
                .spacing(8)
            }
        },
    ]
    .spacing(2)
    .padding(Padding::from(12));

    let border_color = if is_dark {
        gui_theme::DARK_CARD_BORDER
    } else {
        gui_theme::LIGHT_CARD_BORDER
    };

    container(panel)
        .width(Length::FillPortion(2))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(bg.into()),
            border: iced::Border {
                radius: 4.0.into(),
                width: 1.0,
                color: border_color,
            },
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// log_feed_view
// ---------------------------------------------------------------------------

pub fn log_feed_view<'a>(entries: &'a [LogEntry], is_dark: bool) -> Element<'a, Message> {
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
    let log_bg = if is_dark {
        gui_theme::DARK_LOG_BG
    } else {
        gui_theme::LIGHT_LOG_BG
    };

    let mut log_col = column![].spacing(2);

    for entry in entries {
        let level_color = gui_theme::log_level_color(&entry.level, is_dark);
        let level_str = match &entry.level {
            LogLevel::Stable => "STABLE",
            LogLevel::Error => "ERROR",
            LogLevel::Mce => "MCE",
            LogLevel::Default => "INFO",
        };

        let entry_row = row![
            text(format!("[{}]", entry.timestamp))
                .size(11)
                .color(text_secondary),
            container(text(level_str).size(10).color(level_color)).padding(Padding::from([1, 4])),
            text(&entry.message).size(12).color(text_primary),
        ]
        .spacing(8);

        log_col = log_col.push(entry_row);
    }

    let scroll = scrollable(log_col)
        .id(iced::widget::Id::new("log_feed"))
        .width(Length::Fill)
        .height(Length::Fill);

    let log_border = if is_dark {
        gui_theme::DARK_CARD_BORDER
    } else {
        gui_theme::LIGHT_CARD_BORDER
    };

    let section = column![
        text("Log Output").size(14).color(text_primary),
        container(scroll)
            .width(Length::Fill)
            .padding(Padding::from(8))
            .style(move |_theme: &iced::Theme| container::Style {
                background: Some(log_bg.into()),
                border: iced::Border {
                    radius: 2.0.into(),
                    width: 1.0,
                    color: log_border,
                },
                ..Default::default()
            }),
    ]
    .width(Length::Fill)
    .spacing(4);

    section.into()
}

// ---------------------------------------------------------------------------
// status_bar_view
// ---------------------------------------------------------------------------

pub fn status_bar_view<'a>(
    progress: &TestProgress,
    test_running: bool,
    is_dark: bool,
) -> Element<'a, Message> {
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
    let status_bg = if is_dark {
        gui_theme::DARK_STATUS_BAR_BG
    } else {
        gui_theme::LIGHT_STATUS_BAR_BG
    };

    // Status text
    let status_text = if test_running {
        match progress.current_core {
            Some(core) => format!(
                "Testing Core {} \u{2014} {}/{} done",
                core, progress.cores_completed, progress.total_cores
            ),
            None => "Starting test...".to_string(),
        }
    } else if progress.total_cores > 0 && progress.cores_completed >= progress.total_cores {
        format!(
            "Complete \u{2014} {}/{} cores tested",
            progress.cores_completed, progress.total_cores
        )
    } else {
        "Ready to test".to_string()
    };

    let ratio = if progress.total_cores > 0 {
        progress.cores_completed as f32 / progress.total_cores as f32
    } else {
        0.0
    };

    let progress_info = if progress.total_cores > 0 {
        format!("{}/{}", progress.cores_completed, progress.total_cores)
    } else {
        String::new()
    };

    let bar = row![
        text(status_text)
            .size(12)
            .color(text_primary)
            .width(Length::FillPortion(2)),
        container(
            AnimationBuilder::new(ratio, |r| { progress_bar(0.0..=1.0, r).into() })
                .animates_layout(false)
                .animation(Motion::SMOOTH)
        )
        .height(Length::Fixed(8.0))
        .width(Length::FillPortion(3)),
        text(progress_info)
            .size(12)
            .color(text_secondary)
            .width(Length::FillPortion(1)),
    ]
    .spacing(12)
    .align_y(iced::Alignment::Center);

    container(bar)
        .width(Length::Fill)
        .padding(Padding::from([6, 12]))
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(status_bg.into()),
            ..Default::default()
        })
        .into()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_5900x_core_layout_when_grouping_then_produces_two_ccds() {
        // AMD Ryzen 9 5900X: cores 0-5 (CCD0), 8-13 (CCD1)
        let core_map: BTreeMap<u32, Vec<u32>> = BTreeMap::from([
            (0, vec![0, 12]),
            (1, vec![1, 13]),
            (2, vec![2, 14]),
            (3, vec![3, 15]),
            (4, vec![4, 16]),
            (5, vec![5, 17]),
            (8, vec![6, 18]),
            (9, vec![7, 19]),
            (10, vec![8, 20]),
            (11, vec![9, 21]),
            (12, vec![10, 22]),
            (13, vec![11, 23]),
        ]);

        let groups = group_cores_by_ccd(&core_map);

        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].0, "CCD0");
        assert_eq!(groups[0].1, vec![0, 1, 2, 3, 4, 5]);
        assert_eq!(groups[1].0, "CCD1");
        assert_eq!(groups[1].1, vec![8, 9, 10, 11, 12, 13]);
    }

    #[test]
    fn given_contiguous_8_cores_when_grouping_then_produces_single_ccd() {
        // Contiguous 8-core layout: 0-7
        let core_map: BTreeMap<u32, Vec<u32>> = BTreeMap::from([
            (0, vec![0, 8]),
            (1, vec![1, 9]),
            (2, vec![2, 10]),
            (3, vec![3, 11]),
            (4, vec![4, 12]),
            (5, vec![5, 13]),
            (6, vec![6, 14]),
            (7, vec![7, 15]),
        ]);

        let groups = group_cores_by_ccd(&core_map);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "CCD0");
        assert_eq!(groups[0].1, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn given_single_core_when_grouping_then_produces_single_ccd() {
        let core_map: BTreeMap<u32, Vec<u32>> = BTreeMap::from([(0, vec![0, 1])]);

        let groups = group_cores_by_ccd(&core_map);

        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].0, "CCD0");
        assert_eq!(groups[0].1, vec![0]);
    }

    #[test]
    fn given_empty_core_map_when_grouping_then_returns_empty() {
        let core_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();

        let groups = group_cores_by_ccd(&core_map);

        assert!(groups.is_empty());
    }

    #[test]
    fn given_auto_pbo_status_when_classifying_badge_then_returns_auto() {
        assert_eq!(classify_pbo_badge(Some("Auto")), "PBO: AUTO");
    }

    #[test]
    fn given_enabled_like_statuses_when_classifying_badge_then_returns_enabled() {
        assert_eq!(classify_pbo_badge(Some("Enabled")), "PBO: ENABLED");
        assert_eq!(classify_pbo_badge(Some("Advanced")), "PBO: ENABLED");
        assert_eq!(classify_pbo_badge(Some("Motherboard")), "PBO: ENABLED");
        assert_eq!(classify_pbo_badge(Some("Manual")), "PBO: ENABLED");
        assert_eq!(classify_pbo_badge(Some("Eco Mode")), "PBO: ENABLED");
    }

    #[test]
    fn given_disabled_pbo_status_when_classifying_badge_then_returns_disabled() {
        assert_eq!(classify_pbo_badge(Some("Disabled")), "PBO: DISABLED");
    }

    #[test]
    fn given_missing_or_unrecognized_pbo_status_when_classifying_badge_then_returns_unknown() {
        assert_eq!(classify_pbo_badge(None), "PBO: UNKNOWN");
        assert_eq!(classify_pbo_badge(Some("Custom profile")), "PBO: UNKNOWN");
    }

    /// BDD: Given roundoff error with FFT, when formatting, then shows label and FFT
    #[test]
    fn given_roundoff_error_with_fft_when_formatting_then_shows_label_and_fft() {
        let errors = vec![MprimeError {
            error_type: MprimeErrorType::RoundoffError,
            message: String::from("ROUNDOFF > 0.40"),
            fft_size: Some(448),
            timestamp: None,
        }];
        assert_eq!(
            format_error_summary(&errors),
            Some("ROUNDOFF @ 448K".into())
        );
    }

    /// BDD: Given hardware failure without FFT, when formatting, then shows label only
    #[test]
    fn given_hardware_failure_without_fft_when_formatting_then_shows_label_only() {
        let errors = vec![MprimeError {
            error_type: MprimeErrorType::HardwareFailure,
            message: String::from("Hardware failure detected"),
            fft_size: None,
            timestamp: None,
        }];
        assert_eq!(format_error_summary(&errors), Some("HW FAIL".into()));
    }

    /// BDD: Given empty errors, when formatting, then returns None
    #[test]
    fn given_empty_errors_when_formatting_then_returns_none() {
        assert_eq!(format_error_summary(&[]), None);
    }

    /// BDD: Given multiple errors, when formatting, then uses first
    #[test]
    fn given_multiple_errors_when_formatting_then_uses_first() {
        let errors = vec![
            MprimeError {
                error_type: MprimeErrorType::FatalError,
                message: String::from("FATAL ERROR"),
                fft_size: Some(1024),
                timestamp: None,
            },
            MprimeError {
                error_type: MprimeErrorType::RoundoffError,
                message: String::from("ROUNDOFF"),
                fft_size: Some(448),
                timestamp: None,
            },
        ];
        assert_eq!(format_error_summary(&errors), Some("FATAL @ 1024K".into()));
    }

    // GIVEN zero seconds WHEN formatting duration THEN returns empty string
    #[test]
    fn given_zero_seconds_when_formatting_then_empty() {
        assert_eq!(format_duration_label(0), "");
    }

    // GIVEN seconds under one hour WHEN formatting duration THEN returns ~Xm
    #[test]
    fn given_minutes_only_when_formatting_then_shows_minutes() {
        assert_eq!(format_duration_label(360), "~6m");
        assert_eq!(format_duration_label(2160), "~36m");
    }

    // GIVEN seconds over one hour WHEN formatting duration THEN returns ~Xh Ym
    #[test]
    fn given_hours_and_minutes_when_formatting_then_shows_both() {
        assert_eq!(format_duration_label(7800), "~2h 10m");
    }

    // GIVEN exact hours WHEN formatting duration THEN returns ~Xh
    #[test]
    fn given_exact_hours_when_formatting_then_shows_hours_only() {
        assert_eq!(format_duration_label(3600), "~1h");
    }

    // GIVEN config with 12 cores, 6m duration, 3 iterations WHEN estimating THEN returns ~3h 36m
    #[test]
    fn given_all_cores_config_when_estimating_then_shows_total() {
        let topo = make_test_topology(12);
        let config = TestConfig {
            duration: "6m".into(),
            iterations: 3,
            mode: StressTestMode::default(),
            cores: "all".into(),
        };
        assert_eq!(format_estimated_runtime(&config, Some(&topo)), "~3h 36m");
    }

    // GIVEN config with specific cores WHEN estimating THEN counts only those cores
    #[test]
    fn given_specific_cores_when_estimating_then_counts_filtered() {
        let topo = make_test_topology(12);
        let config = TestConfig {
            duration: "6m".into(),
            iterations: 1,
            mode: StressTestMode::default(),
            cores: "0,2,5".into(),
        };
        assert_eq!(format_estimated_runtime(&config, Some(&topo)), "~18m");
    }

    // GIVEN no topology WHEN estimating with "all" cores THEN returns empty
    #[test]
    fn given_no_topology_when_estimating_all_then_empty() {
        let config = TestConfig {
            duration: "6m".into(),
            iterations: 3,
            mode: StressTestMode::default(),
            cores: "all".into(),
        };
        assert_eq!(format_estimated_runtime(&config, None), "");
    }

    fn make_test_topology(core_count: u32) -> CpuTopology {
        let mut core_map = BTreeMap::new();
        for i in 0..core_count {
            core_map.insert(i, vec![i, i + core_count]);
        }
        CpuTopology {
            vendor: String::from("AuthenticAMD"),
            model_name: String::from("Test CPU"),
            core_map,
            physical_core_count: core_count as usize,
            logical_cpu_count: core_count as usize * 2,
            bios_map: BTreeMap::new(),
            physical_map: BTreeMap::new(),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        }
    }
}
