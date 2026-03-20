pub mod benchmark;
pub mod co_decoder;
pub mod co_heuristic;
pub mod co_offsets;
pub mod co_reader;
pub mod coordinator;
pub mod cpu_topology;
pub mod embedded;
pub mod error_parser;
pub mod hii_extractor;
pub mod hii_question;
pub mod ifr_parser;
pub mod mce_monitor;
pub mod mprime_config;
pub mod mprime_runner;
pub mod report;
pub mod signal_handler;
pub mod uefi_reader;

use std::collections::BTreeSet;
use std::fs;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use argh::FromArgs;
use tracing::{error, info, instrument, warn};

use coordinator::{Coordinator, CoreStatus};
use cpu_topology::{detect_cpu_topology, CpuTopology};
use embedded::ExtractedBinaries;
use mprime_config::StressTestMode;
use report::StabilityReport;
use signal_handler::Cleanup;

const EXIT_STABLE: i32 = 0;
const EXIT_UNSTABLE: i32 = 1;
const EXIT_ERROR: i32 = 2;

/// Detect unstable CPU cores on AMD Linux systems using mprime stress testing
#[derive(FromArgs, Debug, PartialEq, Eq)]
struct Args {
    /// duration to test each core, e.g. 6m, 30s, 1h, 1h30m (default: 6m)
    #[argh(option, short = 'd', default = "String::from(\"6m\")")]
    duration: String,

    /// number of full cycles through all cores (default: 3)
    #[argh(option, short = 'i', default = "3_u32")]
    iterations: u32,

    /// only test specific cores (comma-separated, e.g. "0,2,5")
    #[argh(option, short = 'c')]
    cores: Option<String>,

    /// only output machine-readable RESULT line
    #[argh(switch, short = 'q')]
    quiet: bool,

    /// stop testing immediately when the first core fails
    #[argh(switch, short = 'b')]
    bail: bool,

    /// stress test mode: sse, avx, avx2 (default: sse)
    #[argh(option, short = 'm', default = "String::from(\"sse\")")]
    mode: String,

    /// run FFT preset benchmark to find fastest instability detection preset
    #[argh(switch)]
    benchmark: bool,

    /// internal: read UEFI settings as root and print JSON to stdout, then exit
    #[argh(switch)]
    pub uefi_only: bool,
}

fn main() {
    tracing_subscriber::fmt::init();

    let exit_code = match run() {
        Ok(code) => code,
        Err(err) => {
            error!(%err, "unstable-cpu-detector failed");
            eprintln!("Error: {err:#}");
            EXIT_ERROR
        }
    };

    std::process::exit(exit_code);
}

