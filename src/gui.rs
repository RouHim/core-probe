use std::collections::BTreeMap;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use iced::widget::{button, column, container, row, text};
use iced::{Element, Subscription, Task, Theme};

use crate::coordinator::{Coordinator, CoreStatus};
use crate::cpu_topology::{detect_cpu_topology, CpuTopology};
use crate::embedded::ExtractedBinaries;
use crate::error_parser::MprimeError;
use crate::gui_events::{create_event_channel, EventReceiver, LogLevel, TestEvent};
use crate::gui_theme::{dark_theme, detect_system_theme, light_theme, ThemeMode};
use crate::gui_widgets;
use crate::mce_monitor::MceError;
use crate::mprime_config::StressTestMode;
use crate::signal_handler;
use crate::uefi_reader::UefiSettings;

#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone)]
pub struct TestConfig {
    pub duration: String,
    pub iterations: u32,
    pub mode: StressTestMode,
    pub cores: String,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            duration: String::from("6m"),
            iterations: 3,
            mode: StressTestMode::default(),
            cores: String::from("all"),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct TestProgress {
    pub current_core: Option<u32>,
    pub cores_completed: usize,
    pub total_cores: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PerCoreProgress {
    pub elapsed_secs: u64,
    pub duration_secs: u64,
}

#[derive(Debug, Clone)]
pub struct CoreResultInfo {
    pub mprime_errors: Vec<MprimeError>,
    pub mce_errors: Vec<MceError>,
    pub duration_tested: std::time::Duration,
    pub iterations_completed: u32,
}

#[derive(Debug, Clone)]
pub enum ConfigField {
    Duration(String),
    Iterations(u32),
    Mode(StressTestMode),
    Cores(String),
}

#[derive(Debug, Clone)]
pub enum Message {
    StartTest,
    StopTest,
    ThemeToggle,
    ConfigChanged(ConfigField),
    EventReceived(TestEvent),
    Tick,
    DismissError,
}

pub struct CoreProbeApp {
    pub topology: Option<CpuTopology>,
    pub uefi_settings: Option<UefiSettings>,
    pub core_statuses: BTreeMap<u32, CoreStatus>,
    pub core_progress: BTreeMap<u32, PerCoreProgress>,
    pub core_results: BTreeMap<u32, CoreResultInfo>,
    pub log_entries: Vec<LogEntry>,
    pub theme_mode: ThemeMode,
    pub config: TestConfig,
    pub test_running: bool,
    pub progress: TestProgress,
    pub error_banner: Option<String>,
    pub event_receiver: Option<EventReceiver>,
    pub extracted_binaries: Option<ExtractedBinaries>,
}

impl CoreProbeApp {
    pub fn is_dark(&self) -> bool {
        match self.theme_mode {
            ThemeMode::Dark => true,
            ThemeMode::Light => false,
            ThemeMode::System => matches!(detect_system_theme(), ThemeMode::Dark),
        }
    }
}

pub fn boot() -> (CoreProbeApp, Task<Message>) {
    let topology = detect_cpu_topology().ok();
    let mut error_banner = None;

    let core_statuses = topology
        .as_ref()
        .map(|t| {
            t.core_map
                .keys()
                .map(|&id| (id, CoreStatus::Idle))
                .collect()
        })
        .unwrap_or_default();

    let uefi_settings = topology
        .as_ref()
        .map(|t| crate::uefi_reader::attempt_uefi_read_with_escalation(t.physical_core_count));

    let extracted_binaries = match crate::embedded::ExtractedBinaries::extract() {
        Ok(extracted) => Some(extracted),
        Err(error) => {
            error_banner = Some(format!("Failed to extract embedded binaries: {error}"));
            None
        }
    };

    let app = CoreProbeApp {
        topology,
        uefi_settings,
        core_statuses,
        core_progress: BTreeMap::new(),
        core_results: BTreeMap::new(),
        log_entries: Vec::new(),
        theme_mode: ThemeMode::Dark,
        config: TestConfig::default(),
        test_running: false,
        progress: TestProgress::default(),
        error_banner,
        event_receiver: None,
        extracted_binaries,
    };

    (app, Task::none())
}

pub fn update(state: &mut CoreProbeApp, message: Message) -> Task<Message> {
    match message {
        Message::StartTest => {
            if state.test_running {
                return Task::none();
            }

            let Some(topology) = state.topology.clone() else {
                state.error_banner = Some(String::from("CPU topology unavailable"));
                return Task::none();
            };

            if state.extracted_binaries.is_none() {
                match crate::embedded::ExtractedBinaries::extract() {
                    Ok(extracted) => state.extracted_binaries = Some(extracted),
                    Err(error) => {
                        state.error_banner =
                            Some(format!("Failed to extract embedded binaries: {error}"));
                        return Task::none();
                    }
                }
            }

            let Some(extracted) = state.extracted_binaries.clone() else {
                state.error_banner = Some(String::from("Embedded binaries unavailable"));
                return Task::none();
            };

            let core_filter = match parse_core_filter(&state.config.cores, &topology) {
                Ok(filter) => filter,
                Err(error) => {
                    state.error_banner = Some(error);
                    return Task::none();
                }
            };

            let duration = parse_duration(&state.config.duration);
            let iterations = state.config.iterations;
            let mode = state.config.mode;

            signal_handler::reset_shutdown();

            for status in state.core_statuses.values_mut() {
                *status = CoreStatus::Idle;
            }
            state.core_progress.clear();
            state.core_results.clear();

            state.progress = TestProgress {
                current_core: None,
                cores_completed: 0,
                total_cores: core_filter
                    .as_ref()
                    .map(|cores| cores.len())
                    .unwrap_or(topology.core_map.len()),
            };

            let (sender, receiver) = create_event_channel();
            let sender_for_errors = sender.clone();
            state.event_receiver = Some(receiver);
            state.test_running = true;
            state.error_banner = None;

            std::thread::spawn(move || {
                let coordinator = Coordinator::new(
                    duration,
                    iterations,
                    core_filter,
                    false,
                    false,
                    Some(sender),
                    Some(mode),
                );

                if let Err(error) = coordinator.run(&topology, &extracted) {
                    let _ = sender_for_errors.send(TestEvent::TestError {
                        message: format!("Coordinator failed: {error}"),
                    });
                }
            });
        }
        Message::StopTest => {
            signal_handler::request_shutdown();
            state.test_running = false;
        }
        Message::ThemeToggle => {
            state.theme_mode = if state.is_dark() {
                ThemeMode::Light
            } else {
                ThemeMode::Dark
            };
        }
        Message::ConfigChanged(field) => match field {
            ConfigField::Duration(d) => state.config.duration = d,
            ConfigField::Iterations(n) => state.config.iterations = n,
            ConfigField::Mode(m) => state.config.mode = m,
            ConfigField::Cores(c) => state.config.cores = c,
        },
        Message::EventReceived(event) => {
            process_event(state, event);
        }
        Message::Tick => {
            let mut drained = Vec::new();
            let mut disconnected = false;

            if let Some(receiver) = state.event_receiver.as_ref() {
                loop {
                    match receiver.try_recv() {
                        Ok(event) => drained.push(event),
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => {
                            disconnected = true;
                            break;
                        }
                    }
                }
            }

            for event in drained {
                process_event(state, event);
            }

            if disconnected {
                state.event_receiver = None;
                state.test_running = false;
            }
        }
        Message::DismissError => {
            state.error_banner = None;
        }
    }

    Task::none()
}

pub fn view(state: &CoreProbeApp) -> Element<'_, Message> {
    let is_dark = state.is_dark();

    let topology_section: Element<'_, Message> = if let Some(topology) = state.topology.as_ref() {
        gui_widgets::topology_grid_view(
            topology,
            &state.core_statuses,
            &state.progress,
            &state.uefi_settings,
            is_dark,
            &None,
            &state.core_progress,
            &state.core_results,
        )
    } else {
        container(text("CPU topology unavailable"))
            .width(iced::Length::FillPortion(3))
            .padding(12)
            .into()
    };

