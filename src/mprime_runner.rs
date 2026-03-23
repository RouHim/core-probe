use std::collections::BTreeMap;
use std::fs;
use std::os::unix::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use nix::sched::{sched_setaffinity, CpuSet};
use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use tracing::{debug, instrument, warn};

use crate::cpu_topology::CpuTopology;
use crate::embedded::ExtractedBinaries;
use crate::mprime_config::MprimeConfig;

const STOP_TIMEOUT: Duration = Duration::from_secs(5);
const WAIT_POLL_INTERVAL: Duration = Duration::from_secs(1);
const STOP_POLL_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug)]
pub struct MprimeRunner {
    child: Option<Child>,
    mprime_path: PathBuf,
    lib_dir: PathBuf,
    core_map: BTreeMap<u32, Vec<u32>>,
}

impl MprimeRunner {
    pub fn new(mprime_path: PathBuf, lib_dir: PathBuf, core_map: BTreeMap<u32, Vec<u32>>) -> Self {
        Self {
            child: None,
            mprime_path,
            lib_dir,
            core_map,
        }
    }

    pub fn from_dependencies(extracted: &ExtractedBinaries, topology: &CpuTopology) -> Self {
        Self::new(
            extracted.mprime_path.clone(),
            extracted.lib_dir.clone(),
            topology.core_map.clone(),
        )
    }

    pub fn process_id(&self) -> Option<u32> {
        self.child.as_ref().map(Child::id)
    }

    #[instrument(skip(self, working_dir, config), fields(core_id, working_dir = %working_dir.display()))]
    pub fn start(
        &mut self,
        core_id: u32,
        working_dir: &Path,
        config: Option<&MprimeConfig>,
    ) -> Result<()> {
        if self.child.is_some() {
            bail!("mprime process is already running");
        }

        let logical_cpu_id = self
            .core_map
            .get(&core_id)
            .and_then(|v| v.first().copied())
            .with_context(|| format!("physical core {core_id} not found in topology map"))?;

        fs::create_dir_all(working_dir).with_context(|| {
            format!(
                "failed to create mprime working directory {}",
                working_dir.display()
            )
        })?;

        let prime_config = match config {
            Some(cfg) => cfg.clone().generate()?,
            None => MprimeConfig::builder()
                .disable_internal_affinity()
                .generate()?,
        };
        let prime_txt_path = working_dir.join("prime.txt");
        fs::write(&prime_txt_path, prime_config).with_context(|| {
            format!(
                "failed to write mprime config file {}",
                prime_txt_path.display()
            )
        })?;

        let ld_library_path = compose_ld_library_path(&self.lib_dir);

        let mut cmd = Command::new(&self.mprime_path);
        let w_arg = format!("-w{}", working_dir.display());
        cmd.arg("-t")
            .arg("-d")
            .arg(&w_arg)
            .env("LD_LIBRARY_PATH", &ld_library_path)
            .current_dir(working_dir)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .stdin(Stdio::null());

        // SAFETY: pre_exec runs between fork and exec in the child process.
        // We only call sched_setaffinity which is async-signal-safe.
        unsafe {
            cmd.pre_exec(move || {
                let mut cpu_set = CpuSet::new();
                cpu_set.set(logical_cpu_id as usize).map_err(|e| {
                    std::io::Error::other(format!(
                        "failed to set CPU {logical_cpu_id} in cpu set: {e}"
                    ))
                })?;
                sched_setaffinity(Pid::from_raw(0), &cpu_set).map_err(|e| {
                    std::io::Error::other(format!(
                        "failed to set CPU affinity to logical CPU {logical_cpu_id}: {e}"
                    ))
                })?;
                Ok(())
            });
        }

        let child = cmd.spawn().with_context(|| {
            format!("failed to spawn mprime pinned to logical CPU {logical_cpu_id}")
        })?;

        debug!(
            pid = child.id(),
            logical_cpu_id,
            mprime_path = %self.mprime_path.display(),
            lib_dir = %self.lib_dir.display(),
            "started mprime process"
        );

        self.child = Some(child);
        Ok(())
    }

