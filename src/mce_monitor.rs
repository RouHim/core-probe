use std::collections::HashMap;
use std::fs;
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use anyhow::{anyhow, Context, Result};
use regex::Regex;
use time::OffsetDateTime;
use tracing::{info, warn};

use crate::cpu_topology::CpuTopology;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MceError {
    pub cpu_id: u32,
    pub bank: Option<u32>,
    pub error_type: MceErrorType,
    pub message: String,
    pub timestamp: String,
    pub apic_id: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MceErrorType {
    MachineCheck,
    HardwareError,
    EdacCorrectable,
    EdacUncorrectable,
    Unknown,
}

type JournalctlFetcher = Arc<dyn Fn(&str) -> Result<String> + Send + Sync + 'static>;
type CpuInfoReader = Arc<dyn Fn() -> Result<String> + Send + Sync + 'static>;

pub struct MceMonitor {
    thread_handle: Option<JoinHandle<()>>,
    errors: Arc<Mutex<Vec<MceError>>>,
    shutdown: Arc<AtomicBool>,
    logical_to_physical_core: HashMap<u32, u32>,
    apic_to_logical_cpu: HashMap<u32, u32>,
    journalctl_fetcher: JournalctlFetcher,
    cpuinfo_reader: CpuInfoReader,
    poll_interval: Duration,
}

impl MceMonitor {
    pub fn new() -> Self {
        Self {
            thread_handle: None,
            errors: Arc::new(Mutex::new(Vec::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
            logical_to_physical_core: HashMap::new(),
            apic_to_logical_cpu: HashMap::new(),
            journalctl_fetcher: Arc::new(fetch_journalctl_since),
            cpuinfo_reader: Arc::new(read_cpuinfo),
            poll_interval: Duration::from_secs(5),
        }
    }

    #[cfg(test)]
    fn new_with_overrides(
        poll_interval: Duration,
        journalctl_fetcher: JournalctlFetcher,
        cpuinfo_reader: CpuInfoReader,
    ) -> Self {
        Self {
            thread_handle: None,
            errors: Arc::new(Mutex::new(Vec::new())),
            shutdown: Arc::new(AtomicBool::new(false)),
            logical_to_physical_core: HashMap::new(),
            apic_to_logical_cpu: HashMap::new(),
            journalctl_fetcher,
            cpuinfo_reader,
            poll_interval,
        }
    }

    pub fn start(&mut self, topology: &CpuTopology) -> Result<()> {
        if self.thread_handle.is_some() {
            return Ok(());
        }

        self.shutdown.store(false, Ordering::SeqCst);
        self.logical_to_physical_core = build_logical_to_physical_map(topology);
        self.apic_to_logical_cpu = match (self.cpuinfo_reader)()
            .and_then(|cpuinfo| parse_apic_to_logical_cpu_map(&cpuinfo))
        {
            Ok(mapping) => mapping,
            Err(error) => {
                warn!("failed to initialize APIC mapping, continuing with logical CPU IDs only: {error}");
                HashMap::new()
            }
        };

        let errors = Arc::clone(&self.errors);
        let shutdown = Arc::clone(&self.shutdown);
        let fetcher = Arc::clone(&self.journalctl_fetcher);
        let poll_interval = self.poll_interval;

        let thread_handle = thread::spawn(move || {
            info!("starting MCE/EDAC journalctl monitor thread");
            let mut last_check_time = journalctl_since_now();

            while !shutdown.load(Ordering::Relaxed) {
                match fetcher(&last_check_time) {
                    Ok(stdout) => {
                        let mut parsed = Vec::new();
                        for line in stdout.lines() {
                            if let Some(error) = parse_mce_or_edac_line(line) {
                                parsed.push(error);
                            }
                        }

                        if !parsed.is_empty() {
                            if let Ok(mut guard) = errors.lock() {
                                guard.extend(parsed);
                            } else {
                                warn!(
                                    "failed to lock MCE error collection; stopping monitor thread"
                                );
                                break;
                            }
                        }
                    }
                    Err(error) => {
                        if is_journalctl_unavailable_or_permission_denied(&error.to_string()) {
                            warn!("journalctl unavailable, MCE monitoring disabled: {error}");
                            break;
                        }

                        warn!("journalctl poll failed, continuing without new MCE data: {error}");
                    }
                }

                last_check_time = journalctl_since_now();

                let mut waited = Duration::ZERO;
                while waited < poll_interval {
                    if shutdown.load(Ordering::Relaxed) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                    waited += Duration::from_millis(100);
                }
            }

            info!("stopped MCE/EDAC journalctl monitor thread");
        });

        self.thread_handle = Some(thread_handle);
        Ok(())
    }

    pub fn stop(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        if let Some(handle) = self.thread_handle.take() {
            let _ = handle.join();
        }
    }

    pub fn get_errors(&self) -> Vec<MceError> {
        self.errors
            .lock()
            .map(|errors| errors.clone())
            .unwrap_or_default()
    }

    pub fn get_errors_for_core(&self, core_id: u32) -> Vec<MceError> {
        self.get_errors()
            .into_iter()
            .filter(|error| {
                let physical = error
                    .apic_id
                    .and_then(|apic| {
                        resolve_apic_to_physical_core(
                            apic,
                            &self.apic_to_logical_cpu,
                            &self.logical_to_physical_core,
                        )
                    })
                    .or_else(|| self.logical_to_physical_core.get(&error.cpu_id).copied());

                physical == Some(core_id)
            })
            .collect()
    }
}

impl Default for MceMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for MceMonitor {
    fn drop(&mut self) {
        self.stop();
    }
}

fn fetch_journalctl_since(since: &str) -> Result<String> {
    let output = Command::new("journalctl")
        .args(["-k", "-b", "--since", since, "--no-pager"])
        .output()
        .context("failed to execute journalctl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "journalctl exited with status {}: {}",
            output.status,
            stderr.trim()
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn read_cpuinfo() -> Result<String> {
    fs::read_to_string("/proc/cpuinfo").context("failed to read /proc/cpuinfo for APIC mapping")
}

fn parse_mce_or_edac_line(line: &str) -> Option<MceError> {
    if let Some(machine_check) = machine_check_regex() {
        if let Some(captures) = machine_check.captures(line) {
            let cpu_id = captures.get(1)?.as_str().parse::<u32>().ok()?;
            let bank = captures.get(2)?.as_str().parse::<u32>().ok();

            return Some(MceError {
                cpu_id,
                bank,
                error_type: MceErrorType::MachineCheck,
                message: line.to_string(),
                timestamp: extract_timestamp(line),
                apic_id: extract_apic_id(line),
            });
        }
    }

    if edac_uncorrectable_regex().is_some_and(|regex| regex.is_match(line)) {
        return Some(MceError {
            cpu_id: 0,
            bank: None,
            error_type: MceErrorType::EdacUncorrectable,
            message: line.to_string(),
            timestamp: extract_timestamp(line),
            apic_id: extract_apic_id(line),
        });
    }

    if edac_correctable_regex().is_some_and(|regex| regex.is_match(line)) {
        return Some(MceError {
            cpu_id: 0,
            bank: None,
            error_type: MceErrorType::EdacCorrectable,
            message: line.to_string(),
            timestamp: extract_timestamp(line),
            apic_id: extract_apic_id(line),
        });
    }

    if hardware_error_regex().is_some_and(|regex| regex.is_match(line)) {
        let cpu_id = cpu_regex()
            .and_then(|regex| regex.captures(line))
            .and_then(|captures| captures.get(1))
            .and_then(|cpu| cpu.as_str().parse::<u32>().ok())
            .unwrap_or(0);

        return Some(MceError {
            cpu_id,
            bank: None,
            error_type: MceErrorType::HardwareError,
            message: line.to_string(),
            timestamp: extract_timestamp(line),
            apic_id: extract_apic_id(line),
        });
    }

    None
}

fn extract_timestamp(line: &str) -> String {
    if let Some((prefix, _)) = line.split_once(" kernel:") {
        return prefix.trim().to_string();
    }
    current_local_timestamp_string()
}

fn extract_apic_id(line: &str) -> Option<u32> {
    apic_regex()
        .and_then(|regex| regex.captures(line))
        .and_then(|captures| captures.get(1))
        .and_then(|value| value.as_str().parse::<u32>().ok())
}

fn build_logical_to_physical_map(topology: &CpuTopology) -> HashMap<u32, u32> {
    topology
        .core_map
        .iter()
        .flat_map(|(physical, logicals)| logicals.iter().map(move |logical| (*logical, *physical)))
        .collect()
}

fn parse_apic_to_logical_cpu_map(cpuinfo: &str) -> Result<HashMap<u32, u32>> {
    let mut map = HashMap::new();
    let mut current_processor: Option<u32> = None;

    for line in cpuinfo.lines() {
        if let Some((key, value)) = line.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            if key == "processor" {
                current_processor = value.parse::<u32>().ok();
            } else if key == "apicid" {
                if let (Some(logical_cpu), Ok(apic_id)) = (current_processor, value.parse::<u32>())
                {
                    map.insert(apic_id, logical_cpu);
                }
            }
        }
    }

    if map.is_empty() {
        return Err(anyhow!(
            "failed to build APIC ID mapping from /proc/cpuinfo"
        ));
    }

    Ok(map)
}

fn resolve_apic_to_physical_core(
    apic_id: u32,
    apic_to_logical_cpu: &HashMap<u32, u32>,
    logical_to_physical_core: &HashMap<u32, u32>,
) -> Option<u32> {
    let logical_cpu = apic_to_logical_cpu.get(&apic_id)?;
    logical_to_physical_core.get(logical_cpu).copied()
}

fn is_journalctl_unavailable_or_permission_denied(error_text: &str) -> bool {
    let error = error_text.to_ascii_lowercase();
    error.contains("permission denied")
        || error.contains("operation not permitted")
        || error.contains("not found")
        || error.contains("no such file")
        || error.contains("failed to execute journalctl")
}

fn journalctl_since_now() -> String {
    let now = OffsetDateTime::now_utc();
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn current_local_timestamp_string() -> String {
    let now = OffsetDateTime::now_local().unwrap_or_else(|_| OffsetDateTime::now_utc());
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        now.year(),
        u8::from(now.month()),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    )
}

fn machine_check_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| {
            Regex::new(
            r"(?i)mce:\s*\[hardware error\]:\s*cpu\s*(\d+):\s*machine check(?: exception)?:\s*\d+\s*bank\s*(\d+):\s*(.+)",
        )
        })
        .as_ref()
        .ok()
}

fn hardware_error_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)mce:\s*\[hardware error\].*"))
        .as_ref()
        .ok()
}