    let header_row: Element<'_, Message> = if let Some(topology) = state.topology.as_ref() {
        gui_widgets::header_view(topology, &state.uefi_settings, is_dark)
    } else {
        row![
            container(text("core-probe")).width(iced::Length::Fill),
            button(text(if is_dark { "\u{263e}" } else { "\u{2600}" }).size(18))
                .on_press(Message::ThemeToggle)
                .padding(iced::Padding::from([4, 8])),
        ]
        .spacing(8)
        .align_y(iced::Alignment::Center)
        .into()
    };

    let right_col = column![
        gui_widgets::config_panel_view(&state.config, state.test_running, is_dark),
        container(gui_widgets::log_feed_view(&state.log_entries, is_dark))
            .height(iced::Length::Fill)
            .width(iced::Length::Fill),
    ]
    .spacing(8)
    .width(iced::Length::FillPortion(2));

    let mut main_col = column![
        header_row,
        row![topology_section, right_col,]
            .spacing(8)
            .height(iced::Length::Fill),
        gui_widgets::status_bar_view(&state.progress, state.test_running, is_dark),
    ]
    .spacing(8)
    .padding(8);

    if let Some(message) = &state.error_banner {
        let error_banner = container(
            row![
                text(message.clone()),
                button(text("×")).on_press(Message::DismissError),
            ]
            .spacing(8),
        )
        .padding(8)
        .style(|_theme: &Theme| iced::widget::container::Style {
            background: Some(iced::Color::from_rgb(0.45, 0.12, 0.12).into()),
            text_color: Some(iced::Color::WHITE),
            ..Default::default()
        });

        main_col = column![error_banner, main_col].spacing(8);
    }

    main_col.into()
}

