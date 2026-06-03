use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use tracing::debug;

/// A single temperature snapshot read from k10temp hwmon.
#[derive(Debug, Clone, Copy)]
pub struct ThermalSnapshot {
    /// Package temperature in °C (Tctl).
    pub tctl: f32,
    /// CCD0 temperature in °C, or 0.0 if single-CCD.
    pub tccd1: f32,
    /// CCD1 temperature in °C, or 0.0 if single-CCD / not present.
    pub tccd2: f32,
}

/// Detects and reads AMD k10temp hwmon sensors for thermal monitoring.
///
/// On detection failure (no k10temp device), all methods gracefully degrade:
/// `is_available()` returns `false` and readings return errors.
#[derive(Debug, Clone)]
pub struct ThermalMonitor {
    k10temp_base: Option<PathBuf>,
    /// Index of the tempN_input file for each sensor label.
    tctl_idx: Option<u32>,
    tccd1_idx: Option<u32>,
    tccd2_idx: Option<u32>,
}

impl ThermalMonitor {
    /// Scan `/sys/class/hwmon/` for a `k10temp` device and identify sensor
    /// indices by reading their `tempN_label` files.
    pub fn detect() -> Self {
        let hwmon_dir = Path::new("/sys/class/hwmon");
        let entries = match fs::read_dir(hwmon_dir) {
            Ok(entries) => entries,
            Err(e) => {
                debug!(%e, "cannot read /sys/class/hwmon");
                return Self::unavailable();
            }
        };

        for entry in entries.flatten() {
            let name_path = entry.path().join("name");
            let name = match fs::read_to_string(&name_path) {
                Ok(s) => s.trim().to_string(),
                Err(_) => continue,
            };

            if name != "k10temp" {
                continue;
            }

            let base = entry.path();
            let mut tctl_idx = None;
            let mut tccd1_idx = None;
            let mut tccd2_idx = None;

            // Scan temp1..tempN_label files to identify sensors.
            for idx in 1..=16 {
                let label_path = base.join(format!("temp{idx}_label"));
                let label = match fs::read_to_string(&label_path) {
                    Ok(s) => s.trim().to_string(),
                    Err(_) => continue,
                };

                let input_path = base.join(format!("temp{idx}_input"));
                if !input_path.exists() {
                    continue;
                }

                match label.as_str() {
                    "Tctl" => tctl_idx = Some(idx),
                    "Tccd1" => tccd1_idx = Some(idx),
                    "Tccd2" => tccd2_idx = Some(idx),
                    _ => {}
                }
            }

            // Fallback: if label scanning found nothing, try the common layout.
            if tctl_idx.is_none() && base.join("temp1_input").exists() {
                tctl_idx = Some(1);
            }
            if tccd1_idx.is_none() && base.join("temp3_input").exists() {
                tccd1_idx = Some(3);
            }
            if tccd2_idx.is_none() && base.join("temp4_input").exists() {
                tccd2_idx = Some(4);
            }

            debug!(
                k10temp = %base.display(),
                tctl = ?tctl_idx,
                tccd1 = ?tccd1_idx,
                tccd2 = ?tccd2_idx,
                "detected k10temp"
            );

            return Self {
                k10temp_base: Some(base),
                tctl_idx,
                tccd1_idx,
                tccd2_idx,
            };
        }

        debug!("no k10temp hwmon device found; thermal monitoring disabled");
        Self::unavailable()
    }

    fn unavailable() -> Self {
        Self {
            k10temp_base: None,
            tctl_idx: None,
            tccd1_idx: None,
            tccd2_idx: None,
        }
    }

    pub fn is_available(&self) -> bool {
        self.k10temp_base.is_some()
    }

    /// Read current temperatures. Returns an error if k10temp is unavailable
    /// or a sensor file cannot be read.
    pub fn read_temps(&self) -> Result<ThermalSnapshot> {
        let base = self
            .k10temp_base
            .as_ref()
            .context("k10temp not available")?;

        let read = |idx: Option<u32>| -> Result<f32> {
            let idx = idx.context("sensor index unknown")?;
            let path = base.join(format!("temp{idx}_input"));
            let raw = fs::read_to_string(&path)
                .with_context(|| format!("failed to read {}", path.display()))?;
            let millideg: i64 = raw
                .trim()
                .parse()
                .with_context(|| format!("invalid temp value in {}: {raw}", path.display()))?;
            Ok(millideg as f32 / 1000.0)
        };

        Ok(ThermalSnapshot {
            tctl: read(self.tctl_idx)?,
            tccd1: read(self.tccd1_idx).unwrap_or(0.0),
            tccd2: read(self.tccd2_idx).unwrap_or(0.0),
        })
    }
}