fn edac_correctable_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)edac.*ce.*"))
        .as_ref()
        .ok()
}

fn edac_uncorrectable_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)edac.*ue.*"))
        .as_ref()
        .ok()
}

fn cpu_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)cpu\s*(\d+)"))
        .as_ref()
        .ok()
}

fn apic_regex() -> Option<&'static Regex> {
    static REGEX: OnceLock<Result<Regex, regex::Error>> = OnceLock::new();
    REGEX
        .get_or_init(|| Regex::new(r"(?i)apic(?:id)?[^0-9]*(\d+)"))
        .as_ref()
        .ok()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn given_mce_log_line_when_parsing_then_extracts_cpu_and_bank() {
        let line =
            "Feb 27 12:00:00 host kernel: mce: [Hardware Error]: CPU 0: Machine Check Exception: 5 Bank 27: bea0000001000108";

        let parsed = parse_mce_or_edac_line(line).expect("MCE line should be parsed");

        assert_eq!(parsed.cpu_id, 0);
        assert_eq!(parsed.bank, Some(27));
        assert_eq!(parsed.error_type, MceErrorType::MachineCheck);
        assert!(parsed.message.contains("Machine Check Exception"));
    }

    #[test]
    fn given_hardware_error_line_when_parsing_then_detects_error() {
        let line =
            "Feb 27 12:00:00 host kernel: mce: [Hardware Error]: generic hardware error on CPU 3";

        let parsed = parse_mce_or_edac_line(line).expect("hardware error should be parsed");

        assert_eq!(parsed.error_type, MceErrorType::HardwareError);
        assert_eq!(parsed.cpu_id, 3);
    }

    #[test]
    fn given_edac_correctable_line_when_parsing_then_extracts_details() {
        let line = "Feb 27 12:00:00 host kernel: EDAC MC0: CE on CPU#2Channel#1_DIMM#0";

        let parsed = parse_mce_or_edac_line(line).expect("EDAC CE should be parsed");

        assert_eq!(parsed.error_type, MceErrorType::EdacCorrectable);
        assert_eq!(parsed.bank, None);
    }

    #[test]
    fn given_apic_id_when_mapping_then_resolves_to_physical_core() {
        let cpuinfo = "processor : 0
apicid : 0

processor : 8
apicid : 16
";

        let apic_to_logical =
            parse_apic_to_logical_cpu_map(cpuinfo).expect("APIC map should parse from cpuinfo");
        let topology = CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen".to_string(),
            physical_core_count: 2,
            logical_cpu_count: 2,
            core_map: BTreeMap::from([(0, vec![0]), (6, vec![8])]),
            bios_map: BTreeMap::from([(0, 0), (6, 1)]),
            physical_map: BTreeMap::from([(0, 0), (1, 6)]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        };
        let logical_to_physical = build_logical_to_physical_map(&topology);

        let core = resolve_apic_to_physical_core(16, &apic_to_logical, &logical_to_physical);

        assert_eq!(core, Some(6));
    }

    #[test]
    fn given_clean_journal_when_monitoring_then_returns_no_errors() {
        let topology = CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen".to_string(),
            physical_core_count: 1,
            logical_cpu_count: 1,
            core_map: BTreeMap::from([(0, vec![0])]),
            bios_map: BTreeMap::from([(0, 0)]),
            physical_map: BTreeMap::from([(0, 0)]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        };

        let mut monitor = MceMonitor::new_with_overrides(
            Duration::from_millis(50),
            Arc::new(|_| Ok(String::new())),
            Arc::new(|| Ok("processor : 0\napicid : 0\n".to_string())),
        );

        monitor
            .start(&topology)
            .expect("monitor should start successfully");
        thread::sleep(Duration::from_millis(120));
        monitor.stop();

        assert!(monitor.get_errors().is_empty());
    }

    #[test]
    fn given_multiple_errors_when_monitoring_then_aggregates_by_core() {
        let topology = CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen".to_string(),
            physical_core_count: 2,
            logical_cpu_count: 2,
            core_map: BTreeMap::from([(0, vec![0]), (6, vec![8])]),
            bios_map: BTreeMap::from([(0, 0), (6, 1)]),
            physical_map: BTreeMap::from([(0, 0), (1, 6)]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        };

        let stdout = "Feb 27 12:00:00 host kernel: mce: [Hardware Error]: CPU 0: Machine Check Exception: 5 Bank 27: bea0000001000108
Feb 27 12:00:01 host kernel: mce: [Hardware Error]: CPU 8: Machine Check Exception: 5 Bank 10: deadbeef";

        let mut monitor = MceMonitor::new_with_overrides(
            Duration::from_millis(200),
            Arc::new(move |_| Ok(stdout.to_string())),
            Arc::new(|| Ok("processor : 0\napicid : 0\nprocessor : 8\napicid : 16\n".to_string())),
        );

        monitor
            .start(&topology)
            .expect("monitor should start successfully");
        thread::sleep(Duration::from_millis(80));
        monitor.stop();

        let core0 = monitor.get_errors_for_core(0);
        let core6 = monitor.get_errors_for_core(6);

        assert_eq!(core0.len(), 1);
        assert_eq!(core6.len(), 1);
        assert_eq!(monitor.get_errors().len(), 2);
    }

    #[test]
    fn given_edac_uncorrectable_line_when_parsing_then_extracts_details() {
        let line = "Feb 27 12:00:00 host kernel: EDAC MC0: UE row 1, channel 0";

        let parsed = parse_mce_or_edac_line(line).expect("EDAC UE should be parsed");

        assert_eq!(parsed.error_type, MceErrorType::EdacUncorrectable);
    }

    #[test]
    fn given_journalctl_unavailable_when_monitoring_then_degrades_gracefully() {
        let topology = CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen".to_string(),
            physical_core_count: 1,
            logical_cpu_count: 1,
            core_map: BTreeMap::from([(0, vec![0])]),
            bios_map: BTreeMap::from([(0, 0)]),
            physical_map: BTreeMap::from([(0, 0)]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        };

        let mut monitor = MceMonitor::new_with_overrides(
            Duration::from_millis(20),
            Arc::new(|_| Err(anyhow!("No such file or directory"))),
            Arc::new(|| Ok("processor : 0\napicid : 0\n".to_string())),
        );

        monitor
            .start(&topology)
            .expect("missing journalctl should not fail start");
        thread::sleep(Duration::from_millis(60));
        monitor.stop();

        assert!(monitor.get_errors().is_empty());
    }
}