pub fn subscription(_state: &CoreProbeApp) -> Subscription<Message> {
    iced::time::every(Duration::from_millis(100)).map(|_| Message::Tick)
}

pub fn theme(state: &CoreProbeApp) -> Theme {
    match state.theme_mode {
        ThemeMode::Dark => dark_theme(),
        ThemeMode::Light => light_theme(),
        ThemeMode::System => match detect_system_theme() {
            ThemeMode::Dark => dark_theme(),
            ThemeMode::Light => light_theme(),
            ThemeMode::System => dark_theme(),
        },
    }
}

fn process_event(state: &mut CoreProbeApp, event: TestEvent) {
    match event {
        TestEvent::TestStarted { total_cores } => {
            state.progress.total_cores = total_cores;
            state.progress.cores_completed = 0;
            state.progress.current_core = None;
            state.core_progress.clear();
            state.core_results.clear();
        }
        TestEvent::CoreTestStarting {
            physical_core_id,
            bios_index,
            iteration,
        } => {
            state.progress.current_core = Some(physical_core_id);
            state
                .core_statuses
                .insert(physical_core_id, CoreStatus::Testing);
            append_log(
                state,
                LogLevel::Default,
                format!("Core {bios_index} starting (iteration {iteration})"),
            );
        }
        TestEvent::CoreTestProgress {
            physical_core_id,
            bios_index,
            elapsed_secs,
            duration_secs,
        } => {
            state.progress.current_core = Some(physical_core_id);
            if matches!(
                state.core_statuses.get(&physical_core_id),
                Some(CoreStatus::Idle) | None
            ) {
                state
                    .core_statuses
                    .insert(physical_core_id, CoreStatus::Testing);
            }
            state.core_progress.insert(
                physical_core_id,
                PerCoreProgress {
                    elapsed_secs,
                    duration_secs,
                },
            );
            let _ = bios_index;
        }
        TestEvent::CoreTestCompleted { result } => {
            let status = result.status.clone();
            state
                .core_statuses
                .insert(result.physical_core_id, status.clone());
            state.progress.cores_completed = state.progress.cores_completed.saturating_add(1);
            state.core_progress.remove(&result.physical_core_id);
            state.core_results.insert(
                result.physical_core_id,
                CoreResultInfo {
                    mprime_errors: result.mprime_errors.clone(),
                    mce_errors: result.mce_errors.clone(),
                    duration_tested: result.duration_tested,
                    iterations_completed: result.iterations_completed,
                },
            );

            let bios_idx = result.bios_index;
            let (level, message) = match status {
                CoreStatus::Passed => (LogLevel::Stable, format!("Core {bios_idx} stable")),
                CoreStatus::Failed => (LogLevel::Error, format!("Core {bios_idx} unstable")),
                CoreStatus::Interrupted => (LogLevel::Mce, format!("Core {bios_idx} interrupted")),
                CoreStatus::Skipped => (LogLevel::Default, format!("Core {bios_idx} skipped")),
                CoreStatus::Idle | CoreStatus::Testing => {
                    (LogLevel::Default, format!("Core {bios_idx} updated"))
                }
            };

            append_log(state, level, message);
        }
        TestEvent::IterationCompleted { iteration, total } => {
            append_log(
                state,
                LogLevel::Default,
                format!("Completed iteration {iteration}/{total}"),
            );
        }
        TestEvent::TestCompleted { results } => {
            state.test_running = false;
            state.event_receiver = None;
            state.progress.current_core = None;
            state.progress.total_cores = results.results.len();
            state.progress.cores_completed = results.results.len();
            let (summary, body, urgency) = build_completion_notification(&results.results);
            send_desktop_notification(&summary, &body, urgency);
        }
        TestEvent::LogMessage { level, message } => {
            append_log(state, level, message);
        }
        TestEvent::TestError { message } => {
            state.error_banner = Some(message.clone());
            state.test_running = false;
            state.event_receiver = None;
            append_log(state, LogLevel::Error, message.clone());
            send_desktop_notification("core-probe: Test Error", &message, "critical");
        }
    }
}