    /// Re-pins all threads of the running mprime process to the specified logical CPU.
    ///
    /// mprime v30.19 uses hwloc internally and overrides OS-level CPU affinity for its
    /// worker threads. This method counteracts that by enumerating all threads via
    /// `/proc/PID/task/` and calling `sched_setaffinity` on each thread to force them
    /// back to the target CPU.
    ///
    /// Should be called after mprime has had time to spawn its worker threads (~2-3s).
    #[instrument(skip(self), fields(logical_cpu_id))]
    pub fn pin_all_threads(&self, logical_cpu_id: u32) -> Result<u32> {
        let pid = self
            .child
            .as_ref()
            .map(Child::id)
            .context("cannot pin threads: mprime process is not running")?;

        let mut cpu_set = CpuSet::new();
        cpu_set
            .set(logical_cpu_id as usize)
            .with_context(|| format!("failed to set CPU {logical_cpu_id} in cpu set"))?;

        let task_dir = format!("/proc/{pid}/task");
        let entries = fs::read_dir(&task_dir)
            .with_context(|| format!("failed to read thread directory {task_dir}"))?;

        let mut pinned_count = 0u32;
        for entry in entries {
            let entry = entry.context("failed to read thread directory entry")?;
            let tid_str = entry.file_name().to_string_lossy().to_string();
            let tid: i32 = tid_str
                .parse()
                .with_context(|| format!("invalid thread id '{tid_str}'"))?;

            match sched_setaffinity(Pid::from_raw(tid), &cpu_set) {
                Ok(()) => {
                    pinned_count += 1;
                }
                Err(nix::errno::Errno::ESRCH) => {
                    // Thread exited between readdir and setaffinity — harmless
                    debug!(tid, "thread exited before affinity could be set");
                }
                Err(e) => {
                    warn!(tid, %e, "failed to set affinity for mprime thread");
                }
            }
        }

        debug!(
            pid,
            pinned_count, logical_cpu_id, "pinned mprime threads to target CPU"
        );
        Ok(pinned_count)
    }

    #[instrument(skip(self))]
    pub fn stop(&mut self) -> Result<()> {
        let Some(child) = self.child.as_mut() else {
            return Ok(());
        };

        if let Some(status) = child
            .try_wait()
            .context("failed to query mprime status before stop")?
        {
            debug!(?status, "mprime already exited before stop");
            self.child = None;
            return Ok(());
        }

        let pid = child.id();
        send_signal(pid, Signal::SIGTERM).context("failed to send SIGTERM to mprime")?;

        if !wait_for_process_exit(child, STOP_TIMEOUT)? {
            warn!(pid, "mprime did not exit after SIGTERM, sending SIGKILL");
            send_signal(pid, Signal::SIGKILL).context("failed to send SIGKILL to mprime")?;
            let _ = wait_for_process_exit(child, Duration::from_secs(1))?;
        }

        if let Err(error) = child.wait() {
            warn!(pid, %error, "failed to reap mprime process after termination");
        }
        self.child = None;
        Ok(())
    }

    #[instrument(skip(self))]
    pub fn is_running(&mut self) -> Result<bool> {
        let Some(child) = self.child.as_mut() else {
            return Ok(false);
        };

        match child
            .try_wait()
            .context("failed to query mprime process status")?
        {
            Some(status) => {
                debug!(?status, "mprime process exited");
                self.child = None;
                Ok(false)
            }
            None => Ok(true),
        }
    }

    #[instrument(skip(self))]
    pub fn wait_for(&mut self, duration: Duration) -> Result<()> {
        let deadline = Instant::now() + duration;

        while Instant::now() < deadline {
            if !self.is_running()? {
                return Ok(());
            }

            let remaining = deadline.saturating_duration_since(Instant::now());
            let sleep_for = remaining.min(WAIT_POLL_INTERVAL);
            thread::sleep(sleep_for);
        }

        Ok(())
    }
}

