use std::fmt;
use std::fs;
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use tracing::{info, info_span, warn};

use crate::cpu_topology::CpuTopology;
use crate::embedded::ExtractedBinaries;
use crate::error_parser::{ErrorParser, MprimeErrorType};
use crate::mprime_config::{FftPreset, MprimeConfig};
use crate::mprime_runner::MprimeRunner;
use crate::signal_handler;

const TARGET_CORE: u32 = 2;
const ITERATIONS_PER_PRESET: u32 = 3;
const SHUTDOWN_POLL_INTERVAL: Duration = Duration::from_secs(1);
const ERROR_POLL_INTERVAL: Duration = Duration::from_secs(5);
const INITIAL_PIN_DELAY: Duration = Duration::from_secs(3);
const BENCHMARK_TIMEOUT: Duration = Duration::from_secs(180);

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub preset: FftPreset,
    pub iteration: u32,
    pub time_to_error: Option<Duration>,
    pub error_type: Option<MprimeErrorType>,
    pub fft_size: Option<u32>,
    pub timed_out: bool,
}

#[derive(Debug, Clone)]
pub struct PresetSummary {
    pub preset: FftPreset,
    pub results: Vec<BenchmarkResult>,
    pub fastest: Option<Duration>,
    pub average: Option<Duration>,
    pub slowest: Option<Duration>,
    pub error_rate: f64,
}

#[derive(Debug, Clone)]
pub struct BenchmarkReport {
    pub summaries: Vec<PresetSummary>,
    pub target_core: u32,
    pub winner: Option<FftPreset>,
    pub total_duration: Duration,
}

pub fn run_benchmark(
    topology: &CpuTopology,
    extracted: &ExtractedBinaries,
) -> Result<BenchmarkReport> {
    if !topology.core_map.contains_key(&TARGET_CORE) {
        bail!(
            "benchmark target core {} is not present in topology; available cores: {}",
            TARGET_CORE,
            topology
                .core_map
                .keys()
                .map(u32::to_string)
                .collect::<Vec<_>>()
                .join(",")
        );
    }

    let start = Instant::now();
    let mut runner = MprimeRunner::from_dependencies(extracted, topology);
    let mut summaries = Vec::with_capacity(FftPreset::all_presets().len());

    for &preset in FftPreset::all_presets() {
        let preset_span = info_span!("benchmark_preset", preset = %preset);
        let _preset_guard = preset_span.enter();
        info!("starting preset benchmark");

        let mut iteration_results = Vec::with_capacity(ITERATIONS_PER_PRESET as usize);
        let mut skip_preset = false;

        for iteration in 1..=ITERATIONS_PER_PRESET {
            if signal_handler::is_shutdown_requested() {
                let _ = runner.stop();
                summaries.push(summarize_preset(preset, iteration_results));
                let winner = select_winner(&summaries);
                return Ok(BenchmarkReport {
                    summaries,
                    target_core: TARGET_CORE,
                    winner,
                    total_duration: start.elapsed(),
                });
            }

            let config = MprimeConfig::builder()
                .fft_preset(preset)
                .disable_internal_affinity();

            let working_dir = extracted
                .temp_dir
                .join("benchmark")
                .join(format!("preset-{}", preset.name().to_ascii_lowercase()))
                .join(format!("iter-{iteration}"));
            fs::create_dir_all(&working_dir).with_context(|| {
                format!(
                    "failed to create benchmark working directory {}",
                    working_dir.display()
                )
            })?;

            if let Err(error) = runner.start(TARGET_CORE, &working_dir, Some(&config)) {
                warn!(
                    %error,
                    %preset,
                    iteration,
                    "failed to start mprime for benchmark iteration; skipping preset"
                );
                skip_preset = true;
                break;
            }

            let logical_cpu_id = topology
                .core_map
                .get(&TARGET_CORE)
                .and_then(|cpus| cpus.first().copied())
                .with_context(|| {
                    format!("target core {} is missing logical CPU mapping", TARGET_CORE)
                })?;

            thread::sleep(INITIAL_PIN_DELAY);
            let _ = runner.pin_all_threads(logical_cpu_id);

            let mut parser = ErrorParser::new();
            let mut elapsed = INITIAL_PIN_DELAY;
            let mut next_error_poll = ERROR_POLL_INTERVAL;
            let mut result = BenchmarkResult {
                preset,
                iteration,
                time_to_error: None,
                error_type: None,
                fft_size: None,
                timed_out: false,
            };

            while elapsed < BENCHMARK_TIMEOUT {
                if signal_handler::is_shutdown_requested() {
                    runner
                        .stop()
                        .context("failed to stop mprime after shutdown request")?;
                    iteration_results.push(result);
                    summaries.push(summarize_preset(preset, iteration_results));
                    let winner = select_winner(&summaries);
                    return Ok(BenchmarkReport {
                        summaries,
                        target_core: TARGET_CORE,
                        winner,
                        total_duration: start.elapsed(),
                    });
                }

                if elapsed >= next_error_poll {
                    let results_path = working_dir.join("results.txt");
                    if results_path.exists() {
                        let errors = parser.parse_results(&results_path).with_context(|| {
                            format!(
                                "failed to parse mprime benchmark results {}",
                                results_path.display()
                            )
                        })?;

                        if let Some(first_error) = errors.first() {
                            result.time_to_error = Some(elapsed);
                            result.error_type = Some(first_error.error_type.clone());
                            result.fft_size = first_error.fft_size;
                            runner
                                .stop()
                                .context("failed to stop mprime after benchmark error")?;
                            break;
                        }
                    }

                    next_error_poll += ERROR_POLL_INTERVAL;
                }

                if !runner
                    .is_running()
                    .context("failed to check benchmark mprime process liveness")?
                {
                    let results_path = working_dir.join("results.txt");
                    if results_path.exists() {
                        let errors = parser.parse_results(&results_path)?;
                        if let Some(first_error) = errors.first() {
                            result.time_to_error = Some(elapsed);
                            result.error_type = Some(first_error.error_type.clone());
                            result.fft_size = first_error.fft_size;
                        }
                    }
                    if result.time_to_error.is_none() {
                        result.time_to_error = Some(elapsed);
                        result.error_type = Some(MprimeErrorType::Unknown);
                    }
                    break;
                }

                thread::sleep(SHUTDOWN_POLL_INTERVAL);
                elapsed += SHUTDOWN_POLL_INTERVAL;
            }

            if elapsed >= BENCHMARK_TIMEOUT && result.time_to_error.is_none() {
                result.timed_out = true;
                runner
                    .stop()
                    .context("failed to stop mprime after benchmark timeout")?;
            }

            iteration_results.push(result);
        }

        if skip_preset {
            let _ = runner.stop();
        }

        summaries.push(summarize_preset(preset, iteration_results));
    }

    let winner = select_winner(&summaries);
    Ok(BenchmarkReport {
        summaries,
        target_core: TARGET_CORE,
        winner,
        total_duration: start.elapsed(),
    })
}

