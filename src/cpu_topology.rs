use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use sysinfo::System;
use tracing::info;

type LogicalToPhysicalCore = Vec<(u32, u32)>;
type ThreadSiblingsMap = HashMap<u32, Vec<u32>>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CpuTopology {
    pub vendor: String,
    pub model_name: String,
    pub physical_core_count: usize,
    pub logical_cpu_count: usize,
    pub core_map: BTreeMap<u32, Vec<u32>>,
    pub bios_map: BTreeMap<u32, u32>,
    pub physical_map: BTreeMap<u32, u32>,
    pub cpu_brand: Option<String>,
    pub cpu_frequency_mhz: Option<u64>,
}

impl CpuTopology {
    pub fn bios_index(&self, physical_core_id: u32) -> Option<u32> {
        self.bios_map.get(&physical_core_id).copied()
    }

    pub fn physical_id(&self, bios_index: u32) -> Option<u32> {
        self.physical_map.get(&bios_index).copied()
    }
}

#[derive(Debug)]
struct CpuInfoSummary {
    vendor: String,
    model_name: String,
    has_long_mode: bool,
}

pub fn detect_cpu_topology() -> Result<CpuTopology> {
    let cpuinfo_content = fs::read_to_string("/proc/cpuinfo")
        .context("failed to read /proc/cpuinfo for CPU validation")?;
    let (core_ids, thread_siblings) = read_sysfs_topology("/sys/devices/system/cpu")?;
    let (cpu_brand, cpu_frequency_mhz) = read_sysinfo_supplementary();

    let topology = detect_cpu_topology_from_parsed_sources(
        &cpuinfo_content,
        &core_ids,
        Some((cpu_brand, cpu_frequency_mhz)),
    )?;

    let smt_enabled = thread_siblings.values().any(|siblings| siblings.len() > 1);

    info!(
        vendor = %topology.vendor,
        model_name = %topology.model_name,
        physical_core_count = topology.physical_core_count,
        logical_cpu_count = topology.logical_cpu_count,
        smt_enabled,
        core_map = ?topology.core_map,
        "detected CPU topology"
    );

    Ok(topology)
}

fn read_sysinfo_supplementary() -> (Option<String>, Option<u64>) {
    let mut system = System::new_all();
    system.refresh_cpu_all();

    if let Some(cpu) = system.cpus().first() {
        let brand = cpu.brand().trim().to_string();
        let frequency = cpu.frequency();

        let brand_opt = if brand.is_empty() { None } else { Some(brand) };
        let frequency_opt = if frequency == 0 {
            None
        } else {
            Some(frequency)
        };

        (brand_opt, frequency_opt)
    } else {
        (None, None)
    }
}

fn read_sysfs_topology(
    base_path: impl AsRef<Path>,
) -> Result<(LogicalToPhysicalCore, ThreadSiblingsMap)> {
    let mut core_ids = Vec::new();
    let mut siblings = HashMap::new();

    for entry_result in
        fs::read_dir(base_path.as_ref()).context("failed to read sysfs CPU directory")?
    {
        let entry = entry_result.context("failed to read a sysfs CPU directory entry")?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        let Some(logical_cpu_id) = parse_cpu_directory_name(&name) else {
            continue;
        };

        let topology_dir = entry.path().join("topology");
        if !topology_dir.exists() {
            continue;
        }

        let core_id_raw = fs::read_to_string(topology_dir.join("core_id"))
            .with_context(|| format!("failed to read core_id for logical CPU {logical_cpu_id}"))?;
        let core_id = core_id_raw
            .trim()
            .parse::<u32>()
            .with_context(|| format!("invalid core_id for logical CPU {logical_cpu_id}"))?;
        core_ids.push((logical_cpu_id, core_id));

        let siblings_raw = fs::read_to_string(topology_dir.join("thread_siblings_list"))
            .with_context(|| {
                format!("failed to read thread_siblings_list for logical CPU {logical_cpu_id}")
            })?;
        let siblings_for_cpu = parse_cpu_list(siblings_raw.trim()).with_context(|| {
            format!(
                "invalid thread_siblings_list for logical CPU {logical_cpu_id}: {}",
                siblings_raw.trim()
            )
        })?;
        siblings.insert(logical_cpu_id, siblings_for_cpu);
    }

    if core_ids.is_empty() {
        bail!("no CPU topology entries found in /sys/devices/system/cpu");
    }

    core_ids.sort_unstable_by_key(|(logical_cpu_id, _)| *logical_cpu_id);

    Ok((core_ids, siblings))
}

fn parse_cpu_directory_name(name: &str) -> Option<u32> {
    let suffix = name.strip_prefix("cpu")?;
    if suffix.is_empty() {
        return None;
    }

    suffix.parse::<u32>().ok()
}