/// Read the current CPU frequency for a logical CPU from sysfs cpufreq.
/// Returns frequency in kHz.
pub fn read_freq(logical_cpu_id: u32) -> Result<u64> {
    let path = format!("/sys/devices/system/cpu/cpu{logical_cpu_id}/cpufreq/scaling_cur_freq");
    let raw = fs::read_to_string(&path).with_context(|| format!("failed to read {path}"))?;
    raw.trim()
        .parse::<u64>()
        .with_context(|| format!("invalid frequency value in {path}: {raw}"))
}

/// Read the maximum CPU frequency for a logical CPU.
/// Returns frequency in kHz.
pub fn read_max_freq(logical_cpu_id: u32) -> Result<u64> {
    let path = format!("/sys/devices/system/cpu/cpu{logical_cpu_id}/cpufreq/cpuinfo_max_freq");
    let raw = fs::read_to_string(&path).with_context(|| format!("failed to read {path}"))?;
    raw.trim()
        .parse::<u64>()
        .with_context(|| format!("invalid frequency value in {path}: {raw}"))
}

/// Determine which CCD (0 or 1) a physical core belongs to, based on the
/// core map's gap heuristic.
///
/// On a multi-CCD Ryzen (e.g. 5900X), physical core IDs have a gap in the
/// middle (cores 0-5, 8-13). The first half of sorted IDs belongs to CCD0,
/// the second to CCD1.
pub fn ccd_for_core(physical_core_id: u32, core_map: &BTreeMap<u32, Vec<u32>>) -> Option<u32> {
    let mut sorted: Vec<u32> = core_map.keys().copied().collect();
    sorted.sort_unstable();

    // Detect multi-CCD by checking for a gap in physical core IDs.
    // On a single-CCD CPU (e.g. 5600X: 0-5), all cores are on CCD0.
    let has_gap = sorted.len() > 1
        && (sorted.last().unwrap() - sorted.first().unwrap() + 1) as usize > sorted.len();

    if !has_gap {
        // Single-CCD: all cores on CCD0.
        return if core_map.contains_key(&physical_core_id) {
            Some(0)
        } else {
            None
        };
    }

    let mid = sorted.len() / 2;
    sorted[..mid]
        .iter()
        .position(|&c| c == physical_core_id)
        .map(|_| 0)
        .or_else(|| {
            sorted[mid..]
                .iter()
                .position(|&c| c == physical_core_id)
                .map(|_| 1)
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ccd_for_core_splits_12core() {
        // 5900X: physical IDs 0-5, 8-13
        let core_map: BTreeMap<u32, Vec<u32>> = [
            (0, vec![0, 1]),
            (1, vec![2, 3]),
            (2, vec![4, 5]),
            (3, vec![6, 7]),
            (4, vec![8, 9]),
            (5, vec![10, 11]),
            (8, vec![12, 13]),
            (9, vec![14, 15]),
            (10, vec![16, 17]),
            (11, vec![18, 19]),
            (12, vec![20, 21]),
            (13, vec![22, 23]),
        ]
        .into();

        // First half (0-5) → CCD0
        for &core in &[0, 1, 2, 3, 4, 5] {
            assert_eq!(ccd_for_core(core, &core_map), Some(0));
        }
        // Second half (8-13) → CCD1
        for &core in &[8, 9, 10, 11, 12, 13] {
            assert_eq!(ccd_for_core(core, &core_map), Some(1));
        }
    }

    #[test]
    fn ccd_for_core_single_ccd() {
        // 5600X: 6 cores, no gap
        let core_map: BTreeMap<u32, Vec<u32>> = [
            (0, vec![0, 1]),
            (1, vec![2, 3]),
            (2, vec![4, 5]),
            (3, vec![6, 7]),
            (4, vec![8, 9]),
            (5, vec![10, 11]),
        ]
        .into();

        // All cores in first half → CCD0
        for &core in &[0, 1, 2, 3, 4, 5] {
            assert_eq!(ccd_for_core(core, &core_map), Some(0));
        }
    }

    #[test]
    fn ccd_for_core_unknown_core() {
        let core_map: BTreeMap<u32, Vec<u32>> = [(0, vec![0, 1]), (1, vec![2, 3])].into();
        assert_eq!(ccd_for_core(99, &core_map), None);
    }
}
