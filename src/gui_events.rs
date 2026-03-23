use std::sync::mpsc;
use std::thread;

use crate::coordinator::{CoreTestResult, CycleResults};

#[derive(Debug, Clone)]
pub enum LogLevel {
    Stable,
    Error,
    Mce,
    Default,
}

#[derive(Debug, Clone)]
pub enum TestEvent {
    TestStarted {
        total_cores: usize,
    },
    CoreTestStarting {
        physical_core_id: u32,
        bios_index: u32,
        iteration: u32,
    },
    CoreTestProgress {
        physical_core_id: u32,
        bios_index: u32,
        elapsed_secs: u64,
        duration_secs: u64,
    },
    CoreTestCompleted {
        result: CoreTestResult,
    },
    IterationCompleted {
        iteration: u32,
        total: u32,
    },
    TestCompleted {
        results: CycleResults,
    },
    LogMessage {
        level: LogLevel,
        message: String,
    },
    TestError {
        message: String,
    },
}

pub type EventSender = mpsc::Sender<TestEvent>;
pub type EventReceiver = mpsc::Receiver<TestEvent>;

pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::channel()
}

pub fn create_cli_event_printer(receiver: EventReceiver) -> thread::JoinHandle<()> {
    thread::spawn(move || {
        for event in receiver {
            match event {
                TestEvent::TestStarted { total_cores } => {
                    println!("Starting core stability test for {total_cores} cores");
                }
                TestEvent::CoreTestStarting {
                    physical_core_id: _,
                    bios_index,
                    iteration,
                } => {
                    println!("Testing core {bios_index:2} (iteration {iteration})");
                }
                TestEvent::CoreTestProgress {
                    physical_core_id: _,
                    bios_index,
                    elapsed_secs,
                    duration_secs,
                } => {
                    println!("  Core {bios_index:2}: {elapsed_secs}s / {duration_secs}s elapsed");
                }
                TestEvent::CoreTestCompleted { result } => {
                    print_intermediate_result(&result);
                }
                TestEvent::IterationCompleted { iteration, total } => {
                    println!("Completed iteration {iteration}/{total}");
                }
                TestEvent::TestCompleted { results } => {
                    print_cycle_summary(&results);
                }
                TestEvent::LogMessage { level, message } => {
                    let prefix = match level {
                        LogLevel::Stable => "[stable]",
                        LogLevel::Error => "[error]",
                        LogLevel::Mce => "[mce]",
                        LogLevel::Default => "[info]",
                    };
                    println!("{prefix} {message}");
                }
                TestEvent::TestError { message } => {
                    eprintln!("[error] {message}");
                }
            }
        }
    })
}

fn print_cycle_summary(results: &CycleResults) {
    println!(
        "Test completed: {} cores, {} iteration(s), {:?}",
        results.results.len(),
        results.iterations_completed,
        results.total_duration
    );
}

fn print_intermediate_result(result: &CoreTestResult) {
    if let Some(line) = format_intermediate_result(result) {
        let use_colors = std::io::IsTerminal::is_terminal(&std::io::stdout());
        let (color, reset) = if use_colors {
            let c = match result.status {
                crate::coordinator::CoreStatus::Passed => "\x1b[32m",
                crate::coordinator::CoreStatus::Failed => "\x1b[31m",
                crate::coordinator::CoreStatus::Interrupted => "\x1b[33m",
                crate::coordinator::CoreStatus::Idle
                | crate::coordinator::CoreStatus::Testing
                | crate::coordinator::CoreStatus::Skipped => "",
            };
            let r = if c.is_empty() { "" } else { "\x1b[0m" };
            (c, r)
        } else {
            ("", "")
        };
        println!("{color}{line}{reset}");
    }
}

fn format_intermediate_result(result: &CoreTestResult) -> Option<String> {
    match result.status {
        crate::coordinator::CoreStatus::Idle
        | crate::coordinator::CoreStatus::Testing
        | crate::coordinator::CoreStatus::Skipped => None,
        crate::coordinator::CoreStatus::Passed => {
            Some(format!("  \u{2713} Core {:2}: STABLE", result.bios_index))
        }
        crate::coordinator::CoreStatus::Interrupted => Some(format!(
            "  \u{2298} Core {:2}: INTERRUPTED",
            result.bios_index
        )),
        crate::coordinator::CoreStatus::Failed => {
            let detail = format_error_summary(result);
            if detail.is_empty() {
                Some(format!("  \u{2717} Core {:2}: UNSTABLE", result.bios_index))
            } else {
                Some(format!(
                    "  \u{2717} Core {:2}: UNSTABLE \u{2014} {}",
                    result.bios_index, detail
                ))
            }
        }
    }
}

fn format_error_summary(result: &CoreTestResult) -> String {
    if let Some(error) = result.mprime_errors.first() {
        let error_type = match error.error_type {
            crate::error_parser::MprimeErrorType::RoundoffError => "ROUNDOFF",
            crate::error_parser::MprimeErrorType::HardwareFailure => "Hardware failure",
            crate::error_parser::MprimeErrorType::FatalError => "FATAL ERROR",
            crate::error_parser::MprimeErrorType::PossibleHardwareFailure => {
                "Possible hardware failure"
            }
            crate::error_parser::MprimeErrorType::IllegalSumout => "ILLEGAL SUMOUT",
            crate::error_parser::MprimeErrorType::SumMismatch => "SUM mismatch",
            crate::error_parser::MprimeErrorType::TortureTestFailed => "TORTURE TEST FAILED",
            crate::error_parser::MprimeErrorType::TortureTestSummaryError => {
                "Torture test summary error"
            }
            crate::error_parser::MprimeErrorType::Unknown => "Unknown error",
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