fn send_desktop_notification(summary: &str, body: &str, urgency: &str) {
    let _ = std::process::Command::new("notify-send")
        .args(["--urgency", urgency, summary, body])
        .spawn();
}

fn build_completion_notification(
    results: &[crate::coordinator::CoreTestResult],
) -> (String, String, &'static str) {
    let failed: Vec<usize> = results
        .iter()
        .filter(|r| r.status == crate::coordinator::CoreStatus::Failed)
        .map(|r| r.bios_index as usize)
        .collect();
    let total = results.len();
    if failed.is_empty() {
        (
            "core-probe: All cores stable".to_string(),
            format!("{total}/{total} cores passed"),
            "normal",
        )
    } else {
        let cores: Vec<String> = failed.iter().map(|i| i.to_string()).collect();
        (
            format!("core-probe: {} core(s) unstable", failed.len()),
            format!("Failed cores: {}", cores.join(", ")),
            "critical",
        )
    }
}

fn append_log(state: &mut CoreProbeApp, level: LogLevel, message: String) {
    state.log_entries.push(LogEntry {
        timestamp: current_time_label(),
        level,
        message,
    });

    if state.log_entries.len() > 1000 {
        let overflow = state.log_entries.len() - 1000;
        state.log_entries.drain(0..overflow);
    }
}

fn current_time_label() -> String {
    let seconds_since_midnight = match SystemTime::now().duration_since(UNIX_EPOCH) {
        Ok(duration) => duration.as_secs() % 86_400,
        Err(_) => 0,
    };

    let hour = seconds_since_midnight / 3_600;
    let minute = (seconds_since_midnight % 3_600) / 60;
    let second = seconds_since_midnight % 60;

    format!("{hour:02}:{minute:02}:{second:02}")
}

