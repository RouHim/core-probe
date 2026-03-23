use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{Context, Result};
use tracing::{info, info_span, instrument, warn};

use crate::cpu_topology::CpuTopology;
use crate::embedded::ExtractedBinaries;
use crate::error_parser::{ErrorParser, MprimeError, MprimeErrorType};
use crate::gui_events::{EventSender, LogLevel, TestEvent};
use crate::mce_monitor::{MceError, MceMonitor};
use crate::mprime_config::{MprimeConfig, StressTestMode};
use crate::mprime_runner::MprimeRunner;
use crate::signal_handler;

const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_secs(1);
const ERROR_POLL_INTERVAL: Duration = Duration::from_secs(5);
const THREAD_PIN_INTERVAL: Duration = Duration::from_secs(5);
const INITIAL_PIN_DELAY: Duration = Duration::from_secs(3);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoreStatus {
    Idle,
    Testing,
    Passed,
    Failed,
    Skipped,
    Interrupted,
}

#[derive(Debug, Clone)]
pub struct CoreTestResult {
    pub core_id: u32,
    pub logical_cpu_ids: Vec<u32>,
    pub status: CoreStatus,
    pub mprime_errors: Vec<MprimeError>,
    pub mce_errors: Vec<MceError>,
    pub duration_tested: Duration,
    pub iterations_completed: u32,
}

#[derive(Debug, Clone)]
pub struct CycleResults {
    pub results: Vec<CoreTestResult>,
    pub total_duration: Duration,
    pub iterations_completed: u32,
    pub interrupted: bool,
}

pub struct Coordinator {
    duration_per_core: Duration,
    iteration_count: u32,
    core_filter: Option<Vec<u32>>,
    quiet: bool,
    bail: bool,
    event_sender: Option<EventSender>,
    stress_mode: Option<StressTestMode>,
}

trait RunnerControl {
    fn start(
        &mut self,
        core_id: u32,
        working_dir: &Path,
        config: Option<&MprimeConfig>,
    ) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn is_running(&mut self) -> Result<bool>;
    fn pin_all_threads(&self, logical_cpu_id: u32) -> Result<u32>;
}

impl RunnerControl for MprimeRunner {
    fn start(
        &mut self,
        core_id: u32,
        working_dir: &Path,
        config: Option<&MprimeConfig>,
    ) -> Result<()> {
        self.start(core_id, working_dir, config)
    }

    fn stop(&mut self) -> Result<()> {
        MprimeRunner::stop(self)
    }

    fn is_running(&mut self) -> Result<bool> {
        MprimeRunner::is_running(self)
    }

    fn pin_all_threads(&self, logical_cpu_id: u32) -> Result<u32> {
        MprimeRunner::pin_all_threads(self, logical_cpu_id)
    }
}

trait ErrorParseControl {
    fn parse_results(&mut self, path: &Path) -> Result<Vec<MprimeError>>;
}

impl ErrorParseControl for ErrorParser {
    fn parse_results(&mut self, path: &Path) -> Result<Vec<MprimeError>> {
        ErrorParser::parse_results(self, path)
    }
}

trait MceControl {
    fn start(&mut self, topology: &CpuTopology) -> Result<()>;
    fn stop(&mut self);
    fn get_errors_for_core(&self, core_id: u32) -> Vec<MceError>;
}

impl MceControl for MceMonitor {
    fn start(&mut self, topology: &CpuTopology) -> Result<()> {
        MceMonitor::start(self, topology)
    }

    fn stop(&mut self) {
        MceMonitor::stop(self);
    }

    fn get_errors_for_core(&self, core_id: u32) -> Vec<MceError> {
        MceMonitor::get_errors_for_core(self, core_id)
    }
}

struct PollHooks<'a, ShutdownFn, SleepFn>
where
    ShutdownFn: Fn() -> bool,
    SleepFn: Fn(Duration),
{
    is_shutdown_requested: &'a ShutdownFn,
    sleep_fn: &'a SleepFn,
}

impl Coordinator {
    pub fn new(
        duration_per_core: Duration,
        iteration_count: u32,
        core_filter: Option<Vec<u32>>,
        quiet: bool,
        bail: bool,
        event_sender: Option<EventSender>,
        stress_mode: Option<StressTestMode>,
    ) -> Self {
        Self {
            duration_per_core,
            iteration_count,
            core_filter,
            quiet,
            bail,
            event_sender,
            stress_mode,
        }
    }

