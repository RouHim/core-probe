use std::collections::BTreeMap;
use std::sync::mpsc;

use crate::coordinator::{CoreTestResult, CycleResults};

#[derive(Debug, Clone)]
pub struct CpuLoadSnapshot {
    pub loads: BTreeMap<u32, f32>,
}

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
    CpuLoadSnapshot(CpuLoadSnapshot),
    ThermalSnapshot {
        tctl: f32,
        core_temps: BTreeMap<u32, f32>,
        core_freqs: BTreeMap<u32, u64>,
    },
    ThermalThrottlePause {
        physical_core_id: u32,
        bios_index: u32,
        tctl: f32,
    },
    ThermalThrottleResume {
        physical_core_id: u32,
        bios_index: u32,
        tctl: f32,
    },
}

pub type EventSender = mpsc::Sender<TestEvent>;
pub type EventReceiver = mpsc::Receiver<TestEvent>;

pub fn create_event_channel() -> (EventSender, EventReceiver) {
    mpsc::channel()
}