#[instrument]
fn run() -> Result<i32> {
    let args: Args = argh::from_env();

    if args.uefi_only {
        let physical_core_count = match detect_cpu_topology() {
            Ok(topo) => topo.physical_core_count,
            Err(e) => {
                tracing::warn!(error = %e, "topology detection failed before UEFI read, defaulting to 16 cores");
                16
            }
        };
        let settings = uefi_reader::attempt_uefi_read_with_escalation(physical_core_count);
        let json = serde_json::to_string(&settings).unwrap_or_else(|_| {
            r#"{"available":false,"unavailable_reason":"JSON serialization failed"}"#.to_string()
        });
        println!("{json}");
        return Ok(EXIT_STABLE);
    }

    validate_platform(cfg!(target_os = "linux"), std::mem::size_of::<usize>())?;

    let topology = detect_cpu_topology().context("failed to detect CPU topology")?;
    validate_amd_vendor(&topology)?;
    info!(
        cpu_model = %topology.model_name,
        vendor = %topology.vendor,
        physical_cores = topology.physical_core_count,
        logical_cpus = topology.logical_cpu_count,
        "CPU topology detected"
    );
    let uefi_settings =
        uefi_reader::attempt_uefi_read_with_escalation(topology.physical_core_count);
    if let Some(pbo) = &uefi_settings.pbo_status {
        info!(pbo_status = %pbo, "UEFI PBO status detected");
    }
    if !uefi_settings.available {
        info!(reason = ?uefi_settings.unavailable_reason, "UEFI settings unavailable — report will not include BIOS settings");
    }
    warn_if_root();
    check_temp_dir_writable().context("temporary directory pre-flight check failed")?;

    let core_filter = parse_core_filter(args.cores.as_deref(), &topology)?;
    let mode = parse_stress_mode(&args.mode)?;
    print_startup_banner(
        &topology,
        &args,
        mode,
        core_filter.as_deref(),
        Some(&uefi_settings),
    );

    let extracted = ExtractedBinaries::extract().context("failed to extract embedded binaries")?;
    signal_handler::register_handler().context("failed to register signal handler")?;

    if args.benchmark {
        let report = benchmark::run_benchmark(&topology, &extracted).context("benchmark failed")?;
        println!("{report}");
        return Ok(EXIT_STABLE);
    }

    let cleanup = Cleanup::new();
    {
        let mut guard = cleanup
            .lock()
            .map_err(|_| anyhow::anyhow!("cleanup state lock poisoned"))?;
        guard.register_temp_dir(extracted.temp_dir.clone());
    }

    let run_result =
        run_coordinator_and_report(&args, &topology, &extracted, core_filter, &uefi_settings);
    let cleanup_result = {
        let mut guard = cleanup
            .lock()
            .map_err(|_| anyhow::anyhow!("cleanup state lock poisoned"))?;
        guard.execute()
    };

    let exit_code = run_result?;
    cleanup_result.context("cleanup failed after test execution")?;
    Ok(exit_code)
}

#[instrument(skip(args, topology, extracted, core_filter, uefi_settings))]
fn run_coordinator_and_report(
    args: &Args,
    topology: &CpuTopology,
    extracted: &ExtractedBinaries,
    core_filter: Option<Vec<u32>>,
    uefi_settings: &uefi_reader::UefiSettings,
) -> Result<i32> {
    let duration_per_core = parse_duration(&args.duration).context("invalid --duration value")?;
    let coordinator = Coordinator::new(
        duration_per_core,
        args.iterations,
        core_filter,
        args.quiet,
        args.bail,
    );
    let results = coordinator
        .run(topology, extracted)
        .context("coordinator run failed")?;

    let report = StabilityReport::new(&results, topology, Some(uefi_settings))
        .with_quiet(args.quiet)
        .generate()
        .context("failed to generate stability report")?;
    print!("{report}");

    let has_unstable_core = results
        .results
        .iter()
        .any(|result| result.status == CoreStatus::Failed);

    if has_unstable_core {
        Ok(EXIT_UNSTABLE)
    } else {
        Ok(EXIT_STABLE)
    }
}

fn validate_platform(is_linux: bool, pointer_width_bytes: usize) -> Result<()> {
    if !is_linux {
        bail!("unsupported operating system: Linux is required");
    }

    if pointer_width_bytes != 8 {
        bail!("unsupported CPU architecture: 64-bit execution is required");
    }

    Ok(())
}

fn validate_amd_vendor(topology: &CpuTopology) -> Result<()> {
    if topology.vendor != "AuthenticAMD" {
        bail!(
            "Non-AMD CPU detected ('{}'). This tool only supports AMD processors.",
            topology.vendor
        );
    }

    Ok(())
}

fn warn_if_root() {
    if nix::unistd::getuid().is_root() {
        warn!("running as root is not required for this tool");
    }
}

fn check_temp_dir_writable() -> Result<()> {
    let probe_dir = std::env::temp_dir().join(format!(
        "unstable-cpu-detector-probe-{}",
        uuid::Uuid::new_v4()
    ));

    fs::create_dir_all(&probe_dir).with_context(|| {
        format!(
            "failed to create temp probe directory {}",
            probe_dir.display()
        )
    })?;

    let probe_file = probe_dir.join("write-test.bin");
    fs::write(&probe_file, [0_u8])
        .with_context(|| format!("failed to write temp probe file {}", probe_file.display()))?;

    fs::remove_dir_all(&probe_dir).with_context(|| {
        format!(
            "failed to clean temp probe directory {}",
            probe_dir.display()
        )
    })?;

    Ok(())
}

