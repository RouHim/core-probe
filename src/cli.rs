//! Headless CLI interface: runs the same per-core mprime test cycle as the
//! GUI, prints live progress and a final text report to stdout, and exits
//! with a status code (0 = all stable, 1 = failures/interruption, 2 = error).

use std::io::Write;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::coordinator::{
    format_error_summary, Coordinator, CoreStatus, CoreTestResult, CycleResults,
};
use crate::cpu_topology::detect_cpu_topology;
use crate::embedded::ExtractedBinaries;
use crate::gui::{parse_core_filter, parse_duration};
use crate::gui_events::{create_event_channel, TestEvent};
use crate::mprime_config::StressTestMode;
use crate::signal_handler;

#[derive(argh::FromArgs)]
/// core-probe - AMD CPU stability tester
pub(crate) struct CliArgs {
    /// run headless in the terminal (default: launch the graphical interface)
    #[argh(switch, short = 'c')]
    pub(crate) cli: bool,
    /// duration per core, e.g. "90s", "6m", "1h30m" (default: 6m; invalid values fall back to 6m)
    #[argh(option)]
    duration: Option<String>,
    /// number of test iterations (default: 3)
    #[argh(option)]
    iterations: Option<u32>,
    /// stress mode: sse, avx, avx2, avx512 (default: sse)
    #[argh(option)]
    mode: Option<String>,
    /// BIOS core indices to test, comma-separated, or "all" (default: all)
    #[argh(option)]
    cores: Option<String>,
}

