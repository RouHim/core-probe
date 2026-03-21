//! Signal handling and resource cleanup for graceful shutdown.
//!
//! This module provides:
//! - Ctrl+C (SIGINT) handler registration via the `ctrlc` crate
//! - Global atomic shutdown flag for cross-thread coordination
//! - Resource tracking and cleanup (temp dirs, child processes)
//! - Graceful shutdown with partial result preservation

use anyhow::Context;
use nix::sys::signal::{self, Signal};
use nix::sys::wait::{waitpid, WaitPidFlag, WaitStatus};
use nix::unistd::Pid;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tracing::{debug, error, info, instrument, warn};

/// Global shutdown flag set by Ctrl+C handler.
/// Uses SeqCst ordering to ensure all threads see the flag change immediately.
static SHUTDOWN_REQUESTED: AtomicBool = AtomicBool::new(false);

/// Check if shutdown was requested via Ctrl+C.
///
/// This function is polled by the main test loop to detect interruption.
/// Returns immediately without blocking.
pub fn is_shutdown_requested() -> bool {
    SHUTDOWN_REQUESTED.load(Ordering::SeqCst)
}

pub fn request_shutdown() {
    SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
}

pub fn reset_shutdown() {
    SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
}

/// Register Ctrl+C (SIGINT) handler.
///
/// This should be called once at program startup.
/// The handler sets the global SHUTDOWN_REQUESTED flag and prints a message.
///
/// # Errors
///
/// Returns error if signal handler registration fails (e.g., another handler already installed).
#[instrument]
pub fn register_handler() -> anyhow::Result<()> {
    ctrlc::set_handler(move || {
        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);
        eprintln!("\nShutdown signal received, stopping gracefully...");
    })
    .context("Failed to register Ctrl+C handler")?;

    debug!("Ctrl+C handler registered successfully");
    Ok(())
}

/// Resource cleanup tracker for graceful shutdown.
///
/// Tracks:
/// - Temporary directories (for removal on clean exit)
/// - Child process PIDs (for SIGTERM/SIGKILL on exit)
/// - Error state (to preserve temp dirs for debugging)
///
/// Thread-safe via Arc<Mutex<>> wrapper.
#[derive(Default)]
pub struct Cleanup {
    /// Temporary directories to remove on clean exit
    temp_dirs: Vec<PathBuf>,
    /// Child process PIDs to terminate on exit
    child_pids: Vec<u32>,
    /// If true, preserve temp directories for debugging (set when errors occur)
    preserve_on_error: bool,
    /// Track if cleanup has already been executed (idempotency)
    executed: bool,
}

impl Cleanup {
    /// Create a new thread-safe Cleanup instance.
    ///
    /// Returns an Arc<Mutex<>> wrapper for shared access across threads.
    pub fn new() -> Arc<Mutex<Self>> {
        Arc::new(Mutex::new(Self {
            temp_dirs: Vec::new(),
            child_pids: Vec::new(),
            preserve_on_error: false,
            executed: false,
        }))
    }

    /// Register a temporary directory for cleanup.
    ///
    /// The directory will be removed during cleanup unless preserve_on_error is set.
    pub fn register_temp_dir(&mut self, path: PathBuf) {
        debug!(?path, "Registering temporary directory for cleanup");
        self.temp_dirs.push(path);
    }

    /// Register a child process for termination during cleanup.
    ///
    /// The process will receive SIGTERM, then SIGKILL if still alive after 5 seconds.
    pub fn register_child_process(&mut self, pid: u32) {
        debug!(pid, "Registering child process for cleanup");
        self.child_pids.push(pid);
    }

    /// Set whether to preserve temporary directories on error.
    ///
    /// When true, temp directories are kept for debugging.
    /// Should be set to true when any test errors are detected.
    pub fn set_preserve_on_error(&mut self, preserve: bool) {
        debug!(preserve, "Setting preserve_on_error flag");
        self.preserve_on_error = preserve;
    }

    /// Execute cleanup sequence.
    ///
    /// Order:
    /// 1. Terminate child processes (SIGTERM → wait 5s → SIGKILL → reap)
    /// 2. Remove temporary directories (unless preserve_on_error)
    ///
    /// This method is idempotent - safe to call multiple times.
    ///
    /// # Errors
    ///
    /// Returns error if cleanup operations fail. Continues with remaining cleanup
    /// even if some operations fail.
    #[instrument(skip(self))]
    pub fn execute(&mut self) -> anyhow::Result<()> {
        if self.executed {
            debug!("Cleanup already executed, skipping");
            return Ok(());
        }
        self.executed = true;

        info!("Starting cleanup sequence");

        // 1. Terminate child processes
        self.cleanup_child_processes()?;

        // 2. Remove temporary directories
        self.cleanup_temp_dirs()?;

        info!("Cleanup sequence completed");
        Ok(())
    }

