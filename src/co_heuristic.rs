use crate::co_offsets::CoByteLayout;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeuristicConfidence {
    High,
    Medium,
    Low,
}

#[derive(Debug, Clone)]
struct Candidate {
    layout: CoByteLayout,
    confidence: HeuristicConfidence,
}

pub fn scan_for_co_pattern(
    data: &[u8],
    physical_core_count: usize,
) -> Option<(CoByteLayout, HeuristicConfidence)> {
    if data.len() < 100 || physical_core_count < 4 {
        return None;
    }

    let mut candidates = Vec::new();

    for mode_offset in 0..data.len() {
        let mode = data[mode_offset];
        if mode != 0x01 && mode != 0x02 {
            continue;
        }

        if !mode_gap_is_valid(data, mode_offset) {
            continue;
        }

        let signs_offset = mode_offset + 4;
        if !signs_region_is_valid(data, signs_offset, physical_core_count) {
            continue;
        }

        let expected_magnitudes_offset = signs_offset + 0x40;
        if magnitudes_region_is_valid(data, expected_magnitudes_offset, physical_core_count) {
            candidates.push(Candidate {
                layout: CoByteLayout {
                    mode_offset,
                    signs_offset,
                    magnitudes_offset: expected_magnitudes_offset,
                    max_cores: physical_core_count,
                },
                confidence: HeuristicConfidence::High,
            });
            continue;
        }

        if let Some(magnitudes_offset) = find_shifted_magnitudes_offset(
            data,
            signs_offset,
            physical_core_count,
            expected_magnitudes_offset,
        ) {
            candidates.push(Candidate {
                layout: CoByteLayout {
                    mode_offset,
                    signs_offset,
                    magnitudes_offset,
                    max_cores: physical_core_count,
                },
                confidence: HeuristicConfidence::Medium,
            });
            continue;
        }

        candidates.push(Candidate {
            layout: CoByteLayout {
                mode_offset,
                signs_offset,
                magnitudes_offset: expected_magnitudes_offset,
                max_cores: physical_core_count,
            },
            confidence: HeuristicConfidence::Low,
        });
    }

    pick_best_candidate(&candidates, data.len())
}

fn signs_region_is_valid(data: &[u8], signs_offset: usize, core_count: usize) -> bool {
    let Some(signs_end) = signs_offset.checked_add(core_count) else {
        return false;
    };

    if signs_end > data.len() {
        return false;
    }

    data[signs_offset..signs_end]
        .iter()
        .all(|value| *value == 0x00 || *value == 0x01)
}

fn mode_gap_is_valid(data: &[u8], mode_offset: usize) -> bool {
    let Some(gap_end) = mode_offset.checked_add(4) else {
        return false;
    };

    if gap_end > data.len() {
        return false;
    }

    data[mode_offset + 1..gap_end]
        .iter()
        .all(|value| *value == 0x00)
}

fn magnitudes_region_is_valid(data: &[u8], magnitudes_offset: usize, core_count: usize) -> bool {
    let Some(span_bytes) = core_count.checked_mul(2) else {
        return false;
    };
    let Some(magnitudes_end) = magnitudes_offset.checked_add(span_bytes) else {
        return false;
    };

    if magnitudes_end > data.len() {
        return false;
    }

    let mut non_zero_count = 0usize;
    let all_in_range = (0..core_count).all(|index| {
        let start = magnitudes_offset + index * 2;
        let value = u16::from_le_bytes([data[start], data[start + 1]]);
        if value != 0 {
            non_zero_count += 1;
        }
        value <= 30
    });

    all_in_range && non_zero_count >= core_count / 2
}

fn find_shifted_magnitudes_offset(
    data: &[u8],
    signs_offset: usize,
    core_count: usize,
    expected_magnitudes_offset: usize,
) -> Option<usize> {
    let span_bytes = core_count.checked_mul(2)?;

    let scan_start = signs_offset + 4;
    let scan_limit = signs_offset + 0x80;
    let max_bound = data.len().saturating_sub(span_bytes);
    let max_start = scan_limit.min(max_bound);

    if scan_start > max_start {
        return None;
    }

    (scan_start..=max_start).find(|offset| {
        *offset != expected_magnitudes_offset
            && magnitudes_region_is_valid(data, *offset, core_count)
    })
}

fn pick_best_candidate(
    candidates: &[Candidate],
    data_len: usize,
) -> Option<(CoByteLayout, HeuristicConfidence)> {
    let middle = data_len / 2;

    candidates
        .iter()
        .max_by_key(|candidate| {
            (
                confidence_rank(&candidate.confidence),
                distance_score(candidate.layout.mode_offset, middle),
            )
        })
        .map(|candidate| (candidate.layout.clone(), candidate.confidence.clone()))
}

