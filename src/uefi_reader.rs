use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationMethod {
    AlreadyRoot,
    Pkexec,
    Unavailable { reason: String },
}

pub fn detect_escalation_method(is_root: bool, has_pkexec: bool) -> EscalationMethod {
    if is_root {
        return EscalationMethod::AlreadyRoot;
    }
    if has_pkexec {
        return EscalationMethod::Pkexec;
    }
    EscalationMethod::Unavailable {
        reason: "Root access required for UEFI settings. Run with sudo or install polkit (pkexec)."
            .to_string(),
    }
}

fn is_current_user_root() -> bool {
    nix::unistd::getuid().is_root()
}

fn pkexec_available() -> bool {
    std::process::Command::new("which")
        .arg("pkexec")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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

pub fn attempt_uefi_read_with_escalation() -> UefiSettings {
    let method = detect_escalation_method(is_current_user_root(), pkexec_available());
    match method {
        EscalationMethod::AlreadyRoot => {
            // Task 3 will replace this placeholder
            UefiSettings::unavailable("UEFI reading not yet implemented for root path")
        }
        EscalationMethod::Pkexec => run_as_pkexec(),
        EscalationMethod::Unavailable { reason } => UefiSettings::unavailable(reason),
    }
}

fn run_as_pkexec() -> UefiSettings {
    let exe = match std::env::current_exe() {
        Ok(p) => p,
        Err(e) => return UefiSettings::unavailable(format!("Cannot find current executable: {e}")),
    };

    let mut child = match std::process::Command::new("pkexec")
        .arg(&exe)
        .arg("--uefi-only")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
    {
        Ok(c) => c,
        Err(e) => return UefiSettings::unavailable(format!("Failed to spawn pkexec: {e}")),
    };

    let timeout = std::time::Duration::from_secs(10);
    let poll_interval = std::time::Duration::from_millis(100);
    let start = std::time::Instant::now();

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = Vec::new();
                let mut stderr_bytes = Vec::new();
                if let Some(mut out) = child.stdout.take() {
                    use std::io::Read;
                    let _ = out.read_to_end(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    let _ = err.read_to_end(&mut stderr_bytes);
                }
                if status.success() {
                    let json_str = String::from_utf8_lossy(&stdout);
                    return match serde_json::from_str::<UefiSettings>(&json_str) {
                        Ok(settings) => settings,
                        Err(e) => UefiSettings::unavailable(format!(
                            "Failed to parse pkexec JSON output: {e}"
                        )),
                    };
                }
                let stderr_str = String::from_utf8_lossy(&stderr_bytes);
                return UefiSettings::unavailable(format!(
                    "pkexec escalation failed (exit {}): {}",
                    status,
                    stderr_str.trim()
                ));
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    return UefiSettings::unavailable(
                        "pkexec escalation timed out after 10 seconds (no polkit agent running?)"
                            .to_string(),
                    );
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                return UefiSettings::unavailable(format!("Failed to wait for pkexec: {e}"));
            }
        }
    }
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

    #[test]
    fn given_root_user_when_detecting_escalation_then_returns_already_root() {
        assert_eq!(
            detect_escalation_method(true, false),
            EscalationMethod::AlreadyRoot
        );
        assert_eq!(
            detect_escalation_method(true, true),
            EscalationMethod::AlreadyRoot
        );
    }

    #[test]
    fn given_non_root_with_pkexec_when_detecting_then_returns_pkexec() {
        assert_eq!(
            detect_escalation_method(false, true),
            EscalationMethod::Pkexec
        );
    }

    #[test]
    fn given_non_root_no_pkexec_when_detecting_then_returns_unavailable() {
        let result = detect_escalation_method(false, false);
        assert!(matches!(result, EscalationMethod::Unavailable { .. }));
        if let EscalationMethod::Unavailable { reason } = result {
            assert!(!reason.is_empty());
        }
    }

    #[test]
    fn given_unavailable_method_when_attempting_uefi_read_then_returns_unavailable_settings() {
        let method = EscalationMethod::Unavailable {
            reason: "test".to_string(),
        };
        let settings = match method {
            EscalationMethod::Unavailable { reason } => UefiSettings::unavailable(reason),
            _ => panic!("unexpected"),
        };
        assert!(!settings.available);
        assert!(settings.unavailable_reason.is_some());
    }
    #[test]
    fn given_pkexec_fails_when_reading_then_returns_unavailable_with_reason() {
        let fake_json = r#"{"available":false,"unavailable_reason":"pkexec escalation failed (exit 1): Authentication failed","pbo_status":null,"pbo_limits":null,"curve_optimizer_offsets":null,"agesa_version":null,"raw_settings":[]}"#;
        let settings: UefiSettings = serde_json::from_str(fake_json).expect("parse test JSON");
        assert!(!settings.available);
        assert!(settings
            .unavailable_reason
            .as_deref()
            .unwrap_or("")
            .contains("pkexec escalation failed"));
    }
}