fn parse_duration(input: &str) -> Duration {
    let fallback = Duration::from_secs(360);
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return fallback;
    }

    if let Ok(minutes) = trimmed.parse::<u64>() {
        return Duration::from_secs(minutes.saturating_mul(60));
    }

    let mut total_secs: u64 = 0;
    let mut current_num = String::new();
    let mut found_any_unit = false;
    let mut seen_h = false;
    let mut seen_m = false;
    let mut seen_s = false;

    for ch in trimmed.chars() {
        match ch {
            '0'..='9' => current_num.push(ch),
            'h' | 'H' => {
                if seen_h || current_num.is_empty() {
                    return fallback;
                }
                let Ok(hours) = current_num.parse::<u64>() else {
                    return fallback;
                };
                total_secs = total_secs.saturating_add(hours.saturating_mul(3_600));
                current_num.clear();
                found_any_unit = true;
                seen_h = true;
            }
            'm' | 'M' => {
                if seen_m || current_num.is_empty() {
                    return fallback;
                }
                let Ok(minutes) = current_num.parse::<u64>() else {
                    return fallback;
                };
                total_secs = total_secs.saturating_add(minutes.saturating_mul(60));
                current_num.clear();
                found_any_unit = true;
                seen_m = true;
            }
            's' | 'S' => {
                if seen_s || current_num.is_empty() {
                    return fallback;
                }
                let Ok(seconds) = current_num.parse::<u64>() else {
                    return fallback;
                };
                total_secs = total_secs.saturating_add(seconds);
                current_num.clear();
                found_any_unit = true;
                seen_s = true;
            }
            _ => return fallback,
        }
    }

    if !current_num.is_empty() || !found_any_unit {
        return fallback;
    }

    Duration::from_secs(total_secs)
}

fn parse_core_filter(input: &str, topology: &CpuTopology) -> Result<Option<Vec<u32>>, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("all") {
        return Ok(None);
    }

    let mut bios_indices = Vec::new();
    for token in trimmed.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        let bios_idx = token
            .parse::<u32>()
            .map_err(|_| format!("Invalid BIOS core index '{token}' in core filter"))?;
        bios_indices.push(bios_idx);
    }

    if bios_indices.is_empty() {
        return Ok(None);
    }

    bios_indices.sort_unstable();
    bios_indices.dedup();

    let max_bios_index = topology.core_map.len() as u32;
    let invalid: Vec<u32> = bios_indices
        .iter()
        .copied()
        .filter(|&idx| idx >= max_bios_index)
        .collect();

    if !invalid.is_empty() {
        let invalid_list = invalid
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        return Err(format!(
            "Invalid BIOS core indices: {invalid_list}. Valid range: 0-{}",
            max_bios_index.saturating_sub(1)
        ));
    }

    let physical_ids: Vec<u32> = bios_indices
        .iter()
        .map(|&idx| topology.physical_id(idx).unwrap_or(idx))
        .collect();

    Ok(Some(physical_ids))
}