fn confidence_rank(confidence: &HeuristicConfidence) -> u8 {
    match confidence {
        HeuristicConfidence::High => 3,
        HeuristicConfidence::Medium => 2,
        HeuristicConfidence::Low => 1,
    }
}

fn distance_score(mode_offset: usize, middle: usize) -> usize {
    usize::MAX - mode_offset.abs_diff(middle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set_u16_le_values(data: &mut [u8], offset: usize, count: usize, value: u16) {
        for index in 0..count {
            let bytes = value.to_le_bytes();
            let position = offset + index * 2;
            data[position] = bytes[0];
            data[position + 1] = bytes[1];
        }
    }

    #[test]
    fn given_standard_layout_bytes_when_scanning_then_detects_with_high_confidence() {
        let mut data = vec![0_u8; 1020];
        data[0x174] = 0x02;
        data[0x178..0x178 + 12].fill(0x01);
        set_u16_le_values(&mut data, 0x1B8, 12, 15);

        let result = scan_for_co_pattern(&data, 12);

        assert_eq!(
            result,
            Some((
                CoByteLayout {
                    mode_offset: 0x174,
                    signs_offset: 0x178,
                    magnitudes_offset: 0x1B8,
                    max_cores: 12,
                },
                HeuristicConfidence::High,
            ))
        );
    }

    #[test]
    fn given_shifted_layout_when_scanning_then_detects_with_medium_confidence() {
        let mut data = vec![0_u8; 1020];
        data[0x100] = 0x02;
        data[0x104..0x104 + 12].fill(0x01);
        set_u16_le_values(&mut data, 0x104 + 0x50, 12, 15);

        let result = scan_for_co_pattern(&data, 12);

        assert!(matches!(result, Some((_, HeuristicConfidence::Medium))));
    }

    #[test]
    fn given_no_co_pattern_when_scanning_then_returns_none() {
        let data = vec![0_u8; 1020];

        let result = scan_for_co_pattern(&data, 12);

        assert_eq!(result, None);
    }

    #[test]
    fn given_disabled_mode_only_when_scanning_then_returns_none() {
        let mut data = vec![0_u8; 1020];
        data[10] = 0x00;

        let result = scan_for_co_pattern(&data, 12);

        assert_eq!(result, None);
    }

    #[test]
    fn given_all_core_mode_when_scanning_then_detects_pattern() {
        let mut data = vec![0_u8; 1020];
        data[0x174] = 0x01;
        data[0x178..0x178 + 6].fill(0x01);
        set_u16_le_values(&mut data, 0x1B8, 6, 10);

        let result = scan_for_co_pattern(&data, 6);

        assert_eq!(
            result,
            Some((
                CoByteLayout {
                    mode_offset: 0x174,
                    signs_offset: 0x178,
                    magnitudes_offset: 0x1B8,
                    max_cores: 6,
                },
                HeuristicConfidence::High,
            ))
        );
    }

    #[test]
    fn given_multiple_candidate_modes_when_scanning_then_picks_best() {
        let mut data = vec![0_u8; 1020];
        data[0x50] = 0x02;
        data[0x54..0x54 + 3].fill(0x01);
        data[0x54 + 3..0x54 + 12].fill(0x05);

        data[0x174] = 0x02;
        data[0x178..0x178 + 12].fill(0x01);
        set_u16_le_values(&mut data, 0x1B8, 12, 15);

        let result = scan_for_co_pattern(&data, 12);

        assert!(matches!(
            result,
            Some((
                CoByteLayout {
                    mode_offset: 0x174,
                    ..
                },
                HeuristicConfidence::High,
            ))
        ));
    }

    #[test]
    fn given_data_too_short_when_scanning_then_returns_none() {
        let mut data = vec![0_u8; 50];
        data[0] = 0x02;

        let result = scan_for_co_pattern(&data, 4);

        assert_eq!(result, None);
    }

    #[test]
    fn given_per_core_6_cores_when_scanning_then_detects() {
        let mut data = vec![0_u8; 1020];
        data[0x174] = 0x02;
        data[0x178..0x178 + 6].fill(0x01);
        set_u16_le_values(&mut data, 0x1B8, 6, 20);

        let result = scan_for_co_pattern(&data, 6);

        assert_eq!(
            result,
            Some((
                CoByteLayout {
                    mode_offset: 0x174,
                    signs_offset: 0x178,
                    magnitudes_offset: 0x1B8,
                    max_cores: 6,
                },
                HeuristicConfidence::High,
            ))
        );
    }
}