fn parse_cpu_list(input: &str) -> Result<Vec<u32>> {
    if input.is_empty() {
        bail!("CPU list is empty");
    }

    let mut cpus = Vec::new();
    for token in input.split(',') {
        let token = token.trim();
        if token.is_empty() {
            continue;
        }

        if let Some((start, end)) = token.split_once('-') {
            let start = start
                .trim()
                .parse::<u32>()
                .with_context(|| format!("invalid CPU range start '{start}'"))?;
            let end = end
                .trim()
                .parse::<u32>()
                .with_context(|| format!("invalid CPU range end '{end}'"))?;
            if start > end {
                bail!("invalid CPU range '{token}': start is greater than end");
            }

            cpus.extend(start..=end);
        } else {
            cpus.push(
                token
                    .parse::<u32>()
                    .with_context(|| format!("invalid CPU identifier '{token}'"))?,
            );
        }
    }

    cpus.sort_unstable();
    cpus.dedup();

    if cpus.is_empty() {
        bail!("CPU list did not contain any entries");
    }

    Ok(cpus)
}

fn parse_cpuinfo(content: &str) -> Result<CpuInfoSummary> {
    let mut vendor = None;
    let mut model_name = None;
    let mut has_long_mode = false;

    for line in content.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };

        let key = key.trim();
        let value = value.trim();

        match key {
            "vendor_id" if vendor.is_none() => vendor = Some(value.to_string()),
            "model name" if model_name.is_none() => model_name = Some(value.to_string()),
            "flags" => {
                if value.split_whitespace().any(|flag| flag == "lm") {
                    has_long_mode = true;
                }
            }
            _ => {}
        }
    }

    let vendor = vendor.ok_or_else(|| anyhow!("missing vendor_id in /proc/cpuinfo"))?;
    let model_name = model_name.ok_or_else(|| anyhow!("missing model name in /proc/cpuinfo"))?;

    Ok(CpuInfoSummary {
        vendor,
        model_name,
        has_long_mode,
    })
}

