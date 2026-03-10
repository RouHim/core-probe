use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PboLimits {
    pub ppt_limit: Option<String>,
    pub tdc_limit: Option<String>,
    pub edc_limit: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct UefiSettings {
    pub available: bool,
    pub unavailable_reason: Option<String>,
    pub pbo_status: Option<String>,
    pub pbo_limits: Option<PboLimits>,
    pub curve_optimizer_offsets: Option<BTreeMap<u32, i32>>,
    pub agesa_version: Option<String>,
    pub raw_settings: Vec<(String, String)>,
}

impl UefiSettings {
    pub fn unavailable(reason: impl Into<String>) -> Self {
        Self {
            available: false,
            unavailable_reason: Some(reason.into()),
            pbo_status: None,
            pbo_limits: None,
            curve_optimizer_offsets: None,
            agesa_version: None,
            raw_settings: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_unavailable_reason_when_creating_then_stores_reason() {
        let settings = UefiSettings::unavailable("test reason");

        assert_eq!(settings.available, false);
        assert_eq!(settings.unavailable_reason, Some("test reason".to_string()));
    }

    #[test]
    fn given_default_settings_when_checking_then_not_available() {
        let settings = UefiSettings::default();

        assert_eq!(settings.available, false);
    }

    #[test]
    fn given_pbo_status_set_when_checking_then_returns_value() {
        let settings = UefiSettings {
            available: true,
            pbo_status: Some("Enabled".to_string()),
            ..Default::default()
        };

        assert_eq!(settings.pbo_status, Some("Enabled".to_string()));
    }
}