fn summarize_preset(preset: FftPreset, results: Vec<BenchmarkResult>) -> PresetSummary {
    let mut error_times: Vec<Duration> = results
        .iter()
        .filter_map(|result| result.time_to_error)
        .collect();
    error_times.sort_unstable();

    let fastest = error_times.first().copied();
    let slowest = error_times.last().copied();
    let average = if error_times.is_empty() {
        None
    } else {
        let total_secs: u64 = error_times.iter().map(Duration::as_secs).sum();
        Some(Duration::from_secs(total_secs / error_times.len() as u64))
    };

    let error_count = results
        .iter()
        .filter(|result| result.time_to_error.is_some())
        .count();
    let error_rate = if results.is_empty() {
        0.0
    } else {
        error_count as f64 / results.len() as f64
    };

    PresetSummary {
        preset,
        results,
        fastest,
        average,
        slowest,
        error_rate,
    }
}

fn select_winner(summaries: &[PresetSummary]) -> Option<FftPreset> {
    summaries
        .iter()
        .filter_map(|summary| summary.average.map(|avg| (summary.preset, avg)))
        .min_by_key(|(_, avg)| *avg)
        .map(|(preset, _)| preset)
}

impl fmt::Display for BenchmarkReport {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const WIDTH: usize = 100;
        let top = format!("╔{}╗", "═".repeat(WIDTH));
        let sep = format!("╠{}╣", "═".repeat(WIDTH));
        let bottom = format!("╚{}╝", "═".repeat(WIDTH));

        writeln!(f, "{top}")?;
        write_centered_line(
            f,
            WIDTH,
            &format!("FFT Preset Benchmark Results — Core {}", self.target_core),
        )?;
        writeln!(f, "{sep}")?;

        write_line(
            f,
            WIDTH,
            " Rank | Preset     | Range       | Errors Found | Fastest | Average | Slowest",
        )?;
        write_line(
            f,
            WIDTH,
            "──────┼────────────┼─────────────┼──────────────┼─────────┼─────────┼────────",
        )?;

        let mut ranked: Vec<&PresetSummary> = self
            .summaries
            .iter()
            .filter(|summary| summary.average.is_some())
            .collect();
        ranked.sort_by_key(|summary| summary.average);