fn parse_core_filter(cores: Option<&str>, topology: &CpuTopology) -> Result<Option<Vec<u32>>> {
    let Some(cores) = cores else {
        return Ok(None);
    };

    let mut parsed = Vec::new();
    for token in cores.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        let core_id = token
            .parse::<u32>()
            .with_context(|| format!("invalid core id '{token}' in --cores list"))?;
        parsed.push(core_id);
    }

    if parsed.is_empty() {
        bail!("--cores was provided but no valid core IDs were found");
    }

    parsed.sort_unstable();
    parsed.dedup();

    let available: BTreeSet<u32> = topology.core_map.keys().copied().collect();
    let invalid: Vec<u32> = parsed
        .iter()
        .copied()
        .filter(|core| !available.contains(core))
        .collect();

    if !invalid.is_empty() {
        let invalid_list = invalid
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");
        let valid_list = available
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(",");

        bail!("invalid core id(s): {invalid_list}. Available physical core IDs: {valid_list}");
    }

    Ok(Some(parsed))
}

fn parse_stress_mode(mode: &str) -> Result<StressTestMode> {
    match mode.trim().to_ascii_lowercase().as_str() {
        "sse" => Ok(StressTestMode::SSE),
        "avx" => Ok(StressTestMode::AVX),
        "avx2" => Ok(StressTestMode::AVX2),
        other => bail!("invalid --mode '{other}'. Supported values: sse, avx, avx2"),
    }
}

/// Parses a human-friendly duration string into a `Duration`.
///
/// Supported formats:
///   - `30s`     → 30 seconds
///   - `6m`     → 6 minutes
///   - `1h`     → 1 hour
///   - `1h30m`  → 1 hour 30 minutes
///   - `2h15m30s` → 2 hours 15 minutes 30 seconds
///   - `6`      → 6 minutes (bare number, backward compatible)
fn parse_duration(input: &str) -> Result<Duration> {
    let input = input.trim();
    if input.is_empty() {
        bail!("duration cannot be empty");
    }

    // Bare number: treat as minutes for backward compatibility
    if let Ok(minutes) = input.parse::<u64>() {
        return Ok(Duration::from_secs(minutes * 60));
    }

    let mut total_secs: u64 = 0;
    let mut current_num = String::new();
    let mut found_any_unit = false;
    let mut seen_h = false;
    let mut seen_m = false;
    let mut seen_s = false;

    for ch in input.chars() {
        match ch {
            '0'..='9' => current_num.push(ch),
            'h' | 'H' => {
                if seen_h {
                    bail!("duplicate 'h' in duration '{input}'");
                }
                if current_num.is_empty() {
                    bail!("missing number before 'h' in duration '{input}'");
                }
                let hours: u64 = current_num.parse().with_context(|| {
                    format!("invalid hours value in duration '{input}'")
                })?;
                total_secs += hours * 3600;
                current_num.clear();
                found_any_unit = true;
                seen_h = true;
            }
            'm' | 'M' => {
                if seen_m {
                    bail!("duplicate 'm' in duration '{input}'");
                }
                if current_num.is_empty() {
                    bail!("missing number before 'm' in duration '{input}'");
                }
                let minutes: u64 = current_num.parse().with_context(|| {
                    format!("invalid minutes value in duration '{input}'")
                })?;
                total_secs += minutes * 60;
                current_num.clear();
                found_any_unit = true;
                seen_m = true;
            }
            's' | 'S' => {
                if seen_s {
                    bail!("duplicate 's' in duration '{input}'");
                }
                if current_num.is_empty() {
                    bail!("missing number before 's' in duration '{input}'");
                }
                let seconds: u64 = current_num.parse().with_context(|| {
                    format!("invalid seconds value in duration '{input}'")
                })?;
                total_secs += seconds;
                current_num.clear();
                found_any_unit = true;
                seen_s = true;
            }
            _ => bail!("unexpected character '{ch}' in duration '{input}'. Use combinations of h, m, s (e.g. 1h30m, 5m, 90s)"),
        }
    }

    if !current_num.is_empty() {
        bail!(
            "trailing number '{current_num}' without unit in duration '{input}'. Use h, m, or s suffix"
        );
    }

    if !found_any_unit {
        bail!("no valid duration components found in '{input}'");
    }

    Ok(Duration::from_secs(total_secs))
}

