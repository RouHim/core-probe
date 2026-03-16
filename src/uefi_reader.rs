use crate::hii_question::HiiQuestion;
use anyhow::Context;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::IsTerminal;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
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

fn matches_pbo(name: &str, help: &str) -> bool {
    let combined = format!("{name} {help}").to_lowercase();
    combined.contains("precision boost")
        || combined.contains("pbo")
        || combined.contains("core performance boost")
        || combined.contains("cpb")
}

fn matches_co(name: &str, help: &str) -> bool {
    let combined = format!("{name} {help}").to_lowercase();
    combined.contains("curve optimizer")
        || combined.contains("co offset")
        || combined.contains("per core")
}

fn matches_limits(name: &str, help: &str) -> bool {
    let combined = format!("{name} {help}").to_lowercase();
    combined.contains("pbo limits")
        || combined.contains("ppt")
        || combined.contains("tdc")
        || combined.contains("edc")
        || combined.contains("power limit")
}

fn matches_agesa(name: &str, help: &str) -> bool {
    let combined = format!("{name} {help}").to_lowercase();
    combined.contains("agesa")
}

fn matches_cbs(name: &str, help: &str) -> bool {
    let combined = format!("{name} {help}").to_lowercase();
    combined.contains("cbs") || combined.contains("amd overclocking")
}

