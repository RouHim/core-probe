//! Error parser for mprime results.txt and stdout output.
//!
//! Parses mprime error messages and progress indicators using regex patterns.
//! Supports incremental parsing (byte offset tracking) for efficient monitoring
//! of long-running tests.

use anyhow::{Context, Result};
use regex::Regex;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;
use std::sync::OnceLock;
use tracing::instrument;

/// Type of error detected in mprime output
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MprimeErrorType {
    /// ROUND OFF > 0.40 - numerical instability
    RoundoffError,
    /// Hardware failure detected running {size}K FFT
    HardwareFailure,
    /// FATAL ERROR - catastrophic failure
    FatalError,
    /// Possible hardware failure
    PossibleHardwareFailure,
    /// ILLEGAL SUMOUT - checksum verification failed
    IllegalSumout,
    /// SUM(INPUTS) != SUM(OUTPUTS) - data corruption
    SumMismatch,
    /// TORTURE TEST FAILED on worker — mprime detected test failure
    TortureTestFailed,
    /// Torture Test completed N tests in M minutes - X errors (when errors > 0)
    TortureTestSummaryError,
    /// Unknown error type
    Unknown,
}

/// Detected error from mprime output
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprimeError {
    /// Type of error detected
    pub error_type: MprimeErrorType,
    /// Full error message
    pub message: String,
    /// FFT size if mentioned in error (e.g., "Hardware failure detected running 1344K FFT")
    pub fft_size: Option<u32>,
    /// Timestamp from log line if present
    pub timestamp: Option<String>,
}

/// Parser for mprime results.txt with incremental reading support
#[derive(Debug)]
pub struct ErrorParser {
    /// Byte offset for incremental file reading
    byte_offset: u64,
}

impl ErrorParser {
    /// Create new parser with byte offset starting at 0
    pub fn new() -> Self {
        Self { byte_offset: 0 }
    }

    /// Parse results.txt from current byte offset, updating offset for next call
    ///
    /// Only reads new bytes since last parse call (incremental).
    #[instrument(skip(self))]
    pub fn parse_results(&mut self, path: &Path) -> Result<Vec<MprimeError>> {
        let mut file = File::open(path)
            .with_context(|| format!("Failed to open results file: {}", path.display()))?;

        file.seek(SeekFrom::Start(self.byte_offset))
            .context("Failed to seek to byte offset")?;
        let mut content = String::new();
        let bytes_read = file
            .read_to_string(&mut content)
            .context("Failed to read results file (invalid UTF-8)")?;
        self.byte_offset += bytes_read as u64;

        Ok(Self::parse_lines(&content))
    }

    /// Parse a single stdout line for real-time error detection
    pub fn parse_line(line: &str) -> Option<MprimeError> {
        Self::parse_lines(line).into_iter().next()
    }

    /// Internal: parse multiple lines and extract all errors
    fn parse_lines(text: &str) -> Vec<MprimeError> {
        text.lines()
            .filter_map(|line| {
                Self::try_roundoff_error(line)
                    .or_else(|| Self::try_hardware_failure(line))
                    .or_else(|| Self::try_fatal_error(line))
                    .or_else(|| Self::try_possible_hardware_failure(line))
                    .or_else(|| Self::try_illegal_sumout(line))
                    .or_else(|| Self::try_sum_mismatch(line))
                    .or_else(|| Self::try_torture_test_failed(line))
                    .or_else(|| Self::try_torture_summary_error(line))
            })
            .collect()
    }