fn print_startup_banner(
    topology: &CpuTopology,
    args: &Args,
    mode: StressTestMode,
    core_filter: Option<&[u32]>,
    uefi_settings: Option<&uefi_reader::UefiSettings>,
) {
    let mode_name = match mode {
        StressTestMode::SSE => "sse",
        StressTestMode::AVX => "avx",
        StressTestMode::AVX2 => "avx2",
        StressTestMode::AVX512 => "avx512",
        StressTestMode::Custom { .. } => "custom",
    };

    let selected_cores = match core_filter {
        Some(cores) => cores
            .iter()
            .map(u32::to_string)
            .collect::<Vec<_>>()
            .join(","),
        None => "all".to_string(),
    };

    info!(
        cpu_model = %topology.model_name,
        physical_cores = topology.physical_core_count,
        logical_cpus = topology.logical_cpu_count,
        duration = %args.duration,
        iterations = args.iterations,
        mode = mode_name,
        selected_cores = %selected_cores,
        quiet = args.quiet,
        "startup configuration"
    );

    if !args.quiet {
        println!("unstable-cpu-detector");
        println!("CPU: {}", topology.model_name);
        println!("{}", format_uefi_status_line(uefi_settings));
        for co_line in format_co_offsets_lines(uefi_settings) {
            println!("{co_line}");
        }
        println!(
            "Config: duration={}/core iterations={} mode={} cores={} quiet={}",
            args.duration, args.iterations, mode_name, selected_cores, args.quiet
        );
    }
}

fn format_uefi_status_line(uefi_settings: Option<&uefi_reader::UefiSettings>) -> String {
    match uefi_settings {
        Some(settings) if settings.available => match settings.pbo_status.as_deref() {
            Some(pbo_status) => format!("UEFI Settings: Available (PBO: {pbo_status})"),
            None => "UEFI Settings: Available".to_string(),
        },
        Some(settings) => format!(
            "UEFI Settings: Unavailable ({})",
            settings
                .unavailable_reason
                .as_deref()
                .unwrap_or("unknown reason")
        ),
        None => "UEFI Settings: Not checked".to_string(),
    }
}

