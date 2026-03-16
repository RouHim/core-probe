#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoByteLayout {
    pub mode_offset: usize, // offset of CO mode byte (0=Disabled, 1=AllCore, 2=PerCore)
    pub signs_offset: usize, // offset of first sign byte (one u8 per core)
    pub magnitudes_offset: usize, // offset of first magnitude value (u16 LE per core)
    pub max_cores: usize,   // maximum cores this layout supports
}

pub fn lookup_co_layout(agesa_version: &str) -> Option<CoByteLayout> {
    let trimmed = agesa_version.trim();

    // AGESA version patterns to CoByteLayout entries
    let known_layouts: &[(_, CoByteLayout)] = &[(
        "1.2.0.7",
        CoByteLayout {
            mode_offset: 0x174,
            signs_offset: 0x178,
            magnitudes_offset: 0x1B8,
            max_cores: 16,
        },
    )];

    for (pattern, layout) in known_layouts {
        if trimmed.contains(pattern) {
            return Some(layout.clone());
        }
    }

    None
}

pub fn known_aod_guids() -> &'static [&'static str] {
    &["5ed15dc0-edef-4161-9151-6014c4cc630c"]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_known_agesa_version_when_looking_up_then_returns_layout() {
        let result = lookup_co_layout("1.2.0.7");
        assert!(result.is_some());
        let layout = result.unwrap();
        assert_eq!(layout.mode_offset, 0x174);
        assert_eq!(layout.signs_offset, 0x178);
        assert_eq!(layout.magnitudes_offset, 0x1B8);
        assert_eq!(layout.max_cores, 16);
    }

    #[test]
    fn given_unknown_agesa_version_when_looking_up_then_returns_none() {
        let result = lookup_co_layout("9.9.9.9");
        assert_eq!(result, None);
    }

    #[test]
    fn given_agesa_with_prefix_when_looking_up_then_still_matches() {
        let result = lookup_co_layout("AGESA V2 PI 1.2.0.7");
        assert!(result.is_some());
        let layout = result.unwrap();
        assert_eq!(layout.mode_offset, 0x174);
        assert_eq!(layout.signs_offset, 0x178);
        assert_eq!(layout.magnitudes_offset, 0x1B8);
        assert_eq!(layout.max_cores, 16);
    }

    #[test]
    fn given_agesa_with_extra_text_when_looking_up_then_still_matches() {
        let result = lookup_co_layout("1.2.0.7 Patch C");
        assert!(result.is_some());
        let layout = result.unwrap();
        assert_eq!(layout.mode_offset, 0x174);
        assert_eq!(layout.signs_offset, 0x178);
        assert_eq!(layout.magnitudes_offset, 0x1B8);
        assert_eq!(layout.max_cores, 16);
    }

    #[test]
    fn given_known_aod_guids_when_querying_then_returns_at_least_one() {
        let guids = known_aod_guids();
        assert!(!guids.is_empty());
        assert_eq!(guids[0], "5ed15dc0-edef-4161-9151-6014c4cc630c");
    }
}