pub fn run_gui() -> iced::Result {
    iced::application(boot, update, view)
        .title("core-probe — CPU Stability Tester")
        .subscription(subscription)
        .theme(theme)
        .window_size(iced::Size::new(1400.0, 900.0))
        .centered()
        .run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::{CoreStatus, CoreTestResult};
    use crate::gui_events::{LogLevel, TestEvent};

    fn make_app() -> CoreProbeApp {
        CoreProbeApp {
            topology: None,
            uefi_settings: None,
            core_statuses: BTreeMap::new(),
            core_progress: BTreeMap::new(),
            core_results: BTreeMap::new(),
            log_entries: Vec::new(),
            theme_mode: ThemeMode::Dark,
            config: TestConfig::default(),
            test_running: false,
            progress: TestProgress::default(),
            error_banner: None,
            event_receiver: None,
            extracted_binaries: None,
        }
    }

    /// BDD: Given app with error banner, when DismissError, then error_banner cleared
    #[test]
    fn given_error_banner_when_dismiss_then_cleared() {
        let mut app = make_app();
        app.error_banner = Some(String::from("test error"));
        let _ = update(&mut app, Message::DismissError);
        assert!(app.error_banner.is_none());
    }

    /// BDD: Given config, when ConfigChanged(Duration), then config.duration updated
    #[test]
    fn given_config_when_duration_changed_then_config_updated() {
        let mut app = make_app();
        let _ = update(
            &mut app,
            Message::ConfigChanged(ConfigField::Duration(String::from("10m"))),
        );
        assert_eq!(app.config.duration, "10m");
    }

    /// BDD: Given config, when ConfigChanged(Iterations), then config.iterations updated
    #[test]
    fn given_config_when_iterations_changed_then_config_updated() {
        let mut app = make_app();
        let _ = update(&mut app, Message::ConfigChanged(ConfigField::Iterations(5)));
        assert_eq!(app.config.iterations, 5);
    }

    /// BDD: Given dark theme, when ThemeToggle twice, then toggles Dark→Light→Dark
    #[test]
    fn given_dark_theme_when_toggled_then_switches_between_light_and_dark() {
        let mut app = make_app();
        app.theme_mode = ThemeMode::Dark;
        let _ = update(&mut app, Message::ThemeToggle);
        assert!(matches!(app.theme_mode, ThemeMode::Light));
        let _ = update(&mut app, Message::ThemeToggle);
        assert!(matches!(app.theme_mode, ThemeMode::Dark));
    }

    /// BDD: Given system theme mode, when ThemeToggle, then switches to explicit opposite of effective theme
    #[test]
    fn given_system_theme_when_toggled_then_switches_to_explicit_opposite_mode() {
        let mut app = make_app();
        app.theme_mode = ThemeMode::System;

        let was_dark = app.is_dark();
        let _ = update(&mut app, Message::ThemeToggle);

        if was_dark {
            assert!(matches!(app.theme_mode, ThemeMode::Light));
        } else {
            assert!(matches!(app.theme_mode, ThemeMode::Dark));
        }
    }

    /// BDD: Given LogMessage event, when processed, then log entry appended with correct message
    #[test]
    fn given_log_message_event_when_processed_then_appended() {
        let mut app = make_app();
        process_event(
            &mut app,
            TestEvent::LogMessage {
                level: LogLevel::Default,
                message: String::from("hello"),
            },
        );
        assert_eq!(app.log_entries.len(), 1);
        assert_eq!(app.log_entries[0].message, "hello");
    }

    /// BDD: Given 1001 log entries, when one more appended, then capped at 1000
    #[test]
    fn given_1001_log_entries_when_appended_then_capped_at_1000() {
        let mut app = make_app();
        for i in 0..1001 {
            append_log(&mut app, LogLevel::Default, format!("msg {i}"));
        }
        assert_eq!(app.log_entries.len(), 1000);
    }

    /// BDD: Given CoreTestCompleted event with Passed, when processed, then core status updated
    #[test]
    fn given_core_test_completed_passed_when_processed_then_status_updated() {
        let mut app = make_app();
        app.core_statuses.insert(0, CoreStatus::Testing);

        let result = CoreTestResult {
            physical_core_id: 0,
            bios_index: 0,
            logical_cpu_ids: vec![0, 1],
            status: CoreStatus::Passed,
            mprime_errors: Vec::new(),
            mce_errors: Vec::new(),
            duration_tested: std::time::Duration::from_secs(360),
            iterations_completed: 3,
        };
        process_event(&mut app, TestEvent::CoreTestCompleted { result });
        assert_eq!(*app.core_statuses.get(&0).unwrap(), CoreStatus::Passed);
    }

    /// BDD: Given TestStarted event, when processed, then progress.total_cores set
    #[test]
    fn given_test_started_event_when_processed_then_total_cores_set() {
        let mut app = make_app();
        process_event(&mut app, TestEvent::TestStarted { total_cores: 12 });
        assert_eq!(app.progress.total_cores, 12);
        assert_eq!(app.progress.cores_completed, 0);
        assert!(app.progress.current_core.is_none());
    }

    /// BDD: Given CoreTestStarting event, when processed, then core status set to Testing
    #[test]
    fn given_core_test_starting_when_processed_then_core_status_testing() {
        let mut app = make_app();
        app.core_statuses.insert(5, CoreStatus::Idle);
        process_event(
            &mut app,
            TestEvent::CoreTestStarting {
                physical_core_id: 5,
                bios_index: 5,
                iteration: 1,
            },
        );
        assert_eq!(*app.core_statuses.get(&5).unwrap(), CoreStatus::Testing);
        assert_eq!(app.progress.current_core, Some(5));
    }

    /// BDD: Given TestError event, when processed, then error_banner set and test stopped
    #[test]
    fn given_test_error_event_when_processed_then_banner_set_and_test_stopped() {
        let mut app = make_app();
        app.test_running = true;
        process_event(
            &mut app,
            TestEvent::TestError {
                message: String::from("coordinator failed"),
            },
        );
        assert_eq!(app.error_banner.as_deref(), Some("coordinator failed"));
        assert!(!app.test_running);
        assert_eq!(app.log_entries.len(), 1);
    }

    /// BDD: Given config, when ConfigChanged(Mode), then config.mode updated
    #[test]
    fn given_config_when_mode_changed_then_config_updated() {
        let mut app = make_app();
        let _ = update(
            &mut app,
            Message::ConfigChanged(ConfigField::Mode(StressTestMode::AVX)),
        );
        assert_eq!(app.config.mode, StressTestMode::AVX);
    }

    /// BDD: Given config, when ConfigChanged(Cores), then config.cores updated
    #[test]
    fn given_config_when_cores_changed_then_config_updated() {
        let mut app = make_app();
        let _ = update(
            &mut app,
            Message::ConfigChanged(ConfigField::Cores(String::from("0,1,2"))),
        );
        assert_eq!(app.config.cores, "0,1,2");
    }

    /// BDD: Given CoreTestProgress event, when processed, then per-core progress stored
    #[test]
    fn given_core_test_progress_when_processed_then_per_core_progress_stored() {
        let mut app = make_app();
        app.core_statuses.insert(3, CoreStatus::Testing);
        process_event(
            &mut app,
            TestEvent::CoreTestProgress {
                physical_core_id: 3,
                bios_index: 3,
                elapsed_secs: 120,
                duration_secs: 360,
            },
        );
        let progress = app.core_progress.get(&3).expect("progress should exist");
        assert_eq!(progress.elapsed_secs, 120);
        assert_eq!(progress.duration_secs, 360);
    }

    /// BDD: Given core with progress, when CoreTestCompleted, then per-core progress removed
    #[test]
    fn given_core_test_completed_when_processed_then_per_core_progress_removed() {
        let mut app = make_app();
        app.core_progress.insert(
            3,
            PerCoreProgress {
                elapsed_secs: 300,
                duration_secs: 360,
            },
        );
        app.core_statuses.insert(3, CoreStatus::Testing);

        let result = CoreTestResult {
            physical_core_id: 3,
            bios_index: 3,
            logical_cpu_ids: vec![3, 15],
            status: CoreStatus::Passed,
            mprime_errors: Vec::new(),
            mce_errors: Vec::new(),
            duration_tested: std::time::Duration::from_secs(360),
            iterations_completed: 3,
        };
        process_event(&mut app, TestEvent::CoreTestCompleted { result });
        assert!(app.core_progress.get(&3).is_none());
    }

    /// BDD: Given pre-existing progress, when TestStarted, then per-core progress cleared
    #[test]
    fn given_test_started_when_processed_then_per_core_progress_cleared() {
        let mut app = make_app();
        app.core_progress.insert(
            0,
            PerCoreProgress {
                elapsed_secs: 100,
                duration_secs: 360,
            },
        );
        process_event(&mut app, TestEvent::TestStarted { total_cores: 12 });
        assert!(app.core_progress.is_empty());
    }

    /// BDD: Given CoreTestCompleted with errors, when processed, then result info stored
    #[test]
    fn given_core_test_completed_with_errors_when_processed_then_result_info_stored() {
        use crate::error_parser::{MprimeError, MprimeErrorType};

        let mut app = make_app();
        app.core_statuses.insert(5, CoreStatus::Testing);

        let result = CoreTestResult {
            physical_core_id: 5,
            bios_index: 5,
            logical_cpu_ids: vec![5, 17],
            status: CoreStatus::Failed,
            mprime_errors: vec![MprimeError {
                error_type: MprimeErrorType::RoundoffError,
                message: String::from("ROUNDOFF > 0.40"),
                fft_size: Some(448),
                timestamp: None,
            }],
            mce_errors: Vec::new(),
            duration_tested: std::time::Duration::from_secs(120),
            iterations_completed: 1,
        };
        process_event(&mut app, TestEvent::CoreTestCompleted { result });

        let info = app.core_results.get(&5).expect("result info should exist");
        assert_eq!(info.mprime_errors.len(), 1);
        assert_eq!(info.mprime_errors[0].fft_size, Some(448));
        assert_eq!(info.iterations_completed, 1);
    }

    /// BDD: Given pre-existing results, when TestStarted, then core results cleared
    #[test]
    fn given_test_started_when_processed_then_core_results_cleared() {
        use crate::error_parser::{MprimeError, MprimeErrorType};

        let mut app = make_app();
        app.core_results.insert(
            0,
            CoreResultInfo {
                mprime_errors: vec![MprimeError {
                    error_type: MprimeErrorType::HardwareFailure,
                    message: String::from("Hardware failure"),
                    fft_size: None,
                    timestamp: None,
                }],
                mce_errors: Vec::new(),
                duration_tested: std::time::Duration::from_secs(60),
                iterations_completed: 1,
            },
        );
        process_event(&mut app, TestEvent::TestStarted { total_cores: 12 });
        assert!(app.core_results.is_empty());
    }

    /// BDD: Given all passed results, when building notification, then stable message is returned
    #[test]
    fn given_test_completed_all_passed_when_building_notification_then_message_contains_stable() {
        use crate::coordinator::{CoreStatus, CoreTestResult};

        let results = vec![
            CoreTestResult {
                physical_core_id: 0,
                bios_index: 0,
                logical_cpu_ids: vec![0, 1],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: std::time::Duration::from_secs(360),
                iterations_completed: 3,
            },
            CoreTestResult {
                physical_core_id: 1,
                bios_index: 1,
                logical_cpu_ids: vec![2, 3],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: std::time::Duration::from_secs(360),
                iterations_completed: 3,
            },
        ];
        let (summary, body, urgency) = build_completion_notification(&results);
        assert!(
            summary.contains("stable"),
            "Expected 'stable' in summary: {summary}"
        );
        assert_eq!(urgency, "normal");
        assert!(body.contains("2/2"), "Expected '2/2' in body: {body}");
    }

    /// BDD: Given mixed results, when building notification, then unstable message is returned
    #[test]
    fn given_test_completed_with_failures_when_building_notification_then_message_contains_unstable(
    ) {
        use crate::coordinator::{CoreStatus, CoreTestResult};

        let results = vec![
            CoreTestResult {
                physical_core_id: 0,
                bios_index: 0,
                logical_cpu_ids: vec![0, 1],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: std::time::Duration::from_secs(360),
                iterations_completed: 3,
            },
            CoreTestResult {
                physical_core_id: 8,
                bios_index: 6,
                logical_cpu_ids: vec![8, 9],
                status: CoreStatus::Failed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: std::time::Duration::from_secs(120),
                iterations_completed: 1,
            },
        ];
        let (summary, body, urgency) = build_completion_notification(&results);
        assert!(
            summary.contains("unstable"),
            "Expected 'unstable' in summary: {summary}"
        );
        assert_eq!(urgency, "critical");
        assert!(
            body.contains("6"),
            "Expected failed bios_index '6' in body: {body}"
        );
    }
}
