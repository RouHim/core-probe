//! Stability report generation for CPU test cycle results.
//!
//! Generates human-readable terminal reports with ANSI colors and Unicode
//! box-drawing characters. Supports machine-readable output for scripting.

use anyhow::Result;
use std::io::IsTerminal;
use std::time::Duration;
use tracing::instrument;

use crate::coordinator::{CoreStatus, CoreTestResult, CycleResults};
use crate::cpu_topology::CpuTopology;
use crate::error_parser::{MprimeError, MprimeErrorType};
use crate::mce_monitor::{MceError, MceErrorType};
use crate::uefi_reader::UefiSettings;
use std::collections::BTreeMap;

/// ANSI color codes for terminal output
const COLOR_RED: &str = "\x1b[31m";
const COLOR_GREEN: &str = "\x1b[32m";
const COLOR_YELLOW: &str = "\x1b[33m";
const COLOR_RESET: &str = "\x1b[0m";

/// Unicode box-drawing characters
const BOX_TOP_LEFT: &str = "╔";
const BOX_TOP_RIGHT: &str = "╗";
const BOX_BOTTOM_LEFT: &str = "╚";
const BOX_BOTTOM_RIGHT: &str = "╝";
const BOX_HORIZONTAL: &str = "═";
const BOX_VERTICAL: &str = "║";
const BOX_TEE_LEFT: &str = "╠";
const BOX_TEE_RIGHT: &str = "╣";

/// Stability report generator
pub struct StabilityReport<'a> {
    results: &'a CycleResults,
    topology: &'a CpuTopology,
    uefi_settings: Option<&'a UefiSettings>,
    quiet: bool,
}

/// Aggregated test result for a single physical core across all iterations.
/// Used internally for deduplication before report generation.
struct AggregatedCoreResult {
    core_id: u32,
    #[allow(dead_code)]
    logical_cpu_ids: Vec<u32>,
    worst_status: CoreStatus,
    all_mprime_errors: Vec<MprimeError>,
    all_mce_errors: Vec<MceError>,
    #[allow(dead_code)]
    total_iterations: u32,
}

impl<'a> StabilityReport<'a> {
    /// Create a new report from cycle results
    pub fn new(
        results: &'a CycleResults,
        topology: &'a CpuTopology,
        uefi_settings: Option<&'a UefiSettings>,
    ) -> Self {
        Self {
            results,
            topology,
            uefi_settings,
            quiet: false,
        }
    }

    /// Enable quiet mode (only output machine-readable RESULT line)
    pub fn with_quiet(mut self, quiet: bool) -> Self {
        self.quiet = quiet;
        self
    }

    /// Generate formatted report string
    #[instrument(skip(self))]
    pub fn generate(&self) -> Result<String> {
        let use_colors = std::io::stdout().is_terminal();

        if self.quiet {
            return Ok(self.generate_result_line());
        }

        let mut output = String::new();
        let deduped = self.deduplicate_results();

        output.push_str(&self.format_header(use_colors));
        output.push('\n');

        if deduped.is_empty() {
            output.push_str(&self.format_no_data(use_colors));
        } else {
            for result in &deduped {
                output.push_str(&self.format_aggregated_core(result, use_colors));
                output.push('\n');
            }
        }

        // Summary separator
        if !deduped.is_empty() {
            output.push_str(&self.format_separator(use_colors));
            output.push('\n');
        }

        // Summary section
        output.push_str(&self.format_summary(&deduped, use_colors));
        output.push('\n');

        // Footer
        output.push_str(&self.format_footer(use_colors));
        output.push('\n');

        // Machine-readable result line
        output.push_str(&self.generate_result_line());

        Ok(output)
    }

