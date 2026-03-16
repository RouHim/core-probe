use std::collections::BTreeMap;

use crate::co_decoder::decode_co_bytes;
use crate::co_heuristic::scan_for_co_pattern;
use crate::co_offsets::{known_aod_guids, lookup_co_layout};
use crate::hii_extractor::read_efi_variable;

/// Read per-core Curve Optimizer values from the AOD_SETUP EFI variable.
///
/// Never returns an error -- all failures result in None with warning logs.
///
/// Returns:
/// - Some(non-empty map): CO values found
/// - Some(empty map): CO present but mode is Disabled
/// - None: AOD_SETUP unreadable, or layout unknown and heuristic failed
pub fn read_curve_optimizer(
    agesa_version: Option<&str>,
    physical_core_count: usize,
) -> Option<BTreeMap<u32, i32>> {
    match try_read_aod_setup() {
        None => {
            tracing::info!("AOD_SETUP not found; cannot read CO values");
            None
        }
        Some(aod_data) => {
            tracing::info!(bytes = aod_data.len(), "AOD_SETUP read");
            try_decode_co(&aod_data, agesa_version, physical_core_count)
        }
    }
}

/// Try to read the raw AOD_SETUP EFI variable data.
/// Tries each GUID from known_aod_guids() in order.
fn try_read_aod_setup() -> Option<Vec<u8>> {
    for guid in known_aod_guids() {
        match read_efi_variable("AOD_SETUP", guid) {
            Ok(data) => return Some(data),
            Err(e) => tracing::debug!(guid, error = %e, "AOD_SETUP not found for GUID"),
        }
    }
    tracing::info!("AOD_SETUP not found on any known GUID");
    None
}

/// Decode CO offsets from raw AOD_SETUP bytes using table -> heuristic pipeline.
/// This is the testable core -- all tests call this directly.
fn try_decode_co(
    aod_data: &[u8],
    agesa_version: Option<&str>,
    physical_core_count: usize,
) -> Option<BTreeMap<u32, i32>> {
    if let Some(version) = agesa_version {
        if let Some(layout) = lookup_co_layout(version) {
            tracing::info!(agesa = version, "CO layout found via offset table");
            let decoded = decode_co_bytes(aod_data, &layout, physical_core_count);
            return Some(decoded.per_core_offsets);
        }
        tracing::debug!(agesa = version, "No offset table entry, trying heuristic");
    }

    match scan_for_co_pattern(aod_data, physical_core_count) {
        Some((layout, confidence)) => {
            tracing::info!(?confidence, "CO layout detected by heuristic");
            let decoded = decode_co_bytes(aod_data, &layout, physical_core_count);
            Some(decoded.per_core_offsets)
        }
        None => {
            tracing::warn!("No CO layout found -- neither table nor heuristic matched");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_aod_data_12_cores_per_core(magnitudes: &[u16; 12]) -> Vec<u8> {
        let mut data = vec![0u8; 1024];
        data[0x174] = 0x02; // PerCore mode
        for i in 0..12 {
            data[0x178 + i] = 0x01; // all negative
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&magnitudes[i].to_le_bytes());
        }
        data
    }

    #[test]
    fn given_known_agesa_and_valid_aod_data_when_decoding_then_returns_offsets() {
        let data =
            build_aod_data_12_cores_per_core(&[15, 15, 30, 15, 15, 15, 15, 15, 15, 15, 15, 15]);

        let result = try_decode_co(&data, Some("1.2.0.7"), 12);

        assert!(result.is_some(), "expected Some(map), got None");
        let map = result.unwrap();
        assert_eq!(map[&0], -15);
        assert_eq!(map[&1], -15);
        assert_eq!(map[&2], -30);
        assert_eq!(map[&11], -15);
    }

    #[test]
    fn given_unknown_agesa_and_valid_aod_data_when_decoding_then_falls_back_to_heuristic() {
        let data = build_aod_data_12_cores_per_core(&[15; 12]);

        let result = try_decode_co(&data, Some("9.9.9.9"), 12);

        assert!(result.is_some(), "expected Some(map), got None");
        let map = result.unwrap();
        assert_eq!(map.len(), 12);
        for core in 0..12u32 {
            assert_eq!(map[&core], -15, "core {} should be -15", core);
        }
    }

    #[test]
    fn given_unknown_agesa_and_no_pattern_when_decoding_then_returns_none() {
        let data = vec![0u8; 1024]; // all zeros, no CO mode anchor

        let result = try_decode_co(&data, Some("9.9.9.9"), 12);

        assert!(result.is_none(), "expected None for all-zeros data");
    }

    #[test]
    fn given_no_agesa_and_valid_data_when_decoding_then_uses_heuristic() {
        let data = build_aod_data_12_cores_per_core(&[20u16; 12]);

        let result = try_decode_co(&data, None, 12);

        assert!(result.is_some(), "expected Some(map), got None");
        let map = result.unwrap();
        assert_eq!(map.len(), 12);
        for core in 0..12u32 {
            assert_eq!(map[&core], -20, "core {} should be -20", core);
        }
    }

    #[test]
    fn given_disabled_co_mode_when_decoding_then_returns_empty_map() {
        let mut data = vec![0u8; 1024];
        data[0x174] = 0x00; // Disabled mode

        let result = try_decode_co(&data, Some("1.2.0.7"), 12);

        assert!(
            result.is_some(),
            "expected Some(empty map) for disabled mode, got None"
        );
        let map = result.unwrap();
        assert!(
            map.is_empty(),
            "expected empty map for disabled mode, got {} entries",
            map.len()
        );
    }

    #[test]
    fn given_12_core_data_when_decoding_then_returns_12_entries() {
        let data = build_aod_data_12_cores_per_core(&[10u16; 12]);

        let result = try_decode_co(&data, Some("1.2.0.7"), 12);

        assert!(result.is_some(), "expected Some(map), got None");
        let map = result.unwrap();
        assert_eq!(map.len(), 12, "expected 12 entries, got {}", map.len());
        for core in 0..12u32 {
            assert_eq!(map[&core], -10, "core {} should be -10", core);
        }
    }

    #[test]
    fn given_truncated_aod_data_when_decoding_then_handles_gracefully() {
        let mut data = vec![0u8; 200];
        data[0] = 0x02; // mode byte at offset 0
        data[4..8].fill(0x01); // sign bytes
        for i in 0..4 {
            let o = 68 + i * 2;
            data[o..o + 2].copy_from_slice(&5u16.to_le_bytes());
        }

        // Should NOT panic -- either Some or None is acceptable
        let result = try_decode_co(&data, Some("9.9.9.9"), 4);

        // We just verify no panic and the result is valid Option
        match result {
            Some(map) => {
                // If found, values should be reasonable
                for value in map.values() {
                    assert!(
                        *value >= -30 && *value <= 30,
                        "CO value {} out of expected range",
                        value
                    );
                }
            }
            None => {
                // None is also acceptable for truncated data
            }
        }
    }
}