    #[instrument(skip(self, topology, extracted), fields(iteration_count = self.iteration_count, duration_per_core_secs = self.duration_per_core.as_secs()))]
    pub fn run(
        &self,
        topology: &CpuTopology,
        extracted: &ExtractedBinaries,
    ) -> Result<CycleResults> {
        let mut monitor = MceMonitor::new();
        let mut runner = MprimeRunner::from_dependencies(extracted, topology);
        let mut parser = ErrorParser::new();

        self.run_with_components(
            topology,
            extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &signal_handler::is_shutdown_requested,
                sleep_fn: &thread::sleep,
            },
        )
    }

    fn run_with_components<R, P, M, ShutdownFn, SleepFn>(
        &self,
        topology: &CpuTopology,
        extracted: &ExtractedBinaries,
        runner: &mut R,
        parser: &mut P,
        monitor: &mut M,
        hooks: PollHooks<'_, ShutdownFn, SleepFn>,
    ) -> Result<CycleResults>
    where
        R: RunnerControl,
        P: ErrorParseControl,
        M: MceControl,
        ShutdownFn: Fn() -> bool,
        SleepFn: Fn(Duration),
    {
        let start_time = Instant::now();
        monitor
            .start(topology)
            .context("failed to start MCE monitor thread")?;

        let all_cores: Vec<u32> = order_cores_alternate(&topology.core_map);
        self.emit_event(TestEvent::TestStarted {
            total_cores: all_cores.len(),
        });
        let allowed_cores: Option<BTreeSet<u32>> = self
            .core_filter
            .as_ref()
            .map(|core_ids| core_ids.iter().copied().collect());

        let mut results = Vec::new();
        let mut interrupted = false;
        let mut completed_iterations = 0;

        'iterations: for iteration in 0..self.iteration_count {
            let iteration_span = info_span!("iteration", iteration = iteration + 1);
            let _iteration_guard = iteration_span.enter();
            info!("starting iteration");

            for core_id in &all_cores {
                if (hooks.is_shutdown_requested)() {
                    interrupted = true;
                    break 'iterations;
                }

                if !is_core_selected(*core_id, allowed_cores.as_ref()) {
                    results.push(CoreTestResult {
                        core_id: *core_id,
                        logical_cpu_ids: logical_cpu_ids_for_core(topology, *core_id),
                        status: CoreStatus::Skipped,
                        mprime_errors: Vec::new(),
                        mce_errors: Vec::new(),
                        duration_tested: Duration::ZERO,
                        iterations_completed: iteration + 1,
                    });
                    continue;
                }

                let result = self.test_core(
                    *core_id,
                    iteration + 1,
                    topology,
                    extracted,
                    runner,
                    parser,
                    monitor,
                    &hooks,
                )?;

                if result.status == CoreStatus::Interrupted {
                    interrupted = true;
                    self.emit_intermediate_result(&result);
                    self.emit_event(TestEvent::CoreTestCompleted {
                        result: result.clone(),
                    });
                    results.push(result);
                    break 'iterations;
                }

                let failed = result.status == CoreStatus::Failed;
                self.emit_intermediate_result(&result);
                self.emit_event(TestEvent::CoreTestCompleted {
                    result: result.clone(),
                });
                results.push(result);

                if self.bail && failed {
                    interrupted = true;
                    break 'iterations;
                }
            }
            completed_iterations += 1;
            self.emit_event(TestEvent::IterationCompleted {
                iteration: iteration + 1,
                total: self.iteration_count,
            });
            info!(iteration = iteration + 1, "completed iteration");
        }

        monitor.stop();

        let cycle_results = CycleResults {
            results,
            total_duration: start_time.elapsed(),
            iterations_completed: completed_iterations,
            interrupted,
        };
        self.emit_event(TestEvent::TestCompleted {
            results: cycle_results.clone(),
        });

        Ok(cycle_results)
    }

    #[allow(clippy::too_many_arguments)]
    fn test_core<R, P, M, ShutdownFn, SleepFn>(
        &self,
        core_id: u32,
        iteration: u32,
        topology: &CpuTopology,
        extracted: &ExtractedBinaries,
        runner: &mut R,
        parser: &mut P,
        monitor: &M,
        hooks: &PollHooks<'_, ShutdownFn, SleepFn>,
    ) -> Result<CoreTestResult>
    where
        R: RunnerControl,
        P: ErrorParseControl,
        M: MceControl,
        ShutdownFn: Fn() -> bool,
        SleepFn: Fn(Duration),
    {
        let core_span = info_span!("core_test", core_id, iteration);
        let _core_guard = core_span.enter();

        let logical_cpu_ids = logical_cpu_ids_for_core(topology, core_id);
        let working_dir = extracted
            .temp_dir
            .join(format!("iteration-{iteration}"))
            .join(format!("core-{core_id}"));
        fs::create_dir_all(&working_dir).with_context(|| {
            format!(
                "failed to create isolated working directory {}",
                working_dir.display()
            )
        })?;

        let total_cores = topology.core_map.len();
        let core_index = topology
            .core_map
            .keys()
            .position(|&c| c == core_id)
            .unwrap_or(0)
            + 1;
        self.emit_event(TestEvent::CoreTestStarting { core_id, iteration });
        self.emit_event(TestEvent::LogMessage {
            level: LogLevel::Default,
            message: format!(
                "[{}/{}] Testing core {} \u{2014} iteration {}/{}",
                core_index, total_cores, core_id, iteration, self.iteration_count,
            ),
        });

        let config = self.stress_mode.as_ref().map(MprimeConfig::from_mode);
        runner
            .start(core_id, &working_dir, config.as_ref())
            .with_context(|| format!("failed to start mprime on physical core {core_id}"))?;

        // Wait for mprime to spawn worker threads, then pin all threads to the target CPU.
        // mprime v30.19 uses hwloc internally and overrides OS-level CPU affinity, so we
        // must re-pin all threads after they are created.
        if let Some(&first_cpu) = logical_cpu_ids.first() {
            (hooks.sleep_fn)(INITIAL_PIN_DELAY);
            let pinned = runner.pin_all_threads(first_cpu).with_context(|| {
                format!("failed initial thread pinning for physical core {core_id}")
            })?;
            info!(
                core_id,
                pinned,
                logical_cpu_id = first_cpu,
                "initial thread pinning complete"
            );
        }

        let mut mprime_errors = Vec::new();
        let mut mce_errors = Vec::new();
        let mut seen_mce_count = 0;
        let mut elapsed = INITIAL_PIN_DELAY;
        let mut next_error_poll = ERROR_POLL_INTERVAL;
        let mut next_pin_poll = INITIAL_PIN_DELAY + THREAD_PIN_INTERVAL;

        while elapsed < self.duration_per_core {
            if (hooks.is_shutdown_requested)() {
                runner
                    .stop()
                    .context("failed to stop mprime after shutdown")?;
                return Ok(CoreTestResult {
                    core_id,
                    logical_cpu_ids,
                    status: CoreStatus::Interrupted,
                    mprime_errors,
                    mce_errors,
                    duration_tested: elapsed,
                    iterations_completed: iteration,
                });
            }

            if elapsed >= next_error_poll {
                collect_new_errors(
                    core_id,
                    &working_dir,
                    parser,
                    monitor,
                    &mut seen_mce_count,
                    &mut mprime_errors,
                    &mut mce_errors,
                )?;

                if !mprime_errors.is_empty() || !mce_errors.is_empty() {
                    runner
                        .stop()
                        .context("failed to stop mprime after error detection")?;
                    return Ok(CoreTestResult {
                        core_id,
                        logical_cpu_ids,
                        status: CoreStatus::Failed,
                        mprime_errors,
                        mce_errors,
                        duration_tested: elapsed,
                        iterations_completed: iteration,
                    });
                }

                next_error_poll += ERROR_POLL_INTERVAL;
            }

            // Periodically re-pin all mprime threads to the target CPU.
            // mprime may spawn new threads or re-set affinity via hwloc during execution.
            if elapsed >= next_pin_poll {
                if let Some(&first_cpu) = logical_cpu_ids.first() {
                    let _ = runner.pin_all_threads(first_cpu);
                }
                next_pin_poll += THREAD_PIN_INTERVAL;
            }

            if !runner
                .is_running()
                .with_context(|| format!("failed to check mprime liveness for core {core_id}"))?
            {
                warn!(core_id, "mprime exited before requested duration");
                collect_new_errors(
                    core_id,
                    &working_dir,
                    parser,
                    monitor,
                    &mut seen_mce_count,
                    &mut mprime_errors,
                    &mut mce_errors,
                )?;

                if mprime_errors.is_empty() && mce_errors.is_empty() {
                    mprime_errors.push(MprimeError {
                        error_type: MprimeErrorType::Unknown,
                        message: "mprime process exited unexpectedly before test duration elapsed"
                            .to_string(),
                        fft_size: None,
                        timestamp: None,
                    });
                }

                return Ok(CoreTestResult {
                    core_id,
                    logical_cpu_ids,
                    status: CoreStatus::Failed,
                    mprime_errors,
                    mce_errors,
                    duration_tested: elapsed,
                    iterations_completed: iteration,
                });
            }

            (hooks.sleep_fn)(SHUTDOWN_POLL_INTERVAL);
            elapsed += SHUTDOWN_POLL_INTERVAL;
        }

        runner
            .stop()
            .context("failed to stop mprime after core duration completed")?;

        collect_new_errors(
            core_id,
            &working_dir,
            parser,
            monitor,
            &mut seen_mce_count,
            &mut mprime_errors,
            &mut mce_errors,
        )?;

        let status = if mprime_errors.is_empty() && mce_errors.is_empty() {
            CoreStatus::Passed
        } else {
            CoreStatus::Failed
        };

        Ok(CoreTestResult {
            core_id,
            logical_cpu_ids,
            status,
            mprime_errors,
            mce_errors,
            duration_tested: elapsed,
            iterations_completed: iteration,
        })
    }

    fn emit_event(&self, event: TestEvent) {
        if self.quiet && self.event_sender.is_none() {
            return;
        }

        if let Some(sender) = &self.event_sender {
            let _ = sender.send(event);
        }
    }

    fn emit_intermediate_result(&self, result: &CoreTestResult) {
        if let Some(message) = format_intermediate_result(result) {
            let level = match result.status {
                CoreStatus::Passed => LogLevel::Stable,
                CoreStatus::Failed => {
                    if !result.mce_errors.is_empty() {
                        LogLevel::Mce
                    } else {
                        LogLevel::Error
                    }
                }
                CoreStatus::Interrupted
                | CoreStatus::Idle
                | CoreStatus::Testing
                | CoreStatus::Skipped => LogLevel::Default,
            };

            self.emit_event(TestEvent::LogMessage { level, message });
        }
    }
}