        for (index, summary) in ranked.iter().enumerate() {
            let (min_fft, max_fft) = summary.preset.fft_range_kb();
            let line = format!(
                " {:>4} | {:<10} | {:>4}K-{:>5}K | {:>2}/{:<9} | {:>7} | {:>7} | {:>7}",
                index + 1,
                summary.preset.name(),
                min_fft,
                max_fft,
                summary
                    .results
                    .iter()
                    .filter(|r| r.time_to_error.is_some())
                    .count(),
                summary.results.len(),
                summary
                    .fastest
                    .map(format_duration_human)
                    .unwrap_or_else(|| "-".to_string()),
                summary
                    .average
                    .map(format_duration_human)
                    .unwrap_or_else(|| "-".to_string()),
                summary
                    .slowest
                    .map(format_duration_human)
                    .unwrap_or_else(|| "-".to_string())
            );
            write_line(f, WIDTH, &line)?;
        }

        let no_error_presets: Vec<&PresetSummary> = self
            .summaries
            .iter()
            .filter(|summary| summary.average.is_none())
            .collect();
        if !no_error_presets.is_empty() {
            write_line(f, WIDTH, "")?;
            write_line(f, WIDTH, " No errors detected:")?;
            for summary in no_error_presets {
                let (min_fft, max_fft) = summary.preset.fft_range_kb();
                let line = format!(
                    "  - {:<10} ({}K-{}K)",
                    summary.preset.name(),
                    min_fft,
                    max_fft
                );
                write_line(f, WIDTH, &line)?;
            }
        }

        writeln!(f, "{sep}")?;
        match self.winner {
            Some(winner) => {
                write_line(
                    f,
                    WIDTH,
                    &format!(
                        " Winner: {} (fastest average time-to-first-error)",
                        winner.name()
                    ),
                )?;
            }
            None => {
                write_line(
                    f,
                    WIDTH,
                    " WARNING: No errors detected for any preset. Core 2 may be stable.",
                )?;
            }
        }
        write_line(
            f,
            WIDTH,
            &format!(
                " Total benchmark duration: {}",
                format_duration_human(self.total_duration)
            ),
        )?;
        write!(f, "{bottom}")
    }
}

fn write_centered_line(f: &mut fmt::Formatter<'_>, width: usize, text: &str) -> fmt::Result {
    let chars = text.chars().count();
    let left_pad = width.saturating_sub(chars) / 2;
    let right_pad = width.saturating_sub(chars + left_pad);
    writeln!(
        f,
        "║{}{}{}║",
        " ".repeat(left_pad),
        text,
        " ".repeat(right_pad)
    )
}

fn write_line(f: &mut fmt::Formatter<'_>, width: usize, text: &str) -> fmt::Result {
    let text_len = text.chars().count();
    let padding = width.saturating_sub(text_len);
    writeln!(f, "║{}{}║", text, " ".repeat(padding))
}

fn format_duration_human(duration: Duration) -> String {
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

    #[test]
    fn given_summary_with_errors_when_calculating_then_computes_timing_stats() {
        let summary = summarize_preset(
            FftPreset::Huge,
            vec![
                BenchmarkResult {
                    preset: FftPreset::Huge,
                    iteration: 1,
                    time_to_error: Some(Duration::from_secs(60)),
                    error_type: Some(MprimeErrorType::RoundoffError),
                    fft_size: Some(1344),
                    timed_out: false,
                },
                BenchmarkResult {
                    preset: FftPreset::Huge,
                    iteration: 2,
                    time_to_error: None,
                    error_type: None,
                    fft_size: None,
                    timed_out: true,
                },
                BenchmarkResult {
                    preset: FftPreset::Huge,
                    iteration: 3,
                    time_to_error: Some(Duration::from_secs(120)),
                    error_type: Some(MprimeErrorType::FatalError),
                    fft_size: None,
                    timed_out: false,
                },
            ],
        );

        assert_eq!(summary.fastest, Some(Duration::from_secs(60)));
        assert_eq!(summary.average, Some(Duration::from_secs(90)));
        assert_eq!(summary.slowest, Some(Duration::from_secs(120)));
        assert!((summary.error_rate - (2.0 / 3.0)).abs() < f64::EPSILON);
    }

    #[test]
    fn given_no_error_summaries_when_rendering_report_then_shows_warning() {
        let report = BenchmarkReport {
            summaries: vec![PresetSummary {
                preset: FftPreset::Small,
                results: vec![BenchmarkResult {
                    preset: FftPreset::Small,
                    iteration: 1,
                    time_to_error: None,
                    error_type: None,
                    fft_size: None,
                    timed_out: true,
                }],
                fastest: None,
                average: None,
                slowest: None,
                error_rate: 0.0,
            }],
            target_core: 2,
            winner: None,
            total_duration: Duration::from_secs(180),
        };

        let rendered = report.to_string();
        assert!(rendered.contains("FFT Preset Benchmark Results — Core 2"));
        assert!(rendered.contains("No errors detected"));
        assert!(rendered.contains("Core 2 may be stable"));
    }
}