    fn format_header(&self, use_colors: bool) -> String {
        let width = 62;
        let mut output = String::new();

        output.push_str(BOX_TOP_LEFT);
        output.push_str(&BOX_HORIZONTAL.repeat(width));
        output.push_str(BOX_TOP_RIGHT);
        output.push('\n');

        let title = format!("CPU Stability Report - {}", self.topology.model_name);
        let padding = (width.saturating_sub(title.len())) / 2;
        output.push_str(BOX_VERTICAL);
        output.push_str(&" ".repeat(padding));
        output.push_str(&title);
        output.push_str(&" ".repeat(width - padding - title.len()));
        output.push_str(BOX_VERTICAL);
        output.push('\n');

        if let Some(settings) = self.uefi_settings {
            if !settings.available {
                let yellow = if use_colors { COLOR_YELLOW } else { "" };
                let reset = if use_colors { COLOR_RESET } else { "" };
                output.push_str(&format_box_line(&format!(
                    " {yellow}PBO: ⚠ unavailable (run as root){reset}"
                )));
                output.push('\n');
            } else {
                if let Some(pbo_status) = settings.pbo_status.as_deref() {
                    output.push_str(&format_box_line(&format!(" PBO Status: {pbo_status}")));
                    output.push('\n');
                }

                if let Some(limits) = &settings.pbo_limits {
                    let parts: Vec<String> = [
                        limits.ppt_limit.as_deref().map(|v| format!("PPT: {v}")),
                        limits.tdc_limit.as_deref().map(|v| format!("TDC: {v}")),
                        limits.edc_limit.as_deref().map(|v| format!("EDC: {v}")),
                    ]
                    .into_iter()
                    .flatten()
                    .collect();

                    if !parts.is_empty() {
                        output
                            .push_str(&format_box_line(&format!(" Limits: {}", parts.join(" | "))));
                        output.push('\n');
                    }
                }

                if let Some(agesa_version) = settings.agesa_version.as_deref() {
                    output.push_str(&format_box_line(&format!(
                        " AGESA Version: {agesa_version}"
                    )));
                    output.push('\n');
                }
            }
        }

        output.push_str(BOX_TEE_LEFT);
        output.push_str(&BOX_HORIZONTAL.repeat(width));
        output.push_str(BOX_TEE_RIGHT);

        output
    }

    fn format_separator(&self, _use_colors: bool) -> String {
        let width = 62;
        let mut output = String::new();

        output.push_str(BOX_TEE_LEFT);
        output.push_str(&BOX_HORIZONTAL.repeat(width));
        output.push_str(BOX_TEE_RIGHT);

        output
    }

    fn format_footer(&self, _use_colors: bool) -> String {
        let width = 62;
        let mut output = String::new();

        output.push_str(BOX_BOTTOM_LEFT);
        output.push_str(&BOX_HORIZONTAL.repeat(width));
        output.push_str(BOX_BOTTOM_RIGHT);

        output
    }

    fn format_no_data(&self, _use_colors: bool) -> String {
        format!(
            "{} No test data available                                      {}\n",
            BOX_VERTICAL, BOX_VERTICAL
        )
    }

