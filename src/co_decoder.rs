use std::collections::BTreeMap;

use crate::co_offsets::CoByteLayout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurveOptimizerMode {
    Disabled,
    AllCore,
    PerCore,
    Unknown(u8),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedCurveOptimizer {
    pub mode: CurveOptimizerMode,
    pub per_core_offsets: BTreeMap<u32, i32>,
}

pub fn decode_co_bytes(
    data: &[u8],
    layout: &CoByteLayout,
    physical_core_count: usize,
) -> DecodedCurveOptimizer {
    if data.len() <= layout.mode_offset {
        return DecodedCurveOptimizer {
            mode: CurveOptimizerMode::Unknown(0),
            per_core_offsets: BTreeMap::new(),
        };
    }

    let mode_byte = data[layout.mode_offset];
    match mode_byte {
        0x00 => DecodedCurveOptimizer {
            mode: CurveOptimizerMode::Disabled,
            per_core_offsets: BTreeMap::new(),
        },
        0x01 => {
            if layout.signs_offset >= data.len() {
                tracing::warn!(
                    signs_offset = layout.signs_offset,
                    data_len = data.len(),
                    "all-core sign byte out of bounds"
                );
                return DecodedCurveOptimizer {
                    mode: CurveOptimizerMode::AllCore,
                    per_core_offsets: BTreeMap::new(),
                };
            }
            if layout.magnitudes_offset + 2 > data.len() {
                tracing::warn!(
                    magnitudes_offset = layout.magnitudes_offset,
                    data_len = data.len(),
                    "all-core magnitude bytes out of bounds"
                );
                return DecodedCurveOptimizer {
                    mode: CurveOptimizerMode::AllCore,
                    per_core_offsets: BTreeMap::new(),
                };
            }

            let sign = data[layout.signs_offset];
            let magnitude = u16::from_le_bytes([
                data[layout.magnitudes_offset],
                data[layout.magnitudes_offset + 1],
            ]);
            if magnitude > 30 {
                tracing::warn!("magnitude out of expected range: {}", magnitude);
            }

            let value = if sign == 1 {
                -(magnitude as i32)
            } else {
                magnitude as i32
            };
            let mut per_core_offsets = BTreeMap::new();
            for core in 0..physical_core_count {
                per_core_offsets.insert(core as u32, value);
            }

            DecodedCurveOptimizer {
                mode: CurveOptimizerMode::AllCore,
                per_core_offsets,
            }
        }
        0x02 => {
            let mut per_core_offsets = BTreeMap::new();
            let core_limit = physical_core_count.min(layout.max_cores);

            for i in 0..core_limit {
                let sign_offset = layout.signs_offset + i;
                let magnitude_offset = layout.magnitudes_offset + i * 2;
                if sign_offset >= data.len() || magnitude_offset + 2 > data.len() {
                    tracing::warn!(
                        core_index = i,
                        sign_offset,
                        magnitude_offset,
                        data_len = data.len(),
                        "per-core bytes out of bounds; stopping decode"
                    );
                    break;
                }

                let mut sign = data[sign_offset];
                let magnitude =
                    u16::from_le_bytes([data[magnitude_offset], data[magnitude_offset + 1]]);
                if sign > 1 {
                    tracing::warn!(
                        core_index = i,
                        sign,
                        "invalid sign byte; treating as positive"
                    );
                    sign = 0;
                }
                if magnitude > 30 {
                    tracing::warn!(
                        core_index = i,
                        "magnitude out of expected range: {}",
                        magnitude
                    );
                }

                let value = if sign == 1 {
                    -(magnitude as i32)
                } else {
                    magnitude as i32
                };
                per_core_offsets.insert(i as u32, value);
            }

            DecodedCurveOptimizer {
                mode: CurveOptimizerMode::PerCore,
                per_core_offsets,
            }
        }
        other => DecodedCurveOptimizer {
            mode: CurveOptimizerMode::Unknown(other),
            per_core_offsets: BTreeMap::new(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_layout() -> CoByteLayout {
        CoByteLayout {
            mode_offset: 0x174,
            signs_offset: 0x178,
            magnitudes_offset: 0x1B8,
            max_cores: 16,
        }
    }

    #[test]
    fn given_per_core_mode_12_cores_when_decoding_then_returns_all_offsets() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x02;
        for i in 0..12 {
            data[0x178 + i] = 0x01;
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&15_u16.to_le_bytes());
        }

        let decoded = decode_co_bytes(&data, &layout, 12);

        assert_eq!(decoded.mode, CurveOptimizerMode::PerCore);
        assert_eq!(decoded.per_core_offsets.len(), 12);
        for core in 0..12 {
            assert_eq!(decoded.per_core_offsets.get(&(core as u32)), Some(&-15));
        }
    }

    #[test]
    fn given_per_core_core2_minus30_when_decoding_then_matches_hardware() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x02;

        for i in 0..12 {
            data[0x178 + i] = 0x01;
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&15_u16.to_le_bytes());
        }
        data[0x1BC..0x1BE].copy_from_slice(&30_u16.to_le_bytes());

        let decoded = decode_co_bytes(&data, &layout, 12);

        assert_eq!(decoded.per_core_offsets[&0], -15);
        assert_eq!(decoded.per_core_offsets[&1], -15);
        assert_eq!(decoded.per_core_offsets[&2], -30);
        assert_eq!(decoded.per_core_offsets[&11], -15);
    }

    #[test]
    fn given_all_core_mode_when_decoding_then_applies_single_offset_to_all() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x01;
        data[0x178] = 0x01;
        data[0x1B8..0x1BA].copy_from_slice(&20_u16.to_le_bytes());

        let decoded = decode_co_bytes(&data, &layout, 6);

        assert_eq!(decoded.mode, CurveOptimizerMode::AllCore);
        assert_eq!(decoded.per_core_offsets.len(), 6);
        for core in 0..6 {
            assert_eq!(decoded.per_core_offsets.get(&(core as u32)), Some(&-20));
        }
    }

    #[test]
    fn given_disabled_mode_when_decoding_then_returns_empty_map() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x00;

        let decoded = decode_co_bytes(&data, &layout, 12);

        assert_eq!(decoded.mode, CurveOptimizerMode::Disabled);
        assert!(decoded.per_core_offsets.is_empty());
    }

    #[test]
    fn given_6_cores_when_decoding_then_returns_6_entries() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x02;
        for i in 0..6 {
            data[0x178 + i] = 0x01;
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&10_u16.to_le_bytes());
        }

        let decoded = decode_co_bytes(&data, &layout, 6);

        assert_eq!(decoded.per_core_offsets.len(), 6);
        for core in 0..6 {
            assert_eq!(decoded.per_core_offsets.get(&(core as u32)), Some(&-10));
        }
    }

    #[test]
    fn given_truncated_data_when_decoding_then_returns_partial() {
        let mut data = vec![0_u8; 0x179];
        let layout = default_layout();
        data[0x174] = 0x02;
        data[0x178] = 0x01;

        let decoded = decode_co_bytes(&data, &layout, 4);

        assert_eq!(decoded.mode, CurveOptimizerMode::PerCore);
    }

    #[test]
    fn given_positive_offsets_when_decoding_then_values_positive() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x02;
        for i in 0..4 {
            data[0x178 + i] = 0x00;
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&25_u16.to_le_bytes());
        }

        let decoded = decode_co_bytes(&data, &layout, 4);

        for core in 0..4 {
            assert_eq!(decoded.per_core_offsets.get(&(core as u32)), Some(&25));
        }
    }

    #[test]
    fn given_mixed_signs_when_decoding_then_correctly_applies_each() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x02;
        data[0x178] = 0x00;
        data[0x179] = 0x01;
        data[0x17A] = 0x00;
        data[0x17B] = 0x01;
        for i in 0..4 {
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&10_u16.to_le_bytes());
        }

        let decoded = decode_co_bytes(&data, &layout, 4);

        assert_eq!(decoded.per_core_offsets.get(&0), Some(&10));
        assert_eq!(decoded.per_core_offsets.get(&1), Some(&-10));
        assert_eq!(decoded.per_core_offsets.get(&2), Some(&10));
        assert_eq!(decoded.per_core_offsets.get(&3), Some(&-10));
    }

    #[test]
    fn given_zero_magnitudes_when_decoding_then_all_offsets_zero() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0x02;
        for i in 0..4 {
            data[0x178 + i] = 0x01;
            let offset = 0x1B8 + i * 2;
            data[offset..offset + 2].copy_from_slice(&0_u16.to_le_bytes());
        }

        let decoded = decode_co_bytes(&data, &layout, 4);

        for core in 0..4 {
            assert_eq!(decoded.per_core_offsets.get(&(core as u32)), Some(&0));
        }
    }

    #[test]
    fn given_unknown_mode_byte_when_decoding_then_returns_unknown_empty() {
        let mut data = vec![0_u8; 1024];
        let layout = default_layout();
        data[0x174] = 0xFF;

        let decoded = decode_co_bytes(&data, &layout, 12);

        assert_eq!(decoded.mode, CurveOptimizerMode::Unknown(0xFF));
        assert!(decoded.per_core_offsets.is_empty());
    }
}