/// Format a duration as "1h 2m 5s", "6m 12s", or "59s".
fn format_duration(d: Duration) -> String {
    let total = d.as_secs();
    let hours = total / 3600;
    let minutes = (total % 3600) / 60;
    let seconds = total % 60;
    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// Build the final text report from the cycle results.
fn build_report(results: &CycleResults, total_iterations: u32) -> String {
    let mut lines = vec!["core-probe results".to_string(), "-".repeat(40)];
    if results.interrupted {
        lines.push("INTERRUPTED: test stopped early, results are partial".to_string());
    }

    let failed: Vec<&CoreTestResult> = results
        .results
        .iter()
        .filter(|r| r.status == CoreStatus::Failed)
        .collect();
    let passed: Vec<&CoreTestResult> = results
        .results
        .iter()
        .filter(|r| r.status == CoreStatus::Passed)
        .collect();

    if failed.is_empty() && passed.is_empty() {
        lines.push("NO CORES TESTED".to_string());
    } else {
        if !failed.is_empty() {
            let entries = failed
                .iter()
                .map(|r| {
                    let summary = format_error_summary(r);
                    if summary.is_empty() {
                        format!("core {}: UNSTABLE", r.bios_index)
                    } else {
                        format!("core {}: {}", r.bios_index, summary)
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("UNSTABLE ({}): {}", failed.len(), entries));
        }
        if !passed.is_empty() {
            let entries = passed
                .iter()
                .map(|r| r.bios_index.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            lines.push(format!("STABLE ({}): {}", passed.len(), entries));
        }
    }

    lines.push(format!(
        "Duration: {} | Iterations: {}/{} | Interrupted: {}",
        format_duration(results.total_duration),
        results.iterations_completed,
        total_iterations,
        if results.interrupted { "yes" } else { "no" }
    ));

    let mut out = String::new();
    for line in lines {
        out.push_str(&line);
        out.push('\n');
    }
    out
}

/// 0 = all stable, 1 = any failure or interruption (not a clean pass).
fn exit_code_for(results: &CycleResults) -> i32 {
    if results.interrupted
        || results
            .results
            .iter()
            .any(|r| r.status == CoreStatus::Failed)
    {
        1
    } else {
        0
    }
}

/// Run the headless test cycle and return the process exit code.
pub(crate) fn run(args: CliArgs) -> Result<i32> {
    signal_handler::register_handler().context("failed to register Ctrl+C handler")?;
    let topology = detect_cpu_topology().context("failed to detect CPU topology")?;

    // Resolve and validate options before extraction so validation errors
    // never leak the extracted temporary directory.
    let duration = parse_duration(&args.duration.unwrap_or_else(|| "6m".to_string()));
    let iterations = args.iterations.unwrap_or(3);
    if iterations == 0 {
        bail!("iterations must be at least 1");
    }
    let mode = match args.mode.as_deref() {
        Some(m) => m.parse::<StressTestMode>().map_err(anyhow::Error::msg)?,
        None => StressTestMode::SSE,
    };
    let core_filter = match args.cores.as_deref() {
        Some(c) => parse_core_filter(c, &topology).map_err(anyhow::Error::msg)?,
        None => None,
    };

    let extracted = ExtractedBinaries::extract().context("failed to extract embedded binaries")?;

    signal_handler::reset_shutdown();

    let total_cores = core_filter
        .as_ref()
        .map(Vec::len)
        .unwrap_or(topology.core_map.len());
    println!(
        "core-probe: testing {total_cores} core(s), {iterations} iteration(s), {} per core",
        format_duration(duration)
    );

    let (sender, receiver) = create_event_channel();
    let sender_for_errors = sender.clone();
    let extracted_for_run = extracted.clone();
    std::thread::spawn(move || {
        let coordinator =
            Coordinator::new(duration, iterations, core_filter, Some(sender), Some(mode));
        if let Err(error) = coordinator.run(&topology, &extracted_for_run) {
            let _ = sender_for_errors.send(TestEvent::TestError {
                message: format!("Coordinator failed: {error}"),
            });
        }
    });

    let mut progress_shown = false;
    let mut had_error = false;
    let mut results: Option<CycleResults> = None;

    loop {
        match receiver.recv() {
            Ok(TestEvent::CoreTestProgress {
                bios_index,
                elapsed_secs,
                duration_secs,
                ..
            }) => {
                print!("\r  core {bios_index}: {elapsed_secs}s/{duration_secs}s");
                let _ = std::io::stdout().flush();
                progress_shown = true;
            }
            Ok(TestEvent::LogMessage { message, .. }) => {
                if progress_shown {
                    println!();
                    progress_shown = false;
                }
                println!("{message}");
            }
            Ok(TestEvent::IterationCompleted { iteration, total }) => {
                if progress_shown {
                    println!();
                    progress_shown = false;
                }
                println!("Iteration {iteration}/{total} complete");
            }
            Ok(TestEvent::ThermalThrottlePause {
                bios_index, tctl, ..
            }) => {
                if progress_shown {
                    println!();
                    progress_shown = false;
                }
                println!("Thermal pause: core {bios_index} at {tctl:.0}°C - cooling");
            }
            Ok(TestEvent::ThermalThrottleResume {
                bios_index, tctl, ..
            }) => {
                if progress_shown {
                    println!();
                    progress_shown = false;
                }
                println!("Resumed core {bios_index} at {tctl:.0}°C");
            }
            Ok(TestEvent::TestError { message }) => {
                eprintln!("Error: {message}");
                had_error = true;
            }
            Ok(TestEvent::TestCompleted {
                results: cycle_results,
            }) => {
                results = Some(cycle_results);
                break;
            }
            Ok(TestEvent::TestStarted { .. })
            | Ok(TestEvent::CoreTestStarting { .. })
            | Ok(TestEvent::CoreTestCompleted { .. })
            | Ok(TestEvent::CpuLoadSnapshot(_))
            | Ok(TestEvent::ThermalSnapshot { .. }) => {}
            Err(_) => {
                // Channel closed: the coordinator thread ended without
                // completing. Fall through if TestCompleted was already seen.
                if results.is_none() {
                    eprintln!("test run ended unexpectedly");
                    had_error = true;
                }
                break;
            }
        }
    }

    let _ = extracted.cleanup();

    match results {
        Some(results) if !had_error => {
            print!("{}", build_report(&results, iterations));
            Ok(exit_code_for(&results))
        }
        _ => Ok(2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error_parser::{MprimeError, MprimeErrorType};

    fn passed(bios_index: u32) -> CoreTestResult {
        CoreTestResult {
            physical_core_id: bios_index,
            bios_index,
            logical_cpu_ids: vec![bios_index],
            status: CoreStatus::Passed,
            mprime_errors: vec![],
            mce_errors: vec![],
            duration_tested: Duration::ZERO,
            iterations_completed: 1,
            freq_samples: vec![],
            freq_max_khz: None,
            cooldown_count: 0,
        }
    }

    fn failed_with_roundoff(bios_index: u32) -> CoreTestResult {
        CoreTestResult {
            status: CoreStatus::Failed,
            mprime_errors: vec![MprimeError {
                error_type: MprimeErrorType::RoundoffError,
                message: "ROUND OFF > 0.40 at 1344K FFT".into(),
                fft_size: Some(1344),
                timestamp: None,
            }],
            ..passed(bios_index)
        }
    }

    fn failed_without_errors(bios_index: u32) -> CoreTestResult {
        CoreTestResult {
            status: CoreStatus::Failed,
            ..passed(bios_index)
        }
    }

    fn cycle(results: Vec<CoreTestResult>, interrupted: bool) -> CycleResults {
        CycleResults {
            results,
            total_duration: Duration::from_secs(372),
            iterations_completed: 1,
            interrupted,
            system_mce_errors: vec![],
        }
    }

    #[test]
    fn given_all_cores_stable_when_building_report_then_lists_stable_without_unstable() {
        // Given: all 12 cores passed
        let results = cycle((0..12).map(passed).collect(), false);

        // When: building the report
        let report = build_report(&results, 3);

        // Then: STABLE lists every core, no UNSTABLE line, clean footer
        assert!(report.contains("STABLE (12): 0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11"));
        assert!(!report.contains("UNSTABLE"));
        assert!(report.contains("Iterations: 1/3"));
        assert!(report.contains("Interrupted: no"));
    }

    #[test]
    fn given_one_failed_core_when_building_report_then_lists_mprime_error() {
        // Given: core 3 failed with a roundoff error, all others passed
        let mut results = (0..12).map(passed).collect::<Vec<_>>();
        results[3] = failed_with_roundoff(3);
        let results = cycle(results, false);

        // When: building the report
        let report = build_report(&results, 3);

        // Then: failed core shows the mprime error and is excluded from STABLE
        assert!(report.contains("UNSTABLE (1): core 3: mprime: ROUNDOFF at 1344K FFT"));
        assert!(report.contains("STABLE (11): 0, 1, 2, 4, 5, 6, 7, 8, 9, 10, 11"));
    }

    #[test]
    fn given_failed_core_without_errors_when_building_report_then_marks_unstable() {
        // Given: core 3 failed with no recorded errors
        let results = cycle(vec![failed_without_errors(3)], false);

        // When: building the report
        let report = build_report(&results, 3);

        // Then: the entry falls back to a plain UNSTABLE marker
        assert!(report.contains("UNSTABLE (1): core 3: UNSTABLE"));
    }

    #[test]
    fn given_interrupted_run_when_building_report_then_shows_banner_and_footer() {
        // Given: run interrupted with all passed cores
        let results = cycle((0..12).map(passed).collect(), true);

        // When: building the report
        let report = build_report(&results, 3);

        // Then: the banner line and footer flag reflect the interruption
        assert!(report.contains("INTERRUPTED: test stopped early, results are partial"));
        assert!(report.contains("Interrupted: yes"));
    }

    #[test]
    fn given_no_completed_cores_when_building_report_then_shows_no_cores_tested() {
        // Given: interrupted before any core finished
        let results = cycle(vec![], true);

        // When: building the report
        let report = build_report(&results, 3);

        // Then: both list lines are replaced by a single notice
        assert!(report.contains("NO CORES TESTED"));
        assert!(!report.contains("STABLE ("));
        assert!(!report.contains("UNSTABLE ("));
    }

    #[test]
    fn given_all_stable_when_exit_code_then_returns_zero() {
        // Given: all cores passed, no interruption
        let results = cycle((0..12).map(passed).collect(), false);

        // When/Then: exit code is 0
        assert_eq!(exit_code_for(&results), 0);
    }

    #[test]
    fn given_failed_core_when_exit_code_then_returns_one() {
        // Given: one core failed
        let results = cycle(vec![passed(0), failed_with_roundoff(3)], false);

        // When/Then: exit code is 1
        assert_eq!(exit_code_for(&results), 1);
    }

    #[test]
    fn given_interrupted_all_stable_when_exit_code_then_returns_one() {
        // Given: interrupted run, all passed cores
        let results = cycle((0..12).map(passed).collect(), true);

        // When/Then: exit code is 1 (not a clean pass)
        assert_eq!(exit_code_for(&results), 1);
    }

    #[test]
    fn given_durations_when_formatting_then_returns_readable_strings() {
        // Given/When/Then: seconds, minutes, and hours render compactly
        assert_eq!(format_duration(Duration::from_secs(59)), "59s");
        assert_eq!(format_duration(Duration::from_secs(372)), "6m 12s");
        assert_eq!(format_duration(Duration::from_secs(3725)), "1h 2m 5s");
    }
}