/// Orders physical core IDs using CoreCycler's "alternate" strategy.
/// This interleaves cores from the first and second halves of the core list,
/// bouncing between CCDs on multi-chiplet AMD processors to distribute heat evenly.
///
/// For a 12-core CPU with physical core IDs [0,1,2,3,4,5,8,9,10,11,12,13]:
///   half = 6
///   result: [0,8,1,9,2,10,3,11,4,12,5,13]
///
/// This prevents thermal buildup on a single CCD and allows higher boost clocks.
fn order_cores_alternate(core_map: &std::collections::BTreeMap<u32, Vec<u32>>) -> Vec<u32> {
    let sorted_cores: Vec<u32> = core_map.keys().copied().collect();
    let count = sorted_cores.len();
    if count <= 1 {
        return sorted_cores;
    }

    let half = count / 2;
    let mut ordered = Vec::with_capacity(count);
    let first_half = &sorted_cores[..half];
    let second_half = &sorted_cores[half..];

    for i in 0..half {
        ordered.push(first_half[i]);
        if i < second_half.len() {
            ordered.push(second_half[i]);
        }
    }

    // If odd number of cores, second_half has one extra element at the end
    if second_half.len() > half {
        ordered.push(second_half[half]);
    }

    ordered
}

fn collect_new_errors<P, M>(
    core_id: u32,
    working_dir: &Path,
    parser: &mut P,
    monitor: &M,
    seen_mce_count: &mut usize,
    mprime_errors: &mut Vec<MprimeError>,
    mce_errors: &mut Vec<MceError>,
) -> Result<()>
where
    P: ErrorParseControl,
    M: MceControl,
{
    let results_path = working_dir.join("results.txt");
    if results_path.exists() {
        let parsed = parser.parse_results(&results_path).with_context(|| {
            format!(
                "failed to parse mprime results file for core {core_id}: {}",
                results_path.display()
            )
        })?;
        mprime_errors.extend(parsed);
    }

    let latest_core_errors = monitor.get_errors_for_core(core_id);
    if latest_core_errors.len() > *seen_mce_count {
        mce_errors.extend(latest_core_errors[*seen_mce_count..].iter().cloned());
        *seen_mce_count = latest_core_errors.len();
    }

    Ok(())
}

