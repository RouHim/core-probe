use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;

const RELEASE_BINARY_PATH: &str = "./target/release/unstable-cpu-detector";
const TEMP_DIR_PREFIX: &str = "unstable-cpu-detector-";

#[test]
fn given_binary_when_run_with_help_then_shows_usage() {
    let _lock = integration_test_lock();
    ensure_release_binary();

    let output = Command::new(binary_path())
        .arg("--help")
        .output()
        .expect("failed to execute --help");

    assert!(output.status.success(), "--help should exit successfully");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lower = stdout.to_ascii_lowercase();

    assert!(lower.contains("usage"), "help output should include usage");
    assert!(
        lower.contains("duration"),
        "help output should include duration option"
    );
    assert!(
        lower.contains("iterations"),
        "help output should include iterations option"
    );
    assert!(
        lower.contains("cores"),
        "help output should include cores option"
    );
    assert!(
        lower.contains("quiet"),
        "help output should include quiet option"
    );
    assert!(
        lower.contains("mode"),
        "help output should include mode option"
    );
    assert!(
        lower.contains("default: 6"),
        "help output should show duration default"
    );
    assert!(
        lower.contains("default: 3"),
        "help output should show iterations default"
    );
    assert!(
        lower.contains("sse"),
        "help output should show SSE mode default"
    );
}

#[test]
fn given_binary_when_built_release_then_compiles_without_warnings() {
    let _lock = integration_test_lock();
    let output = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .output()
        .expect("failed to run cargo build --release");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    let combined_lower = combined.to_ascii_lowercase();

    assert!(
        output.status.success(),
        "cargo build --release failed:\n{combined}"
    );
    assert!(
        !combined_lower.contains("warning:"),
        "cargo build --release emitted warnings:\n{combined}"
    );

    let metadata = fs::metadata(binary_path()).expect("release binary should exist after build");
    assert!(
        metadata.len() >= 25 * 1024 * 1024 && metadata.len() <= 50 * 1024 * 1024,
        "release binary should be in expected embedded size range (25-50MB), got {} bytes",
        metadata.len()
    );
}

#[test]
fn given_binary_when_run_on_amd_then_detects_cpu_topology() {
    let _lock = integration_test_lock();
    ensure_release_binary();
    let baseline_temp_dirs = list_detector_temp_dirs().expect("failed to read baseline temp dirs");

    let child = Command::new(binary_path())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn binary");

    thread::sleep(Duration::from_secs(3));
    let raw_pid = i32::try_from(child.id()).expect("child PID should fit into i32");
    let _ = kill(Pid::from_raw(raw_pid), Signal::SIGINT);
    let output = child
        .wait_with_output()
        .expect("failed to collect subprocess output");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let merged = format!("{stdout}\n{stderr}");
    let merged_lower = merged.to_ascii_lowercase();

    if running_on_amd() {
        assert!(
            merged.contains("CPU:")
                || merged_lower.contains("authenticamd")
                || merged_lower.contains("amd"),
            "startup output should include CPU topology details on AMD:\n{merged}"
        );
        assert!(
            merged_lower.contains("amd"),
            "startup output should mention AMD on AMD host:\n{merged}"
        );
    } else {
        assert!(
            merged.contains("Non-AMD CPU detected") || merged_lower.contains("only supports amd"),
            "non-AMD systems should fail pre-flight with explicit message:\n{merged}"
        );
    }

    let temp_dirs_after =
        list_detector_temp_dirs().expect("failed to read temp dirs after CPU test");
    if temp_dirs_after != baseline_temp_dirs {
        assert!(
            wait_for_temp_dirs(&baseline_temp_dirs, Duration::from_secs(10)),
            "CPU topology integration test leaked temp dirs"
        );
    }
}