fn non_empty_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn non_empty_non_auto_value(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("auto") {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Extract a version string from text by splitting on the first `:` or `=` separator.
/// Returns the trimmed right-hand side if non-empty, None otherwise.
fn extract_agesa_from_text(text: &str) -> Option<String> {
    let sep_pos = text.find([':', '='])?;
    let rhs = text[sep_pos + 1..].trim();
    if rhs.is_empty() {
        None
    } else {
        Some(rhs.to_string())
    }
}

fn matches_any_amd(name: &str, help: &str) -> bool {
    matches_pbo(name, help)
        || matches_co(name, help)
        || matches_limits(name, help)
        || matches_agesa(name, help)
        || matches_cbs(name, help)
}

/// Extract a core ID from a question name by finding digits near "core" keyword.
/// E.g. "Core 0 Curve Optimizer" → Some(0), "Per Core CO Offset 12" → Some(12)
fn extract_core_id(name: &str) -> Option<u32> {
    let lower = name.to_lowercase();
    if !lower.contains("core") {
        return None;
    }
    let mut num_str = String::new();
    let mut found_digits = false;
    for ch in name.chars() {
        if ch.is_ascii_digit() {
            num_str.push(ch);
            found_digits = true;
        } else if found_digits {
            break;
        }
    }
    if found_digits {
        num_str.parse::<u32>().ok()
    } else {
        None
    }
}

/// Parse HII questions into structured UefiSettings.
/// This is the testable core of HII database interpretation.
pub fn parse_hii_questions(questions: &[HiiQuestion]) -> UefiSettings {
    let mut pbo_status: Option<String> = None;
    let mut ppt_limit: Option<String> = None;
    let mut tdc_limit: Option<String> = None;
    let mut edc_limit: Option<String> = None;
    let mut co_offsets: BTreeMap<u32, i32> = BTreeMap::new();
    let mut saw_co_setting = false;
    let mut agesa_version: Option<String> = None;
    let mut pbo_limits_mode: Option<String> = None;
    let mut raw_settings: Vec<(String, String)> = Vec::new();

    tracing::debug!(
        total_questions = questions.len(),
        "starting AMD CBS question matching"
    );

    for q in questions {
        let name = &q.name;
        let help = &q.help;
        let answer = &q.answer;

        if matches_any_amd(name, help) {
            raw_settings.push((name.clone(), answer.clone()));
            tracing::info!(name = %name, answer = %answer, "matched AMD setting");
            tracing::debug!(name = %name, "question matched AMD filter");
        } else {
            let combined = format!("{name} {help}").to_lowercase();
            if combined.contains("amd") || combined.contains("ryzen") {
                tracing::debug!(name = %name, answer = %answer, "unmatched AMD-looking setting");
            }
            continue;
        }

        if pbo_status.is_none() && matches_pbo(name, help) {
            let name_lower = name.to_lowercase();
            let is_pbo_main = name_lower.contains("precision boost")
                || name_lower.contains("pbo")
                || name_lower.contains("core performance boost")
                || name_lower.contains("cpb");
            let is_co = matches_co(name, help) && extract_core_id(name).is_some();
            if is_pbo_main && !is_co {
                if let Some(value) = non_empty_value(answer) {
                    pbo_status = Some(value);
                }
            }
        }

        if matches_limits(name, help) {
            let name_lower = name.to_lowercase();
            if pbo_limits_mode.is_none() && name_lower.contains("pbo limits") {
                pbo_limits_mode = non_empty_value(answer);
            }
            if name_lower.contains("ppt") && ppt_limit.is_none() {
                ppt_limit = non_empty_value(answer);
            }
            if name_lower.contains("tdc") && tdc_limit.is_none() {
                tdc_limit = non_empty_value(answer);
            }
            if name_lower.contains("edc") && edc_limit.is_none() {
                edc_limit = non_empty_value(answer);
            }
        }

        if matches_co(name, help) {
            saw_co_setting = true;
            if let Some(core_id) = extract_core_id(name) {
                if let Ok(offset) = answer.trim().parse::<i32>() {
                    co_offsets.insert(core_id, offset);
                    tracing::debug!(core_id = core_id, offset = offset, "CO offset found");
                }
            }
        }

        if agesa_version.is_none() && matches_agesa(name, help) {
            agesa_version = non_empty_non_auto_value(answer)
                .or_else(|| {
                    let ver = extract_agesa_from_text(name);
                    if let Some(ref v) = ver {
                        tracing::debug!(source = "name", version = %v, "AGESA version extracted");
                    }
                    ver
                })
                .or_else(|| {
                    let ver = extract_agesa_from_text(help);
                    if let Some(ref v) = ver {
                        tracing::debug!(source = "help", version = %v, "AGESA version extracted");
                    }
                    ver
                });
        }
    }

    let has_limits = ppt_limit.is_some() || tdc_limit.is_some() || edc_limit.is_some();
    let pbo_limits = if has_limits {
        Some(PboLimits {
            ppt_limit,
            tdc_limit,
            edc_limit,
        })
    } else {
        pbo_limits_mode.map(|mode| PboLimits {
            ppt_limit: Some(mode),
            tdc_limit: None,
            edc_limit: None,
        })
    };

    let co_map = if saw_co_setting || !co_offsets.is_empty() {
        Some(co_offsets)
    } else {
        None
    };

    let available = !raw_settings.is_empty();

    tracing::info!(
        available = available,
        pbo_status = ?pbo_status,
        agesa_version = ?agesa_version,
        co_cores = co_map.as_ref().map(|m| m.len()).unwrap_or(0),
        raw_count = raw_settings.len(),
        "UEFI settings parsed"
    );

    UefiSettings {
        available,
        unavailable_reason: None,
        pbo_status,
        pbo_limits,
        curve_optimizer_offsets: co_map,
        agesa_version,
        raw_settings,
    }
}

/// Read UEFI settings by extracting the HII database (requires root).
pub fn read_uefi_settings_as_root(physical_core_count: usize) -> anyhow::Result<UefiSettings> {
    use crate::hii_extractor;
    use crate::ifr_parser;

    let bios_info = hii_extractor::read_bios_info();
    tracing::info!(
        bios_vendor = %bios_info.bios_vendor,
        bios_version = %bios_info.bios_version,
        product_name = %bios_info.product_name,
        "UEFI machine identified"
    );

    if !hii_extractor::check_hii_available() {
        return Ok(UefiSettings::unavailable(
            "HII backend not available on this machine (HiiDB efivar not found)",
        ));
    }

    let hii_db =
        hii_extractor::extract_hii_db().with_context(|| "Failed to extract HII database")?;

    let questions = ifr_parser::parse_ifr_to_questions(&hii_db)
        .with_context(|| "Failed to parse IFR questions")?;

    tracing::info!(question_count = questions.len(), "HII questions loaded");

    let mut settings = parse_hii_questions(&questions);

    let aod_co = crate::co_reader::read_curve_optimizer(
        settings.agesa_version.as_deref(),
        physical_core_count,
    );
    merge_aod_co_into_settings(&mut settings, aod_co);

    Ok(settings)
}

pub fn attempt_uefi_read_with_escalation(physical_core_count: usize) -> UefiSettings {
    let method = detect_escalation_method(is_current_user_root(), pkexec_available());
    match method {
        EscalationMethod::AlreadyRoot => read_uefi_settings_as_root(physical_core_count)
            .unwrap_or_else(|e| UefiSettings::unavailable(format!("UEFI reading failed: {e}"))),
        EscalationMethod::Pkexec => {
            if std::io::stdin().is_terminal() && std::io::stdout().is_terminal() {
                run_as_pkexec()
            } else {
                UefiSettings::unavailable("requires interactive root escalation or --uefi-only")
            }
        }
        EscalationMethod::Unavailable { reason } => UefiSettings::unavailable(reason),
    }
}

fn merge_aod_co_into_settings(settings: &mut UefiSettings, aod_co: Option<BTreeMap<u32, i32>>) {
    match aod_co {
        Some(map) => {
            let core_count = map.len();
            settings.curve_optimizer_offsets = Some(map);
            tracing::info!(
                co_source = "aod_setup",
                cores = core_count,
                "CO offsets merged from AOD_SETUP"
            );
        }
        None => {
            tracing::debug!("AOD_SETUP CO not available, preserving IFR-derived CO values");
        }
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
                    if let Err(e) = out.read_to_end(&mut stdout) {
                        tracing::debug!(error = %e, "partial read of pkexec stdout");
                    }
                }
                if let Some(mut err) = child.stderr.take() {
                    use std::io::Read;
                    if let Err(e) = err.read_to_end(&mut stderr_bytes) {
                        tracing::debug!(error = %e, "partial read of pkexec stderr");
                    }
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
                    if let Err(e) = child.kill() {
                        tracing::debug!(error = %e, "failed to kill timed-out pkexec child");
                    }
                    if let Err(e) = child.wait() {
                        tracing::debug!(error = %e, "failed to wait for timed-out pkexec child");
                    }
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
    use crate::hii_question::HiiQuestion;

    fn build_question(name: &str, answer: &str, help: &str) -> HiiQuestion {
        HiiQuestion {
            name: name.to_string(),
            answer: answer.to_string(),
            help: help.to_string(),
        }
    }

    fn build_test_questions() -> Vec<HiiQuestion> {
        vec![
            build_question(
                "Precision Boost Overdrive",
                "Enabled",
                "PBO control setting",
            ),
            build_question("PPT Limit", "142", "Platform Power Throttle limit in watts"),
            build_question("TDC Limit", "95", "Thermal Design Current limit in amps"),
            build_question(
                "EDC Limit",
                "140",
                "Electrical Design Current limit in amps",
            ),
            build_question(
                "Core 0 Curve Optimizer Offset",
                "-15",
                "Per core curve optimizer offset",
            ),
            build_question(
                "Core 1 Curve Optimizer Offset",
                "-10",
                "Per core curve optimizer offset",
            ),
            build_question(
                "Core 5 Curve Optimizer Offset",
                "-20",
                "Per core curve optimizer offset",
            ),
            build_question("AGESA Version", "1.2.0.7", "AGESA firmware version string"),
            build_question("CBS Debug Options", "Auto", "AMD CBS debug configuration"),
            build_question(
                "Boot Option #1",
                "UEFI OS",
                "First boot device in boot order",
            ),
        ]
    }

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

    #[test]
    fn given_hii_questions_with_pbo_enabled_when_parsing_then_extracts_pbo_status() {
        let questions = build_test_questions();

        let settings = parse_hii_questions(&questions);

        assert!(settings.available);
        assert_eq!(settings.pbo_status, Some("Enabled".to_string()));
    }

    #[test]
    fn given_hii_questions_with_co_offsets_when_parsing_then_populates_btreemap() {
        let questions = build_test_questions();

        let settings = parse_hii_questions(&questions);

        let co = settings
            .curve_optimizer_offsets
            .expect("CO offsets should be populated");
        assert_eq!(co.len(), 3);
        assert_eq!(co[&0], -15);
        assert_eq!(co[&1], -10);
        assert_eq!(co[&5], -20);
    }

    #[test]
    fn given_hii_questions_with_agesa_when_parsing_then_extracts_version() {
        let questions = build_test_questions();

        let settings = parse_hii_questions(&questions);

        assert_eq!(settings.agesa_version, Some("1.2.0.7".to_string()));
    }

    #[test]
    fn given_hii_questions_with_no_amd_settings_when_parsing_then_returns_empty_raw() {
        let questions = vec![
            build_question("Boot Option #1", "UEFI OS", "First boot device"),
            build_question("Secure Boot", "Enabled", "Enable secure boot"),
            build_question("CSM Support", "Disabled", "Compatibility support module"),
        ];

        let settings = parse_hii_questions(&questions);

        assert!(!settings.available);
        assert!(settings.raw_settings.is_empty());
        assert!(settings.pbo_status.is_none());
        assert!(settings.curve_optimizer_offsets.is_none());
        assert!(settings.agesa_version.is_none());
    }

    #[test]
    fn given_extract_db_fails_when_reading_then_returns_unavailable() {
        let err: anyhow::Result<UefiSettings> = Err(anyhow::anyhow!(
            "Failed to extract HII database: permission denied"
        ));

        let settings =
            err.unwrap_or_else(|e| UefiSettings::unavailable(format!("UEFI reading failed: {e}")));

        assert!(!settings.available);
        assert!(settings
            .unavailable_reason
            .as_deref()
            .unwrap_or("")
            .contains("UEFI reading failed"));
        assert!(settings
            .unavailable_reason
            .as_deref()
            .unwrap_or("")
            .contains("permission denied"));
    }

    #[test]
    fn given_mixed_case_keywords_when_searching_then_matches_case_insensitive() {
        let questions = vec![
            build_question(
                "PRECISION BOOST OVERDRIVE",
                "Disabled",
                "PBO control SETTING",
            ),
            build_question("cOrE 3 cUrVe OpTiMiZeR", "-5", "per CORE offset adjustment"),
            build_question("Agesa VERSION", "1.0.0.4", "agesa firmware"),
            build_question("PPT LIMIT", "200", "power limit"),
        ];

        let settings = parse_hii_questions(&questions);

        assert!(settings.available);
        assert_eq!(settings.pbo_status, Some("Disabled".to_string()));
        assert_eq!(settings.agesa_version, Some("1.0.0.4".to_string()));

        let co = settings
            .curve_optimizer_offsets
            .expect("CO offsets should be populated");
        assert_eq!(co[&3], -5);

        let limits = settings.pbo_limits.expect("limits should exist");
        assert_eq!(limits.ppt_limit, Some("200".to_string()));
    }

    #[test]
    fn given_hii_questions_with_pbo_limits_when_parsing_then_extracts_all_three() {
        let questions = build_test_questions();

        let settings = parse_hii_questions(&questions);

        let limits = settings.pbo_limits.expect("PBO limits should be populated");
        assert_eq!(limits.ppt_limit, Some("142".to_string()));
        assert_eq!(limits.tdc_limit, Some("95".to_string()));
        assert_eq!(limits.edc_limit, Some("140".to_string()));
    }

    #[test]
    fn given_hii_questions_with_cbs_when_parsing_then_includes_in_raw() {
        let questions = vec![build_question(
            "CBS Debug Options",
            "Auto",
            "AMD CBS debug configuration",
        )];

        let settings = parse_hii_questions(&questions);

        assert!(settings.available);
        assert!(settings
            .raw_settings
            .iter()
            .any(|(name, _)| name == "CBS Debug Options"));
    }

    #[test]
    fn given_core_name_with_digits_when_extracting_id_then_parses_correctly() {
        assert_eq!(extract_core_id("Core 0 Curve Optimizer"), Some(0));
        assert_eq!(extract_core_id("Core 12 CO Offset"), Some(12));
        assert_eq!(extract_core_id("Per Core 5 Setting"), Some(5));
        assert_eq!(extract_core_id("No digits here core"), None);
        assert_eq!(extract_core_id("PPT Limit 200"), None);
    }

    // ── Integration / pipeline tests ──

    use crate::coordinator::{CoreStatus, CoreTestResult, CycleResults};
    use crate::cpu_topology::CpuTopology;
    use crate::report::StabilityReport;
    use std::time::Duration;

    fn build_test_topology() -> CpuTopology {
        CpuTopology {
            vendor: "AuthenticAMD".to_string(),
            model_name: "AMD Ryzen 9 5900X".to_string(),
            physical_core_count: 2,
            logical_cpu_count: 2,
            core_map: BTreeMap::from([(0, vec![0]), (1, vec![1])]),
            cpu_brand: None,
            cpu_frequency_mhz: None,
        }
    }

    #[test]
    fn given_non_root_no_pkexec_when_reading_then_report_shows_unavailable_notice() {
        // GIVEN: Non-root, no pkexec → unavailable UefiSettings
        let uefi = UefiSettings::unavailable(
            "Root access required for UEFI settings. Run with sudo or install polkit (pkexec).",
        );
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![CoreTestResult {
                core_id: 0,
                logical_cpu_ids: vec![0],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: Duration::from_secs(360),
                iterations_completed: 1,
            }],
            total_duration: Duration::from_secs(360),
            iterations_completed: 1,
            interrupted: false,
        };

        // WHEN: Generate full report
        let report = StabilityReport::new(&results, &topology, Some(&uefi));
        let output = report.generate().expect("report generation should succeed");

        // THEN: Unavailable notice present, full report structure intact
        assert!(
            output.contains("PBO: ⚠ unavailable (run as root)"),
            "should contain new unavailable notice in header"
        );
        assert!(output.contains("RESULT:"), "should contain RESULT line");
        assert!(
            output.contains("CPU Stability Report"),
            "should contain report header"
        );
        assert!(output.contains('╚'), "should contain footer box-drawing");
        assert!(
            !output.contains("UEFI/BIOS Settings"),
            "should NOT contain the available UEFI section title"
        );
    }

    #[test]
    fn given_full_uefi_data_when_generating_report_then_shows_header_uefi_and_co_annotations() {
        // GIVEN: Full UEFI data with PBO, CO for 2 cores, AGESA
        let uefi = UefiSettings {
            available: true,
            unavailable_reason: None,
            pbo_status: Some("Enabled".to_string()),
            pbo_limits: None,
            curve_optimizer_offsets: Some(BTreeMap::from([(0, -25), (1, -10)])),
            agesa_version: Some("1.2.0.7".to_string()),
            raw_settings: vec![("PBO".to_string(), "Enabled".to_string())],
        };
        let topology = build_test_topology();
        // One failed core (0) and one passed core (1)
        let results = CycleResults {
            results: vec![
                CoreTestResult {
                    core_id: 0,
                    logical_cpu_ids: vec![0],
                    status: CoreStatus::Failed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(120),
                    iterations_completed: 1,
                },
                CoreTestResult {
                    core_id: 1,
                    logical_cpu_ids: vec![1],
                    status: CoreStatus::Passed,
                    mprime_errors: Vec::new(),
                    mce_errors: Vec::new(),
                    duration_tested: Duration::from_secs(360),
                    iterations_completed: 1,
                },
            ],
            total_duration: Duration::from_secs(480),
            iterations_completed: 1,
            interrupted: false,
        };

        // WHEN: Generate full report
        let report = StabilityReport::new(&results, &topology, Some(&uefi));
        let output = report.generate().expect("report generation should succeed");

        // THEN: UEFI section present with PBO + AGESA
        assert!(
            output.contains("PBO Status: Enabled"),
            "should show PBO status"
        );
        assert!(
            output.contains("AGESA Version: 1.2.0.7"),
            "should show AGESA version"
        );

        // CO annotation on failed core 0 (aggressive, -25)
        assert!(
            output.contains("CO offset: -25 (aggressive)"),
            "failed core 0 should have CO annotation"
        );
        // CO annotation on passed core 1 (moderate, -10)
        assert!(
            output.contains("CO offset: -10 (moderate)"),
            "passed core 1 should have CO annotation"
        );
    }

    #[test]
    fn given_uefi_settings_when_serializing_then_round_trip_preserves_all_fields() {
        // GIVEN: Fully populated UefiSettings
        let original = UefiSettings {
            available: true,
            unavailable_reason: None,
            pbo_status: Some("Enabled".to_string()),
            pbo_limits: Some(PboLimits {
                ppt_limit: Some("142".to_string()),
                tdc_limit: Some("95".to_string()),
                edc_limit: Some("140".to_string()),
            }),
            curve_optimizer_offsets: Some(BTreeMap::from([(0, -15), (1, -10), (5, -20)])),
            agesa_version: Some("1.2.0.7".to_string()),
            raw_settings: vec![
                ("PBO".to_string(), "Enabled".to_string()),
                ("AGESA".to_string(), "1.2.0.7".to_string()),
            ],
        };

        // WHEN: Serialize to JSON and deserialize back
        let json = serde_json::to_string(&original).expect("serialization should succeed");
        let deserialized: UefiSettings =
            serde_json::from_str(&json).expect("deserialization should succeed");

        // THEN: All fields preserved
        assert_eq!(original, deserialized);
    }

    #[test]
    fn given_partial_uefi_data_when_generating_report_then_shows_pbo_without_co_section() {
        // GIVEN: Partial UEFI — only pbo_status, no CO offsets
        let uefi = UefiSettings {
            available: true,
            unavailable_reason: None,
            pbo_status: Some("Auto".to_string()),
            pbo_limits: None,
            curve_optimizer_offsets: None,
            agesa_version: None,
            raw_settings: vec![("PBO".to_string(), "Auto".to_string())],
        };
        let topology = build_test_topology();
        let results = CycleResults {
            results: vec![CoreTestResult {
                core_id: 0,
                logical_cpu_ids: vec![0],
                status: CoreStatus::Passed,
                mprime_errors: Vec::new(),
                mce_errors: Vec::new(),
                duration_tested: Duration::from_secs(360),
                iterations_completed: 1,
            }],
            total_duration: Duration::from_secs(360),
            iterations_completed: 1,
            interrupted: false,
        };

        // WHEN: Generate full report
        let report = StabilityReport::new(&results, &topology, Some(&uefi));
        let output = report.generate().expect("report generation should succeed");

        // THEN: PBO shown but no CO section, no panic
        assert!(
            output.contains("PBO Status: Auto"),
            "should show PBO status"
        );
        assert!(
            !output.contains("Curve Optimizer Offsets:"),
            "should NOT show CO section header"
        );
        assert!(
            !output.contains("CO offset:"),
            "should NOT show per-core CO annotation"
        );
    }

    #[test]
    fn given_agesa_in_question_name_with_colon_when_parsing_then_extracts_version() {
        let questions = vec![build_question(
            "AGESA Version : ComboAm4v2PI 1.2.0.7",
            "",
            "",
        )];

        let settings = parse_hii_questions(&questions);

        assert_eq!(
            settings.agesa_version,
            Some("ComboAm4v2PI 1.2.0.7".to_string())
        );
    }

    #[test]
    fn given_agesa_in_answer_when_parsing_then_uses_answer() {
        let questions = vec![build_question("AGESA", "1.2.0.7", "")];

        let settings = parse_hii_questions(&questions);

        assert_eq!(settings.agesa_version, Some("1.2.0.7".to_string()));
    }

    #[test]
    fn given_agesa_in_help_text_when_parsing_then_extracts_from_help() {
        let questions = vec![build_question(
            "AgesaVersion",
            "",
            "Version : ComboAm4v2PI 1.2.0.7",
        )];

        let settings = parse_hii_questions(&questions);

        assert_eq!(
            settings.agesa_version,
            Some("ComboAm4v2PI 1.2.0.7".to_string())
        );
    }

    #[test]
    fn given_agesa_name_without_separator_when_parsing_then_uses_full_name() {
        let questions = vec![build_question("AGESA ComboAm4v2PI", "", "")];

        let settings = parse_hii_questions(&questions);

        assert_eq!(settings.agesa_version, None);
    }

    #[test]
    fn given_agesa_auto_before_colon_name_when_parsing_then_uses_colon_version() {
        let questions = vec![
            build_question("AGESA Version", "Auto", ""),
            build_question("AGESA Version : ComboAm4v2PI 1207", "", ""),
        ];

        let settings = parse_hii_questions(&questions);

        assert_eq!(
            settings.agesa_version,
            Some("ComboAm4v2PI 1207".to_string())
        );
    }

    #[test]
    fn given_pbo_limits_mode_only_when_parsing_then_populates_limits_with_mode() {
        let questions = vec![build_question("PBO Limits", "Auto", "")];

        let settings = parse_hii_questions(&questions);

        let limits = settings.pbo_limits.expect("pbo_limits should be present");
        assert_eq!(limits.ppt_limit, Some("Auto".to_string()));
        assert_eq!(limits.tdc_limit, None);
        assert_eq!(limits.edc_limit, None);
    }

    #[test]
    fn given_curve_optimizer_present_but_no_per_core_values_when_parsing_then_returns_empty_map() {
        let questions = vec![build_question("Curve Optimizer", "Disabled", "")];

        let settings = parse_hii_questions(&questions);

        let offsets = settings
            .curve_optimizer_offsets
            .expect("curve_optimizer_offsets should be present");
        assert!(offsets.is_empty());
    }

    // ── AOD CO merge tests ──

    #[test]
    fn given_aod_co_with_values_when_merging_then_replaces_empty_settings() {
        let mut settings = UefiSettings {
            curve_optimizer_offsets: None,
            ..Default::default()
        };
        let aod_co = Some(BTreeMap::from([(0u32, -15i32), (2u32, -30i32)]));

        merge_aod_co_into_settings(&mut settings, aod_co);

        assert_eq!(
            settings.curve_optimizer_offsets,
            Some(BTreeMap::from([(0, -15), (2, -30)]))
        );
    }

    #[test]
    fn given_aod_co_with_values_when_merging_then_replaces_ifr_values() {
        let mut settings = UefiSettings {
            curve_optimizer_offsets: Some(BTreeMap::new()),
            ..Default::default()
        };
        let aod_co = Some(BTreeMap::from([(0u32, -15i32)]));

        merge_aod_co_into_settings(&mut settings, aod_co);

        assert_eq!(
            settings.curve_optimizer_offsets,
            Some(BTreeMap::from([(0, -15)]))
        );
    }

    #[test]
    fn given_aod_co_none_when_merging_then_preserves_existing() {
        let mut settings = UefiSettings {
            curve_optimizer_offsets: Some(BTreeMap::from([(0u32, -10i32)])),
            ..Default::default()
        };

        merge_aod_co_into_settings(&mut settings, None);

        assert_eq!(
            settings.curve_optimizer_offsets,
            Some(BTreeMap::from([(0, -10)]))
        );
    }

    #[test]
    fn given_aod_co_empty_map_when_merging_then_sets_empty() {
        let mut settings = UefiSettings {
            curve_optimizer_offsets: Some(BTreeMap::from([(0u32, -15i32)])),
            ..Default::default()
        };
        let aod_co = Some(BTreeMap::new());

        merge_aod_co_into_settings(&mut settings, aod_co);

        assert_eq!(settings.curve_optimizer_offsets, Some(BTreeMap::new()));
    }

    #[test]
    fn given_core_count_when_passed_to_read_settings_then_accepted() {
        let _result = read_uefi_settings_as_root(12);
    }
}