fn format_co_offsets_lines(uefi_settings: Option<&uefi_reader::UefiSettings>) -> Vec<String> {
    let Some(settings) = uefi_settings else {
        return vec![];
    };
    if !settings.available {
        return vec![];
    }
    let Some(offsets) = settings.curve_optimizer_offsets.as_ref() else {
        return vec![];
    };
    if offsets.is_empty() {
        return vec![];
    }
    let mut lines = vec!["CO offsets:".to_string()];
    for (core, offset) in offsets {
        lines.push(format!("  Core {core:>2}: {offset}"));
    }
    lines
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;

    fn test_topology() -> CpuTopology {
        CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen Test".to_string(),
            physical_core_count: 3,
            logical_cpu_count: 3,
            core_map: BTreeMap::from([(0, vec![0]), (1, vec![1]), (5, vec![5])]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        }
    }

    #[test]
    fn given_available_uefi_settings_when_formatting_status_line_then_includes_pbo_status() {
        let settings = uefi_reader::UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            ..Default::default()
        };

        let line = format_uefi_status_line(Some(&settings));

        assert_eq!(line, "UEFI Settings: Available (PBO: Enabled)");
    }

    #[test]
    fn given_unavailable_uefi_settings_when_formatting_status_line_then_includes_reason() {
        let settings = uefi_reader::UefiSettings::unavailable("requires root");

        let line = format_uefi_status_line(Some(&settings));

        assert_eq!(line, "UEFI Settings: Unavailable (requires root)");
    }

    #[test]
    fn given_available_uefi_with_co_offsets_when_formatting_then_shows_per_core_values() {
        let settings = uefi_reader::UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            curve_optimizer_offsets: Some(BTreeMap::from([(0, -25), (1, -10), (5, -30)])),
            ..Default::default()
        };

        let lines = format_co_offsets_lines(Some(&settings));

        assert_eq!(
            lines,
            vec![
                "CO offsets:",
                "  Core  0: -25",
                "  Core  1: -10",
                "  Core  5: -30",
            ]
        );
    }

    #[test]
    fn given_available_uefi_without_co_offsets_when_formatting_then_returns_empty() {
        let settings = uefi_reader::UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            ..Default::default()
        };

        assert!(format_co_offsets_lines(Some(&settings)).is_empty());
    }

    #[test]
    fn given_unavailable_uefi_when_formatting_co_lines_then_returns_empty() {
        let settings = uefi_reader::UefiSettings::unavailable("requires root");

        assert!(format_co_offsets_lines(Some(&settings)).is_empty());
    }

    #[test]
    fn given_empty_co_offsets_when_formatting_then_returns_empty() {
        let settings = uefi_reader::UefiSettings {
            available: true,
            curve_optimizer_offsets: Some(BTreeMap::new()),
            ..Default::default()
        };

        assert!(format_co_offsets_lines(Some(&settings)).is_empty());
    }

    #[test]
    fn given_no_uefi_settings_when_formatting_co_lines_then_returns_empty() {
        assert!(format_co_offsets_lines(None).is_empty());
    }

    #[test]
    fn given_no_args_when_running_then_uses_sensible_defaults() {
        let args = Args::from_args(&["unstable-cpu-detector"], &[])
            .expect("default args should parse successfully");

        assert_eq!(args.duration, "6m");
        assert_eq!(args.iterations, 3);
        assert!(!args.quiet);
        assert!(!args.bail);
        assert_eq!(args.mode, "sse");
        assert_eq!(args.cores, None);
    }

    #[test]
    fn given_help_flag_when_running_then_shows_usage() {
        let result = Args::from_args(&["unstable-cpu-detector"], &["--help"])
            .expect_err("--help should produce early-exit output");
        let help_text = result.output;

        assert!(help_text.to_ascii_lowercase().contains("usage"));
        assert!(help_text.contains("--duration"));
        assert!(help_text.contains("--iterations"));
    }

    #[test]
    fn given_duration_arg_when_parsing_then_sets_per_core_duration() {
        let args = Args::from_args(&["unstable-cpu-detector"], &["--duration", "9"])
            .expect("duration arg should parse successfully");

        assert_eq!(args.duration, "9");
    }

    #[test]
    fn given_iterations_arg_when_parsing_then_sets_iteration_count() {
        let args = Args::from_args(&["unstable-cpu-detector"], &["--iterations", "7"])
            .expect("iterations arg should parse successfully");

        assert_eq!(args.iterations, 7);
    }

    #[test]
    fn given_quiet_flag_when_parsing_then_enables_machine_readable_output() {
        let args = Args::from_args(&["unstable-cpu-detector"], &["--quiet"])
            .expect("quiet arg should parse successfully");

        assert!(args.quiet);
    }

    #[test]
    fn given_cores_arg_when_parsing_then_filters_to_specified_cores() {
        let topology = test_topology();

        let parsed = parse_core_filter(Some("0,5"), &topology)
            .expect("valid --cores list should parse successfully");

        assert_eq!(parsed, Some(vec![0, 5]));
    }

    #[test]
    fn given_invalid_core_id_when_parsing_then_exits_with_error() {
        let topology = test_topology();

        let error = parse_core_filter(Some("99"), &topology)
            .expect_err("invalid core id should return an error")
            .to_string();

        assert!(error.contains("invalid core id"));
        assert!(error.contains("0,1,5"));
    }

    #[test]
    fn given_non_amd_cpu_when_starting_then_exits_with_error() {
        let mut topology = test_topology();
        topology.vendor = "GenuineIntel".to_string();

        let error = validate_amd_vendor(&topology)
            .expect_err("non-AMD topology should be rejected")
            .to_string();

        assert!(error.contains("Non-AMD CPU detected"));
    }

    #[test]
    fn given_non_64bit_when_starting_then_exits_with_error() {
        let error = validate_platform(true, 4)
            .expect_err("32-bit pointer width should be rejected")
            .to_string();

        assert!(error.contains("64-bit"));
    }

    #[test]
    fn given_bare_number_when_parsing_duration_then_treats_as_minutes() {
        let duration = parse_duration("6").expect("bare number should parse as minutes");
        assert_eq!(duration, Duration::from_secs(360));
    }

    #[test]
    fn given_seconds_suffix_when_parsing_duration_then_returns_seconds() {
        let duration = parse_duration("30s").expect("seconds suffix should parse");
        assert_eq!(duration, Duration::from_secs(30));
    }

    #[test]
    fn given_minutes_suffix_when_parsing_duration_then_returns_minutes() {
        let duration = parse_duration("6m").expect("minutes suffix should parse");
        assert_eq!(duration, Duration::from_secs(360));
    }

    #[test]
    fn given_hours_suffix_when_parsing_duration_then_returns_hours() {
        let duration = parse_duration("1h").expect("hours suffix should parse");
        assert_eq!(duration, Duration::from_secs(3600));
    }

    #[test]
    fn given_combined_hours_and_minutes_when_parsing_then_sums_components() {
        let duration = parse_duration("1h30m").expect("combined h+m should parse");
        assert_eq!(duration, Duration::from_secs(5400));
    }

    #[test]
    fn given_all_components_when_parsing_then_sums_all() {
        let duration = parse_duration("2h15m30s").expect("combined h+m+s should parse");
        assert_eq!(duration, Duration::from_secs(2 * 3600 + 15 * 60 + 30));
    }

    #[test]
    fn given_only_seconds_when_parsing_then_handles_large_values() {
        let duration = parse_duration("90s").expect("90 seconds should parse");
        assert_eq!(duration, Duration::from_secs(90));
    }

    #[test]
    fn given_empty_string_when_parsing_duration_then_returns_error() {
        let error = parse_duration("")
            .expect_err("empty string should fail")
            .to_string();
        assert!(error.contains("empty"));
    }

    #[test]
    fn given_zero_duration_when_parsing_then_returns_zero() {
        let duration = parse_duration("0").expect("zero should be valid");
        assert_eq!(duration, Duration::from_secs(0));
    }

    #[test]
    fn given_zero_seconds_when_parsing_then_returns_zero() {
        let duration = parse_duration("0s").expect("zero seconds should be valid");
        assert_eq!(duration, Duration::from_secs(0));
    }

    #[test]
    fn given_invalid_characters_when_parsing_duration_then_returns_error() {
        let error = parse_duration("5x")
            .expect_err("invalid char should fail")
            .to_string();
        assert!(error.contains("unexpected character"));
    }

    #[test]
    fn given_trailing_number_when_parsing_duration_then_returns_error() {
        let error = parse_duration("1h30")
            .expect_err("trailing number should fail")
            .to_string();
        assert!(error.contains("trailing number"));
    }

    #[test]
    fn given_uppercase_units_when_parsing_duration_then_accepts() {
        let duration = parse_duration("1H30M").expect("uppercase should parse");
        assert_eq!(duration, Duration::from_secs(5400));
    }
}