fn logical_cpu_ids_for_core(topology: &CpuTopology, core_id: u32) -> Vec<u32> {
    topology.core_map.get(&core_id).cloned().unwrap_or_default()
}

fn is_core_selected(core_id: u32, filter: Option<&BTreeSet<u32>>) -> bool {
    match filter {
        Some(allowed) => allowed.contains(&core_id),
        None => true,
    }
}

/// Format a compact intermediate result line for a single core test.
/// Returns `None` for skipped cores (no output desired).
fn format_intermediate_result(result: &CoreTestResult) -> Option<String> {
    match result.status {
        CoreStatus::Idle | CoreStatus::Testing | CoreStatus::Skipped => None,
        CoreStatus::Passed => Some(format!("  \u{2713} Core {:2}: STABLE", result.core_id)),
        CoreStatus::Interrupted => {
            Some(format!("  \u{2298} Core {:2}: INTERRUPTED", result.core_id))
        }
        CoreStatus::Failed => {
            let detail = format_error_summary(result);
            if detail.is_empty() {
                Some(format!("  \u{2717} Core {:2}: UNSTABLE", result.core_id))
            } else {
                Some(format!(
                    "  \u{2717} Core {:2}: UNSTABLE \u{2014} {}",
                    result.core_id, detail
                ))
            }
        }
    }
}

/// Build a compact one-line error summary for a failed core result.
/// Prioritizes the first mprime error, then the first MCE error.
fn format_error_summary(result: &CoreTestResult) -> String {
    if let Some(error) = result.mprime_errors.first() {
        let error_type = match error.error_type {
            MprimeErrorType::RoundoffError => "ROUNDOFF",
            MprimeErrorType::HardwareFailure => "Hardware failure",
            MprimeErrorType::FatalError => "FATAL ERROR",
            MprimeErrorType::PossibleHardwareFailure => "Possible hardware failure",
            MprimeErrorType::IllegalSumout => "ILLEGAL SUMOUT",
            MprimeErrorType::SumMismatch => "SUM mismatch",
            MprimeErrorType::TortureTestFailed => "TORTURE TEST FAILED",
            MprimeErrorType::TortureTestSummaryError => "Torture test summary error",
            MprimeErrorType::Unknown => "Unknown error",
        };
        let fft_info = if let Some(fft) = error.fft_size {
            format!(" at {}K FFT", fft)
        } else {
            String::new()
        };
        format!("mprime: {}{}", error_type, fft_info)
    } else if let Some(error) = result.mce_errors.first() {
        let error_type = match error.error_type {
            crate::mce_monitor::MceErrorType::MachineCheck => "Machine Check",
            crate::mce_monitor::MceErrorType::HardwareError => "Hardware Error",
            crate::mce_monitor::MceErrorType::EdacCorrectable => "EDAC correctable",
            crate::mce_monitor::MceErrorType::EdacUncorrectable => "EDAC uncorrectable",
            crate::mce_monitor::MceErrorType::Unknown => "Unknown",
        };
        let bank_info = if let Some(bank) = error.bank {
            format!(", Bank {}", bank)
        } else {
            String::new()
        };
        format!("MCE: {}{}", error_type, bank_info)
    } else {
        String::new()
    }
}