    /// Terminate all tracked child processes.
    ///
    /// For each process:
    /// 1. Send SIGTERM (graceful shutdown)
    /// 2. Wait up to 5 seconds
    /// 3. Check if process still exists
    /// 4. If alive: send SIGKILL (forceful termination)
    /// 5. Reap zombie process with waitpid()
    #[instrument(skip(self))]
    fn cleanup_child_processes(&mut self) -> anyhow::Result<()> {
        if self.child_pids.is_empty() {
            debug!("No child processes to clean up");
            return Ok(());
        }

        info!(count = self.child_pids.len(), "Terminating child processes");

        for pid in self.child_pids.drain(..) {
            let nix_pid = Pid::from_raw(pid as i32);

            // Send SIGTERM for graceful shutdown
            match signal::kill(nix_pid, Signal::SIGTERM) {
                Ok(_) => {
                    info!(pid, "Sent SIGTERM to child process");

                    // Wait up to 5 seconds for graceful exit
                    let mut terminated = false;
                    for _ in 0..50 {
                        thread::sleep(Duration::from_millis(100));

                        // Check if process still exists by attempting a non-blocking waitpid
                        match waitpid(nix_pid, Some(WaitPidFlag::WNOHANG)) {
                            Ok(WaitStatus::Exited(_, _)) | Ok(WaitStatus::Signaled(_, _, _)) => {
                                info!(pid, "Child process terminated gracefully");
                                terminated = true;
                                break;
                            }
                            Ok(WaitStatus::StillAlive) => {
                                // Process still running, continue waiting
                                continue;
                            }
                            Ok(_) => {
                                // Other status (stopped, continued) - treat as terminated
                                terminated = true;
                                break;
                            }
                            Err(nix::errno::Errno::ECHILD) => {
                                // Process already reaped or not our child
                                info!(pid, "Child process already terminated");
                                terminated = true;
                                break;
                            }
                            Err(e) => {
                                warn!(pid, error = %e, "Error checking child process status");
                                break;
                            }
                        }
                    }

                    // If process still alive after 5 seconds, send SIGKILL
                    if !terminated {
                        warn!(
                            pid,
                            "Child process did not terminate gracefully, sending SIGKILL"
                        );
                        if let Err(e) = signal::kill(nix_pid, Signal::SIGKILL) {
                            error!(pid, error = %e, "Failed to send SIGKILL");
                        } else {
                            info!(pid, "Sent SIGKILL to child process");
                            // Final waitpid to reap zombie
                            if let Err(e) = waitpid(nix_pid, None) {
                                warn!(pid, error = %e, "Failed to reap child process");
                            }
                        }
                    }
                }
                Err(nix::errno::Errno::ESRCH) => {
                    // Process already terminated
                    info!(pid, "Child process already terminated");
                }
                Err(e) => {
                    error!(pid, error = %e, "Failed to send SIGTERM to child process");
                }
            }
        }

        Ok(())
    }