#[test]
#[ignore]
fn given_binary_when_run_with_duration_1_then_completes_one_core() {
    let _lock = integration_test_lock();
    if !running_on_amd() {
        eprintln!("skipping long integration run on non-AMD CPU");
        return;
    }

    ensure_release_binary();
    let baseline_temp_dirs = list_detector_temp_dirs().expect("failed to read baseline temp dirs");

    let output = Command::new(binary_path())
        .args(["--duration", "1", "--iterations", "1", "--cores", "0"])
        .output()
        .expect("failed to execute short integration run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let merged = format!("{stdout}\n{stderr}");
    let code = output.status.code();

    assert!(
        matches!(code, Some(0) | Some(1)),
        "expected exit code 0 or 1 for completed run, got {:?}. Output:\n{merged}",
        code
    );
    assert!(
        merged.contains("RESULT: STABLE") || merged.contains("RESULT: UNSTABLE"),
        "expected RESULT line in output:\n{merged}"
    );

    let temp_dirs_after =
        list_detector_temp_dirs().expect("failed to read temp dirs after short run");
    if temp_dirs_after != baseline_temp_dirs {
        assert!(
            wait_for_temp_dirs(&baseline_temp_dirs, Duration::from_secs(10)),
            "short integration run leaked temp dirs"
        );
    }
}

#[test]
#[ignore]
fn given_binary_when_interrupted_then_cleans_up() {
    let _lock = integration_test_lock();
    if !running_on_amd() {
        eprintln!("skipping SIGTERM integration run on non-AMD CPU");
        return;
    }

    ensure_release_binary();
    let baseline_temp_dirs = list_detector_temp_dirs().expect("failed to read baseline temp dirs");

    let mut child = Command::new(binary_path())
        .args([
            "--duration",
            "1",
            "--iterations",
            "1",
            "--cores",
            "0",
            "--quiet",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn long-running subprocess");

    thread::sleep(Duration::from_secs(5));

    let raw_pid = i32::try_from(child.id()).expect("child PID should fit into i32");
    kill(Pid::from_raw(raw_pid), Signal::SIGINT).expect("failed to send SIGINT");

    let status = wait_with_timeout(&mut child, Duration::from_secs(20))
        .expect("subprocess should terminate after SIGINT")
        .expect("failed waiting for subprocess exit status");

    assert!(
        status.code().is_some(),
        "interrupted run should have a concrete exit code"
    );

    let temp_dirs_after =
        list_detector_temp_dirs().expect("failed to read temp dirs after SIGTERM");
    if temp_dirs_after != baseline_temp_dirs {
        assert!(
            wait_for_temp_dirs(&baseline_temp_dirs, Duration::from_secs(10)),
            "SIGTERM path leaked temp dirs"
        );
    }
}

#[test]
fn given_binary_when_run_quiet_then_outputs_result_line_only() {
    let _lock = integration_test_lock();
    if !running_on_amd() {
        eprintln!("skipping quiet-mode runtime verification on non-AMD CPU");
        return;
    }

    ensure_release_binary();
    let baseline_temp_dirs = list_detector_temp_dirs().expect("failed to read baseline temp dirs");

    let output = Command::new(binary_path())
        .args([
            "--duration",
            "0",
            "--iterations",
            "1",
            "--cores",
            "0",
            "--quiet",
        ])
        .output()
        .expect("failed to execute quiet mode run");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let lines: Vec<&str> = stdout
        .lines()
        .filter(|line| !line.trim().is_empty())
        .collect();

    assert!(
        matches!(output.status.code(), Some(0) | Some(1)),
        "quiet mode run should finish with exit 0 or 1, got {:?}",
        output.status.code()
    );
    assert_eq!(
        lines.len(),
        1,
        "quiet mode should emit exactly one non-empty line, got: {lines:?}"
    );
    assert!(
        lines[0].starts_with("RESULT:"),
        "quiet mode line should start with RESULT:, got {:?}",
        lines[0]
    );
    assert!(
        !stdout.contains("unstable-cpu-detector") && !stdout.contains("Config:"),
        "quiet mode should not print startup banner"
    );

    let temp_dirs_after =
        list_detector_temp_dirs().expect("failed to read temp dirs after quiet test");
    if temp_dirs_after != baseline_temp_dirs {
        assert!(
            wait_for_temp_dirs(&baseline_temp_dirs, Duration::from_secs(10)),
            "quiet mode integration test leaked temp dirs"
        );
    }
}

fn binary_path() -> &'static str {
    RELEASE_BINARY_PATH
}

fn integration_test_lock() -> std::sync::MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    match LOCK.get_or_init(|| Mutex::new(())).lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    }
}

fn ensure_release_binary() {
    let binary_exists = Path::new(binary_path()).exists();
    if binary_exists {
        return;
    }

    let output = Command::new("cargo")
        .arg("build")
        .arg("--release")
        .output()
        .unwrap_or_else(|error| panic!("failed to build release binary: {error}"));

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        panic!("failed to build release binary:\n{stdout}\n{stderr}");
    }
}

fn running_on_amd() -> bool {
    match fs::read_to_string("/proc/cpuinfo") {
        Ok(cpuinfo) => {
            cpuinfo.contains("vendor_id\t: AuthenticAMD") || cpuinfo.contains("AuthenticAMD")
        }
        Err(_) => false,
    }
}

fn list_detector_temp_dirs() -> std::io::Result<BTreeSet<PathBuf>> {
    let mut dirs = BTreeSet::new();
    for entry in fs::read_dir(std::env::temp_dir())? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        if !file_type.is_dir() {
            continue;
        }

        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(TEMP_DIR_PREFIX) {
            dirs.insert(entry.path());
        }
    }

    Ok(dirs)
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Option<std::io::Result<ExitStatus>> {
    let deadline = Instant::now() + timeout;

    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(Ok(status)),
            Ok(None) => {
                if Instant::now() >= deadline {
                    let _ = child.kill();
                    return None;
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(error) => return Some(Err(error)),
        }
    }
}

fn wait_for_temp_dirs(expected: &BTreeSet<PathBuf>, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        match list_detector_temp_dirs() {
            Ok(current) if &current == expected => return true,
            Ok(_) | Err(_) => thread::sleep(Duration::from_millis(200)),
        }
    }

    false
}