    fn format_aggregated_core(&self, result: &AggregatedCoreResult, use_colors: bool) -> String {
        let mut output = String::new();

        let status_symbol = match result.worst_status {
            CoreStatus::Passed => "✓",
            CoreStatus::Failed => "✗",
            CoreStatus::Interrupted => "⊘",
            CoreStatus::Idle => "◇",
            CoreStatus::Testing => "◈",
            CoreStatus::Skipped => "○",
        };

        let status_text = match result.worst_status {
            CoreStatus::Passed => "STABLE",
            CoreStatus::Failed => "UNSTABLE",
            CoreStatus::Interrupted => "INTERRUPTED",
            CoreStatus::Idle => "IDLE",
            CoreStatus::Testing => "TESTING",
            CoreStatus::Skipped => "SKIPPED",
        };

        let status_color = if use_colors {
            match result.worst_status {
                CoreStatus::Passed => COLOR_GREEN,
                CoreStatus::Failed => COLOR_RED,
                CoreStatus::Interrupted => COLOR_YELLOW,
                CoreStatus::Idle | CoreStatus::Testing | CoreStatus::Skipped => "",
            }
        } else {
            ""
        };

        let reset = if use_colors && !status_color.is_empty() {
            COLOR_RESET
        } else {
            ""
        };

        let co_part = if let Some(settings) = self.uefi_settings {
            if settings.available {
                if let Some(offsets) = &settings.curve_optimizer_offsets {
                    if let Some(&offset) = offsets.get(&result.core_id) {
                        let (label, use_yellow) =
                            co_offset_label(offset, result.worst_status == CoreStatus::Failed);
                        if use_yellow && use_colors {
                            format!(
                                "  {}⚠ CO offset: {} ({}){}",
                                COLOR_YELLOW, offset, label, COLOR_RESET
                            )
                        } else {
                            format!("  CO offset: {} ({})", offset, label)
                        }
                    } else {
                        String::new()
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        let line = format!(
            "{} Core {:2}: {}{} {}{}{}",
            BOX_VERTICAL, result.core_id, status_color, status_symbol, status_text, reset, co_part
        );

        let vis = visible_len(&line);
        let padding = 65_usize.saturating_sub(vis);
        output.push_str(&line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);
        output.push('\n');

        for error in &result.all_mprime_errors {
            let error_type = match error.error_type {
                MprimeErrorType::RoundoffError => "mprime: ROUNDOFF",
                MprimeErrorType::HardwareFailure => "mprime: Hardware failure",
                MprimeErrorType::FatalError => "mprime: FATAL ERROR",
                MprimeErrorType::PossibleHardwareFailure => "mprime: Possible hardware failure",
                MprimeErrorType::IllegalSumout => "mprime: ILLEGAL SUMOUT",
                MprimeErrorType::SumMismatch => "mprime: SUM mismatch",
                MprimeErrorType::TortureTestFailed => "mprime: TORTURE TEST FAILED",
                MprimeErrorType::TortureTestSummaryError => "mprime: Torture test summary error",
                MprimeErrorType::Unknown => "mprime: Unknown error",
            };

            let fft_info = if let Some(fft) = error.fft_size {
                format!(" at {}K FFT", fft)
            } else {
                String::new()
            };

            let detail = format!("{}   └─ {}{}", BOX_VERTICAL, error_type, fft_info);
            let vis = visible_len(&detail);
            let padding = 65_usize.saturating_sub(vis);
            output.push_str(&detail);
            output.push_str(&" ".repeat(padding));
            output.push_str(BOX_VERTICAL);
            output.push('\n');
        }

        for error in &result.all_mce_errors {
            let error_type = match error.error_type {
                MceErrorType::MachineCheck => "MCE: Machine Check",
                MceErrorType::HardwareError => "MCE: Hardware Error",
                MceErrorType::EdacCorrectable => "MCE: EDAC correctable",
                MceErrorType::EdacUncorrectable => "MCE: EDAC uncorrectable",
                MceErrorType::Unknown => "MCE: Unknown",
            };

            let bank_info = if let Some(bank) = error.bank {
                format!(", Bank {}", bank)
            } else {
                String::new()
            };

            let detail = format!("{}   └─ {}{}", BOX_VERTICAL, error_type, bank_info);
            let vis = visible_len(&detail);
            let padding = 65_usize.saturating_sub(vis);
            output.push_str(&detail);
            output.push_str(&" ".repeat(padding));
            output.push_str(BOX_VERTICAL);
            output.push('\n');
        }

        output
    }

    fn format_summary(&self, deduped: &[AggregatedCoreResult], _use_colors: bool) -> String {
        let mut output = String::new();

        let duration_str = format_duration(self.results.total_duration);
        let duration_line = format!(
            "{} Duration: {} | Iterations: {}",
            BOX_VERTICAL, duration_str, self.results.iterations_completed
        );
        let vis = visible_len(&duration_line);
        let padding = 65_usize.saturating_sub(vis);
        output.push_str(&duration_line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);
        output.push('\n');

        let (corrected_count, uncorrected_count) = Self::count_mce_errors_deduped(deduped);
        let mce_line = format!(
            "{} MCE Errors: {} corrected, {} uncorrected",
            BOX_VERTICAL, corrected_count, uncorrected_count
        );
        let vis = visible_len(&mce_line);
        let padding = 65_usize.saturating_sub(vis);
        output.push_str(&mce_line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);

        output
    }

    fn count_mce_errors_deduped(deduped: &[AggregatedCoreResult]) -> (usize, usize) {
        let mut corrected = 0;
        let mut uncorrected = 0;

        for result in deduped {
            for error in &result.all_mce_errors {
                match error.error_type {
                    MceErrorType::EdacCorrectable => corrected += 1,
                    MceErrorType::EdacUncorrectable => uncorrected += 1,
                    MceErrorType::MachineCheck | MceErrorType::HardwareError => corrected += 1,
                    MceErrorType::Unknown => {}
                }
            }
        }

        (corrected, uncorrected)
    }

    fn generate_result_line(&self) -> String {
        let deduped = self.deduplicate_results();
        let failed_cores: Vec<u32> = deduped
            .iter()
            .filter(|r| r.worst_status == CoreStatus::Failed)
            .map(|r| r.core_id)
            .collect();

        if failed_cores.is_empty() {
            "RESULT: STABLE\n".to_string()
        } else {
            let core_list = failed_cores
                .iter()
                .map(|id| id.to_string())
                .collect::<Vec<_>>()
                .join(",");
            format!("RESULT: UNSTABLE cores={}\n", core_list)
        }
    }

    fn deduplicate_results(&self) -> Vec<AggregatedCoreResult> {
        let mut by_core: BTreeMap<u32, Vec<&CoreTestResult>> = BTreeMap::new();
        for result in &self.results.results {
            by_core.entry(result.core_id).or_default().push(result);
        }

        by_core
            .into_values()
            .filter_map(|entries| {
                if entries.iter().all(|e| e.status == CoreStatus::Skipped) {
                    return None;
                }

                let core_id = entries[0].core_id;
                let logical_cpu_ids = entries[0].logical_cpu_ids.clone();

                let worst_status = entries.iter().fold(CoreStatus::Passed, |worst, entry| {
                    match (&worst, &entry.status) {
                        (_, CoreStatus::Failed) => CoreStatus::Failed,
                        (CoreStatus::Failed, _) => CoreStatus::Failed,
                        (_, CoreStatus::Interrupted) => CoreStatus::Interrupted,
                        (CoreStatus::Interrupted, _) => CoreStatus::Interrupted,
                        _ => CoreStatus::Passed,
                    }
                });

                let all_mprime_errors: Vec<MprimeError> = entries
                    .iter()
                    .flat_map(|e| e.mprime_errors.iter().cloned())
                    .collect();
                let all_mce_errors: Vec<MceError> = entries
                    .iter()
                    .flat_map(|e| e.mce_errors.iter().cloned())
                    .collect();

                let total_iterations = entries
                    .iter()
                    .map(|e| e.iterations_completed)
                    .max()
                    .unwrap_or(0);

                Some(AggregatedCoreResult {
                    core_id,
                    logical_cpu_ids,
                    worst_status,
                    all_mprime_errors,
                    all_mce_errors,
                    total_iterations,
                })
            })
            .collect()
    }
}

fn format_box_line(content: &str) -> String {
    let padding = 62usize.saturating_sub(visible_len(content));
    format!(
        "{BOX_VERTICAL}{content}{}{BOX_VERTICAL}",
        " ".repeat(padding)
    )
}

fn visible_len(text: &str) -> usize {
    let mut len = 0;
    let mut chars = text.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\u{1b}' {
            if matches!(chars.peek(), Some('[')) {
                chars.next();
                for next in chars.by_ref() {
                    if ('@'..='~').contains(&next) {
                        break;
                    }
                }
            }
            continue;
        }

        len += 1;
    }

    len
}

fn co_offset_label(offset: i32, is_failed: bool) -> (&'static str, bool) {
    match offset {
        i32::MIN..=-21 => ("aggressive", is_failed),
        -20..=-10 => ("moderate", false),
        -9..=-1 => ("conservative", false),
        0 => ("stock", false),
        _ => ("positive", is_failed),
    }
}

fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::coordinator::{CoreStatus, CoreTestResult, CycleResults};
    use crate::cpu_topology::CpuTopology;
    use crate::error_parser::{MprimeError, MprimeErrorType};
    use crate::mce_monitor::{MceError, MceErrorType};
    use crate::uefi_reader::{PboLimits, UefiSettings};
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn build_test_topology() -> CpuTopology {
        CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen 9 5900X".to_string(),
            physical_core_count: 2,
            logical_cpu_count: 2,
            core_map: BTreeMap::from([(0, vec![0]), (1, vec![1])]),
            bios_map: BTreeMap::from([(0, 0), (1, 1)]),
            physical_map: BTreeMap::from([(0, 0), (1, 1)]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        }
    }

    fn build_test_cycle_results_stable() -> CycleResults {
        CycleResults {
            results: vec![CoreTestResult {
                core_id: 0,
                logical_cpu_ids: vec![0],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: Duration::from_secs(360),
                iterations_completed: 3,
            }],
            total_duration: Duration::from_secs(360),
            iterations_completed: 3,
            interrupted: false,
        }
    }

    #[test]
    fn given_all_cores_passed_when_reporting_then_shows_stable_summary() {
        // GIVEN: All cores passed
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
            ],
            total_duration: Duration::from_secs(720),
            iterations_completed: 3,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows stable summary
        assert!(output.contains("STABLE"));
        assert!(output.contains("RESULT: STABLE"));
        assert!(output.contains("AMD Ryzen 9 5900X"));
        assert!(output.contains("Duration:"));
        assert!(output.contains("Iterations:"));
    }