fn detect_cpu_topology_from_parsed_sources(
    cpuinfo_content: &str,
    core_ids: &[(u32, u32)],
    sysinfo_override: Option<(Option<String>, Option<u64>)>,
) -> Result<CpuTopology> {
    let cpu_info = parse_cpuinfo(cpuinfo_content)?;

    if cpu_info.vendor != "AuthenticAMD" {
        bail!(
            "unsupported CPU vendor '{}'; only AuthenticAMD is supported",
            cpu_info.vendor
        );
    }

    if !cpu_info.has_long_mode {
        bail!("unsupported CPU architecture: 64-bit long mode (lm) flag is required");
    }

    if core_ids.is_empty() {
        bail!("logical CPU list is empty");
    }

    let mut core_map: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for (logical_cpu_id, physical_core_id) in core_ids {
        core_map
            .entry(*physical_core_id)
            .or_default()
            .push(*logical_cpu_id);
    }
    for siblings in core_map.values_mut() {
        siblings.sort_unstable();
        siblings.dedup();
    }

    let bios_map: BTreeMap<u32, u32> = core_map
        .keys()
        .enumerate()
        .map(|(idx, &phys)| (phys, idx as u32))
        .collect();
    let physical_map: BTreeMap<u32, u32> =
        bios_map.iter().map(|(&phys, &bios)| (bios, phys)).collect();

    let (cpu_brand, cpu_frequency_mhz) = sysinfo_override.unwrap_or((None, None));
    let model_name = if cpu_info.model_name.is_empty() {
        cpu_brand
            .clone()
            .unwrap_or_else(|| "unknown model".to_string())
    } else {
        cpu_info.model_name
    };

    Ok(CpuTopology {
        vendor: cpu_info.vendor,
        model_name,
        physical_core_count: core_map.len(),
        logical_cpu_count: core_ids.len(),
        core_map,
        bios_map,
        physical_map,
        cpu_brand,
        cpu_frequency_mhz,
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::detect_cpu_topology_from_parsed_sources;

    const AMD_CPUINFO: &str = "processor : 0
vendor_id : AuthenticAMD
model name : AMD Ryzen 9 5900X 12-Core Processor
flags : fpu sse sse2 lm avx avx2";

    const INTEL_CPUINFO: &str = "processor : 0
vendor_id : GenuineIntel
model name : Intel(R) Core(TM)
flags : fpu sse sse2 lm";

    const AMD_NON_64BIT_CPUINFO: &str = "processor : 0
vendor_id : AuthenticAMD
model name : AMD Sample CPU
flags : fpu sse sse2 avx";

    #[test]
    fn given_amd_cpu_when_detecting_topology_then_returns_physical_cores() {
        let core_ids = vec![(0, 0), (1, 1), (2, 0), (3, 1)];

        let topology =
            detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, Some((None, None)))
                .expect("AMD topology should parse successfully");

        let expected_map = BTreeMap::from([(0, vec![0, 2]), (1, vec![1, 3])]);

        assert_eq!(topology.vendor, "AuthenticAMD");
        assert_eq!(topology.model_name, "AMD Ryzen 9 5900X 12-Core Processor");
        assert_eq!(topology.logical_cpu_count, 4);
        assert_eq!(topology.physical_core_count, 2);
        assert_eq!(topology.core_map, expected_map);
        assert_eq!(topology.bios_index(0), Some(0));
        assert_eq!(topology.bios_index(1), Some(1));
    }

    #[test]
    fn given_non_amd_cpu_when_validating_then_returns_error() {
        let core_ids = vec![(0, 0), (1, 1)];

        let result = detect_cpu_topology_from_parsed_sources(INTEL_CPUINFO, &core_ids, None);

        let error_text = result
            .expect_err("non-AMD CPU must be rejected")
            .to_string();
        assert!(error_text.contains("AuthenticAMD"));
    }

    #[test]
    fn given_non_64bit_when_validating_then_returns_error() {
        let core_ids = vec![(0, 0), (1, 1)];

        let result =
            detect_cpu_topology_from_parsed_sources(AMD_NON_64BIT_CPUINFO, &core_ids, None);

        let error_text = result
            .expect_err("CPU without lm flag must be rejected")
            .to_string();
        assert!(error_text.contains("64-bit"));
    }

    #[test]
    fn given_amd_5900x_when_mapping_cores_then_handles_non_contiguous_ids() {
        let core_ids = vec![
            (0, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 5),
            (6, 8),
            (7, 9),
            (8, 10),
            (9, 11),
            (10, 12),
            (11, 13),
            (12, 0),
            (13, 1),
            (14, 2),
            (15, 3),
            (16, 4),
            (17, 5),
            (18, 8),
            (19, 9),
            (20, 10),
            (21, 11),
            (22, 12),
            (23, 13),
        ];

        let topology = detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, None)
            .expect("non-contiguous core IDs should be supported");

        let expected_map = BTreeMap::from([
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
        ]);

        assert_eq!(topology.core_map, expected_map);
        assert_eq!(topology.physical_core_count, 12);
        assert_eq!(topology.logical_cpu_count, 24);
    }

    #[test]
    fn given_smt_enabled_when_enumerating_then_returns_first_thread_per_core() {
        let core_ids = vec![(6, 0), (0, 0), (7, 1), (1, 1)];

        let topology = detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, None)
            .expect("SMT core mapping should parse successfully");

        let expected_map = BTreeMap::from([(0, vec![0, 6]), (1, vec![1, 7])]);
        assert_eq!(topology.core_map, expected_map);
    }

    #[test]
    fn given_contiguous_core_ids_when_computing_bios_map_then_identity_mapping() {
        let core_ids = vec![(0, 0), (1, 1), (2, 2), (3, 3)];

        let topology = detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, None)
            .expect("contiguous core IDs should produce identity BIOS mapping");

        assert_eq!(topology.bios_index(0), Some(0));
        assert_eq!(topology.bios_index(1), Some(1));
        assert_eq!(topology.bios_index(2), Some(2));
        assert_eq!(topology.bios_index(3), Some(3));
    }

    #[test]
    fn given_non_contiguous_5900x_ids_when_computing_bios_map_then_sequential_indices() {
        let core_ids = vec![
            (0, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 5),
            (6, 8),
            (7, 9),
            (8, 10),
            (9, 11),
            (10, 12),
            (11, 13),
        ];

        let topology = detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, None)
            .expect("non-contiguous 5900X IDs should produce sequential BIOS indices");

        assert_eq!(topology.bios_index(0), Some(0));
        assert_eq!(topology.bios_index(5), Some(5));
        assert_eq!(topology.bios_index(8), Some(6));
        assert_eq!(topology.bios_index(13), Some(11));
    }

    #[test]
    fn given_bios_index_when_reverse_mapping_then_returns_physical_id() {
        let core_ids = vec![
            (0, 0),
            (1, 1),
            (2, 2),
            (3, 3),
            (4, 4),
            (5, 5),
            (6, 8),
            (7, 9),
            (8, 10),
            (9, 11),
            (10, 12),
            (11, 13),
        ];

        let topology = detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, None)
            .expect("reverse mapping should be computed from BIOS map");

        assert_eq!(topology.physical_id(6), Some(8));
        assert_eq!(topology.physical_id(7), Some(9));
    }

    #[test]
    fn given_single_core_when_computing_bios_map_then_maps_to_zero() {
        let core_ids = vec![(0, 8)];

        let topology = detect_cpu_topology_from_parsed_sources(AMD_CPUINFO, &core_ids, None)
            .expect("single core should map to BIOS index zero");

        assert_eq!(topology.bios_index(8), Some(0));
        assert_eq!(topology.physical_id(0), Some(8));
    }
}
