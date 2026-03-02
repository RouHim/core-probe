# Unstable CPU Detector for Linux

## TL;DR

> **Quick Summary**: Build a Rust CLI tool that detects unstable AMD CPU cores by cycling mprime (Prime95) stress tests per-core and monitoring journalctl for MCE hardware errors, then reporting which cores are unstable.
> 
> **Deliverables**:
> - Rust binary `unstable-cpu-detector` with embedded mprime + libgmp.so
> - Per-core stress testing with configurable duration, FFT presets, and optional core filtering
> - MCE/EDAC hardware error detection via journalctl parsing
> - Final stability report identifying unstable cores
> - AGENTS.md file for agent guidance
> 
> **Estimated Effort**: Large
> **Parallel Execution**: YES — 4 waves
> **Critical Path**: Task 1 → Task 3 → Task 6 → Task 8 → Task 10 → Task 12 → Task 13 → F1-F4

---

## Context

### Original Request
Build a simplified version of [CoreCycler](https://github.com/sp00n/CoreCycler) for Linux CLI that identifies unstable CPU cores using mprime (Prime95), targeting AMD 64-bit processors only. Sensible defaults, minimal configuration.

### Interview Summary
**Key Discussions**:
- CLI parser: User explicitly chose `argh` over `clap` for lighter footprint
- mprime is fully controllable via config files (prime.txt) — no TUI interaction needed
- MCE/EDAC monitoring via journalctl is essential complement to stress testing
- Sensible defaults: SSE mode, Huge FFT, 6min/core, 1 thread (matches CoreCycler proven defaults)
- Test strategy: TDD with BDD-style tests as specified in task spec

**Research Findings**:
- mprime v30.19b20: 27MB ELF binary, needs libgmp.so at runtime, controlled via prime.txt config
- mprime error patterns: "ROUND OFF > 0.40", "Hardware failure detected", "FATAL ERROR", "ILLEGAL SUMOUT"
- CoreCycler architecture: config-file generation + thread affinity + results.txt polling every 10s
- SSE mode preferred: lighter load → higher boost clocks → exposes PBO/CO instabilities that AVX misses
- Alternatives evaluated: stress-ng (no error detection), y-cruncher (proprietary), OCCT (no CLI) — mprime confirmed best
- Target machine: AMD Ryzen 9 5900X, core IDs 0-5,8-13 (non-contiguous), active MCE errors on CPU:0
- CPU topology: `/sys/devices/system/cpu/cpuN/topology/` provides core_id, physical_package_id, thread_siblings

### Self-Identified Gaps (Metis consultation timed out)
**Addressed in plan**:
- Signal handling (Ctrl+C): Must cleanly kill mprime child process and report partial results
- Temp file cleanup: Extracted mprime binary and working dirs must be cleaned up on exit
- mprime license: Freeware license — embedding is allowed but license.txt should be displayed/included
- libgmp.so symlink: mprime expects `libgmp.so.10` symlink, not just the versioned file
- Non-contiguous core IDs: Must handle core_id mapping (0-5, 8-13) not assuming 0..N
- Concurrent MCE monitoring: journalctl watcher must run in parallel thread during stress test
- Process isolation: Each core test needs separate working directory to avoid results.txt conflicts

---

## Work Objectives

### Core Objective
Detect unstable AMD CPU cores on Linux by running per-core mprime torture tests with simultaneous MCE/EDAC hardware error monitoring, then reporting which cores failed.

### Concrete Deliverables
- `src/main.rs` — Entry point, CLI arg parsing with argh, orchestration
- `src/cpu_topology.rs` — AMD CPU detection, core enumeration, topology mapping
- `src/mprime_config.rs` — prime.txt configuration file generation
- `src/mprime_runner.rs` — mprime process spawning, lifecycle management, affinity pinning
- `src/error_parser.rs` — results.txt and stdout error pattern parsing
- `src/mce_monitor.rs` — journalctl MCE/EDAC monitoring thread
- `src/report.rs` — Final stability report generation
- `src/embedded.rs` — Binary extraction (mprime + libgmp.so) to temp directory
- `src/signal_handler.rs` — Ctrl+C handling, cleanup
- `AGENTS.md` — Agent guidance document
- Test files alongside each source file

### Definition of Done
- [x] `cargo build --release` succeeds with zero warnings
- [x] `cargo test` passes all tests
- [x] `cargo clippy -- -D warnings` passes
- [x] `cargo fmt --check` passes
- [x] Binary runs on AMD 64-bit Linux, rejects non-AMD/non-64-bit
- [x] Correctly detects and reports unstable cores via mprime errors
- [x] Correctly detects and reports MCE/EDAC errors from journalctl
- [x] Cleans up temp files on normal exit and Ctrl+C
- [x] No `unwrap()`/`expect()` in non-test code

### Must Have
- Per-core isolation: one mprime instance per core with CPU affinity pinning
- Error detection: parse mprime results.txt for all known error patterns
- MCE monitoring: concurrent journalctl parsing during stress tests
- Sensible defaults: works with zero configuration
- AMD/64-bit validation: reject incompatible hardware at startup
- Embedded binaries: mprime + libgmp.so extracted at runtime
- Clean shutdown: Ctrl+C kills mprime, cleans temp files, reports partial results
- Structured logging with tracing

### Must NOT Have (Guardrails)
- **No GUI/TUI**: CLI output only, no ratatui/crossterm/ncurses
- **No configuration files**: All config via CLI args with sensible defaults (no TOML/YAML/JSON config)
- **No network access**: UsePrimenet=0, no telemetry, no update checks
- **No sudo escalation**: Never run sudo automatically; if MCE monitoring needs it, inform the user
- **No OpenSSL**: RustTLS only if TLS is ever needed (it shouldn't be)
- **No Rust modules**: Flat file structure in `src/`, no `mod.rs` or nested module directories
- **No premature abstraction**: No trait hierarchies, generic engines, or plugin systems
- **No multi-engine support**: mprime only, no y-cruncher/stress-ng/AIDA64 support
- **No Windows/macOS code**: No cfg(target_os) branching, Linux-only
- **No excessive CLI options**: Minimal args with smart defaults, not 50+ flags like CoreCycler
- **No over-commenting**: No JSDoc-style comments on every function; comments only for non-obvious logic
- **No async runtime**: No tokio/async-std; use std threads for concurrent MCE monitoring

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed. No exceptions.
> Acceptance criteria requiring "user manually tests/confirms" are FORBIDDEN.

### Test Decision
- **Infrastructure exists**: NO (greenfield project — test infra set up in Task 1)
- **Automated tests**: YES (TDD with BDD-style naming)
- **Framework**: Rust built-in `#[cfg(test)]` with BDD naming conventions (`given_X_when_Y_then_Z`)
- **If TDD**: Each task follows RED (failing test) → GREEN (minimal impl) → REFACTOR

### QA Policy
Every task MUST include agent-executed QA scenarios (see TODO template below).
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **CLI binary**: Use Bash — Run binary with various args, assert exit codes + stdout/stderr
- **Library/Module**: Use Bash (`cargo test`) — Import, call functions, compare output
- **Build verification**: Use Bash — `cargo build --release`, `cargo clippy`, `cargo fmt --check`

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately — foundation + scaffolding):
├── Task 1: Project scaffolding + Cargo.toml + test infra [quick]
├── Task 2: AGENTS.md [quick]
├── Task 3: CPU topology detection [deep]
├── Task 4: Embedded binary extraction [unspecified-high]
├── Task 5: mprime config generation [unspecified-high]

Wave 2 (After Wave 1 — core modules):
├── Task 6: mprime process runner + affinity pinning (depends: 3, 4, 5) [deep]
├── Task 7: Error parser for results.txt (depends: 5) [unspecified-high]
├── Task 8: MCE/EDAC journalctl monitor (depends: 3) [deep]
├── Task 9: Signal handling + cleanup (depends: 4) [unspecified-high]

Wave 3 (After Wave 2 — orchestration + reporting):
├── Task 10: Core cycling orchestrator (depends: 6, 7, 8, 9) [deep]
├── Task 11: Result reporting (depends: 7, 8) [unspecified-high]
├── Task 12: CLI interface with argh + main.rs (depends: 10, 11) [deep]

Wave 4 (After Wave 3 — integration):
├── Task 13: Integration testing + build verification (depends: 12) [deep]

Wave FINAL (After ALL tasks — independent review, 4 parallel):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA (unspecified-high)
├── Task F4: Scope fidelity check (deep)

Critical Path: Task 1 → Task 3 → Task 6 → Task 10 → Task 12 → Task 13 → F1-F4
Parallel Speedup: ~55% faster than sequential
Max Concurrent: 5 (Wave 1)
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1 | — | 2-13 | 1 |
| 2 | 1 | — | 1 |
| 3 | 1 | 6, 8, 10 | 1 |
| 4 | 1 | 6, 9 | 1 |
| 5 | 1 | 6, 7 | 1 |
| 6 | 3, 4, 5 | 10 | 2 |
| 7 | 5 | 10, 11 | 2 |
| 8 | 3 | 10, 11 | 2 |
| 9 | 4 | 10 | 2 |
| 10 | 6, 7, 8, 9 | 12 | 3 |
| 11 | 7, 8 | 12 | 3 |
| 12 | 10, 11 | 13 | 3 |
| 13 | 12 | F1-F4 | 4 |
| F1-F4 | 13 | — | FINAL |

### Agent Dispatch Summary

- **Wave 1 (5 tasks)**: T1 → `quick`, T2 → `quick`, T3 → `deep`, T4 → `unspecified-high`, T5 → `unspecified-high`
- **Wave 2 (4 tasks)**: T6 → `deep`, T7 → `unspecified-high`, T8 → `deep`, T9 → `unspecified-high`
- **Wave 3 (3 tasks)**: T10 → `deep`, T11 → `unspecified-high`, T12 → `deep`
- **Wave 4 (1 task)**: T13 → `deep`
- **FINAL (4 tasks)**: F1 → `oracle`, F2 → `unspecified-high`, F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

- [x] 1. Project Scaffolding + Cargo.toml + Test Infrastructure

  **What to do**:
  - Initialize Rust project: `cargo init --name unstable-cpu-detector`
  - Configure `Cargo.toml` with dependencies:
    - `argh` (latest) — CLI argument parsing
    - `anyhow` (1.0) — Error handling
    - `tracing` (0.1) — Structured logging
    - `tracing-subscriber` (0.3) with `env-filter` feature — Log output
    - `core_affinity` (0.8) — CPU pinning
    - `sysinfo` (latest) — System info
  - Add `[profile.release]` with `strip = true`, `lto = true` for smaller binary
  - Create `.gitignore` (target/, *.swp, .sisyphus/evidence/)
  - Create minimal `src/main.rs` with tracing init and anyhow Result
  - Verify `cargo build` and `cargo test` work
  - Run `cargo fmt` and `cargo clippy`

  **Must NOT do**:
  - Do NOT add tokio or any async runtime
  - Do NOT add clap — use argh
  - Do NOT add OpenSSL-dependent crates
  - Do NOT create module directories or mod.rs files

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Straightforward project scaffolding with known dependencies
  - **Skills**: []
    - No special skills needed for cargo init

  **Parallelization**:
  - **Can Run In Parallel**: NO (must be first)
  - **Parallel Group**: Wave 1 — but must complete before others start
  - **Blocks**: Tasks 2, 3, 4, 5 (all subsequent tasks)
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `mprime-rust-task.md` — Full task specification with all constraints (read entirely before starting)

  **External References**:
  - argh crate: https://crates.io/crates/argh — CLI parser (user chose this over clap)
  - tracing crate: https://docs.rs/tracing — Logging framework
  - core_affinity: https://docs.rs/core_affinity — CPU pinning

  **Acceptance Criteria**:
  - [x] `cargo build` succeeds with zero errors
  - [x] `cargo test` runs (even if no tests yet)
  - [x] `cargo clippy -- -D warnings` passes
  - [x] `cargo fmt --check` passes
  - [x] Cargo.toml contains argh, anyhow, tracing, tracing-subscriber, core_affinity, sysinfo
  - [x] Cargo.toml does NOT contain clap, tokio, openssl
  - [x] src/main.rs exists with tracing subscriber init
  - [x] .gitignore exists

  **QA Scenarios:**

  ```
  Scenario: Project builds successfully
    Tool: Bash
    Preconditions: Fresh cargo init completed
    Steps:
      1. Run `cargo build 2>&1`
      2. Assert exit code 0
      3. Run `cargo test 2>&1`
      4. Assert exit code 0
      5. Run `cargo clippy -- -D warnings 2>&1`
      6. Assert exit code 0
    Expected Result: All three commands succeed with exit code 0
    Failure Indicators: Non-zero exit code, compilation errors, clippy warnings
    Evidence: .sisyphus/evidence/task-1-build.txt

  Scenario: No forbidden dependencies
    Tool: Bash
    Preconditions: Cargo.toml written
    Steps:
      1. Run `grep -c 'clap' Cargo.toml` — expect 0
      2. Run `grep -c 'tokio' Cargo.toml` — expect 0
      3. Run `grep -c 'openssl' Cargo.toml` — expect 0
      4. Run `grep -c 'argh' Cargo.toml` — expect 1
    Expected Result: argh present, clap/tokio/openssl absent
    Failure Indicators: clap or tokio found in dependencies
    Evidence: .sisyphus/evidence/task-1-deps.txt
  ```

  **Commit**: YES
  - Message: `chore: scaffold Rust project with dependencies and test infra`
  - Files: `Cargo.toml, src/main.rs, .gitignore`
  - Pre-commit: `cargo build && cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 2. AGENTS.md

  **What to do**:
  - Create `AGENTS.md` in project root
  - Document the project's purpose: Linux CLI tool to detect unstable AMD CPU cores
  - Include ALL constraints from `mprime-rust-task.md` verbatim:
    - Rust only, Linux only, AMD only, 64-bit only
    - argh for CLI, tracing for logging, anyhow for errors
    - No unwrap/expect in non-test code
    - Small focused files WITHOUT Rust modules
    - SOLID, YAGNI
    - TDD with BDD-style tests
    - cargo fmt + cargo clippy after each task
    - No sudo without user permission
    - No OpenSSL — RustTLS only
    - No async runtime
  - Include architecture overview: CPU topology → binary extraction → per-core mprime cycling → error detection → MCE monitoring → report
  - Include key file locations and their purposes
  - Include mprime control details: config file approach, error patterns, working directory isolation

  **Must NOT do**:
  - Do NOT add implementation details that would constrain future changes
  - Do NOT include code snippets — keep it high-level guidance

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Single markdown file creation with known content
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (after Task 1)
  - **Parallel Group**: Wave 1 (with Tasks 3, 4, 5)
  - **Blocks**: None (informational document)
  - **Blocked By**: Task 1 (project must exist)

  **References**:

  **Pattern References**:
  - `mprime-rust-task.md` — Contains ALL constraints to include in AGENTS.md (read entirely)

  **API/Type References**:
  - `mprime-latest/readme.txt` — mprime documentation for reference
  - `mprime-latest/undoc.txt` — Undocumented mprime options (key config settings)

  **Acceptance Criteria**:
  - [x] `AGENTS.md` exists in project root
  - [x] Contains project purpose description
  - [x] Contains ALL key constraints from task spec
  - [x] Contains architecture overview
  - [x] Contains file/directory structure

  **QA Scenarios:**

  ```
  Scenario: AGENTS.md contains required content
    Tool: Bash
    Preconditions: AGENTS.md created
    Steps:
      1. Run `test -f AGENTS.md && echo EXISTS` — expect 'EXISTS'
      2. Run `grep -c 'AMD' AGENTS.md` — expect >= 1
      3. Run `grep -c 'argh' AGENTS.md` — expect >= 1
      4. Run `grep -c 'tracing' AGENTS.md` — expect >= 1
      5. Run `grep -c 'unwrap' AGENTS.md` — expect >= 1 (mentioning the prohibition)
      6. Run `grep -c 'mprime' AGENTS.md` — expect >= 1
    Expected Result: File exists and contains all key terms
    Failure Indicators: File missing or key constraints not documented
    Evidence: .sisyphus/evidence/task-2-agents-md.txt
  ```

  **Commit**: YES
  - Message: `docs: add AGENTS.md for agent guidance`
  - Files: `AGENTS.md`
  - Pre-commit: `test -f AGENTS.md`

---

- [x] 3. AMD CPU Topology Detection

  **What to do**:
  - **RED**: Write BDD tests first in `src/cpu_topology.rs` (`#[cfg(test)]` module):
    - `given_amd_cpu_when_detecting_topology_then_returns_physical_cores`
    - `given_non_amd_cpu_when_validating_then_returns_error`
    - `given_non_64bit_when_validating_then_returns_error`
    - `given_amd_5900x_when_mapping_cores_then_handles_non_contiguous_ids`
    - `given_smt_enabled_when_enumerating_then_returns_first_thread_per_core`
  - **GREEN**: Implement CPU topology detection:
    - Read `/proc/cpuinfo` to detect vendor (must be "AuthenticAMD"), family, model, model name
    - Read `/sys/devices/system/cpu/cpu*/topology/core_id` for physical core IDs
    - Read `/sys/devices/system/cpu/cpu*/topology/thread_siblings_list` for SMT detection
    - Map logical CPU IDs to physical core IDs (handle non-contiguous: 0-5, 8-13 on 5900X)
    - For each physical core, identify the first logical CPU (for affinity pinning)
    - Return struct with: vendor, model_name, physical_core_count, logical_cpu_count, core_map (physical_core_id → logical_cpu_id)
    - Validate: reject non-AMD vendors, reject non-64-bit (check `uname -m` or `/proc/cpuinfo` flags for `lm`)
  - **REFACTOR**: Clean up, ensure proper error handling with anyhow
  - Use `sysinfo` crate for supplementary info (CPU frequency, brand)
  - Use tracing for logging topology detection results

  **Must NOT do**:
  - Do NOT use `unwrap()` or `expect()` — return `anyhow::Result`
  - Do NOT assume contiguous core IDs (AMD Ryzen has gaps: 0-5, 8-13)
  - Do NOT assume core 0 is always the first physical core
  - Do NOT support Intel or non-AMD processors

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Requires understanding of Linux CPU topology sysfs interface, non-trivial parsing, TDD cycle
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `playwright`: Not needed — no browser interaction

  **Parallelization**:
  - **Can Run In Parallel**: YES (after Task 1)
  - **Parallel Group**: Wave 1 (with Tasks 2, 4, 5)
  - **Blocks**: Tasks 6, 8, 10
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `/sys/devices/system/cpu/cpu0/topology/core_id` — Physical core ID (read on this machine: core IDs are 0,1,2,3,4,5,8,9,10,11,12,13)
  - `/sys/devices/system/cpu/cpu0/topology/thread_siblings_list` — SMT sibling mapping
  - `/proc/cpuinfo` — CPU vendor, family, model, model name, flags (look for 'lm' flag for 64-bit)

  **API/Type References**:
  - `sysinfo` crate: `System::physical_core_count()`, `System::cpus()` for supplementary data
  - `core_affinity::get_core_ids()` — Returns available logical core IDs

  **External References**:
  - Linux sysfs CPU topology: https://www.kernel.org/doc/Documentation/cputopology.txt

  **WHY Each Reference Matters**:
  - The sysfs topology files are the authoritative source for core mapping on Linux
  - `/proc/cpuinfo` is needed for AMD vendor validation and 64-bit check (lm flag)
  - The non-contiguous core ID pattern (0-5, 8-13) is AMD-specific and must be handled

  **Acceptance Criteria**:
  - [x] `cargo test` passes all CPU topology tests
  - [x] Correctly identifies AMD vendor from /proc/cpuinfo
  - [x] Correctly maps physical cores to logical CPUs (handles non-contiguous IDs)
  - [x] Rejects non-AMD CPUs with clear error message
  - [x] Returns correct core count for AMD Ryzen 5900X (12 physical cores)
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Detect AMD CPU topology on target machine
    Tool: Bash
    Preconditions: Running on AMD Ryzen 9 5900X machine
    Steps:
      1. Run `cargo test cpu_topology 2>&1`
      2. Assert all tests pass (exit code 0)
      3. Check test output contains 'test result: ok'
    Expected Result: All BDD tests pass, topology correctly detected
    Failure Indicators: Test failures, incorrect core count, wrong vendor detection
    Evidence: .sisyphus/evidence/task-3-cpu-topology-tests.txt

  Scenario: Non-AMD rejection (unit test level)
    Tool: Bash
    Preconditions: Tests written with mock /proc/cpuinfo data for Intel
    Steps:
      1. Run `cargo test non_amd 2>&1`
      2. Assert test passes showing error for non-AMD
    Expected Result: Test verifies non-AMD CPUs are rejected with clear error
    Failure Indicators: Test fails or does not reject Intel CPU data
    Evidence: .sisyphus/evidence/task-3-non-amd-rejection.txt
  ```

  **Commit**: YES
  - Message: `feat(cpu): add AMD CPU topology detection with BDD tests`
  - Files: `src/cpu_topology.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 4. Embedded Binary Extraction (mprime + libgmp.so)

  **What to do**:
  - **RED**: Write BDD tests first in `src/embedded.rs` (`#[cfg(test)]` module):
    - `given_embedded_binaries_when_extracting_then_creates_temp_directory`
    - `given_extracted_mprime_when_checking_permissions_then_is_executable`
    - `given_extracted_libgmp_when_checking_symlinks_then_has_correct_links`
    - `given_temp_dir_when_cleanup_called_then_directory_removed`
  - **GREEN**: Implement binary extraction:
    - Use `include_bytes!` to embed `mprime-latest/mprime` (~27MB) and `mprime-latest/libgmp.so.10.4.1` (~706KB)
    - Create a temp directory under `/tmp/unstable-cpu-detector-XXXXXX/` (use random suffix)
    - Extract mprime binary to temp dir, set executable permission (0o755)
    - Extract libgmp.so.10.4.1, create symlinks: `libgmp.so.10` -> `libgmp.so.10.4.1`, `libgmp.so` -> `libgmp.so.10`
    - Provide function to get the extracted mprime path
    - Provide cleanup function to remove temp directory
    - Return struct `ExtractedBinaries { temp_dir: PathBuf, mprime_path: PathBuf, lib_dir: PathBuf }`
  - **REFACTOR**: Ensure proper error messages if extraction fails (disk full, permissions)
  - Use tracing to log extraction paths and sizes

  **Must NOT do**:
  - Do NOT extract to a fixed path (use random temp dir to avoid conflicts)
  - Do NOT leave temp files on disk after program exits
  - Do NOT use `unwrap()`/`expect()` in non-test code

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: File I/O with symlinks and permissions, binary embedding, needs careful error handling
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (after Task 1)
  - **Parallel Group**: Wave 1 (with Tasks 2, 3, 5)
  - **Blocks**: Tasks 6, 9
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `mprime-latest/mprime` — The 27MB ELF binary to embed (v30.19 build 20)
  - `mprime-latest/libgmp.so.10.4.1` — The 706KB GMP library mprime depends on
  - `mprime-latest/libgmp.so.10` — Symlink to libgmp.so.10.4.1 (must recreate at runtime)

  **API/Type References**:
  - `std::os::unix::fs::PermissionsExt` — For setting executable permissions (mode 0o755)
  - `std::os::unix::fs::symlink` — For creating libgmp symlinks
  - `std::env::temp_dir()` — For getting system temp directory

  **External References**:
  - Rust `include_bytes!` macro: https://doc.rust-lang.org/std/macro.include_bytes.html

  **WHY Each Reference Matters**:
  - mprime dynamically links to libgmp.so.10 — the symlink chain is critical for mprime to find the library
  - Permissions must be set to executable or mprime won't start
  - Random temp dir prevents conflicts if multiple instances run simultaneously

  **Acceptance Criteria**:
  - [x] `cargo test` passes all extraction tests
  - [x] mprime binary extracted with executable permissions (0o755)
  - [x] libgmp.so symlink chain correctly created (libgmp.so -> libgmp.so.10 -> libgmp.so.10.4.1)
  - [x] Cleanup function removes temp directory and all contents
  - [x] Binary sizes match embedded data
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Extract and verify embedded binaries
    Tool: Bash
    Preconditions: Project compiles with include_bytes!
    Steps:
      1. Run `cargo test embedded 2>&1`
      2. Assert all tests pass (exit code 0)
      3. Check test output for 'test result: ok'
    Expected Result: All extraction tests pass
    Failure Indicators: File not found, permission denied, wrong symlinks
    Evidence: .sisyphus/evidence/task-4-extraction-tests.txt

  Scenario: Compilation with large embedded binary
    Tool: Bash
    Preconditions: include_bytes! references mprime-latest/mprime
    Steps:
      1. Run `cargo build --release 2>&1`
      2. Assert exit code 0
      3. Check binary size: `ls -la target/release/unstable-cpu-detector` — should be > 27MB
    Expected Result: Release binary compiles and includes embedded mprime (~28MB+ binary size)
    Failure Indicators: Compilation error, binary too small (mprime not embedded)
    Evidence: .sisyphus/evidence/task-4-release-build.txt
  ```

  **Commit**: YES
  - Message: `feat(embed): add mprime + libgmp.so binary extraction`
  - Files: `src/embedded.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 5. mprime Configuration Generation (prime.txt)

  **What to do**:
  - **RED**: Write BDD tests first in `src/mprime_config.rs` (`#[cfg(test)]` module):
    - `given_sse_mode_when_generating_config_then_disables_avx_flags`
    - `given_avx2_mode_when_generating_config_then_enables_avx_and_avx2`
    - `given_huge_fft_preset_when_generating_then_sets_correct_fft_range`
    - `given_default_config_when_generating_then_disables_primenet`
    - `given_config_when_writing_then_creates_valid_prime_txt`
    - `given_custom_fft_range_when_generating_then_uses_provided_values`
  - **GREEN**: Implement prime.txt generation:
    - Define `StressTestMode` enum: SSE, AVX, AVX2, AVX512, Custom
    - Define `FftPreset` enum: Smallest(4K-21K), Small(36K-248K), Large(426K-8192K), Huge(8960K-MAX), Moderate(1344K-4096K), Heavy(4K-1344K), HeavyShort(4K-160K)
    - Define `MprimeConfig` struct with sensible defaults:
      - mode: SSE (default — highest boost, best for PBO testing)
      - fft_preset: Huge (default — matches CoreCycler)
      - torture_time: 3 (minutes per FFT size within mprime)
      - memory: 0 (in-place FFTs, no RAM allocation)
      - threads: 1 (single thread for maximum boost)
      - error_check: true (enable extra roundoff checks)
      - use_primenet: false (always disabled)
    - Generate prime.txt content as string, write to provided path
    - Key settings to include:
      ```
      V30OptionsConverted=1
      StressTester=1
      UsePrimenet=0
      MinTortureFFT={min}
      MaxTortureFFT={max}
      TortureMem=0
      TortureTime={time}
      CpuSupportsSSE=1
      CpuSupportsSSE2=1
      CpuSupportsAVX={0|1}
      CpuSupportsAVX2={0|1}
      CpuSupportsFMA3={0|1}
      CpuSupportsAVX512F={0|1}
      ErrorCheck=1
      TortureHyperthreading=0
      TortureThreads=1
      ResultsFile=results.txt
      ```
  - **REFACTOR**: Clean up, add tracing for config generation

  **Must NOT do**:
  - Do NOT add unnecessary config options — sensible defaults only
  - Do NOT enable Primenet (UsePrimenet must always be 0)
  - Do NOT use `unwrap()`/`expect()` in non-test code
  - Do NOT create a TOML/YAML config file for the user — this is internal mprime config only

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Requires understanding of mprime config format from research docs
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (after Task 1)
  - **Parallel Group**: Wave 1 (with Tasks 2, 3, 4)
  - **Blocks**: Tasks 6, 7
  - **Blocked By**: Task 1

  **References**:

  **Pattern References**:
  - `mprime-latest/undoc.txt` — **CRITICAL**: Contains all prime.txt config options. Read lines about TortureAlternateInPlace, ErrorCheck, NumCores, Affinity, WorkingDir, ExitWhenOutOfWork, Nice, UseLargePages
  - `mprime-latest/readme.txt` — General mprime config file format and CLI args
  - `mprime-latest/stress.txt` — Torture test presets and FFT size guidance

  **API/Type References**:
  - CoreCycler default.config.ini FFT presets: Smallest(4K-21K), Small(36K-248K), Large(426K-8192K), Huge(8960K-32768K for SSE), Moderate(1344K-4096K), Heavy(4K-1344K), HeavyShort(4K-160K)

  **WHY Each Reference Matters**:
  - undoc.txt is THE definitive source for all prime.txt config keys and their valid values
  - stress.txt explains which FFT sizes stress which CPU components (L1, L2, L3, RAM)
  - CoreCycler presets are the community-validated FFT ranges for stability testing

  **Acceptance Criteria**:
  - [x] `cargo test` passes all config generation tests
  - [x] SSE mode correctly disables AVX/AVX2/AVX512 flags
  - [x] All FFT presets generate correct min/max ranges
  - [x] Generated prime.txt always has UsePrimenet=0
  - [x] Generated prime.txt always has ErrorCheck=1
  - [x] Default config produces valid prime.txt matching CoreCycler SSE+Huge defaults
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Default config generates valid prime.txt
    Tool: Bash
    Preconditions: MprimeConfig::default() implemented
    Steps:
      1. Run `cargo test mprime_config 2>&1`
      2. Assert all tests pass (exit code 0)
      3. Verify test output includes 'given_default_config'
    Expected Result: All BDD tests pass
    Failure Indicators: Wrong FFT ranges, Primenet enabled, missing error check
    Evidence: .sisyphus/evidence/task-5-config-tests.txt

  Scenario: SSE mode disables AVX
    Tool: Bash
    Preconditions: StressTestMode::SSE variant exists
    Steps:
      1. Run `cargo test sse_mode 2>&1`
      2. Assert test verifies CpuSupportsAVX=0 and CpuSupportsAVX2=0 in output
    Expected Result: SSE mode config disables all AVX instruction sets
    Failure Indicators: AVX flags set to 1 in SSE mode
    Evidence: .sisyphus/evidence/task-5-sse-mode.txt
  ```

  **Commit**: YES
  - Message: `feat(config): add mprime prime.txt config generation`
  - Files: `src/mprime_config.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 6. mprime Process Runner + CPU Affinity Pinning

  **What to do**:
  - **RED**: Write BDD tests first in `src/mprime_runner.rs` (`#[cfg(test)]` module):
    - `given_extracted_mprime_when_starting_then_process_spawns_successfully`
    - `given_running_mprime_when_stopping_then_process_terminates`
    - `given_core_id_when_pinning_then_mprime_runs_on_correct_cpu`
    - `given_working_dir_when_starting_then_mprime_uses_isolated_directory`
    - `given_mprime_crash_when_monitoring_then_detects_exit`
  - **GREEN**: Implement mprime process management:
    - Create `MprimeRunner` struct that manages a single mprime process
    - Use `std::process::Command` to spawn mprime with args: `-t` (torture test), `-d` (debug output to stdout)
    - Set `LD_LIBRARY_PATH` env var to point to the extracted libgmp.so directory
    - Use `core_affinity` crate to pin the spawned process to a specific logical CPU
    - Alternative: use `taskset -c {cpu_id}` as the command wrapper if core_affinity can't pin child process
      - Note: `core_affinity::set_for_current()` sets affinity for current thread, but we need child process affinity
      - Best approach: spawn mprime via `taskset -c {logical_cpu_id} /path/to/mprime -t -d`
    - Each core test gets its own working directory (copy prime.txt there, read results.txt from there)
    - Use `-W {working_dir}` mprime flag to set working directory
    - Capture stdout/stderr for logging
    - Provide `start(core_id, working_dir)`, `stop()`, `is_running()`, `wait_for(duration)` methods
    - On `stop()`: send SIGTERM, wait 5s, then SIGKILL if still alive
  - **REFACTOR**: Clean up error handling, add tracing spans per core

  **Must NOT do**:
  - Do NOT use async/tokio — use std::thread and std::process
  - Do NOT interact with mprime's TUI — use config files only
  - Do NOT run mprime without LD_LIBRARY_PATH set (it will fail to find libgmp.so)
  - Do NOT pin affinity without verifying the logical CPU ID exists
  - Do NOT use `unwrap()`/`expect()` in non-test code

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Process management with affinity pinning, signal handling, working directory isolation — complex systems programming
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 7, 8, 9 in Wave 2)
  - **Parallel Group**: Wave 2
  - **Blocks**: Task 10
  - **Blocked By**: Tasks 3, 4, 5

  **References**:

  **Pattern References**:
  - `mprime-latest/readme.txt:1-50` — mprime CLI arguments: `-t` (torture), `-W dir` (working dir), `-d` (debug stdout)
  - `mprime-latest/undoc.txt` — Search for `Affinity`, `WorkingDir`, `ExitWhenOutOfWork` settings

  **API/Type References**:
  - `core_affinity::set_for_current(CoreId)` — Pins current thread to core (for parent thread approach)
  - `std::process::Command::new("taskset").args(["-c", &cpu_id.to_string(), mprime_path])` — For child process affinity
  - `std::process::Child` — Process handle for monitoring and killing
  - `nix::sys::signal::kill(Pid, Signal::SIGTERM)` — Graceful termination via nix crate
  - Task 3 output: `CpuTopology.core_map` — Maps physical core ID to logical CPU ID for pinning
  - Task 4 output: `ExtractedBinaries.mprime_path` — Path to extracted mprime binary
  - Task 5 output: `MprimeConfig` — Generates prime.txt to write into working dir

  **External References**:
  - Linux taskset command: `man taskset` — For pinning child processes to specific CPUs
  - `LD_LIBRARY_PATH`: Required for mprime to find libgmp.so at runtime

  **WHY Each Reference Matters**:
  - mprime's `-W` flag is essential for isolating per-core working directories (each gets own results.txt)
  - `taskset` is the most reliable way to pin a child process on Linux (simpler than sched_setaffinity on child PID)
  - LD_LIBRARY_PATH must point to the directory containing libgmp.so.10 symlink

  **Acceptance Criteria**:
  - [x] `cargo test` passes all runner tests
  - [x] mprime spawns successfully with torture test mode
  - [x] mprime is pinned to the correct CPU core (verify via /proc/{pid}/status Cpus_allowed)
  - [x] Each core test uses isolated working directory
  - [x] Stop terminates mprime cleanly (SIGTERM then SIGKILL)
  - [x] LD_LIBRARY_PATH correctly set for libgmp.so
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: mprime starts and runs on specified core
    Tool: Bash
    Preconditions: mprime binary extracted, prime.txt generated
    Steps:
      1. Run `cargo test mprime_runner 2>&1`
      2. Assert all tests pass
    Expected Result: All runner tests pass including process spawn and affinity
    Failure Indicators: Process spawn failure, wrong CPU affinity, missing libgmp.so
    Evidence: .sisyphus/evidence/task-6-runner-tests.txt

  Scenario: mprime process terminates cleanly
    Tool: Bash
    Preconditions: mprime running as child process
    Steps:
      1. Run `cargo test stop 2>&1`
      2. Assert test verifies process is no longer running after stop()
    Expected Result: Process exits cleanly after SIGTERM
    Failure Indicators: Zombie process, SIGKILL needed, process still alive
    Evidence: .sisyphus/evidence/task-6-clean-stop.txt
  ```

  **Commit**: YES
  - Message: `feat(runner): add mprime process spawning with CPU affinity`
  - Files: `src/mprime_runner.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 7. Error Parser for mprime results.txt

  **What to do**:
  - **RED**: Write BDD tests first in `src/error_parser.rs` (`#[cfg(test)]` module):
    - `given_roundoff_error_line_when_parsing_then_detects_hardware_error`
    - `given_fatal_error_line_when_parsing_then_detects_fatal`
    - `given_hardware_failure_line_when_parsing_then_extracts_fft_size`
    - `given_self_test_passed_line_when_parsing_then_tracks_progress`
    - `given_clean_results_when_parsing_then_returns_no_errors`
    - `given_illegal_sumout_when_parsing_then_detects_error`
    - `given_incremental_read_when_new_lines_added_then_only_parses_new`
  - **GREEN**: Implement results.txt parser:
    - Define `MprimeError` struct: `{ error_type: MprimeErrorType, message: String, fft_size: Option<u32>, timestamp: Option<String> }`
    - Define `MprimeErrorType` enum: `RoundoffError`, `HardwareFailure`, `FatalError`, `IllegalSumout`, `SumMismatch`, `Unknown`
    - Error patterns to detect (regex):
      - `ROUND OFF > 0\.4` or `Rounding was .*, expected less than 0.4`
      - `Hardware failure detected running (\d+)K FFT size`
      - `FATAL ERROR`
      - `Possible hardware failure`
      - `ILLEGAL SUMOUT`
      - `SUM\(INPUTS\) != SUM\(OUTPUTS\)`
    - Progress patterns to track:
      - `Self-test (\d+)K passed` — track last passed FFT size
    - Implement incremental parsing: track file position (byte offset), only read new bytes
    - Return `Vec<MprimeError>` for each check cycle
    - Also parse `-d` stdout output for real-time error detection
  - **REFACTOR**: Compile regex patterns once (lazy_static or OnceLock)

  **Must NOT do**:
  - Do NOT re-read entire results.txt each time — use incremental byte offset
  - Do NOT use `unwrap()`/`expect()` in non-test code
  - Do NOT ignore any of the known error patterns listed above

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Regex pattern matching + incremental file parsing, moderate complexity
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 6, 8, 9 in Wave 2)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 10, 11
  - **Blocked By**: Task 5 (needs to know what results.txt format looks like)

  **References**:

  **Pattern References**:
  - `mprime-latest/readme.txt` — Describes results.txt format and error messages
  - `mprime-latest/stress.txt` — "If the program detects a problem, you'll see an error message" section lists error patterns
  - `mprime-latest/undoc.txt` — Search for `ErrorCheck`, `ErrorCountMessages`, `OutputRoundoff` settings

  **API/Type References**:
  - `std::io::Seek` + `std::io::Read` — For incremental file reading from byte offset
  - `std::sync::OnceLock` — For lazy-initialized compiled regex patterns (Rust 1.70+, no external crate needed)

  **WHY Each Reference Matters**:
  - stress.txt explicitly lists the error messages mprime produces — these are the exact strings to match
  - undoc.txt explains ErrorCheck=1 which enables extra roundoff checking per iteration
  - Incremental reading is critical for performance — results.txt grows continuously during testing

  **Acceptance Criteria**:
  - [x] `cargo test` passes all parser tests
  - [x] Detects all 6 error patterns: roundoff, hardware failure, fatal, possible hardware failure, illegal sumout, sum mismatch
  - [x] Correctly extracts FFT size from error messages when available
  - [x] Incremental parsing only reads new lines (byte offset tracked)
  - [x] Progress tracking identifies last passed FFT size
  - [x] Clean results.txt returns empty error vec
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Detect all error patterns
    Tool: Bash
    Preconditions: Tests with sample error lines from mprime docs
    Steps:
      1. Run `cargo test error_parser 2>&1`
      2. Assert all tests pass
      3. Verify test output includes all BDD test names
    Expected Result: All 7+ BDD tests pass, every error pattern detected
    Failure Indicators: Missed error pattern, wrong error type classification
    Evidence: .sisyphus/evidence/task-7-parser-tests.txt

  Scenario: Incremental parsing works correctly
    Tool: Bash
    Preconditions: Test that writes to temp file in two phases
    Steps:
      1. Run `cargo test incremental 2>&1`
      2. Assert test verifies second parse only reads new content
    Expected Result: Second parse call returns only errors from new content
    Failure Indicators: Duplicate errors reported, entire file re-read
    Evidence: .sisyphus/evidence/task-7-incremental.txt
  ```

  **Commit**: YES
  - Message: `feat(parser): add mprime error pattern detection`
  - Files: `src/error_parser.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 8. MCE/EDAC Journalctl Monitor

  **What to do**:
  - **RED**: Write BDD tests first in `src/mce_monitor.rs` (`#[cfg(test)]` module):
    - `given_mce_log_line_when_parsing_then_extracts_cpu_and_bank`
    - `given_hardware_error_line_when_parsing_then_detects_error`
    - `given_edac_correctable_line_when_parsing_then_extracts_details`
    - `given_apic_id_when_mapping_then_resolves_to_physical_core`
    - `given_clean_journal_when_monitoring_then_returns_no_errors`
    - `given_multiple_errors_when_monitoring_then_aggregates_by_core`
  - **GREEN**: Implement journalctl MCE/EDAC monitor:
    - Define `MceError` struct: `{ cpu_id: u32, bank: Option<u32>, error_type: MceErrorType, message: String, timestamp: String, apic_id: Option<u32> }`
    - Define `MceErrorType` enum: `MachineCheck`, `HardwareError`, `EdacCorrectable`, `EdacUncorrectable`, `Unknown`
    - Parse journalctl output lines matching these patterns:
      - `mce: [Hardware Error]: CPU (\d+): Machine Check: (\d+) Bank (\d+): (.+)`
      - `mce: \[Hardware Error\].*` — generic hardware error
      - `EDAC.*CE.*` — correctable EDAC error
      - `EDAC.*UE.*` — uncorrectable EDAC error
    - Implement `MceMonitor` struct with two modes:
      - **Poll mode**: Run `journalctl -k -b --since "{timestamp}" --no-pager` periodically (every 5s)
      - **Streaming mode**: Spawn `journalctl -k -f --grep='mce|hardware error|edac'` as child process, read stdout line-by-line
      - Use poll mode as primary (simpler, more testable), streaming mode as enhancement
    - APIC ID to physical core mapping: parse `/proc/cpuinfo` for `processor` and `apicid` fields
      - Build HashMap<u32, u32> mapping APIC ID → logical CPU ID
      - Use Task 3's CpuTopology to then map logical CPU ID → physical core ID
    - Thread-safe: `MceMonitor` runs in its own `std::thread`, communicates via `Arc<Mutex<Vec<MceError>>>`
    - Provide `start()`, `stop()`, `get_errors()`, `get_errors_for_core(core_id)` methods
    - **IMPORTANT**: `journalctl` may require user-level access. If it fails with permission error, log a warning and continue without MCE monitoring (graceful degradation). Do NOT use sudo.
  - **REFACTOR**: Compile regex patterns once (OnceLock), clean up thread lifecycle

  **Must NOT do**:
  - Do NOT use sudo — if journalctl requires elevated privileges, warn and skip MCE monitoring
  - Do NOT use async/tokio — use std::thread for background monitoring
  - Do NOT block the main thread — monitor runs in background
  - Do NOT use `unwrap()`/`expect()` in non-test code
  - Do NOT panic if journalctl is unavailable — degrade gracefully

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Concurrent thread with regex parsing, APIC mapping, process management — complex systems programming
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 6, 7, 9 in Wave 2)
  - **Parallel Group**: Wave 2
  - **Blocks**: Tasks 10, 11
  - **Blocked By**: Task 3 (needs CpuTopology for APIC → core mapping)

  **References**:

  **Pattern References**:
  - `mprime-rust-task.md:42-44` — journalctl command for MCE/EDAC detection
  - Machine's actual MCE output: `mce: [Hardware Error]: CPU 0: Machine Check Exception: 5 Bank 27: bea0000001000108` — real pattern to parse

  **API/Type References**:
  - `std::process::Command::new("journalctl").args(["-k", "-b", "--since", &since, "--no-pager"])` — Poll mode command
  - `std::sync::Arc<Mutex<Vec<MceError>>>` — Thread-safe error collection
  - Task 3 output: `CpuTopology` — Maps logical CPU → physical core ID for APIC mapping
  - `/proc/cpuinfo` fields: `processor` (logical ID), `apicid` (APIC ID) — For building APIC mapping

  **External References**:
  - Linux kernel MCE documentation: https://www.kernel.org/doc/html/latest/x86/x86_64/machinecheck.html
  - EDAC kernel module: https://www.kernel.org/doc/html/latest/driver-api/edac.html
  - journalctl man page: `man journalctl` — For --since, -k, -f, --grep flags

  **WHY Each Reference Matters**:
  - The machine already has real MCE errors (CPU:0, Bank 27, L3/GEN) — use this as test validation data
  - APIC ID mapping is non-trivial on AMD: APIC IDs are NOT sequential and don't match logical CPU IDs
  - journalctl permission varies by distro — graceful degradation is essential

  **Acceptance Criteria**:
  - [x] `cargo test` passes all MCE monitor tests
  - [x] Parses all 4 error patterns: machine check, hardware error, EDAC correctable, EDAC uncorrectable
  - [x] APIC ID → physical core mapping works correctly for non-contiguous AMD core IDs
  - [x] Background thread starts and stops cleanly
  - [x] Errors are aggregated by core ID
  - [x] Graceful degradation if journalctl is unavailable or permission denied
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Parse MCE error patterns
    Tool: Bash
    Preconditions: Test with sample journalctl output lines
    Steps:
      1. Run `cargo test mce_monitor 2>&1`
      2. Assert all tests pass
      3. Verify test output includes all BDD test names
    Expected Result: All 6+ BDD tests pass, every MCE pattern parsed correctly
    Failure Indicators: Missed error pattern, wrong CPU ID extraction, APIC mapping failure
    Evidence: .sisyphus/evidence/task-8-mce-tests.txt

  Scenario: Graceful degradation without journalctl access
    Tool: Bash
    Preconditions: Test simulates journalctl permission error
    Steps:
      1. Run `cargo test journalctl_unavailable 2>&1`
      2. Assert test verifies monitor returns Ok with warning, not error
    Expected Result: Monitor logs warning and continues without MCE data
    Failure Indicators: Panic, unwrap failure, hard error on missing journalctl
    Evidence: .sisyphus/evidence/task-8-graceful-degrade.txt
  ```

  **Commit**: YES
  - Message: `feat(mce): add journalctl MCE/EDAC monitoring`
  - Files: `src/mce_monitor.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 9. Signal Handling + Cleanup

  **What to do**:
  - **RED**: Write BDD tests first in `src/signal_handler.rs` (`#[cfg(test)]` module):
    - `given_running_state_when_ctrl_c_then_shutdown_flag_is_set`
    - `given_shutdown_flag_when_checked_then_returns_true`
    - `given_temp_dir_when_cleanup_then_all_files_removed`
    - `given_partial_results_when_shutdown_then_results_preserved`
  - **GREEN**: Implement signal handling and cleanup:
    - Use `ctrlc` crate (lightweight, well-maintained) to register Ctrl+C handler
    - Set an `AtomicBool` flag (`SHUTDOWN_REQUESTED`) on signal receipt
    - Provide `is_shutdown_requested() -> bool` function for main loop polling
    - Implement `Cleanup` struct that tracks resources to clean up:
      - Temporary directories (where mprime binaries and working dirs are extracted)
      - Child process PIDs (mprime processes that need SIGTERM/SIGKILL)
      - MCE monitor thread (needs graceful stop signal)
    - `Cleanup::execute()` method:
      1. Send SIGTERM to all tracked child processes
      2. Wait up to 5 seconds for each
      3. SIGKILL any remaining
      4. Stop MCE monitor thread
      5. Remove temporary directories (if cleanup enabled — keep on error for debugging)
      6. **Print partial results** if any cores were tested before interruption
    - Register cleanup resources as they are created during the test run
    - Thread-safe: use `Arc<Mutex<Cleanup>>` so signal handler and main thread can both access
  - **REFACTOR**: Ensure cleanup is idempotent (safe to call multiple times)

  **Must NOT do**:
  - Do NOT use raw `nix` signal handlers for Ctrl+C — use `ctrlc` crate for registration; use `nix` only for `kill(pid, SIGTERM/SIGKILL)` on child processes
  - Do NOT install signal handlers for SIGKILL (can't be caught) or SIGSEGV
  - Do NOT leave mprime zombie processes — always wait() after kill
  - Do NOT delete temp dirs if there were errors (preserve for debugging)
  - Do NOT use `unwrap()`/`expect()` in non-test code

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Signal handling with process cleanup, atomic operations — moderate-high systems programming complexity
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Tasks 6, 7, 8 in Wave 2)
  - **Parallel Group**: Wave 2
  - **Blocks**: Task 10
  - **Blocked By**: Task 4 (needs to know temp dir structure from embedded extraction)

  **References**:

  **Pattern References**:
  - `ctrlc` crate usage: `ctrlc::set_handler(move || { flag.store(true, Ordering::SeqCst); })` — Standard pattern

  **API/Type References**:
  - `ctrlc::set_handler(Fn)` — Register Ctrl+C handler (called once at startup)
  - `std::sync::atomic::AtomicBool` — Lock-free shutdown flag
  - `std::sync::atomic::Ordering::SeqCst` — Memory ordering for flag
  - `std::process::Child::kill()` — Send SIGKILL to child process
  - `nix::sys::signal::kill(Pid, Signal::SIGTERM)` — Graceful SIGTERM before SIGKILL
  - `nix::unistd::Pid::from_raw(child.id() as i32)` — Convert child PID for nix API
  - Task 4 output: temp directory path where mprime is extracted
  - Task 6 output: `MprimeRunner` — Has child process PID to track

  **External References**:
  - `ctrlc` crate: https://crates.io/crates/ctrlc — Cross-platform Ctrl+C handling (but we're Linux-only so simpler)

  **WHY Each Reference Matters**:
  - `ctrlc` crate is the de facto standard for Ctrl+C in Rust — no need to use raw signal handling
  - AtomicBool is preferred over Mutex<bool> for simple flag — no lock contention in signal handler
  - SIGTERM before SIGKILL is the Linux convention — gives mprime time to flush results.txt

  **Acceptance Criteria**:
  - [x] `cargo test` passes all signal handler tests
  - [x] Ctrl+C sets shutdown flag (AtomicBool)
  - [x] `is_shutdown_requested()` returns true after signal
  - [x] Cleanup kills child processes (SIGTERM then SIGKILL after 5s)
  - [x] Temporary directories removed on clean exit
  - [x] Temp dirs preserved when errors detected (for debugging)
  - [x] Partial results printed if interrupted mid-test
  - [x] No zombie processes after cleanup
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Shutdown flag is set on signal
    Tool: Bash
    Preconditions: Signal handler registered
    Steps:
      1. Run `cargo test signal_handler 2>&1`
      2. Assert all tests pass
      3. Verify test output includes all BDD test names
    Expected Result: All 4 BDD tests pass, shutdown flag works correctly
    Failure Indicators: Flag not set, race condition, panic in handler
    Evidence: .sisyphus/evidence/task-9-signal-tests.txt

  Scenario: Cleanup removes temp directories
    Tool: Bash
    Preconditions: Temp directory exists with test files
    Steps:
      1. Run `cargo test cleanup 2>&1`
      2. Assert test verifies temp dir is removed after cleanup
    Expected Result: Temp directory deleted, no leftover files
    Failure Indicators: Directory still exists, permission error
    Evidence: .sisyphus/evidence/task-9-cleanup.txt
  ```

  **Commit**: YES
  - Message: `feat(signal): add Ctrl+C handling and cleanup`
  - Files: `src/signal_handler.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 10. Core Cycling Orchestrator

  **What to do**:
  - **RED**: Write BDD tests first in `src/coordinator.rs` (`#[cfg(test)]` module):
    - `given_core_list_when_starting_cycle_then_tests_each_core_sequentially`
    - `given_duration_per_core_when_testing_then_runs_for_specified_time`
    - `given_error_detected_when_testing_core_then_marks_core_as_failed`
    - `given_shutdown_signal_when_mid_cycle_then_stops_gracefully`
    - `given_all_cores_tested_when_complete_then_returns_full_results`
    - `given_iteration_count_when_configured_then_repeats_full_cycle`
    - `given_core_failure_during_test_when_monitoring_then_captures_error_details`
    - `given_core_filter_when_subset_specified_then_only_tests_those_cores`
  - **GREEN**: Implement core cycling orchestrator:
    - Define `CoreTestResult` struct: `{ core_id: u32, logical_cpu_ids: Vec<u32>, status: CoreStatus, mprime_errors: Vec<MprimeError>, mce_errors: Vec<MceError>, duration_tested: Duration, iterations_completed: u32 }`
    - Define `CoreStatus` enum: `Passed`, `Failed`, `Skipped`, `Interrupted`
    - Define `CycleResults` struct: `{ results: Vec<CoreTestResult>, total_duration: Duration, iterations_completed: u32, interrupted: bool }`
    - Implement `Coordinator` struct with main orchestration loop:
      1. Get core list from `CpuTopology` (Task 3)
      1b. If optional `core_filter: Option<Vec<u32>>` is set, filter the core list to only those physical core IDs (skip all others, mark them `Skipped`)
      2. Extract mprime binary (Task 4)
      3. Start MCE monitor thread (Task 8)
      4. For each iteration (default: 3 iterations through all cores):
         a. For each physical core (respecting optional core filter):
            - Generate prime.txt config (Task 5)
            - Create isolated working directory
            - Pin mprime to first logical CPU of this core via taskset (Task 6)
            - Run mprime for `duration_per_core` (default: 6 minutes)
            - Poll `is_shutdown_requested()` every 1 second during test (Task 9)
            - Poll results.txt for errors every 5 seconds (Task 7)
            - Poll MCE monitor for new errors every 5 seconds (Task 8)
            - On error: mark core as failed, log, continue to next core
            - On timeout (duration): mark core as passed for this iteration
            - On shutdown signal: mark core as interrupted, break loop
         b. After each full iteration: log summary of iteration results
      5. Stop MCE monitor thread
      6. Collect all results into `CycleResults`
    - Use `std::thread::sleep(Duration::from_secs(1))` for polling intervals
    - Log with tracing: span per iteration, span per core test
  - **REFACTOR**: Extract polling logic into helper functions, ensure clean shutdown path

  **Must NOT do**:
  - Do NOT test multiple cores simultaneously — one core at a time (sequential per iteration)
  - Do NOT use async/tokio — use std::thread for MCE monitor, main thread for orchestration
  - Do NOT skip cores that failed in previous iterations — retest them
  - Do NOT continue if mprime binary extraction fails — that's a fatal error
  - Do NOT hardcode iteration count or duration — take from config/defaults
  - Do NOT use `unwrap()`/`expect()` in non-test code

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Central orchestration loop integrating 6 other modules, complex state management, polling loops, graceful shutdown — most complex task
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 11 in Wave 3, but depends on different upstream tasks)
  - **Parallel Group**: Wave 3
  - **Blocks**: Task 12
  - **Blocked By**: Tasks 6, 7, 8, 9 (needs runner, parser, MCE monitor, signal handler)

  **References**:

  **Pattern References**:
  - CoreCycler's cycling approach: Sequentially test each core, configurable time per core, multiple iterations
  - CoreCycler defaults: SSE mode, 6 min/core, 1 thread, 3 iterations (we match these)

  **API/Type References**:
  - Task 3 output: `CpuTopology` — `get_physical_cores() -> Vec<PhysicalCore>` with logical CPU IDs per core
  - Task 4 output: `ExtractedBinaries` — Paths to extracted mprime and libgmp.so
  - Task 5 output: `MprimeConfig` — `generate_prime_txt(mode, fft_range) -> String`
  - Task 6 output: `MprimeRunner` — `start(core_id, working_dir)`, `stop()`, `is_running()`, `wait_for(duration)`
  - Task 7 output: `ErrorParser` — `parse_results(path) -> Vec<MprimeError>`, incremental reading
  - Task 8 output: `MceMonitor` — `start()`, `stop()`, `get_errors_for_core(core_id) -> Vec<MceError>`
  - Task 9 output: `is_shutdown_requested() -> bool`, `Cleanup` struct

  **WHY Each Reference Matters**:
  - This task is the integration hub — it calls every other module's API, so type contracts must match exactly
  - CoreCycler's sequential approach is deliberate: simultaneous core testing masks which core is unstable
  - Polling interval of 5s for error checking balances responsiveness vs. I/O overhead

  **Acceptance Criteria**:
  - [x] `cargo test` passes all coordinator tests
  - [x] Cycles through all physical cores sequentially
  - [x] Runs mprime for specified duration per core
  - [x] Detects and records mprime errors per core
  - [x] Detects and records MCE errors per core
  - [x] Responds to Ctrl+C within 1 second (polling interval)
  - [x] Supports configurable iteration count and duration
  - [x] Produces complete CycleResults after all iterations
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Orchestrator cycles through cores
    Tool: Bash
    Preconditions: All upstream modules compiled and tested
    Steps:
      1. Run `cargo test coordinator 2>&1`
      2. Assert all tests pass
      3. Verify test output includes all BDD test names
    Expected Result: All 7 BDD tests pass, orchestration logic works correctly
    Failure Indicators: Core skipped, wrong duration, error not captured, shutdown not responsive
    Evidence: .sisyphus/evidence/task-10-coordinator-tests.txt

  Scenario: Graceful shutdown mid-cycle
    Tool: Bash
    Preconditions: Coordinator running with mock components
    Steps:
      1. Run `cargo test shutdown_signal 2>&1`
      2. Assert test verifies shutdown stops current core test and preserves results
    Expected Result: Partial results collected, no resource leaks
    Failure Indicators: Results lost, process leaked, timeout waiting for shutdown
    Evidence: .sisyphus/evidence/task-10-shutdown.txt
  ```

  **Commit**: YES
  - Message: `feat(core): add per-core stress test orchestration`
  - Files: `src/coordinator.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 11. Stability Report Generation

  **What to do**:
  - **RED**: Write BDD tests first in `src/report.rs` (`#[cfg(test)]` module):
    - `given_all_cores_passed_when_reporting_then_shows_stable_summary`
    - `given_one_core_failed_when_reporting_then_highlights_unstable_core`
    - `given_mce_errors_when_reporting_then_includes_hardware_error_details`
    - `given_partial_results_when_reporting_then_shows_interrupted_status`
    - `given_multiple_iterations_when_reporting_then_shows_per_iteration_results`
    - `given_empty_results_when_reporting_then_shows_no_data_message`
  - **GREEN**: Implement stability report generation:
    - Define `StabilityReport` struct that takes `CycleResults` from Task 10
    - Generate a clear, terminal-friendly report using ANSI colors (via `tracing` or raw ANSI codes):
      ```
      ╔══════════════════════════════════════════════════════════╗
      ║           CPU Stability Report - AMD Ryzen 9 5900X      ║
      ╠══════════════════════════════════════════════════════════╣
      ║ Core  0 (CPU  0, 12): ✓ STABLE  (3/3 iterations)       ║
      ║ Core  1 (CPU  1, 13): ✗ UNSTABLE (failed iteration 2)  ║
      ║   └─ mprime: ROUNDOFF > 0.40 at 1344K FFT              ║
      ║   └─ MCE: Bank 5, L3/GEN corrected error               ║
      ║ Core  2 (CPU  2, 14): ✓ STABLE  (3/3 iterations)       ║
      ║ ...                                                     ║
      ╠══════════════════════════════════════════════════════════╣
      ║ Summary: 11/12 cores stable, 1 unstable                 ║
      ║ Duration: 3h 36m | Iterations: 3                        ║
      ║ MCE Errors: 2 corrected, 0 uncorrected                  ║
      ╚══════════════════════════════════════════════════════════╝
      ```
    - Report sections:
      1. **Header**: CPU model, total cores, test date
      2. **Per-core results**: Status (stable/unstable/interrupted), logical CPUs, error details
      3. **Error details per failed core**: mprime errors (type, FFT size) + MCE errors (bank, type)
      4. **Summary**: Total stable/unstable, test duration, iteration count, MCE summary
      5. **Recommendation** (if unstable cores found): "Consider adjusting PBO/CO settings for cores: X, Y"
    - Also generate a simple machine-readable summary line for scripting:
      - `RESULT: STABLE` or `RESULT: UNSTABLE cores=1,5,8`
    - Support `--quiet` mode: only print the RESULT line
    - Support saving report to file (optional, default: stdout only)
  - **REFACTOR**: Extract formatting helpers, ensure Unicode box-drawing works in all terminals

  **Must NOT do**:
  - Do NOT use any external TUI crate — raw ANSI codes + Unicode box-drawing characters are sufficient
  - Do NOT add color as hard dependency — detect if stdout is a TTY, skip ANSI codes if piped
  - Do NOT abbreviate error details — show every error found
  - Do NOT use `unwrap()`/`expect()` in non-test code

  **Recommended Agent Profile**:
  - **Category**: `unspecified-high`
    - Reason: Structured text output with formatting, ANSI colors, TTY detection — moderate complexity
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: YES (with Task 10 in Wave 3 — different upstream deps)
  - **Parallel Group**: Wave 3
  - **Blocks**: Task 12
  - **Blocked By**: Tasks 7, 8 (needs error types from parser and MCE monitor for report formatting)

  **References**:

  **Pattern References**:
  - CoreCycler's summary output: "Core X is unstable" messaging pattern
  - Linux standard: Check `isatty(1)` equivalent in Rust: `std::io::stdout().is_terminal()` (Rust 1.70+)

  **API/Type References**:
  - `std::io::IsTerminal` — TTY detection for ANSI color output (Rust 1.70+, stdlib, no external crate)
  - Task 10 output: `CycleResults` — Full test results to format
  - Task 7 types: `MprimeError`, `MprimeErrorType` — For error detail formatting
  - Task 8 types: `MceError`, `MceErrorType` — For MCE error formatting
  - Task 3 types: `CpuTopology` — For CPU model name in header

  **External References**:
  - ANSI escape codes: `\x1b[31m` (red), `\x1b[32m` (green), `\x1b[0m` (reset)
  - Unicode box-drawing: `\u2550` (═), `\u2554` (╔), `\u2557` (╗) etc.

  **WHY Each Reference Matters**:
  - TTY detection is critical: ANSI codes in piped output break scripting
  - Machine-readable RESULT line enables automation (other tools can parse it)
  - Box-drawing characters make the report visually clear at a glance

  **Acceptance Criteria**:
  - [x] `cargo test` passes all report tests
  - [x] Report shows per-core status (stable/unstable/interrupted)
  - [x] Error details included for each failed core (mprime + MCE)
  - [x] Summary line includes stable/unstable count and duration
  - [x] Machine-readable RESULT line for scripting
  - [x] ANSI colors only when stdout is TTY
  - [x] Recommendation shown when unstable cores found
  - [x] Empty results handled gracefully
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: Report generation with mixed results
    Tool: Bash
    Preconditions: Test with mock CycleResults containing passed and failed cores
    Steps:
      1. Run `cargo test report 2>&1`
      2. Assert all tests pass
      3. Verify test output includes all BDD test names
    Expected Result: All 6 BDD tests pass, report formatting correct
    Failure Indicators: Missing core in report, wrong status, missing error details
    Evidence: .sisyphus/evidence/task-11-report-tests.txt

  Scenario: Machine-readable output line
    Tool: Bash
    Preconditions: Test with unstable cores in results
    Steps:
      1. Run `cargo test result_line 2>&1`
      2. Assert test verifies output contains 'RESULT: UNSTABLE cores=' with correct core IDs
    Expected Result: RESULT line matches expected format for scripting
    Failure Indicators: Wrong format, missing core IDs, ANSI codes in non-TTY mode
    Evidence: .sisyphus/evidence/task-11-result-line.txt
  ```

  **Commit**: YES
  - Message: `feat(report): add stability report generation`
  - Files: `src/report.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 12. CLI Interface with argh + main.rs

  **What to do**:
  - **RED**: Write BDD tests first in `src/cli.rs` (`#[cfg(test)]` module):
    - `given_no_args_when_running_then_uses_sensible_defaults`
    - `given_help_flag_when_running_then_shows_usage`
    - `given_duration_arg_when_parsing_then_sets_per_core_duration`
    - `given_iterations_arg_when_parsing_then_sets_iteration_count`
    - `given_quiet_flag_when_parsing_then_enables_machine_readable_output`
    - `given_cores_arg_when_parsing_then_filters_to_specified_cores`
    - `given_invalid_core_id_when_parsing_then_exits_with_error`
    - `given_non_amd_cpu_when_starting_then_exits_with_error`
    - `given_non_64bit_when_starting_then_exits_with_error`
  - **GREEN**: Implement CLI and main entry point:
    - Define CLI args struct using `argh::FromArgs`:
      ```rust
      /// Detect unstable CPU cores on AMD Linux systems using mprime stress testing
      #[derive(FromArgs)]
      struct Args {
          /// minutes to test each core (default: 6)
          #[argh(option, short = 'd', default = "6")]
          duration: u32,
          /// number of full cycles through all cores (default: 3)
          #[argh(option, short = 'i', default = "3")]
          iterations: u32,
          /// only test specific cores (comma-separated core IDs, e.g. "0,2,5")
          #[argh(option, short = 'c')]
          cores: Option<String>,
          /// only output machine-readable RESULT line
          #[argh(switch, short = 'q')]
          quiet: bool,
          /// stress test mode: sse, avx, avx2 (default: sse)
          #[argh(option, short = 'm', default = "String::from(\"sse\")")]
          mode: String,
      }
      ```
    - Minimal options with sensible defaults (matching CoreCycler):
      - `--duration 6` (6 minutes per core, CoreCycler default)
      - `--iterations 3` (3 full cycles through all cores)
      - `--cores 0,2,5` (optional: only test specific physical core IDs; omit to test all)
      - `--quiet` (machine-readable output only)
      - `--mode sse` (SSE = highest boost = best instability detection)
    - In `main.rs`:
      1. Parse args with `argh::from_env()`
      1b. If `--cores` provided, parse comma-separated core IDs into `Vec<u32>`, validate each exists in detected topology — exit with error listing available cores if invalid
      2. Initialize `tracing_subscriber` with appropriate level (info default, debug if needed)
      3. **Pre-flight checks** (exit with clear error if any fail):
         a. Verify running on Linux (`cfg!(target_os = "linux")` — compile-time, but also runtime uname check)
         b. Verify AMD CPU: check `/proc/cpuinfo` for `vendor_id` containing `AuthenticAMD`
         c. Verify 64-bit: check `std::mem::size_of::<usize>() == 8` (compile-time for 64-bit target)
         d. Verify not running as root: `nix::unistd::getuid().is_root()` → warn (not error) if root
         e. Check if mprime binary can be extracted (disk space, write permissions to temp dir)
      4. Print startup banner: CPU model, core count, test config
      5. Create `Coordinator` (Task 10) with parsed config (including optional core filter)
      6. Register signal handler (Task 9)
      7. Run coordinator loop
      8. Generate and print report (Task 11)
      9. Run cleanup (Task 9)
      10. Exit with code: 0 = all stable, 1 = unstable cores found, 2 = error
    - All `src/*.rs` files must be declared in `main.rs` as flat file includes (no mod.rs, no nested modules)
  - **REFACTOR**: Clean up main.rs, ensure all error paths produce actionable messages

  **Must NOT do**:
  - Do NOT use clap — argh only (user explicitly chose argh)
  - Do NOT add unnecessary CLI options — keep it minimal with sensible defaults
  - Do NOT use `mod` directories or `mod.rs` files — flat file structure in src/
  - Do NOT use async/tokio — synchronous main
  - Do NOT use `unwrap()`/`expect()` in non-test code
  - Do NOT run as sudo — warn if root but don't require it

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Main integration point wiring all modules together, pre-flight checks, exit code handling — needs full system understanding
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on nearly all previous tasks)
  - **Parallel Group**: Wave 3 (after Tasks 10, 11)
  - **Blocks**: Task 13
  - **Blocked By**: Tasks 10, 11 (needs coordinator and report)

  **References**:

  **Pattern References**:
  - `argh` crate usage: `#[derive(FromArgs)]` with `#[argh(option)]`, `#[argh(switch)]` attributes
  - CoreCycler defaults: SSE mode, 6 min/core, 1 thread — match these as our defaults

  **API/Type References**:
  - `argh::from_env::<Args>()` — Parse CLI args from process arguments
  - `tracing_subscriber::fmt::init()` — Initialize tracing with default formatting
  - `std::process::ExitCode` or `std::process::exit(code)` — For exit codes
  - Task 3: `CpuTopology::detect()` — Pre-flight AMD + topology check
  - Task 4: `ExtractedBinaries::extract()` — Extract mprime to temp dir
  - Task 9: `signal_handler::register()`, `Cleanup` — Ctrl+C registration
  - Task 10: `Coordinator::new(config).run()` — Main test orchestration
  - Task 11: `StabilityReport::generate(results)` — Final report

  **External References**:
  - `argh` crate docs: https://docs.rs/argh/latest/argh/ — FromArgs derive macro
  - `tracing-subscriber` crate: https://docs.rs/tracing-subscriber/ — Subscriber initialization

  **WHY Each Reference Matters**:
  - `argh` is much simpler than clap — just derive macro and attribute annotations
  - Pre-flight checks prevent confusing errors later (e.g., running on Intel, 32-bit, non-Linux)
  - Exit codes enable scripting: `./unstable-cpu-detector && echo 'all stable'`

  **Acceptance Criteria**:
  - [x] `cargo test` passes all CLI tests
  - [x] `--help` shows clear usage with defaults
  - [x] Running without args uses sensible defaults (6min/core, 3 iterations, SSE, non-quiet)
  - [x] `--duration`, `--iterations`, `--cores`, `--quiet`, `--mode` args parse correctly
  - [x] Pre-flight check rejects non-AMD CPU with clear error
  - [x] Pre-flight check rejects non-64-bit with clear error
  - [x] Exit code 0 for all-stable, 1 for unstable, 2 for error
  - [x] All src/*.rs files declared as flat includes in main.rs (no mod.rs)
  - [x] `--cores 0,5` filters to only those physical cores
  - [x] `--cores 99` exits with error listing valid core IDs
  - [x] Startup banner shows CPU model and test config
  - [x] No `unwrap()`/`expect()` in non-test code

  **QA Scenarios:**

  ```
  Scenario: CLI help output
    Tool: Bash
    Preconditions: Binary compiled
    Steps:
      1. Run `cargo run -- --help 2>&1`
      2. Assert output contains 'unstable' and 'cpu' and 'duration'
      3. Assert output shows default values for --duration (6) and --iterations (3)
    Expected Result: Help text shows all options with defaults
    Failure Indicators: Missing options, wrong defaults, argh parse error
    Evidence: .sisyphus/evidence/task-12-help-output.txt

  Scenario: Pre-flight AMD check
    Tool: Bash
    Preconditions: Running on AMD system (current machine)
    Steps:
      1. Run `cargo test preflight 2>&1`
      2. Assert AMD detection test passes
      3. Assert non-AMD rejection test passes (with mock /proc/cpuinfo)
    Expected Result: AMD detected on this machine, mock Intel rejected
    Failure Indicators: False positive on Intel, false negative on AMD
    Evidence: .sisyphus/evidence/task-12-preflight.txt

  Scenario: Default args produce valid config
    Tool: Bash
    Preconditions: None
    Steps:
      1. Run `cargo test no_args 2>&1`
      2. Assert test verifies defaults: duration=6, iterations=3, quiet=false, mode=sse
    Expected Result: All defaults match CoreCycler-inspired values
    Failure Indicators: Wrong default value, parse error with no args
    Evidence: .sisyphus/evidence/task-12-defaults.txt
  ```

  ```
  Scenario: --cores flag filters to specific cores
    Tool: Bash
    Preconditions: None
    Steps:
      1. Run `cargo test cores_arg 2>&1`
      2. Assert test verifies --cores '0,5' parses to Vec<u32> [0, 5]
      3. Assert test verifies --cores omitted results in None (all cores)
      4. Assert test verifies --cores '99' with mock topology exits with error listing valid core IDs
    Expected Result: Core filtering parses correctly, invalid IDs rejected with helpful message
    Failure Indicators: Parse error on valid input, missing core IDs in error message
    Evidence: .sisyphus/evidence/task-12-cores-filter.txt

  **Commit**: YES
  - Message: `feat(cli): wire up argh CLI and main entry point`
  - Files: `src/main.rs`, `src/cli.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---

- [x] 13. Integration Tests + Build Verification

  **What to do**:
  - **RED**: Write integration tests in `tests/integration_test.rs`:
    - `given_binary_when_run_with_help_then_shows_usage`
    - `given_binary_when_built_release_then_compiles_without_warnings`
    - `given_binary_when_run_on_amd_then_detects_cpu_topology`
    - `given_binary_when_run_with_duration_1_then_completes_one_core`
    - `given_binary_when_interrupted_then_cleans_up`
    - `given_binary_when_run_quiet_then_outputs_result_line_only`
  - **GREEN**: Implement integration tests:
    - Use `std::process::Command` to run the compiled binary as a subprocess
    - Test `--help` flag: verify output contains expected strings
    - Test release build: `cargo build --release` compiles cleanly
    - Test CPU detection: run binary, verify it detects AMD cores (parse early stdout)
    - Test short run: `--duration 1 --iterations 1` for quick validation (1 min per core, 1 iteration)
      - **NOTE**: This test actually runs mprime, so it's a real integration test
      - May need to be marked `#[ignore]` for CI and run explicitly
    - Test Ctrl+C: spawn binary, wait 5 seconds, send SIGTERM, verify clean exit
    - Test quiet mode: `--quiet` produces only RESULT line
    - Also verify:
      - `cargo clippy -- -D warnings` passes
      - `cargo fmt --check` passes
      - `cargo test` (all unit tests) passes
      - Binary size is reasonable (should be ~30-35MB due to embedded mprime)
      - No temp files left after test run completes or is interrupted
  - **REFACTOR**: Ensure integration tests are idempotent and don't interfere with each other

  **Must NOT do**:
  - Do NOT run long integration tests by default — use `#[ignore]` for tests that take >10 seconds
  - Do NOT leave temp directories from integration test runs
  - Do NOT assume mprime will find errors — short runs may pass all cores
  - Do NOT test on non-AMD systems — integration tests should check and skip
  - Do NOT use `unwrap()`/`expect()` in non-test helper code

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: End-to-end integration testing, process management, signal sending, output parsing — complex verification task
  - **Skills**: []

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on complete binary)
  - **Parallel Group**: Wave 4 (final, before verification wave)
  - **Blocks**: Final Verification Wave
  - **Blocked By**: Task 12 (needs complete binary)

  **References**:

  **Pattern References**:
  - Rust integration test pattern: `tests/` directory, `use std::process::Command`
  - `#[ignore]` attribute for slow tests that require manual opt-in with `cargo test -- --ignored`

  **API/Type References**:
  - `std::process::Command::new("./target/release/unstable-cpu-detector")` — Run compiled binary
  - `std::process::Command::output()` — Capture stdout/stderr
  - `nix::sys::signal::kill(Pid::from_raw(child.id()), Signal::SIGTERM)` — Send interrupt to test Ctrl+C
  - `assert!(output.status.success())` — Verify exit code

  **External References**:
  - Rust book integration tests: https://doc.rust-lang.org/book/ch11-03-test-organization.html#integration-tests

  **WHY Each Reference Matters**:
  - Integration tests in `tests/` are separate compilation units — they test the public API only
  - `#[ignore]` prevents long-running tests from blocking quick CI feedback loops
  - Signal sending via nix crate tests the real Ctrl+C path

  **Acceptance Criteria**:
  - [x] `cargo test` passes all unit tests
  - [x] `cargo test -- --ignored` passes integration tests (on AMD system)
  - [x] `cargo build --release` compiles without warnings
  - [x] `cargo clippy -- -D warnings` passes
  - [x] `cargo fmt --check` passes
  - [x] Binary runs with `--help` and shows usage
  - [x] Binary exits cleanly on Ctrl+C (SIGTERM)
  - [x] Binary produces RESULT line in `--quiet` mode
  - [x] No temp files left after test completion
  - [x] Binary size is reasonable (~30-35MB with embedded mprime)
  - [x] No `unwrap()`/`expect()` in non-test code (verified by clippy or manual review)

  **QA Scenarios:**

  ```
  Scenario: Full build and help check
    Tool: Bash
    Preconditions: All source files written
    Steps:
      1. Run `cargo build --release 2>&1`
      2. Assert exit code 0, no errors
      3. Run `./target/release/unstable-cpu-detector --help 2>&1`
      4. Assert output contains 'duration' and 'iterations' and 'quiet' and 'mode'
      5. Check binary size: `ls -la target/release/unstable-cpu-detector`
      6. Assert size is between 25MB and 50MB
    Expected Result: Clean build, help works, binary size reasonable
    Failure Indicators: Compilation error, missing help text, binary too small (missing embed) or too large
    Evidence: .sisyphus/evidence/task-13-build-help.txt

  Scenario: Quick integration test (1 min per core, 1 iteration)
    Tool: Bash
    Preconditions: Binary compiled, running on AMD system
    Steps:
      1. Run `timeout 300 ./target/release/unstable-cpu-detector --duration 1 --iterations 1 --quiet 2>&1`
      2. Assert output contains 'RESULT:' line
      3. Assert exit code is 0 or 1 (stable or unstable, not 2/error)
      4. Verify no temp directories left: `ls /tmp/unstable-cpu-*` should fail
    Expected Result: Binary completes short run, produces result, cleans up
    Failure Indicators: Timeout, crash, missing result line, leftover temp files
    Evidence: .sisyphus/evidence/task-13-quick-run.txt

  Scenario: Ctrl+C handling
    Tool: Bash
    Preconditions: Binary compiled
    Steps:
      1. Start binary in background: `./target/release/unstable-cpu-detector --duration 1 --iterations 1 &`
      2. Capture PID: `PID=$!`
      3. Wait 10 seconds: `sleep 10`
      4. Send SIGTERM: `kill -TERM $PID`
      5. Wait for exit: `wait $PID; echo "Exit code: $?"`
      6. Verify no temp directories: `ls /tmp/unstable-cpu-*` should fail
      7. Verify no child mprime processes: `pgrep mprime` should fail
    Expected Result: Clean exit after SIGTERM, no zombies, no temp files
    Failure Indicators: Process doesn't exit, zombie mprime, temp files remain
    Evidence: .sisyphus/evidence/task-13-ctrl-c.txt

  Scenario: Code quality gates
    Tool: Bash
    Preconditions: All source files written
    Steps:
      1. Run `cargo clippy -- -D warnings 2>&1`
      2. Assert exit code 0
      3. Run `cargo fmt --check 2>&1`
      4. Assert exit code 0
      5. Run `cargo test 2>&1`
      6. Assert exit code 0, capture test count
    Expected Result: All quality gates pass
    Failure Indicators: Clippy warnings, fmt differences, test failures
    Evidence: .sisyphus/evidence/task-13-quality.txt
  ```

  **Commit**: YES
  - Message: `test: add integration tests and verify full build`
  - Files: `tests/integration_test.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

---
## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Rejection → fix → re-run.

- [x] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [x] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo clippy -- -D warnings` + `cargo fmt --check` + `cargo test`. Review all src/*.rs files for: `unwrap()`/`expect()` in non-test code, empty catches, println! in non-test code (should use tracing), dead code, unused imports. Check for AI slop: excessive comments, over-abstraction, generic names (data/result/item/temp). Verify "small focused files without modules" — no mod.rs, no nested module dirs.
  Output: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Fmt [PASS/FAIL] | Tests [N pass/N fail] | Files [N clean/N issues] | VERDICT`

- [x] F3. **Real Manual QA** — `unspecified-high`
  Build release binary: `cargo build --release`. Run `./target/release/unstable-cpu-detector --help` and verify output. Run on the actual AMD machine — verify it detects cores, starts mprime, monitors for errors, reports results. Test Ctrl+C handling. Test with `--duration 1` (1 minute per core, quick test). Verify temp files are cleaned up after exit. Save terminal output as evidence.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [x] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual code (all src/*.rs). Verify 1:1 — everything in spec was built (no missing), nothing beyond spec was built (no creep). Check "Must NOT Have" compliance: no GUI, no config files, no network, no sudo, no OpenSSL, no modules, no async runtime. Detect scope creep: extra features, premature abstraction, over-engineering. Flag unaccounted files.
  Output: `Tasks [N/N compliant] | Guardrails [N/N respected] | Scope Creep [CLEAN/N issues] | VERDICT`

---

## Commit Strategy

| After Task(s) | Commit Message | Key Files |
|--------------|----------------|-----------|
| 1 | `chore: scaffold Rust project with dependencies and test infra` | Cargo.toml, src/main.rs, .gitignore |
| 2 | `docs: add AGENTS.md for agent guidance` | AGENTS.md |
| 3 | `feat(cpu): add AMD CPU topology detection with BDD tests` | src/cpu_topology.rs |
| 4 | `feat(embed): add mprime + libgmp.so binary extraction` | src/embedded.rs |
| 5 | `feat(config): add mprime prime.txt config generation` | src/mprime_config.rs |
| 6 | `feat(runner): add mprime process spawning with CPU affinity` | src/mprime_runner.rs |
| 7 | `feat(parser): add mprime error pattern detection` | src/error_parser.rs |
| 8 | `feat(mce): add journalctl MCE/EDAC monitoring` | src/mce_monitor.rs |
| 9 | `feat(signal): add Ctrl+C handling and cleanup` | src/signal_handler.rs |
| 10 | `feat(core): add per-core stress test orchestration` | src/coordinator.rs |
| 11 | `feat(report): add stability report generation` | src/report.rs |
| 12 | `feat(cli): wire up argh CLI and main entry point` | src/main.rs |
| 13 | `test: add integration tests and verify full build` | tests/ |

---

## Success Criteria

### Verification Commands
```bash
cargo build --release             # Expected: Compiles without warnings
cargo test                        # Expected: All tests pass
cargo clippy -- -D warnings       # Expected: No warnings
cargo fmt --check                 # Expected: No formatting issues
./target/release/unstable-cpu-detector --help  # Expected: Shows usage
./target/release/unstable-cpu-detector --duration 1  # Expected: Runs 1min/core test
```

### Final Checklist
- [x] All "Must Have" items present and verified
- [x] All "Must NOT Have" items absent (no GUI, no config files, no network, no sudo, no modules)
- [x] All tests pass with `cargo test`
- [x] `cargo clippy -- -D warnings` clean
- [x] `cargo fmt --check` clean
- [x] Binary detects AMD CPU topology correctly
- [x] Binary extracts and runs embedded mprime successfully
- [x] Binary detects mprime errors in results.txt
- [x] Binary monitors journalctl for MCE/EDAC errors
- [x] Binary produces clear stability report
- [x] Ctrl+C cleanly terminates and cleans up
- [x] No `unwrap()`/`expect()` in non-test code