    #[test]
    fn given_one_core_failed_when_reporting_then_highlights_unstable_core() {
        // GIVEN: One core failed with error
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Failed,
                    mprime_errors: vec![MprimeError {
                        error_type: MprimeErrorType::RoundoffError,
                        message: "ROUND OFF > 0.40".to_string(),
                        fft_size: Some(1344),
                        timestamp: None,
                    }],
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 2,
                },
            ],
            total_duration: Duration::from_secs(480),
            iterations_completed: 3,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Highlights unstable core
        assert!(output.contains("UNSTABLE"));
        assert!(output.contains("RESULT: UNSTABLE cores=1"));
        assert!(output.contains("mprime: ROUNDOFF"));
        assert!(output.contains("1344K FFT"));
    }

    #[test]
    fn given_mce_errors_when_reporting_then_includes_hardware_error_details() {
        // GIVEN: Core with MCE errors
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![CoreTestResult {
                core_id: 0,
                logical_cpu_ids: vec![0],
                status: CoreStatus::Failed,
                mprime_errors: Vec::new(),
                mce_errors: vec![MceError {
                    cpu_id: 0,
                    bank: Some(5),
                    error_type: MceErrorType::MachineCheck,
                    message: "Machine Check Exception".to_string(),
                    timestamp: "1234567890".to_string(),
                    apic_id: None,
                }],
                duration_tested: Duration::from_secs(120),
                iterations_completed: 1,
            }],
            total_duration: Duration::from_secs(120),
            iterations_completed: 1,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Includes MCE details
        assert!(output.contains("MCE: Machine Check"));
        assert!(output.contains("Bank 5"));
        assert!(output.contains("1 corrected, 0 uncorrected"));
    }

    #[test]
    fn given_partial_results_when_reporting_then_shows_interrupted_status() {
        // GIVEN: Test interrupted mid-cycle
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Interrupted,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(60),
                    iterations_completed: 1,
                },
            ],
            total_duration: Duration::from_secs(420),
            iterations_completed: 0,
            interrupted: true,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows interrupted status
        assert!(output.contains("INTERRUPTED"));
    }

    #[test]
    fn given_multiple_iterations_when_reporting_then_shows_per_iteration_results() {
        // GIVEN: Multiple iterations completed
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![CoreTestResult {
                core_id: 0,
                logical_cpu_ids: vec![0],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: Duration::from_secs(1080),
                iterations_completed: 3,
            }],
            total_duration: Duration::from_secs(1080),
            iterations_completed: 3,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows iteration count
        assert!(output.contains("Iterations: 3"));
        assert!(output.contains("18m 0s"));
    }

    #[test]
    fn given_uefi_available_with_pbo_when_formatting_then_shows_pbo_status() {
        // GIVEN: Available UEFI settings with PBO status
        let settings = UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            ..Default::default()
        };
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows PBO status in header
        assert!(output.contains("PBO Status: Enabled"));
    }

    #[test]
    fn given_uefi_with_limits_when_formatting_then_shows_present_limits_only() {
        // GIVEN: Available UEFI settings with partial PBO limits
        let settings = UefiSettings {
            available: true,
            pbo_limits: Some(PboLimits {
                ppt_limit: Some("142W".to_string()),
                tdc_limit: None,
                edc_limit: Some("180A".to_string()),
            }),
            ..Default::default()
        };
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows only populated limit fields
        assert!(output.contains("Limits: PPT: 142W | EDC: 180A"));
        assert!(!output.contains("TDC:"));
    }

    #[test]
    fn given_uefi_with_agesa_when_formatting_then_shows_agesa_version() {
        // GIVEN: Available UEFI settings with AGESA version
        let settings = UefiSettings {
            available: true,
            agesa_version: Some("ComboAM4v2PI 1.2.0.C".to_string()),
            ..Default::default()
        };
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows AGESA version in UEFI section
        assert!(output.contains("AGESA Version: ComboAM4v2PI 1.2.0.C"));
    }

    #[test]
    fn given_uefi_with_curve_optimizer_offsets_when_formatting_then_shows_co_on_per_core_line() {
        // GIVEN: Available UEFI settings with multiple CO offsets
        let settings = UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::from([(0, -30), (1, -25), (2, -20), (3, -15)])),
            ..Default::default()
        };
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: CO appears on per-core line, no bulk table
        assert!(!output.contains("Curve Optimizer Offsets:"));
        assert!(output.contains("CO offset: -30 (aggressive)"));
        assert!(!output.contains("Core  1:  -25"));
    }

    #[test]
    fn given_uefi_unavailable_when_formatting_then_shows_unavailable_notice() {
        // GIVEN: Unavailable UEFI settings
        let settings = UefiSettings::unavailable("permission denied");
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows unavailable notice in header
        assert!(output.contains("PBO: ⚠ unavailable (run as root)"));
    }

    #[test]
    fn given_no_uefi_settings_when_formatting_then_omits_uefi_section() {
        // GIVEN: No UEFI settings were provided
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Omits UEFI section entirely
        assert!(!output.contains("UEFI/BIOS Settings"));
        assert!(!output.contains("UEFI Settings: Unavailable"));
        assert!(!output.contains("PBO:"));
    }

    #[test]
    fn given_uefi_settings_when_generating_then_places_section_before_per_core_results() {
        // GIVEN: UEFI settings are available
        let settings = UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            ..Default::default()
        };
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: PBO status appears in header before per-core results
        let uefi_index = output
            .find("PBO Status:")
            .expect("PBO Status present in header");
        let core_index = output.find("Core  0:").expect("core results present");
        assert!(uefi_index < core_index);
    }

    #[test]
    fn given_failed_core_with_aggressive_co_offset_when_formatting_then_shows_warning_annotation() {
        // GIVEN: A failed core with an aggressive CO offset
        let settings = UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::from([(1, -25)])),
            ..Default::default()
        };
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![CoreTestResult {
                core_id: 1,
                logical_cpu_ids: vec![1],
                status: CoreStatus::Failed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: Duration::from_secs(120),
                iterations_completed: 1,
            }],
            total_duration: Duration::from_secs(120),
            iterations_completed: 1,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: The warning annotation is shown on the main status line
        assert!(output.contains("CO offset: -25 (aggressive)"));
    }

    #[test]
    fn given_passed_core_with_co_offset_when_formatting_then_shows_info_annotation() {
        // GIVEN: A passed core with a moderate CO offset
        let settings = UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::from([(0, -15)])),
            ..Default::default()
        };
        let topology = build_test_topology();
        let results = build_test_cycle_results_stable();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: The CO annotation is shown without yellow warning color codes
        assert!(output.contains("CO offset: -15 (moderate)"));
        assert!(!output.contains(COLOR_YELLOW));
    }

    #[test]
    fn given_no_uefi_data_when_formatting_then_no_co_annotation() {
        // GIVEN: No UEFI data
        let topology = build_test_topology();
        let results = build_test_cycle_results_stable();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: No CO annotation is rendered
        assert!(!output.contains("CO offset"));
    }

    #[test]
    fn given_stock_co_offset_when_formatting_then_shows_stock_label() {
        // GIVEN: A core with stock CO offset
        let settings = UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::from([(0, 0)])),
            ..Default::default()
        };
        let topology = build_test_topology();
        let results = build_test_cycle_results_stable();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: The stock label is shown
        assert!(output.contains("CO offset: 0 (stock)"));
    }

    #[test]
    fn given_core_not_in_co_map_when_formatting_then_no_annotation() {
        // GIVEN: CO data exists for a different core only
        let settings = UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::from([(5, -20)])),
            ..Default::default()
        };
        let topology = build_test_topology();
        let results = build_test_cycle_results_stable();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: No CO annotation is shown for unmatched core IDs
        assert!(!output.contains("CO offset"));
    }

    #[test]
    fn given_core_with_co_annotation_when_formatting_then_keeps_box_width() {
        // GIVEN: A core with a CO annotation line
        let settings = UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::from([(0, -15)])),
            ..Default::default()
        };
        let topology = build_test_topology();
        let results = build_test_cycle_results_stable();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");
        let co_line = output
            .lines()
            .find(|line| line.contains("CO offset: -15 (moderate)"))
            .expect("CO annotation line should be present");

        // THEN: The box width remains aligned
        assert_eq!(visible_len(co_line), 66);
    }

    #[test]
    fn given_empty_results_when_reporting_then_shows_no_data_message() {
        // GIVEN: Empty results
        let topology = build_test_topology();
        let results = CycleResults {
            results: Vec::new(),
            total_duration: Duration::from_secs(0),
            iterations_completed: 0,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows no data message
        assert!(output.contains("No test data available"));
        assert!(output.contains("RESULT: STABLE")); // No failures = stable
    }

    #[test]
    fn given_single_iteration_when_dedup_then_returns_same_entries() {
        // GIVEN: 2 cores, 1 iteration each
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
            ],
            total_duration: Duration::from_secs(720),
            iterations_completed: 1,
            interrupted: false,
        };
        // WHEN: Deduplicating
        let report = StabilityReport::new(&results, &topology, None);
        let deduped = report.deduplicate_results();
        // THEN: 2 entries, one per core
        assert_eq!(deduped.len(), 2);
        assert_eq!(deduped[0].core_id, 0);
        assert_eq!(deduped[1].core_id, 1);
    }

    #[test]
    fn given_multi_iteration_passed_when_dedup_then_single_entry_per_core() {
        // GIVEN: Core 0 appears 3 times (all Passed, 3 iterations)
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
            ],
            total_duration: Duration::from_secs(1080),
            iterations_completed: 3,
            interrupted: false,
        };
        // WHEN: Deduplicating
        let report = StabilityReport::new(&results, &topology, None);
        let deduped = report.deduplicate_results();
        // THEN: 1 entry with worst_status=Passed
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].core_id, 0);
        assert_eq!(deduped[0].worst_status, CoreStatus::Passed);
        assert_eq!(deduped[0].total_iterations, 3);
    }

    #[test]
    fn given_mixed_status_when_dedup_then_worst_wins() {
        // GIVEN: Core 1 appears 3x (Passed, Failed, Passed)
        let topology = build_test_topology();
        let error = MprimeError {
            error_type: MprimeErrorType::RoundoffError,
            message: "err".to_string(),
            fft_size: None,
            timestamp: None,
        };
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Failed,
                    mprime_errors: vec![error.clone()],
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 2,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 3,
                },
            ],
            total_duration: Duration::from_secs(840),
            iterations_completed: 3,
            interrupted: false,
        };
        // WHEN: Deduplicating
        let report = StabilityReport::new(&results, &topology, None);
        let deduped = report.deduplicate_results();
        // THEN: 1 entry, worst_status=Failed, error collected
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].worst_status, CoreStatus::Failed);
        assert_eq!(deduped[0].all_mprime_errors.len(), 1);
    }

    #[test]
    fn given_all_skipped_when_dedup_then_omitted() {
        // GIVEN: Core 2 appears 2x (both Skipped)
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 2,
                    logical_cpu_ids: vec![2],
                    status: CoreStatus::Skipped,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(0),
                    iterations_completed: 0,
                },
                CoreTestResult {
                    core_id: 2,
                    logical_cpu_ids: vec![2],
                    status: CoreStatus::Skipped,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(0),
                    iterations_completed: 0,
                },
            ],
            total_duration: Duration::from_secs(0),
            iterations_completed: 2,
            interrupted: false,
        };
        // WHEN: Deduplicating
        let report = StabilityReport::new(&results, &topology, None);
        let deduped = report.deduplicate_results();
        // THEN: Empty — skipped core is omitted
        assert!(deduped.is_empty());
    }

    #[test]
    fn given_errors_across_iterations_when_dedup_then_all_collected() {
        // GIVEN: Core 0: iteration 1 has 1 mprime error, iteration 2 has 1 MCE error
        let topology = build_test_topology();
        let mprime_err = MprimeError {
            error_type: MprimeErrorType::RoundoffError,
            message: "ROUND OFF".to_string(),
            fft_size: Some(1344),
            timestamp: None,
        };
        let mce_err = MceError {
            cpu_id: 0,
            bank: Some(5),
            error_type: MceErrorType::MachineCheck,
            message: "MCE".to_string(),
            timestamp: "123".to_string(),
            apic_id: None,
        };
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Failed,
                    mprime_errors: vec![mprime_err.clone()],
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Failed,
                    mprime_errors: Vec::new(),
                    mce_errors: vec![mce_err.clone()],
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 2,
                },
            ],
            total_duration: Duration::from_secs(240),
            iterations_completed: 2,
            interrupted: false,
        };
        // WHEN: Deduplicating
        let report = StabilityReport::new(&results, &topology, None);
        let deduped = report.deduplicate_results();
        // THEN: Both errors collected
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].all_mprime_errors.len(), 1);
        assert_eq!(deduped[0].all_mce_errors.len(), 1);
    }

    #[test]
    fn given_interrupted_and_passed_when_dedup_then_interrupted_wins() {
        // GIVEN: Core 0 appears 2x (Passed, Interrupted)
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Interrupted,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(60),
                    iterations_completed: 1,
                },
            ],
            total_duration: Duration::from_secs(420),
            iterations_completed: 1,
            interrupted: true,
        };
        // WHEN: Deduplicating
        let report = StabilityReport::new(&results, &topology, None);
        let deduped = report.deduplicate_results();
        // THEN: worst_status=Interrupted
        assert_eq!(deduped.len(), 1);
        assert_eq!(deduped[0].worst_status, CoreStatus::Interrupted);
    }

    #[test]
    fn given_multi_iteration_results_when_generating_then_each_core_appears_once() {
        // GIVEN: 2 cores × 2 iterations (4 total entries)
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 2,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 2,
                },
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 2,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 2,
                },
            ],
            total_duration: Duration::from_secs(1440),
            iterations_completed: 2,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Each core appears exactly once
        let core0_count = output.matches("Core  0:").count();
        let core1_count = output.matches("Core  1:").count();
        assert_eq!(core0_count, 1, "Core 0 should appear exactly once");
        assert_eq!(core1_count, 1, "Core 1 should appear exactly once");
    }

    #[test]
    fn given_failed_core_across_iterations_when_generating_then_result_line_no_duplicates() {
        // GIVEN: Core 1 fails in 2 iterations
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Failed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Failed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 2,
                },
            ],
            total_duration: Duration::from_secs(240),
            iterations_completed: 2,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: RESULT line has no duplicate core IDs
        assert!(
            output.contains("RESULT: UNSTABLE cores=1\n"),
            "should have 'cores=1' not 'cores=1,1'"
        );
        assert!(
            !output.contains("cores=1,1"),
            "duplicate core IDs must not appear"
        );
    }

    #[test]
    fn given_skipped_core_when_generating_then_omitted_from_report() {
        // GIVEN: One passed core, one skipped core
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 2,
                    logical_cpu_ids: vec![2],
                    status: CoreStatus::Skipped,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(0),
                    iterations_completed: 0,
                },
            ],
            total_duration: Duration::from_secs(360),
            iterations_completed: 1,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Skipped core does not appear in report
        assert!(output.contains("Core  0:"), "passed core should be present");
        assert!(
            !output.contains("Core  2:"),
            "skipped core should be omitted"
        );
    }

    #[test]
    fn given_uefi_available_when_generating_then_header_contains_pbo_and_limits() {
        // GIVEN: Full UEFI data
        let settings = UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            pbo_limits: Some(PboLimits {
                ppt_limit: Some("142W".to_string()),
                tdc_limit: Some("95A".to_string()),
                edc_limit: Some("180A".to_string()),
            }),
            agesa_version: Some("1.2.0.7".to_string()),
            ..Default::default()
        };
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: PBO, Limits, AGESA in header before core lines
        let pbo_idx = output
            .find("PBO Status: Enabled")
            .expect("PBO Status in output");
        let core_idx = output.find("Core  0:").expect("Core 0 in output");
        assert!(
            pbo_idx < core_idx,
            "PBO should appear before core lines (in header)"
        );
        assert!(
            output.contains("PPT: 142W | TDC: 95A | EDC: 180A"),
            "limits present"
        );
        assert!(output.contains("AGESA Version: 1.2.0.7"), "AGESA present");
        assert!(
            !output.contains("UEFI/BIOS Settings"),
            "no old UEFI section title"
        );
        assert!(
            !output.contains("Curve Optimizer Offsets:"),
            "no bulk CO table"
        );
    }

    #[test]
    fn given_uefi_unavailable_when_generating_then_header_shows_unavailable() {
        // GIVEN: Unavailable UEFI settings
        let settings = UefiSettings::unavailable("permission denied");
        let results = build_test_cycle_results_stable();
        let topology = build_test_topology();

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, Some(&settings));
        let output = report.generate().expect("report generation should succeed");

        // THEN: Header shows unavailable notice (new format)
        assert!(
            output.contains("PBO: ⚠ unavailable (run as root)"),
            "new unavailable notice"
        );
        assert!(
            !output.contains("⚠ UEFI Settings: Unavailable"),
            "old format gone"
        );
        assert!(
            !output.contains("Run as root for UEFI/BIOS settings"),
            "old hint gone"
        );
    }

    #[test]
    fn given_multi_iteration_errors_when_generating_then_all_errors_shown() {
        // GIVEN: Core 0 iteration 1 has mprime error, iteration 2 has MCE error
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Failed,
                    mprime_errors: vec![MprimeError {
                        error_type: MprimeErrorType::RoundoffError,
                        message: "ROUND OFF".to_string(),
                        fft_size: Some(1344),
                        timestamp: None,
                    }],
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Failed,
                    mprime_errors: Vec::new(),
                    mce_errors: vec![MceError {
                        cpu_id: 0,
                        bank: Some(5),
                        error_type: MceErrorType::MachineCheck,
                        message: "MCE".to_string(),
                        timestamp: "123".to_string(),
                        apic_id: None,
                    }],
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 2,
                },
            ],
            total_duration: Duration::from_secs(240),
            iterations_completed: 2,
            interrupted: false,
        };

        // WHEN: Generating report
        let report = StabilityReport::new(&results, &topology, None);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Both errors shown under Core 0 (appears once)
        let core0_count = output.matches("Core  0:").count();
        assert_eq!(core0_count, 1, "Core 0 appears once");
        assert!(output.contains("mprime: ROUNDOFF"), "mprime error present");
        assert!(output.contains("MCE: Machine Check"), "MCE error present");
    }
}