    /// Extract timestamp from line if present (format: [2025-01-15 12:34:56])
    fn extract_timestamp(line: &str) -> Option<String> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"\[([^\]]+)\]").unwrap());
        re.captures(line)
            .and_then(|cap| cap.get(1))
            .map(|m| m.as_str().to_string())
    }

    fn try_roundoff_error(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"(?i)ROUND\s*OFF\s*>\s*0\.4|Rounding was .*, expected less than 0\.4")
                .unwrap()
        });
        if re.is_match(line) {
            Some(MprimeError {
                error_type: MprimeErrorType::RoundoffError,
                message: line.to_string(),
                fft_size: None,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_hardware_failure(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"(?i)Hardware failure detected running (\d+)K FFT").unwrap()
        });
        if let Some(cap) = re.captures(line) {
            let fft_size = cap.get(1).and_then(|m| m.as_str().parse::<u32>().ok());
            Some(MprimeError {
                error_type: MprimeErrorType::HardwareFailure,
                message: line.to_string(),
                fft_size,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_fatal_error(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)FATAL ERROR").unwrap());
        if re.is_match(line) {
            Some(MprimeError {
                error_type: MprimeErrorType::FatalError,
                message: line.to_string(),
                fft_size: None,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_possible_hardware_failure(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)Possible hardware failure").unwrap());
        if re.is_match(line) {
            Some(MprimeError {
                error_type: MprimeErrorType::PossibleHardwareFailure,
                message: line.to_string(),
                fft_size: None,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_illegal_sumout(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)ILLEGAL SUMOUT").unwrap());
        if re.is_match(line) {
            Some(MprimeError {
                error_type: MprimeErrorType::IllegalSumout,
                message: line.to_string(),
                fft_size: None,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_sum_mismatch(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)SUM\(INPUTS\)\s*!=\s*SUM\(OUTPUTS\)").unwrap());
        if re.is_match(line) {
            Some(MprimeError {
                error_type: MprimeErrorType::SumMismatch,
                message: line.to_string(),
                fft_size: None,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_torture_test_failed(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)TORTURE TEST FAILED").unwrap());
        if re.is_match(line) {
            Some(MprimeError {
                error_type: MprimeErrorType::TortureTestFailed,
                message: line.to_string(),
                fft_size: None,
                timestamp: Self::extract_timestamp(line),
            })
        } else {
            None
        }
    }

    fn try_torture_summary_error(line: &str) -> Option<MprimeError> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| {
            Regex::new(r"(?i)Torture Test completed \d+ tests? in \d+ minutes? - (\d+) errors?")
                .unwrap()
        });
        let caps = re.captures(line)?;
        let errors: u32 = caps.get(1)?.as_str().parse().ok()?;
        if errors == 0 {
            return None;
        }
        Some(MprimeError {
            error_type: MprimeErrorType::TortureTestSummaryError,
            message: line.to_string(),
            fft_size: None,
            timestamp: Self::extract_timestamp(line),
        })
    }

    /// Extract last passed FFT size from "Self-test {size}K passed" lines
    pub fn extract_last_passed_fft(text: &str) -> Option<u32> {
        static RE: OnceLock<Regex> = OnceLock::new();
        let re = RE.get_or_init(|| Regex::new(r"(?i)Self-test (\d+)K passed").unwrap());

        text.lines()
            .filter_map(|line| {
                re.captures(line)
                    .and_then(|cap| cap.get(1))
                    .and_then(|m| m.as_str().parse::<u32>().ok())
            })
            .next_back()
    }
}

impl Default for ErrorParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn given_roundoff_error_line_when_parsing_then_detects_hardware_error() {
        // GIVEN: Line with roundoff error
        let line = "[2025-01-15 12:34:56] FATAL ERROR: Rounding was 0.5, expected less than 0.4";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: Error detected with correct type
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::RoundoffError);
        assert_eq!(err.message, line);
        assert_eq!(err.timestamp, Some("2025-01-15 12:34:56".to_string()));
    }

    #[test]
    fn given_fatal_error_line_when_parsing_then_detects_fatal() {
        // GIVEN: Line with fatal error
        let line = "FATAL ERROR: Something went wrong";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: Fatal error detected
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::FatalError);
    }

    #[test]
    fn given_hardware_failure_line_when_parsing_then_extracts_fft_size() {
        // GIVEN: Line with hardware failure and FFT size
        let line =
            "[Worker #1] Hardware failure detected running 1344K FFT size, consult stress.txt.";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: FFT size extracted correctly
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::HardwareFailure);
        assert_eq!(err.fft_size, Some(1344));
    }

    #[test]
    fn given_self_test_passed_line_when_parsing_then_tracks_progress() {
        // GIVEN: Text with multiple self-test passed lines
        let text = r#"
[2025-01-15 12:00:00] Self-test 896K passed!
[2025-01-15 12:05:00] Self-test 1344K passed!
[2025-01-15 12:10:00] Self-test 1792K passed!
"#;

        // WHEN: Extracting last passed FFT
        let last_fft = ErrorParser::extract_last_passed_fft(text);

        // THEN: Returns last passed FFT size
        assert_eq!(last_fft, Some(1792));
    }

    #[test]
    fn given_clean_results_when_parsing_then_returns_no_errors() {
        // GIVEN: Clean results.txt with no errors
        let text = r#"
[2025-01-15 12:00:00] Self-test 896K passed!
[2025-01-15 12:05:00] Test 1, 1000 Lucas-Lehmer iterations of M62914177 using FMA3 FFT length 3584K
[2025-01-15 12:10:00] Iteration 500 completed
"#;

        // WHEN: Parsing text
        let errors = ErrorParser::parse_lines(text);

        // THEN: No errors detected
        assert!(errors.is_empty());
    }

    #[test]
    fn given_illegal_sumout_when_parsing_then_detects_error() {
        // GIVEN: Line with illegal sumout
        let line = "[Worker #1] ILLEGAL SUMOUT detected";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: Illegal sumout detected
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::IllegalSumout);
    }

    #[test]
    fn given_incremental_read_when_new_lines_added_then_only_parses_new() {
        // GIVEN: Temp file with initial content
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "Initial line - no errors").unwrap();
        file.flush().unwrap();

        let path = file.path().to_path_buf();
        let mut parser = ErrorParser::new();

        // WHEN: First parse (initial content)
        let errors1 = parser.parse_results(&path).unwrap();

        // THEN: No errors in initial content
        assert!(errors1.is_empty());
        let first_offset = parser.byte_offset;
        assert!(first_offset > 0);

        // GIVEN: Append new content with error
        writeln!(
            file,
            "[2025-01-15 12:00:00] Hardware failure detected running 1344K FFT size"
        )
        .unwrap();
        file.flush().unwrap();

        // WHEN: Second parse (incremental - only new content)
        let errors2 = parser.parse_results(&path).unwrap();

        // THEN: Second parse detected new error
        assert_eq!(errors2.len(), 1);
        assert_eq!(errors2[0].error_type, MprimeErrorType::HardwareFailure);
        assert_eq!(errors2[0].fft_size, Some(1344));

        // AND: Byte offset advanced
        assert!(parser.byte_offset > first_offset);

        // WHEN: Third parse (no new content)
        let errors3 = parser.parse_results(&path).unwrap();

        // THEN: No new errors found
        assert!(errors3.is_empty());
    }

    #[test]
    fn given_sum_mismatch_when_parsing_then_detects_error() {
        // GIVEN: Line with sum mismatch error
        let line = "[Worker #1] ERROR: SUM(INPUTS) != SUM(OUTPUTS)";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: Sum mismatch detected
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::SumMismatch);
    }

    #[test]
    fn given_possible_hardware_failure_when_parsing_then_detects_error() {
        // GIVEN: Line with possible hardware failure
        let line = "Possible hardware failure, consult the readme file.";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: Possible hardware failure detected
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::PossibleHardwareFailure);
    }

    #[test]
    fn given_roundoff_simple_format_when_parsing_then_detects() {
        // GIVEN: Simple ROUND OFF format (no "expected less than")
        let line = "[Worker #1] ROUND OFF > 0.40";

        // WHEN: Parsing line
        let error = ErrorParser::parse_line(line);

        // THEN: Roundoff error detected
        assert!(error.is_some());
        let err = error.unwrap();
        assert_eq!(err.error_type, MprimeErrorType::RoundoffError);
    }

    #[test]
    fn given_case_insensitive_errors_when_parsing_then_all_detected() {
        // GIVEN: Errors in various cases
        let lines = [
            "fatal error: test",
            "FATAL ERROR: test",
            "Fatal Error: test",
            "illegal sumout",
            "ILLEGAL SUMOUT",
        ];

        // WHEN: Parsing all lines
        let errors: Vec<_> = lines
            .iter()
            .filter_map(|line| ErrorParser::parse_line(line))
            .collect();

        // THEN: All detected regardless of case
        assert_eq!(errors.len(), 5);
    }

    #[test]
    fn given_torture_test_failed_when_parsing_then_detects_error() {
        // Given: mprime TORTURE TEST FAILED line from real output
        let line = "TORTURE TEST FAILED on worker #1";

        // When: Parsing the line
        let result = ErrorParser::parse_line(line);

        // Then: Error detected with correct type
        assert!(result.is_some(), "should detect TORTURE TEST FAILED");
        let error = result.unwrap();
        assert_eq!(error.error_type, MprimeErrorType::TortureTestFailed);
        assert_eq!(error.message, line);
    }

    #[test]
    fn given_torture_summary_with_errors_when_parsing_then_detects_error() {
        // Given: torture test summary line with errors from real output
        let line = "Torture Test completed 20 tests in 13 minutes - 1 errors, 0 warnings.";

        // When: Parsing the line
        let result = ErrorParser::parse_line(line);

        // Then: Error detected with correct type
        assert!(
            result.is_some(),
            "should detect torture summary with errors"
        );
        let error = result.unwrap();
        assert_eq!(error.error_type, MprimeErrorType::TortureTestSummaryError);
        assert_eq!(error.message, line);
    }

    #[test]
    fn given_torture_summary_no_errors_when_parsing_then_returns_none() {
        // Given: torture test summary line with zero errors (successful run)
        let line = "Torture Test completed 20 tests in 13 minutes - 0 errors, 0 warnings.";

        // When: Parsing the line
        let result = ErrorParser::parse_line(line);

        // Then: No error detected (0 errors means the test passed)
        assert!(
            result.is_none(),
            "should NOT detect error when 0 errors in summary"
        );
    }
}