impl Drop for MprimeRunner {
    fn drop(&mut self) {
        if let Err(error) = self.stop() {
            warn!(%error, "failed to stop mprime runner during drop");
        }
    }
}

fn send_signal(pid: u32, signal: Signal) -> Result<()> {
    let raw_pid = i32::try_from(pid).context("process id overflow converting to i32")?;
    kill(Pid::from_raw(raw_pid), signal)
        .with_context(|| format!("failed to send {:?} to pid {pid}", signal))?;
    Ok(())
}

fn wait_for_process_exit(child: &mut Child, timeout: Duration) -> Result<bool> {
    let deadline = Instant::now() + timeout;

    loop {
        if child
            .try_wait()
            .context("failed to query child process while waiting for exit")?
            .is_some()
        {
            return Ok(true);
        }

        if Instant::now() >= deadline {
            return Ok(false);
        }

        thread::sleep(STOP_POLL_INTERVAL);
    }
}

fn compose_ld_library_path(lib_dir: &Path) -> String {
    let lib_dir_string = lib_dir.to_string_lossy().into_owned();
    match std::env::var("LD_LIBRARY_PATH") {
        Ok(existing) if !existing.is_empty() => format!("{lib_dir_string}:{existing}"),
        _ => lib_dir_string,
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::time::Duration;

    use anyhow::{bail, Context, Result};

    use super::MprimeRunner;
    use crate::embedded::ExtractedBinaries;

    fn acquire_mprime_test_lock() -> MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    #[test]
    fn given_extracted_mprime_when_starting_then_process_spawns_successfully() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_real_mprime();
        let work_dir = fixture.unique_working_dir("spawn-success");

        runner.start(0, &work_dir, None)?;
        let running = runner.is_running()?;

        assert!(running, "mprime should be running after successful start");

        runner.stop()?;
        fixture.cleanup()?;
        Ok(())
    }

    #[test]
    fn given_running_mprime_when_stopping_then_process_terminates() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_real_mprime();
        let work_dir = fixture.unique_working_dir("stop-terminates");

        runner.start(0, &work_dir, None)?;
        runner.stop()?;

        assert!(
            !runner.is_running()?,
            "mprime should no longer be running after stop"
        );

        fixture.cleanup()?;
        Ok(())
    }

    #[test]
    fn given_core_id_when_pinning_then_mprime_runs_on_correct_cpu() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_real_mprime();
        let work_dir = fixture.unique_working_dir("cpu-pinning");

        runner.start(0, &work_dir, None)?;
        let pid = runner
            .process_id()
            .context("runner should expose a pid after start")?;
        let cpus_allowed = read_cpus_allowed_list(pid)?;

        assert!(
            cpulist_contains(&cpus_allowed, fixture.logical_cpu_id),
            "expected Cpus_allowed_list '{cpus_allowed}' to include logical CPU {}",
            fixture.logical_cpu_id
        );

        runner.stop()?;
        fixture.cleanup()?;
        Ok(())
    }

    #[test]
    fn given_working_dir_when_starting_then_mprime_uses_isolated_directory() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_real_mprime();
        let work_dir = fixture.unique_working_dir("isolated-workdir");

        runner.start(0, &work_dir, None)?;

        let prime_txt = work_dir.join("prime.txt");
        assert!(
            prime_txt.exists(),
            "runner should write prime.txt into isolated working directory"
        );

        let pid = runner
            .process_id()
            .context("runner should expose a pid after start")?;

        // Give mprime a moment to start before reading cmdline
        std::thread::sleep(std::time::Duration::from_millis(100));

        let cmdline = read_cmdline(pid)?;
        let work_dir_text = work_dir.to_string_lossy();

        assert!(
            cmdline.contains("-w"),
            "command line should include -w flag"
        );
        assert!(
            cmdline.contains(work_dir_text.as_ref()),
            "command line should include isolated work directory path"
        );

        runner.stop()?;
        fixture.cleanup()?;
        Ok(())
    }

    #[test]
    fn given_mprime_crash_when_monitoring_then_detects_exit() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_crashing_process();
        let work_dir = fixture.unique_working_dir("crash-detection");

        runner.start(0, &work_dir, None)?;
        runner.wait_for(Duration::from_secs(2))?;

        assert!(
            !runner.is_running()?,
            "runner should detect that the child process exited"
        );

        fixture.cleanup()?;
        Ok(())
    }

    struct RunnerFixture {
        extracted: ExtractedBinaries,
        logical_cpu_id: u32,
    }

    impl RunnerFixture {
        fn new() -> Result<Self> {
            let extracted = ExtractedBinaries::extract()?;
            let logical_cpu_id = read_first_allowed_cpu()?;
            Ok(Self {
                extracted,
                logical_cpu_id,
            })
        }

        fn runner_for_real_mprime(&self) -> MprimeRunner {
            MprimeRunner::new(
                self.extracted.mprime_path.clone(),
                self.extracted.lib_dir.clone(),
                BTreeMap::from([(0, vec![self.logical_cpu_id])]),
            )
        }

        fn runner_for_crashing_process(&self) -> MprimeRunner {
            MprimeRunner::new(
                PathBuf::from("/bin/false"),
                self.extracted.lib_dir.clone(),
                BTreeMap::from([(0, vec![self.logical_cpu_id])]),
            )
        }

        fn unique_working_dir(&self, suffix: &str) -> PathBuf {
            self.extracted
                .temp_dir
                .join(format!("core-0-{suffix}-{}", uuid::Uuid::new_v4()))
        }

        fn cleanup(&self) -> Result<()> {
            self.extracted.cleanup()
        }
    }

    fn read_first_allowed_cpu() -> Result<u32> {
        let status = fs::read_to_string("/proc/self/status")
            .context("failed to read /proc/self/status for allowed cpu list")?;
        let cpus_allowed = status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:\t"))
            .or_else(|| {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            })
            .map(str::trim)
            .context("Cpus_allowed_list not found in /proc/self/status")?;

        first_cpu_from_list(cpus_allowed)
    }

    fn read_cpus_allowed_list(pid: u32) -> Result<String> {
        let status_path = format!("/proc/{pid}/status");
        let status = fs::read_to_string(&status_path)
            .with_context(|| format!("failed to read process status {status_path}"))?;
        status
            .lines()
            .find_map(|line| line.strip_prefix("Cpus_allowed_list:\t"))
            .or_else(|| {
                status
                    .lines()
                    .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
            })
            .map(str::trim)
            .map(ToString::to_string)
            .context("Cpus_allowed_list not found in process status")
    }

    fn read_cmdline(pid: u32) -> Result<String> {
        let cmdline_path = format!("/proc/{pid}/cmdline");
        let bytes = fs::read(&cmdline_path)
            .with_context(|| format!("failed to read process cmdline {cmdline_path}"))?;
        let joined = bytes
            .split(|byte| *byte == b'\0')
            .filter(|part| !part.is_empty())
            .map(|part| String::from_utf8_lossy(part).into_owned())
            .collect::<Vec<_>>()
            .join(" ");
        Ok(joined)
    }

    fn first_cpu_from_list(value: &str) -> Result<u32> {
        for item in value.split(',') {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                continue;
            }

            if let Some((start, _end)) = trimmed.split_once('-') {
                let parsed = start
                    .trim()
                    .parse::<u32>()
                    .with_context(|| format!("invalid CPU range start '{start}'"))?;
                return Ok(parsed);
            }

            let parsed = trimmed
                .parse::<u32>()
                .with_context(|| format!("invalid CPU identifier '{trimmed}'"))?;
            return Ok(parsed);
        }

        bail!("CPU list '{value}' did not contain any CPU identifiers")
    }

    fn cpulist_contains(value: &str, cpu_id: u32) -> bool {
        value.split(',').any(|item| {
            let trimmed = item.trim();
            if trimmed.is_empty() {
                return false;
            }

            if let Some((start, end)) = trimmed.split_once('-') {
                let start = match start.trim().parse::<u32>() {
                    Ok(parsed) => parsed,
                    Err(_) => return false,
                };
                let end = match end.trim().parse::<u32>() {
                    Ok(parsed) => parsed,
                    Err(_) => return false,
                };
                (start..=end).contains(&cpu_id)
            } else {
                trimmed.parse::<u32>() == Ok(cpu_id)
            }
        })
    }

    #[test]
    fn given_core_id_when_starting_then_prime_txt_disables_internal_affinity() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        // Given: Runner configured to test physical core 0 mapped to a specific logical CPU
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_real_mprime();
        let work_dir = fixture.unique_working_dir("affinity-config");

        // When: Starting mprime on core 0
        runner.start(0, &work_dir, None)?;

        // Then: prime.txt contains EnableSetAffinity=0 and NumCores=1
        let prime_txt_content = fs::read_to_string(work_dir.join("prime.txt"))
            .context("should be able to read generated prime.txt")?;

        assert!(
            prime_txt_content.contains("EnableSetAffinity=0"),
            "prime.txt should contain EnableSetAffinity=0 to disable mprime's hwloc affinity, got:\n{prime_txt_content}"
        );
        assert!(
            prime_txt_content.contains("NumCores=1"),
            "prime.txt should contain NumCores=1 to restrict mprime to one core, got:\n{prime_txt_content}"
        );

        // Then: No [Worker #1] section (OS handles affinity via sched_setaffinity)
        assert!(
            !prime_txt_content.contains("[Worker #1]"),
            "prime.txt should NOT contain [Worker #1] section \u{2014} OS handles affinity, got:\n{prime_txt_content}"
        );

        runner.stop()?;
        fixture.cleanup()?;
        Ok(())
    }

    #[test]
    fn given_running_mprime_when_checking_threads_then_all_pinned_to_target_cpu() -> Result<()> {
        let _serial = acquire_mprime_test_lock();
        // Given: Runner configured to test physical core 0
        let fixture = RunnerFixture::new()?;
        let mut runner = fixture.runner_for_real_mprime();
        let work_dir = fixture.unique_working_dir("thread-pinning");

        // When: mprime is started, given time to spawn worker threads, then re-pinned
        runner.start(0, &work_dir, None)?;
        std::thread::sleep(Duration::from_secs(3));
        runner.pin_all_threads(fixture.logical_cpu_id)?;

        let pid = runner
            .process_id()
            .context("runner should expose a pid after start")?;
        // Then: All threads of the mprime process should be pinned to the target CPU
        let task_dir = format!("/proc/{pid}/task");
        let thread_dirs = fs::read_dir(&task_dir)
            .with_context(|| format!("failed to read thread directory {task_dir}"))?;

        let mut checked_threads = 0u32;
        for entry in thread_dirs {
            let entry = entry.context("failed to read thread directory entry")?;
            let tid = entry.file_name().to_string_lossy().to_string();
            let status_path = format!("/proc/{pid}/task/{tid}/status");
            let Ok(status) = fs::read_to_string(&status_path) else {
                continue; // Thread may have exited
            };

            let cpus_allowed = status
                .lines()
                .find_map(|line| line.strip_prefix("Cpus_allowed_list:\t"))
                .or_else(|| {
                    status
                        .lines()
                        .find_map(|line| line.strip_prefix("Cpus_allowed_list:"))
                })
                .map(str::trim);

            if let Some(cpus) = cpus_allowed {
                assert!(
                    cpulist_contains(cpus, fixture.logical_cpu_id),
                    "Thread {tid} Cpus_allowed_list '{cpus}' should include target CPU {}",
                    fixture.logical_cpu_id
                );
                checked_threads += 1;
            }
        }

        assert!(
            checked_threads >= 1,
            "should have checked at least one mprime thread"
        );

        runner.stop()?;
        fixture.cleanup()?;
        Ok(())
    }
}