#[cfg(test)]
mod tests {
    use std::cell::{Cell, RefCell};
    use std::collections::{BTreeMap, HashMap};
    use std::path::Path;
    use std::rc::Rc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use tempfile::TempDir;

    use super::*;
    use crate::mce_monitor::MceErrorType;

    #[test]
    fn given_core_list_when_starting_cycle_then_tests_each_core_in_alternate_order() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1]), (2, vec![2])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(1), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert_eq!(runner.start_order, vec![0, 1, 2]);
        assert_eq!(results.results.len(), 3);
        assert!(results
            .results
            .iter()
            .all(|result| result.status == CoreStatus::Passed));
        Ok(())
    }

    #[test]
    fn given_duration_per_core_when_testing_then_runs_for_specified_time() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(3), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert_eq!(results.results[0].duration_tested, Duration::from_secs(3));
        Ok(())
    }

    #[test]
    fn given_error_detected_when_testing_core_then_marks_core_as_failed() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0])])?;
        fixture.write_results(1, 0, "FATAL ERROR: test failure\n")?;
        let coordinator =
            Coordinator::new(Duration::from_secs(6), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert_eq!(results.results[0].status, CoreStatus::Failed);
        assert_eq!(results.results[0].mprime_errors.len(), 1);
        Ok(())
    }

    #[test]
    fn given_shutdown_signal_when_mid_cycle_then_stops_gracefully() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(6), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();
        let checks = AtomicUsize::new(0);

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| checks.fetch_add(1, Ordering::SeqCst) >= 2,
                sleep_fn: &|_| {},
            },
        )?;

        assert!(results.interrupted);
        assert_eq!(results.iterations_completed, 0);
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].status, CoreStatus::Interrupted);
        Ok(())
    }

    #[test]
    fn given_all_cores_tested_when_complete_then_returns_full_results() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1]), (2, vec![2])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(1), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        monitor.push_error(
            1,
            MceError {
                cpu_id: 1,
                bank: Some(5),
                error_type: MceErrorType::MachineCheck,
                message: "mce test".to_string(),
                timestamp: "0".to_string(),
                apic_id: None,
            },
        );

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert_eq!(results.results.len(), 3);
        assert_eq!(results.iterations_completed, 1);
        assert!(!results.interrupted);
        assert_eq!(results.results[1].status, CoreStatus::Failed);
        assert_eq!(results.results[1].mce_errors.len(), 1);
        Ok(())
    }

    #[test]
    fn given_iteration_count_when_configured_then_repeats_full_cycle() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(1), 3, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert_eq!(results.results.len(), 6);
        assert_eq!(results.iterations_completed, 3);
        assert_eq!(runner.start_order, vec![0, 1, 0, 1, 0, 1]);
        Ok(())
    }

    #[test]
    fn given_core_failure_during_test_when_monitoring_then_captures_error_details() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0])])?;
        fixture.write_results(1, 0, "Hardware failure detected running 1344K FFT\n")?;
        let coordinator =
            Coordinator::new(Duration::from_secs(6), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        monitor.push_error(
            0,
            MceError {
                cpu_id: 0,
                bank: Some(3),
                error_type: MceErrorType::MachineCheck,
                message: "MCE bank 3".to_string(),
                timestamp: "0".to_string(),
                apic_id: None,
            },
        );

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        let result = &results.results[0];
        assert_eq!(result.status, CoreStatus::Failed);
        assert_eq!(result.mprime_errors.len(), 1);
        assert_eq!(result.mce_errors.len(), 1);
        assert_eq!(result.mprime_errors[0].fft_size, Some(1344));
        Ok(())
    }

    #[test]
    fn given_core_filter_when_subset_specified_then_only_tests_those_cores() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1]), (2, vec![2])])?;
        let coordinator = Coordinator::new(
            Duration::from_secs(1),
            1,
            Some(vec![0, 2]),
            false,
            false,
            None,
            None,
        );
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert_eq!(runner.start_order, vec![0, 2]);
        assert_eq!(results.results.len(), 3);
        assert_eq!(results.results[1].status, CoreStatus::Skipped);
        assert_eq!(results.results[0].status, CoreStatus::Passed);
        assert_eq!(results.results[2].status, CoreStatus::Passed);
        Ok(())
    }

    struct TestFixture {
        topology: CpuTopology,
        extracted: ExtractedBinaries,
        _temp_dir: TempDir,
    }

    impl TestFixture {
        fn new(core_map: &[(u32, Vec<u32>)]) -> Result<Self> {
            let temp_dir = TempDir::new().context("failed to create temporary test directory")?;
            let logical_cpu_count: usize = core_map.iter().map(|(_, v)| v.len()).sum();
            let topology = CpuTopology {
                vendor: "AuthenticAMD".to_string(),
                model_name: "AMD Test CPU".to_string(),
                physical_core_count: core_map.len(),
                logical_cpu_count,
                core_map: core_map
                    .iter()
                    .cloned()
                    .collect::<BTreeMap<u32, Vec<u32>>>(),
                bios_map: core_map
                    .iter()
                    .enumerate()
                    .map(|(index, (physical_core_id, _))| (*physical_core_id, index as u32))
                    .collect(),
                physical_map: core_map
                    .iter()
                    .enumerate()
                    .map(|(index, (physical_core_id, _))| (index as u32, *physical_core_id))
                    .collect(),
                cpu_brand: None,
                cpu_frequency_mhz: None,
            };
            let extracted = ExtractedBinaries {
                temp_dir: temp_dir.path().to_path_buf(),
                mprime_path: temp_dir.path().join("mprime"),
                lib_dir: temp_dir.path().join("lib"),
            };

            fs::create_dir_all(&extracted.lib_dir)
                .context("failed to create fake library directory for fixture")?;

            Ok(Self {
                topology,
                extracted,
                _temp_dir: temp_dir,
            })
        }

        fn write_results(&self, iteration: u32, core_id: u32, content: &str) -> Result<()> {
            let path = self
                .extracted
                .temp_dir
                .join(format!("iteration-{iteration}"))
                .join(format!("core-{core_id}"))
                .join("results.txt");
            let parent = path
                .parent()
                .context("results file path should always include a parent directory")?;
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create fixture results directory {}",
                    parent.display()
                )
            })?;
            fs::write(&path, content)
                .with_context(|| format!("failed to write fixture results file {}", path.display()))
        }
    }

    #[derive(Default)]
    struct FakeRunner {
        start_order: Vec<u32>,
        running: bool,
    }

    impl RunnerControl for FakeRunner {
        fn start(
            &mut self,
            core_id: u32,
            _working_dir: &Path,
            _config: Option<&MprimeConfig>,
        ) -> Result<()> {
            self.start_order.push(core_id);
            self.running = true;
            Ok(())
        }

        fn stop(&mut self) -> Result<()> {
            self.running = false;
            Ok(())
        }

        fn is_running(&mut self) -> Result<bool> {
            Ok(self.running)
        }

        fn pin_all_threads(&self, _logical_cpu_id: u32) -> Result<u32> {
            Ok(0)
        }
    }

    #[derive(Default)]
    struct FakeParser {
        real_parser: ErrorParser,
    }

    impl ErrorParseControl for FakeParser {
        fn parse_results(&mut self, path: &Path) -> Result<Vec<MprimeError>> {
            if path.exists() {
                self.real_parser.parse_results(path)
            } else {
                Ok(Vec::new())
            }
        }
    }

    #[derive(Default)]
    struct FakeMceMonitor {
        errors_by_core: HashMap<u32, Vec<MceError>>,
    }

    impl FakeMceMonitor {
        fn push_error(&mut self, core_id: u32, error: MceError) {
            self.errors_by_core.entry(core_id).or_default().push(error);
        }
    }

    impl MceControl for FakeMceMonitor {
        fn start(&mut self, _topology: &CpuTopology) -> Result<()> {
            Ok(())
        }

        fn stop(&mut self) {}

        fn get_errors_for_core(&self, core_id: u32) -> Vec<MceError> {
            self.errors_by_core
                .get(&core_id)
                .cloned()
                .unwrap_or_default()
        }
    }

    #[test]
    fn given_monitor_lifecycle_when_running_cycle_then_starts_and_stops_monitor() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(1), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let started = Rc::new(Cell::new(false));
        let stopped = Rc::new(Cell::new(false));

        let mut monitor = LifecycleMonitor {
            started: Rc::clone(&started),
            stopped: Rc::clone(&stopped),
        };

        let _ = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        assert!(started.get());
        assert!(stopped.get());
        Ok(())
    }

    struct LifecycleMonitor {
        started: Rc<Cell<bool>>,
        stopped: Rc<Cell<bool>>,
    }

    impl MceControl for LifecycleMonitor {
        fn start(&mut self, _topology: &CpuTopology) -> Result<()> {
            self.started.set(true);
            Ok(())
        }

        fn stop(&mut self) {
            self.stopped.set(true);
        }

        fn get_errors_for_core(&self, _core_id: u32) -> Vec<MceError> {
            Vec::new()
        }
    }

    #[test]
    fn given_shutdown_signal_when_running_then_polls_at_one_second_intervals() -> Result<()> {
        let fixture = TestFixture::new(&[(0, vec![0])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(6), 1, None, false, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();
        let observed_sleeps = Rc::new(RefCell::new(Vec::new()));
        let sleep_log = Rc::clone(&observed_sleeps);

        let checks = AtomicUsize::new(0);
        let _ = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| checks.fetch_add(1, Ordering::SeqCst) >= 2,
                sleep_fn: &move |duration| sleep_log.borrow_mut().push(duration),
            },
        )?;

        let sleeps = observed_sleeps.borrow();
        // First sleep is the initial thread-pin delay
        assert_eq!(
            sleeps[0],
            Duration::from_secs(3),
            "first sleep should be initial pin delay"
        );
        // Remaining sleeps should all be 1-second poll intervals
        assert!(
            sleeps[1..].iter().all(|d| *d == Duration::from_secs(1)),
            "poll sleeps after initial pin delay should all be 1 second"
        );
        Ok(())
    }

    #[test]
    fn given_12_core_cpu_when_ordering_then_alternates_between_ccds() {
        // AMD Ryzen 9 5900X layout: cores 0-5 (CCD0), 8-13 (CCD1)
        let core_map: BTreeMap<u32, Vec<u32>> = [
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
        ]
        .into_iter()
        .collect();

        let ordered = order_cores_alternate(&core_map);

        assert_eq!(ordered, vec![0, 8, 1, 9, 2, 10, 3, 11, 4, 12, 5, 13]);
    }

    #[test]
    fn given_single_core_when_ordering_then_returns_single_core() {
        let core_map: BTreeMap<u32, Vec<u32>> = [(0, vec![0])].into_iter().collect();

        let ordered = order_cores_alternate(&core_map);

        assert_eq!(ordered, vec![0]);
    }

    #[test]
    fn given_two_cores_when_ordering_then_alternates() {
        let core_map: BTreeMap<u32, Vec<u32>> = [(0, vec![0]), (1, vec![1])].into_iter().collect();

        let ordered = order_cores_alternate(&core_map);

        assert_eq!(ordered, vec![0, 1]);
    }

    #[test]
    fn given_odd_core_count_when_ordering_then_includes_all_cores() {
        let core_map: BTreeMap<u32, Vec<u32>> = [
            (0, vec![0]),
            (1, vec![1]),
            (2, vec![2]),
            (3, vec![3]),
            (4, vec![4]),
        ]
        .into_iter()
        .collect();

        let ordered = order_cores_alternate(&core_map);

        // half=2, first=[0,1], second=[2,3,4]
        // i=0: 0, 2
        // i=1: 1, 3
        // remainder: 4
        assert_eq!(ordered, vec![0, 2, 1, 3, 4]);
    }

    #[test]
    fn given_8_core_cpu_when_ordering_then_alternates_between_halves() {
        let core_map: BTreeMap<u32, Vec<u32>> = [
            (0, vec![0]),
            (1, vec![1]),
            (2, vec![2]),
            (3, vec![3]),
            (4, vec![4]),
            (5, vec![5]),
            (6, vec![6]),
            (7, vec![7]),
        ]
        .into_iter()
        .collect();

        let ordered = order_cores_alternate(&core_map);

        assert_eq!(ordered, vec![0, 4, 1, 5, 2, 6, 3, 7]);
    }

    #[test]
    fn given_non_contiguous_core_ids_when_ordering_then_preserves_all_ids() {
        let core_map: BTreeMap<u32, Vec<u32>> =
            [(0, vec![0]), (2, vec![1]), (5, vec![2]), (9, vec![3])]
                .into_iter()
                .collect();

        let ordered = order_cores_alternate(&core_map);

        // half=2, first=[0,2], second=[5,9]
        assert_eq!(ordered, vec![0, 5, 2, 9]);
    }

    // ── Bail behavior tests ──

    #[test]
    fn given_bail_enabled_when_first_core_fails_then_stops_immediately() -> Result<()> {
        // GIVEN: Two cores, first one fails, bail enabled
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1])])?;
        fixture.write_results(1, 0, "FATAL ERROR: test failure\n")?;
        let coordinator = Coordinator::new(Duration::from_secs(6), 1, None, true, true, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        // WHEN: Running the cycle
        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        // THEN: Stops after first core, second core never tested
        assert!(results.interrupted);
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.results[0].status, CoreStatus::Failed);
        assert_eq!(runner.start_order, vec![0]);
        Ok(())
    }

    #[test]
    fn given_bail_disabled_when_core_fails_then_continues_testing() -> Result<()> {
        // GIVEN: Two cores, first one fails, bail disabled
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1])])?;
        fixture.write_results(1, 0, "FATAL ERROR: test failure\n")?;
        let coordinator =
            Coordinator::new(Duration::from_secs(6), 1, None, true, false, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        // WHEN: Running the cycle
        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        // THEN: Continues testing all cores
        assert!(!results.interrupted);
        assert_eq!(results.results.len(), 2);
        assert_eq!(results.results[0].status, CoreStatus::Failed);
        assert_eq!(results.results[1].status, CoreStatus::Passed);
        assert_eq!(runner.start_order, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn given_bail_enabled_when_all_pass_then_completes_normally() -> Result<()> {
        // GIVEN: Two cores, both pass, bail enabled
        let fixture = TestFixture::new(&[(0, vec![0]), (1, vec![1])])?;
        let coordinator =
            Coordinator::new(Duration::from_secs(1), 1, None, false, true, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        // WHEN: Running the cycle
        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        // THEN: Completes normally
        assert!(!results.interrupted);
        assert_eq!(results.results.len(), 2);
        assert!(results
            .results
            .iter()
            .all(|r| r.status == CoreStatus::Passed));
        assert_eq!(runner.start_order, vec![0, 1]);
        Ok(())
    }

    #[test]
    fn given_bail_enabled_when_failure_in_second_iteration_then_stops() -> Result<()> {
        // GIVEN: One core, fails in iteration 2, bail enabled
        let fixture = TestFixture::new(&[(0, vec![0])])?;
        fixture.write_results(2, 0, "FATAL ERROR: second iteration failure\n")?;
        let coordinator = Coordinator::new(Duration::from_secs(6), 3, None, true, true, None, None);
        let mut runner = FakeRunner::default();
        let mut parser = FakeParser::default();
        let mut monitor = FakeMceMonitor::default();

        // WHEN: Running the cycle
        let results = coordinator.run_with_components(
            &fixture.topology,
            &fixture.extracted,
            &mut runner,
            &mut parser,
            &mut monitor,
            PollHooks {
                is_shutdown_requested: &|| false,
                sleep_fn: &|_| {},
            },
        )?;

        // THEN: Stops after the failed iteration
        assert!(results.interrupted);
        assert_eq!(results.results.len(), 2);
        assert_eq!(results.results[0].status, CoreStatus::Passed);
        assert_eq!(results.results[1].status, CoreStatus::Failed);
        assert_eq!(results.iterations_completed, 1);
        Ok(())
    }

    // ── Intermediate result formatting tests ──

    #[test]
    fn given_passed_core_when_formatting_intermediate_then_shows_stable_symbol() {
        let result = CoreTestResult {
            core_id: 3,
            logical_cpu_ids: vec![3, 15],
            status: CoreStatus::Passed,
            mprime_errors: Vec::new(),
            mce_errors: Vec::new(),
            duration_tested: Duration::from_secs(360),
            iterations_completed: 1,
        };

        let formatted = format_intermediate_result(&result);

        assert!(formatted.is_some());
        let line = formatted.unwrap();
        assert!(line.contains('\u{2713}'));
        assert!(line.contains("STABLE"));
        assert!(line.contains("Core  3"));
    }

    #[test]
    fn given_failed_core_with_roundoff_when_formatting_intermediate_then_shows_error_type() {
        let result = CoreTestResult {
            core_id: 5,
            logical_cpu_ids: vec![5, 17],
            status: CoreStatus::Failed,
            mprime_errors: vec![MprimeError {
                error_type: MprimeErrorType::RoundoffError,
                message: "ROUND OFF > 0.40".to_string(),
                fft_size: Some(1344),
                timestamp: None,
            }],
            mce_errors: Vec::new(),
            duration_tested: Duration::from_secs(120),
            iterations_completed: 1,
        };

        let formatted = format_intermediate_result(&result);

        assert!(formatted.is_some());
        let line = formatted.unwrap();
        assert!(line.contains('\u{2717}'));
        assert!(line.contains("UNSTABLE"));
        assert!(line.contains("mprime: ROUNDOFF at 1344K FFT"));
    }

    #[test]
    fn given_skipped_core_when_formatting_intermediate_then_returns_none() {
        let result = CoreTestResult {
            core_id: 1,
            logical_cpu_ids: vec![1],
            status: CoreStatus::Skipped,
            mprime_errors: Vec::new(),
            mce_errors: Vec::new(),
            duration_tested: Duration::ZERO,
            iterations_completed: 1,
        };

        let formatted = format_intermediate_result(&result);

        assert!(formatted.is_none());
    }

    #[test]
    fn given_interrupted_core_when_formatting_intermediate_then_shows_interrupted() {
        let result = CoreTestResult {
            core_id: 11,
            logical_cpu_ids: vec![9, 21],
            status: CoreStatus::Interrupted,
            mprime_errors: Vec::new(),
            mce_errors: Vec::new(),
            duration_tested: Duration::from_secs(60),
            iterations_completed: 1,
        };

        let formatted = format_intermediate_result(&result);

        assert!(formatted.is_some());
        let line = formatted.unwrap();
        assert!(line.contains('\u{2298}'));
        assert!(line.contains("INTERRUPTED"));
        assert!(line.contains("Core 11"));
    }
}