    /// Remove all tracked temporary directories.
    ///
    /// If preserve_on_error is true, directories are kept and their paths are logged.
    #[instrument(skip(self))]
    fn cleanup_temp_dirs(&mut self) -> anyhow::Result<()> {
        if self.temp_dirs.is_empty() {
            debug!("No temporary directories to clean up");
            return Ok(());
        }

        if self.preserve_on_error {
            info!(
                count = self.temp_dirs.len(),
                "Preserving temporary directories for debugging"
            );
            for path in &self.temp_dirs {
                info!(path = %path.display(), "Preserved temporary directory");
            }
            return Ok(());
        }

        info!(
            count = self.temp_dirs.len(),
            "Removing temporary directories"
        );

        for path in self.temp_dirs.drain(..) {
            match fs::remove_dir_all(&path) {
                Ok(_) => {
                    info!(path = %path.display(), "Removed temporary directory");
                }
                Err(e) => {
                    warn!(path = %path.display(), error = %e, "Failed to remove temporary directory");
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::process::{Command, Stdio};

    /// BDD: Given running state, when Ctrl+C signal received, then shutdown flag is set
    #[test]
    fn given_running_state_when_ctrl_c_then_shutdown_flag_is_set() {
        // Given: Fresh state with shutdown flag false
        // (Note: Cannot directly trigger Ctrl+C in test, so we simulate by setting the flag)
        // In real usage, ctrlc::set_handler sets this flag
        assert!(!is_shutdown_requested());

        // When: Shutdown flag is set (simulating Ctrl+C handler behavior)
        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);

        // Then: is_shutdown_requested returns true
        assert!(is_shutdown_requested());

        // Cleanup: Reset flag for other tests
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    }

    /// BDD: Given shutdown flag set, when checked, then returns true
    #[test]
    fn given_shutdown_flag_when_checked_then_returns_true() {
        // Given: Shutdown flag is set
        SHUTDOWN_REQUESTED.store(true, Ordering::SeqCst);

        // When: We check the shutdown status
        let result = is_shutdown_requested();

        // Then: Result is true
        assert!(result);

        // Cleanup
        SHUTDOWN_REQUESTED.store(false, Ordering::SeqCst);
    }

    /// BDD: Given temp directory, when cleanup executed, then all files removed
    #[test]
    fn given_temp_dir_when_cleanup_then_all_files_removed() {
        // Given: A temporary directory with a file
        let temp_dir = std::env::temp_dir().join(format!("test-cleanup-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        let test_file = temp_dir.join("test.txt");
        File::create(&test_file).expect("Failed to create test file");

        assert!(temp_dir.exists());
        assert!(test_file.exists());

        // Register for cleanup
        let cleanup = Cleanup::new();
        cleanup.lock().unwrap().register_temp_dir(temp_dir.clone());

        // When: Cleanup is executed (without preserve_on_error)
        cleanup.lock().unwrap().execute().expect("Cleanup failed");

        // Then: Temp directory and file are removed
        assert!(!temp_dir.exists());
        assert!(!test_file.exists());
    }

    /// BDD: Given partial results and shutdown requested, when cleanup executed, then results preserved
    #[test]
    fn given_partial_results_when_shutdown_then_results_preserved() {
        // Given: A temporary directory with partial results
        let temp_dir = std::env::temp_dir().join(format!("test-preserve-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");
        let results_file = temp_dir.join("results.txt");
        File::create(&results_file).expect("Failed to create results file");

        assert!(temp_dir.exists());
        assert!(results_file.exists());

        // Register for cleanup with preserve_on_error
        let cleanup = Cleanup::new();
        {
            let mut guard = cleanup.lock().unwrap();
            guard.register_temp_dir(temp_dir.clone());
            guard.set_preserve_on_error(true); // Preserve due to error/interruption
        }

        // When: Cleanup is executed with preserve_on_error
        cleanup.lock().unwrap().execute().expect("Cleanup failed");

        // Then: Temp directory and results are preserved
        assert!(temp_dir.exists());
        assert!(results_file.exists());

        // Manual cleanup for test
        fs::remove_dir_all(&temp_dir).ok();
    }

    /// BDD: Given child process, when cleanup executed, then process is terminated
    #[test]
    fn given_child_process_when_cleanup_then_process_terminated() {
        // Given: A long-running child process (sleep)
        let mut child = Command::new("sleep")
            .arg("300") // 5 minutes
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("Failed to spawn child process");

        let pid = child.id();

        // Verify process is running
        assert!(process_exists(pid));

        // Register for cleanup
        let cleanup = Cleanup::new();
        cleanup.lock().unwrap().register_child_process(pid);

        // When: Cleanup is executed
        cleanup.lock().unwrap().execute().expect("Cleanup failed");

        // Then: Process is terminated
        // Wait a bit for signal delivery
        thread::sleep(Duration::from_millis(200));
        assert!(!process_exists(pid));

        // Ensure child is reaped (if cleanup didn't already do it)
        let _ = child.wait();
    }

    /// BDD: Given cleanup executed, when executed again, then operation is idempotent
    #[test]
    fn given_cleanup_executed_when_executed_again_then_idempotent() {
        // Given: Cleanup with temp dir
        let temp_dir =
            std::env::temp_dir().join(format!("test-idempotent-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

        let cleanup = Cleanup::new();
        cleanup.lock().unwrap().register_temp_dir(temp_dir.clone());

        // When: Cleanup is executed twice
        cleanup
            .lock()
            .unwrap()
            .execute()
            .expect("First cleanup failed");
        let result = cleanup.lock().unwrap().execute();

        // Then: Second execution succeeds and doesn't fail
        assert!(result.is_ok());
        assert!(!temp_dir.exists()); // Should have been removed on first execution
    }

    /// Helper: Check if process exists
    fn process_exists(pid: u32) -> bool {
        let nix_pid = Pid::from_raw(pid as i32);
        // Try to send null signal to check existence
        signal::kill(nix_pid, None).is_ok()
    }
}
