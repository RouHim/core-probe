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
use crate::error_parser::MprimeErrorType;
use crate::mce_monitor::MceErrorType;

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
    quiet: bool,
}

impl<'a> StabilityReport<'a> {
    /// Create a new report from cycle results
    pub fn new(results: &'a CycleResults, topology: &'a CpuTopology) -> Self {
        Self {
            results,
            topology,
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

        // Header
        output.push_str(&self.format_header(use_colors));
        output.push('\n');

        // Per-core results
        if self.results.results.is_empty() {
            output.push_str(&self.format_no_data(use_colors));
        } else {
            for result in &self.results.results {
                output.push_str(&self.format_core_result(result, use_colors));
                output.push('\n');
            }
        }

        // Summary separator
        if !self.results.results.is_empty() {
            output.push_str(&self.format_separator(use_colors));
            output.push('\n');
        }

        // Summary section
        output.push_str(&self.format_summary(use_colors));
        output.push('\n');

        // Footer
        output.push_str(&self.format_footer(use_colors));
        output.push('\n');

        // Machine-readable result line
        output.push_str(&self.generate_result_line());

        Ok(output)
    }

    fn format_header(&self, _use_colors: bool) -> String {
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

    fn format_core_result(&self, result: &CoreTestResult, use_colors: bool) -> String {
        let mut output = String::new();

        // Main status line
        let status_symbol = match result.status {
            CoreStatus::Passed => "✓",
            CoreStatus::Failed => "✗",
            CoreStatus::Interrupted => "⊘",
            CoreStatus::Skipped => "○",
        };

        let status_text = match result.status {
            CoreStatus::Passed => "STABLE",
            CoreStatus::Failed => "UNSTABLE",
            CoreStatus::Interrupted => "INTERRUPTED",
            CoreStatus::Skipped => "SKIPPED",
        };

        let status_color = if use_colors {
            match result.status {
                CoreStatus::Passed => COLOR_GREEN,
                CoreStatus::Failed => COLOR_RED,
                CoreStatus::Interrupted => COLOR_YELLOW,
                CoreStatus::Skipped => "",
            }
        } else {
            ""
        };

        let reset = if use_colors && !status_color.is_empty() {
            COLOR_RESET
        } else {
            ""
        };

        let iteration_info = match result.status {
            CoreStatus::Passed => {
                format!(
                    "({}/{} iterations)",
                    result.iterations_completed, result.iterations_completed
                )
            }
            CoreStatus::Failed => {
                format!("(failed iteration {})", result.iterations_completed)
            }
            CoreStatus::Interrupted => {
                format!("(interrupted at iteration {})", result.iterations_completed)
            }
            CoreStatus::Skipped => String::new(),
        };

        let line = format!(
            "{} Core {:2}: {}{} {}{}  {}",
            BOX_VERTICAL,
            result.core_id,
            status_color,
            status_symbol,
            status_text,
            reset,
            iteration_info
        );

        // Pad to width
        let visible_len = line.chars().filter(|c| !c.is_ascii_control()).count()
            - status_color.len()
            - reset.len();
        let padding = 62_usize.saturating_sub(visible_len);
        output.push_str(&line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);
        output.push('\n');

        // Error details
        for error in &result.mprime_errors {
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
            let visible_len = detail.chars().count();
            let padding = 62_usize.saturating_sub(visible_len.saturating_sub(3));
            output.push_str(&detail);
            output.push_str(&" ".repeat(padding));
            output.push_str(BOX_VERTICAL);
            output.push('\n');
        }

        for error in &result.mce_errors {
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
            let visible_len = detail.chars().count();
            let padding = 62_usize.saturating_sub(visible_len.saturating_sub(3));
            output.push_str(&detail);
            output.push_str(&" ".repeat(padding));
            output.push_str(BOX_VERTICAL);
            output.push('\n');
        }

        output
    }

    fn format_summary(&self, _use_colors: bool) -> String {
        let mut output = String::new();

        let stable_count = self
            .results
            .results
            .iter()
            .filter(|r| r.status == CoreStatus::Passed)
            .count();

        let unstable_count = self
            .results
            .results
            .iter()
            .filter(|r| r.status == CoreStatus::Failed)
            .count();

        let total_count = self
            .results
            .results
            .iter()
            .filter(|r| r.status == CoreStatus::Passed || r.status == CoreStatus::Failed)
            .count();

        let summary_line = format!(
            "{} Summary: {}/{} cores stable, {} unstable",
            BOX_VERTICAL, stable_count, total_count, unstable_count
        );
        let visible_len = summary_line.chars().count();
        let padding = 62_usize.saturating_sub(visible_len.saturating_sub(3));
        output.push_str(&summary_line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);
        output.push('\n');

        // Duration and iterations
        let duration_str = format_duration(self.results.total_duration);
        let duration_line = format!(
            "{} Duration: {} | Iterations: {}",
            BOX_VERTICAL, duration_str, self.results.iterations_completed
        );
        let visible_len = duration_line.chars().count();
        let padding = 62_usize.saturating_sub(visible_len.saturating_sub(3));
        output.push_str(&duration_line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);
        output.push('\n');

        // MCE error counts
        let (corrected_count, uncorrected_count) = self.count_mce_errors();
        let mce_line = format!(
            "{} MCE Errors: {} corrected, {} uncorrected",
            BOX_VERTICAL, corrected_count, uncorrected_count
        );
        let visible_len = mce_line.chars().count();
        let padding = 62_usize.saturating_sub(visible_len.saturating_sub(3));
        output.push_str(&mce_line);
        output.push_str(&" ".repeat(padding));
        output.push_str(BOX_VERTICAL);

        output
    }

    fn count_mce_errors(&self) -> (usize, usize) {
        let mut corrected = 0;
        let mut uncorrected = 0;

        for result in &self.results.results {
            for error in &result.mce_errors {
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
        let failed_cores: Vec<u32> = self
            .results
            .results
            .iter()
            .filter(|r| r.status == CoreStatus::Failed)
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
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn build_test_topology() -> CpuTopology {
        CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen 9 5900X".to_string(),
            physical_core_count: 2,
            logical_cpu_count: 2,
            core_map: BTreeMap::from([(0, vec![0]), (1, vec![1])]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
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
        let report = StabilityReport::new(&results, &topology);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows stable summary
        assert!(output.contains("STABLE"));
        assert!(output.contains("2/2 cores stable, 0 unstable"));
        assert!(output.contains("RESULT: STABLE"));
        assert!(output.contains("AMD Ryzen 9 5900X"));
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
        let report = StabilityReport::new(&results, &topology);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Highlights unstable core
        assert!(output.contains("UNSTABLE"));
        assert!(output.contains("1/2 cores stable, 1 unstable"));
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
        let report = StabilityReport::new(&results, &topology);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Includes MCE details
        assert!(output.contains("MCE: Machine Check"));
        assert!(output.contains("Bank 5"));
        assert!(output.contains("0/1 cores stable, 1 unstable"));
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
        let report = StabilityReport::new(&results, &topology);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows interrupted status
        assert!(output.contains("INTERRUPTED"));
        assert!(output.contains("1/1 cores stable, 0 unstable")); // Interrupted cores don't count as unstable
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
        let report = StabilityReport::new(&results, &topology);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows iteration count
        assert!(output.contains("(3/3 iterations)"));
        assert!(output.contains("Iterations: 3"));
        assert!(output.contains("18m 0s"));
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
        let report = StabilityReport::new(&results, &topology);
        let output = report.generate().expect("report generation should succeed");

        // THEN: Shows no data message
        assert!(output.contains("No test data available"));
        assert!(output.contains("RESULT: STABLE")); // No failures = stable
    }
}
