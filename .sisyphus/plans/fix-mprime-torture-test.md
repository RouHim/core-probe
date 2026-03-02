# Fix mprime Torture Test Execution

## TL;DR

> **Quick Summary**: Fix 3 bugs preventing mprime torture test from running: stdout pipe deadlock, missing config keys in prime.txt, and missing error patterns. Keep Huge FFT default (CoreCycler-proven). A/B test Huge vs Small FFTs on core 2.
> 
> **Deliverables**:
> - Fixed mprime process launch (no more stdout deadlock)
> - Complete prime.txt config matching known-working configuration
> - Huge FFT preset kept as default (CoreCycler-proven: less heat → peak boost → exposes PBO instability)
> - Error parser catches "TORTURE TEST FAILED" and summary line patterns
> - Removed duplicate prime.txt write from coordinator
> 
> **Estimated Effort**: Short
> **Parallel Execution**: YES — 2 waves
> **Critical Path**: Task 1 → Task 2 → Task 3 → Task 6 → F1-F4
> **Validation**: CPU core 2 (known unstable) must fail within <5 minutes

---

## Context

### Original Request
mprime torture testing doesn't actually run when launched with `./mprime -t -d`. The user provided a working expect script that navigates mprime's TUI menu, proving the tool works when configured correctly. CPU core 2 on the test machine (Ryzen 9 5900X with PBO) is known to be highly unstable and should fail within <5 minutes — providing a fast feedback loop for validation.

### Interview Summary
**Key Discussions**:
- User suspects config-file-only approach may not work but wants us to try it first
- User provided actual prime.txt that mprime writes after a successful TUI interaction — this is the reference config
- CPU core 2 is highly unstable — serves as validation target (<5 min detection)
- Small FFTs (cache-focused) are better than Huge FFTs (RAM-focused) for detecting CPU instability

**Research Findings**:
- mprime docs confirm `-t` flag should work non-interactively with proper prime.txt
- The `-d` flag means "detailed stdout output" (NOT daemon mode) — this is what causes the pipe deadlock
- Generated prime.txt is missing 5 critical keys present in the known-working config
- Error parser misses 2 critical patterns from real mprime output
- prime.txt is written twice (coordinator then runner) — runner overwrites coordinator's copy
- CoreCycler defaults to Huge FFTs + SSE mode — we keep this default (less heat → peak boost → exposes PBO instability). A/B test in Task 6 compares Huge vs Small on core 2.
- Note: mprime v30.19 TUI maps its "Small FFTs" option to 73-2773K, but we write prime.txt directly with CoreCycler's 36-248K range

### Metis Review (from prior session ses_3511edfd7ffeYffjDM2kiHw4DT)
**Identified Gaps** (addressed in plan):
- Stdout pipe deadlock is the likely primary root cause — MUST fix
- Must set stdin to Stdio::null() explicitly to prevent TUI interaction
- Missing config keys cause mprime to prompt interactively
- FFT default (Huge) is correct per CoreCycler reasoning — A/B test in Task 6 will compare Huge vs Small on core 2
- Error parser misses critical patterns — unstable cores could go undetected
- Duplicate prime.txt write creates confusion and potential race conditions

---

## Work Objectives

### Core Objective
Fix all bugs preventing mprime torture test from executing and detecting unstable CPU cores, validated by detecting core 2 instability within <5 minutes.

### Concrete Deliverables
- `src/mprime_runner.rs` — Fixed stdout/stdin handling (no deadlock)
- `src/mprime_config.rs` — Complete prime.txt with all required keys, Small FFT default
- `src/coordinator.rs` — Removed duplicate prime.txt write
- `src/error_parser.rs` — Two new error patterns for torture test failures

### Definition of Done
- [ ] `cargo build` succeeds with no errors
- [ ] `cargo test` passes all existing + new tests
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt -- --check` passes
- [ ] Running on core 2 detects instability within <5 minutes (agent QA)

### Must Have
- Stdout pipe deadlock fixed (mprime doesn't silently hang)
- stdin set to Stdio::null() (no TUI interaction possible)
- prime.txt includes: NumWorkers=1, CoresPerTest=1, TortureWeak=0, ComputerGUID, WorkPreference=0
- Default FFT preset kept as Huge (CoreCycler default — matches PBO instability detection use case)
- Error parser catches "TORTURE TEST FAILED" pattern
- Error parser catches summary line "Torture Test completed N tests in M minutes - X errors, Y warnings" when errors > 0
- Duplicate prime.txt write in coordinator.rs removed

### Must NOT Have (Guardrails)
- NO changes to coordinator test infrastructure (trait-based testing architecture)
- NO changes to MCE monitoring logic
- NO new CLI arguments or flags
- NO changes to process lifecycle (stop/wait/drop)
- NO changes to CPU affinity handling (pre_exec + pin_all_threads)
- NO TUI/stdin interaction — config-file-only approach
- NO async code — synchronous execution only
- NO new crate dependencies (uuid already available for ComputerGUID)

---

## Verification Strategy

> **ZERO HUMAN INTERVENTION** — ALL verification is agent-executed. No exceptions.

### Test Decision
- **Infrastructure exists**: YES (cargo test with BDD-style tests)
- **Automated tests**: YES (tests-after — add unit tests for new config keys and error patterns)
- **Framework**: cargo test (built-in)
- **Style**: BDD — Given/When/Then scenarios matching existing test patterns

### QA Policy
Every task MUST include agent-executed QA scenarios.
Evidence saved to `.sisyphus/evidence/task-{N}-{scenario-slug}.{ext}`.

- **Config changes**: Use Bash — generate config, assert key presence
- **Error parser**: Use Bash — run cargo test, verify pattern detection
- **Integration**: Use interactive_bash (tmux) — run actual mprime on core 2, monitor output

---

## Execution Strategy

### Parallel Execution Waves

```
Wave 1 (Start Immediately — independent fixes):
├── Task 1: Fix stdout/stdin handling in mprime_runner.rs [quick]
├── Task 2: Add missing config keys to mprime_config.rs [quick]
├── Task 3: Add missing error patterns to error_parser.rs [quick]
└── Task 4: Remove duplicate prime.txt write from coordinator.rs [quick]

Wave 2 (After Wave 1 — tests + validation):
├── Task 5: Add unit tests for new config keys and error patterns [quick]
└── Task 6: Integration QA — A/B test Huge vs Small FFTs on core 2, validate detection [deep]

Wave FINAL (After ALL tasks — independent review, 4 parallel):
├── Task F1: Plan compliance audit (oracle)
├── Task F2: Code quality review (unspecified-high)
├── Task F3: Real manual QA (unspecified-high)
└── Task F4: Scope fidelity check (deep)

Critical Path: Task 1 → Task 6 → F1-F4
Parallel Speedup: ~50% faster than sequential (Wave 1 runs 4 tasks simultaneously)
Max Concurrent: 4 (Wave 1)
```

### Dependency Matrix

| Task | Depends On | Blocks | Wave |
|------|-----------|--------|------|
| 1 | — | 5, 6 | 1 |
| 2 | — | 5, 6 | 1 |
| 3 | — | 5, 6 | 1 |
| 4 | — | 6 | 1 |
| 5 | 1, 2, 3 | 6 | 2 |
| 6 | 1, 2, 3, 4, 5 | F1-F4 | 2 |

### Agent Dispatch Summary

- **Wave 1**: **4 tasks** — T1 → `quick`, T2 → `quick`, T3 → `quick`, T4 → `quick`
- **Wave 2**: **2 tasks** — T5 → `quick`, T6 → `deep`
- **FINAL**: **4 tasks** — F1 → `oracle`, F2 → `unspecified-high`, F3 → `unspecified-high`, F4 → `deep`

---

## TODOs

> Implementation + Test = ONE Task. Never separate.
> EVERY task MUST have: Recommended Agent Profile + Parallelization info + QA Scenarios.

- [ ] 1. Fix stdout/stdin pipe handling in mprime_runner.rs

  **What to do**:
  - Change `stdout(Stdio::piped())` at line 92 to `stdout(Stdio::null())` — mprime's stdout is not consumed by anything, and `-d` flag causes it to fill the pipe buffer, deadlocking the process
  - Change `stderr(Stdio::piped())` at line 93 to `stderr(Stdio::null())` — stderr is also not consumed
  - Add `.stdin(Stdio::null())` to the command builder (after line 93) — explicitly prevent any TUI interaction; mprime must operate config-file-only
  - Error detection reads from `results.txt` file (coordinator.rs:527-536), NOT stdout — so redirecting stdout to null loses nothing

  **Must NOT do**:
  - Do NOT change process lifecycle (stop/wait_for/drop methods)
  - Do NOT change CPU affinity logic (pre_exec hook or pin_all_threads)
  - Do NOT change the command arguments (-t, -d, -W, working_dir)
  - Do NOT add stdout draining threads or async readers — Stdio::null() is the correct fix since error detection uses results.txt
  - Do NOT touch any test code in this file

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: 3-line change in a single file, no design decisions
  - **Skills**: []
    - No special skills needed for simple Stdio changes
  - **Skills Evaluated but Omitted**:
    - None — this is a trivial mechanical change

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 2, 3, 4)
  - **Blocks**: Tasks 5, 6 (tests and integration need this fix)
  - **Blocked By**: None (can start immediately)

  **References**:

  **Pattern References**:
  - `src/mprime_runner.rs:85-93` — Current Command builder with Stdio::piped() that must be changed
  - `src/mprime_runner.rs:114-116` — spawn() call that follows the Stdio config

  **API/Type References**:
  - `std::process::Stdio::null()` — Redirects to /dev/null, prevents pipe buffer fill
  - `std::process::Stdio::piped()` — Current (broken) approach that creates unread pipes

  **Context References**:
  - `src/coordinator.rs:527-536` — Error detection reads results.txt file, NOT stdout. This proves stdout can safely be discarded
  - The `-d` flag means "detailed output to stdout" per mprime readme.txt — this is what fills the pipe buffer

  **Acceptance Criteria**:
  - [ ] `src/mprime_runner.rs` line 92: `Stdio::piped()` → `Stdio::null()` for stdout
  - [ ] `src/mprime_runner.rs` line 93: `Stdio::piped()` → `Stdio::null()` for stderr
  - [ ] `.stdin(Stdio::null())` added to command builder
  - [ ] `cargo build` succeeds
  - [ ] All existing tests in `mprime_runner.rs` still pass: `cargo test mprime_runner`

  **QA Scenarios:**

  ```
  Scenario: Existing mprime_runner tests still pass after Stdio change
    Tool: Bash
    Preconditions: Code changes applied to mprime_runner.rs
    Steps:
      1. Run: cargo test mprime_runner -- --nocapture
      2. Assert: exit code 0
      3. Assert: output contains "test result: ok"
      4. Assert: output does NOT contain "FAILED"
    Expected Result: All 7 existing tests pass (spawn, stop, pinning, workdir, crash, config, thread-pinning)
    Failure Indicators: Any test failure or compile error
    Evidence: .sisyphus/evidence/task-1-existing-tests.txt

  Scenario: Command builder has correct Stdio configuration
    Tool: Bash
    Preconditions: Code changes applied
    Steps:
      1. Run: grep -n 'Stdio::piped' src/mprime_runner.rs
      2. Assert: zero matches (no piped() remaining in non-test code)
      3. Run: grep -n 'Stdio::null' src/mprime_runner.rs
      4. Assert: at least 3 matches (stdout, stderr, stdin)
      5. Run: grep -n 'stdin' src/mprime_runner.rs | head -5
      6. Assert: contains .stdin(Stdio::null())
    Expected Result: No Stdio::piped() in production code; 3x Stdio::null() present
    Failure Indicators: Any Stdio::piped() remaining, or missing stdin(Stdio::null())
    Evidence: .sisyphus/evidence/task-1-stdio-grep.txt
  ```

  **Commit**: YES (group with Tasks 2, 3, 4)
  - Message: `fix(mprime): resolve torture test execution failures`
  - Files: `src/mprime_runner.rs`
  - Pre-commit: `cargo test mprime_runner`

- [ ] 2. Add missing config keys to mprime_config.rs

  **What to do**:
  - Do NOT change the default FFT preset — keep `FftPreset::Huge` at line 111 in the `Default` impl. CoreCycler's reasoning: Huge FFTs (8960-32768K) generate less heat → CPU reaches peak boost clock → exposes PBO/Curve Optimizer instability. This is exactly our use case.
  - Add 5 missing config keys to the `generate()` method's format string (after line 238, within the `format!` macro):
    - `NumWorkers=1` — tells mprime to use exactly 1 worker thread (currently missing, mprime may auto-detect and use more)
    - `CoresPerTest=1` — restricts each test to 1 core (matches single-core isolation strategy)
    - `TortureWeak=0` — disables weak torture test mode (we want full-strength testing)
    - `WorkPreference=0` — sets work preference to 0 (stress test mode, not searching for primes)
    - `ComputerGUID={guid}` — unique computer identifier (mprime expects this; generate using `uuid::Uuid::new_v4()` formatted as 32-char hex without hyphens)
  - Add a `computer_guid` field to `MprimeConfig` struct, defaulting to a newly generated UUID
  - Do NOT update doc comments about FFT presets — Huge remains the recommended default for PBO instability detection

  **Must NOT do**:
  - Do NOT change the default FFT preset — it's already `FftPreset::Huge` which is correct
  - Do NOT change existing builder methods
  - Do NOT add new builder methods for the new keys — they use fixed values (NumWorkers=1, CoresPerTest=1, etc.)
  - Do NOT change the StressTestMode enum or its flags
  - Do NOT add any `unwrap()` or `expect()` in non-test code
  - Do NOT change existing test assertions (tests will need updating for new default)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Config key additions and a default change in a single file
  - **Skills**: []
    - No special skills needed
  - **Skills Evaluated but Omitted**:
    - None — mechanical additions following existing patterns

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 3, 4)
  - **Blocks**: Tasks 5, 6
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `src/mprime_config.rs:107-120` — Default impl where `FftPreset::Huge` is already set (DO NOT CHANGE — this is correct per CoreCycler)
  - `src/mprime_config.rs:223-255` — `generate()` method format string where new keys must be added
  - `src/mprime_config.rs:214-221` — `affinity_lines` pattern showing how conditional config lines are generated

  **API/Type References**:
  - `uuid::Uuid::new_v4()` — UUID generation (crate already in Cargo.toml)
  - `uuid::Uuid::as_simple()` → `.to_string()` — 32-char hex without hyphens (matches mprime's ComputerGUID format)

  **External References**:
  - Known-working prime.txt (from user's expect script session):
    ```
    NumWorkers=2
    WorkPreference=0
    CoresPerTest=6
    ComputerGUID=bb431a3867ba0f31cb932d581bee7851
    TortureWeak=0
    ```
    Our values: NumWorkers=1, CoresPerTest=1 (single-core isolation), rest same

  **Acceptance Criteria**:
  - [ ] Default FFT preset remains `FftPreset::Huge` (unchanged — CoreCycler default)
  - [ ] Generated config contains `NumWorkers=1`
  - [ ] Generated config contains `CoresPerTest=1`
  - [ ] Generated config contains `TortureWeak=0`
  - [ ] Generated config contains `WorkPreference=0`
  - [ ] Generated config contains `ComputerGUID=` followed by 32 hex chars
  - [ ] Generated config contains `MinTortureFFT=36` and `MaxTortureFFT=248` (Small preset)
  - [ ] `cargo build` succeeds
  - [ ] Doc comments updated to reflect Small FFT default

  **QA Scenarios:**

  ```
  Scenario: Generated config contains all required keys
    Tool: Bash
    Preconditions: Code changes applied to mprime_config.rs
    Steps:
      1. Run: cargo test mprime_config -- --nocapture 2>&1
      2. Assert: all tests pass (some existing tests may need FFT range updates)
      3. Write a quick validation: cargo run -- --help 2>&1 || true (just verify it compiles)
    Expected Result: All config tests pass
    Failure Indicators: Test failure (new keys missing, not FFT range — FFT default is unchanged)
    Evidence: .sisyphus/evidence/task-2-config-tests.txt

  Scenario: Default config still uses Huge FFT range (CoreCycler default)
    Tool: Bash
    Preconditions: Changes applied, new keys added
    Steps:
      1. Check that test `given_huge_fft_preset_when_generating_then_sets_correct_fft_range` still passes (Huge is default)
      2. Verify default config produces MinTortureFFT=8960 and MaxTortureFFT=32768
      3. Verify Small FFT preset still works when explicitly set (existing test)
    Expected Result: Default config uses Huge FFT (8960-32768K), explicit Small preset also works
    Failure Indicators: MinTortureFFT=36 in default output (would mean someone changed default to Small)
    Evidence: .sisyphus/evidence/task-2-fft-default.txt

  Scenario: ComputerGUID is valid 32-char hex
    Tool: Bash
    Preconditions: Config generates with ComputerGUID
    Steps:
      1. Run the new test that checks ComputerGUID format
      2. Assert: ComputerGUID matches regex [0-9a-f]{32}
      3. Assert: ComputerGUID is different on each generate() call (UUID uniqueness)
    Expected Result: Valid 32-char lowercase hex GUID
    Failure Indicators: Missing ComputerGUID, wrong length, or contains hyphens
    Evidence: .sisyphus/evidence/task-2-guid-format.txt
  ```

  **Commit**: YES (group with Tasks 1, 3, 4)
  - Message: `fix(mprime): resolve torture test execution failures`
  - Files: `src/mprime_config.rs`
  - Pre-commit: `cargo test mprime_config`

---

- [ ] 3. Add missing error patterns to error_parser.rs

  **What to do**:
  - Add new enum variant `TortureTestFailed` to `MprimeErrorType` (after `IllegalSumout` at line 27) — for "TORTURE TEST FAILED" messages
  - Add new enum variant `TortureTestSummaryError` to `MprimeErrorType` — for summary lines reporting errors > 0
  - Add `try_torture_test_failed()` method following the existing `OnceLock<Regex>` + `try_*` pattern:
    - Pattern: `(?i)TORTURE TEST FAILED`
    - Returns `MprimeErrorType::TortureTestFailed`
  - Add `try_torture_summary_error()` method:
    - Pattern: `(?i)Torture Test completed \d+ tests? in \d+ minutes? - (\d+) errors?, (\d+) warnings?`
    - Only match when captured errors group > 0 (ignore summary lines with 0 errors)
    - Returns `MprimeErrorType::TortureTestSummaryError`
  - Add both new `try_*` methods to the `parse_lines()` chain at lines 88-94 (after `try_sum_mismatch`)
  - **CRITICAL**: Update exhaustive match arms in other files that match on `MprimeErrorType`:
    - `src/coordinator.rs:586-592` — Add `MprimeErrorType::TortureTestFailed => "TORTURE TEST FAILED"` and `MprimeErrorType::TortureTestSummaryError => "Torture test summary error"` to the error-type-to-string match
    - `src/report.rs:229-235` — Add `MprimeErrorType::TortureTestFailed => "mprime: TORTURE TEST FAILED"` and `MprimeErrorType::TortureTestSummaryError => "mprime: Torture test summary error"` to the display-string match
    - Without these updates, `cargo build` will FAIL with "non-exhaustive patterns" error

  **Must NOT do**:
  - Do NOT modify existing error patterns or their regex
  - Do NOT change the ErrorParser struct or its byte_offset logic
  - Do NOT change `parse_results()` or `parse_line()` method signatures
  - Do NOT change existing test functions
  - Do NOT use `unwrap()` or `expect()` in non-test code

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Add 2 enum variants + 2 try_* methods following exact existing pattern
  - **Skills**: []
    - No special skills needed — copy-paste pattern from existing try_* methods
  - **Skills Evaluated but Omitted**:
    - None

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 2, 4)
  - **Blocks**: Tasks 5, 6
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `src/error_parser.rs:107-123` — `try_roundoff_error()` method — exact pattern to copy for new methods (OnceLock<Regex> + is_match + return MprimeError)
  - `src/error_parser.rs:125-141` — `try_hardware_failure()` method — pattern with regex capture group (needed for summary line errors count)
  - `src/error_parser.rs:85-96` — `parse_lines()` method chain where new try_* calls must be appended
  - `src/error_parser.rs:16-32` — `MprimeErrorType` enum where new variants must be added
  - `src/coordinator.rs:586-592` — Exhaustive match on `MprimeErrorType` that MUST be updated with new variants (compile error otherwise)
  - `src/report.rs:229-235` — Exhaustive match on `MprimeErrorType` for display strings that MUST be updated with new variants

  **External References**:
  - Real mprime error output (from user's test machine):
    ```
    TORTURE TEST FAILED on worker #1
    Torture Test completed 20 tests in 13 minutes - 1 errors, 0 warnings.
    ```
    These are the exact patterns that must be matched

  **Acceptance Criteria**:
  - [ ] `MprimeErrorType::TortureTestFailed` variant exists
  - [ ] `MprimeErrorType::TortureTestSummaryError` variant exists
  - [ ] `try_torture_test_failed()` matches "TORTURE TEST FAILED on worker #1"
  - [ ] `try_torture_summary_error()` matches summary with errors > 0
  - [ ] `try_torture_summary_error()` does NOT match summary with 0 errors (e.g., "0 errors, 0 warnings")
  - [ ] Both methods added to `parse_lines()` chain
  - [ ] Match arm in `coordinator.rs:586-592` updated with both new variants
  - [ ] Match arm in `report.rs:229-235` updated with both new variants
  - [ ] `cargo build` succeeds (exhaustive match arms compile)
  - [ ] Existing error_parser tests still pass

  **QA Scenarios:**

  ```
  Scenario: TORTURE TEST FAILED pattern detected
    Tool: Bash
    Preconditions: Code changes applied to error_parser.rs
    Steps:
      1. Run: cargo test error_parser -- --nocapture 2>&1
      2. Assert: exit code 0
      3. Assert: all existing tests pass
      4. Assert: new test for "TORTURE TEST FAILED on worker #1" passes
    Expected Result: Pattern correctly identified as TortureTestFailed error type
    Failure Indicators: Test failure or compile error
    Evidence: .sisyphus/evidence/task-3-torture-failed-pattern.txt

  Scenario: Summary line with errors > 0 detected, errors = 0 ignored
    Tool: Bash
    Preconditions: Code changes applied
    Steps:
      1. Run test for: "Torture Test completed 20 tests in 13 minutes - 1 errors, 0 warnings."
      2. Assert: Detected as TortureTestSummaryError
      3. Run test for: "Torture Test completed 20 tests in 13 minutes - 0 errors, 0 warnings."
      4. Assert: NOT detected as error (returns None)
    Expected Result: Only summary lines with errors > 0 are flagged
    Failure Indicators: False positive on 0-error summary, or miss on >0 error summary
    Evidence: .sisyphus/evidence/task-3-summary-pattern.txt
  ```

  **Commit**: YES (group with Tasks 1, 2, 4)
  - Message: `fix(mprime): resolve torture test execution failures`
  - Files: `src/error_parser.rs`, `src/coordinator.rs`, `src/report.rs`
  - Pre-commit: `cargo test error_parser`

- [ ] 4. Remove duplicate prime.txt write from coordinator.rs

  **What to do**:
  - Delete lines 300-309 in `coordinator.rs` — this block generates prime.txt via `MprimeConfig::builder().disable_internal_affinity().generate()` and writes it to the working directory
  - This is a duplicate because `mprime_runner.rs:72-81` does the exact same thing (generates config and writes prime.txt to the working dir) — and the runner's write OVERWRITES the coordinator's write since it runs after
  - Keep the directory creation at lines 293-298 (that's still needed)
  - Keep everything after line 310 (core index calculation, progress output, etc.)

  **Must NOT do**:
  - Do NOT change any coordinator test infrastructure (trait-based architecture)
  - Do NOT modify any method signatures or trait definitions
  - Do NOT change the test_core() method beyond removing the duplicate write
  - Do NOT touch MCE monitoring logic
  - Do NOT change the runner's prime.txt write in mprime_runner.rs (that's the one we keep)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Delete 10 lines from a single file, no logic changes
  - **Skills**: []
    - No special skills needed
  - **Skills Evaluated but Omitted**:
    - None

  **Parallelization**:
  - **Can Run In Parallel**: YES
  - **Parallel Group**: Wave 1 (with Tasks 1, 2, 3)
  - **Blocks**: Task 6
  - **Blocked By**: None

  **References**:

  **Pattern References**:
  - `src/coordinator.rs:300-309` — The duplicate prime.txt write block TO DELETE:
    ```rust
    let prime_txt = MprimeConfig::builder()
        .disable_internal_affinity()
        .generate()?;
    let prime_txt_path = working_dir.join("prime.txt");
    fs::write(&prime_txt_path, prime_txt).with_context(|| {
        format!(
            "failed to write mprime prime.txt configuration {}",
            prime_txt_path.display()
        )
    })?;
    ```
  - `src/mprime_runner.rs:72-81` — The KEPT prime.txt write (in runner's start() method) — this is the authoritative write that uses the same pattern
  - `src/coordinator.rs:293-298` — Directory creation block that must be PRESERVED (immediately before the duplicate write)

  **Acceptance Criteria**:
  - [ ] Lines 300-309 of coordinator.rs removed (the MprimeConfig + fs::write block)
  - [ ] Lines 293-298 (create_dir_all) still present and unchanged
  - [ ] `MprimeConfig` import may now be unused in coordinator.rs — remove if so
  - [ ] `cargo build` succeeds
  - [ ] All existing coordinator tests pass: `cargo test coordinator`

  **QA Scenarios:**

  ```
  Scenario: Duplicate prime.txt write removed
    Tool: Bash
    Preconditions: Code changes applied to coordinator.rs
    Steps:
      1. Run: grep -n 'MprimeConfig::builder' src/coordinator.rs
      2. Assert: zero matches (config is no longer generated in coordinator)
      3. Run: grep -n 'MprimeConfig::builder' src/mprime_runner.rs
      4. Assert: exactly 1 match (runner still generates config)
      5. Run: grep -c 'prime.txt' src/coordinator.rs
      6. Assert: count reduced (no more fs::write of prime.txt in coordinator)
    Expected Result: MprimeConfig usage fully removed from coordinator.rs, preserved in runner
    Failure Indicators: MprimeConfig::builder() still present in coordinator
    Evidence: .sisyphus/evidence/task-4-duplicate-removed.txt

  Scenario: Coordinator still compiles and tests pass
    Tool: Bash
    Preconditions: Duplicate write removed
    Steps:
      1. Run: cargo test coordinator -- --nocapture 2>&1
      2. Assert: exit code 0
      3. Assert: output contains "test result: ok"
    Expected Result: All coordinator tests pass (they use trait mocks, not real mprime)
    Failure Indicators: Compile error from unused import, or test failure
    Evidence: .sisyphus/evidence/task-4-coordinator-tests.txt
  ```

  **Commit**: YES (group with Tasks 1, 2, 3)
  - Message: `fix(mprime): resolve torture test execution failures`
  - Files: `src/coordinator.rs`
  - Pre-commit: `cargo test coordinator`

---

- [ ] 5. Add unit tests for new config keys and error patterns

  **What to do**:
  - In `src/mprime_config.rs` tests module, add/update the following tests:
    - Update `given_config_when_writing_then_creates_valid_prime_txt` (line 326) to also assert new keys: `NumWorkers=1`, `CoresPerTest=1`, `TortureWeak=0`, `WorkPreference=0`, `ComputerGUID=` (regex match for 32 hex chars)
    - Update `given_huge_fft_preset_when_generating_then_sets_correct_fft_range` (line 300) — this test is FINE as-is since it explicitly sets `FftPreset::Huge` and Huge is now the default. Add a NEW test `given_default_config_when_generating_then_uses_huge_fft_range` that asserts default config produces `MinTortureFFT=8960` and `MaxTortureFFT=32768`
    - Add test `given_config_when_generating_then_includes_computer_guid` that asserts ComputerGUID is present and is 32 hex chars
    - Add test `given_two_configs_when_generating_then_guids_differ` that generates two configs and asserts their ComputerGUIDs are different
  - In `src/error_parser.rs` tests module, add the following tests:
    - `given_torture_test_failed_when_parsing_then_detects_error` — parse "TORTURE TEST FAILED on worker #1", assert `MprimeErrorType::TortureTestFailed`
    - `given_torture_summary_with_errors_when_parsing_then_detects_error` — parse "Torture Test completed 20 tests in 13 minutes - 1 errors, 0 warnings.", assert `MprimeErrorType::TortureTestSummaryError`
    - `given_torture_summary_no_errors_when_parsing_then_returns_none` — parse "Torture Test completed 20 tests in 13 minutes - 0 errors, 0 warnings.", assert `None` returned
  - Follow existing BDD test style: `given_X_when_Y_then_Z` naming, Given/When/Then comments

  **Must NOT do**:
  - Do NOT modify existing test assertions — the default is still Huge, so no existing tests need FFT range updates
  - Do NOT change test infrastructure (fixtures, helpers)
  - Do NOT add integration tests — unit tests only
  - Do NOT use `unwrap()` outside of test code (in test code it's fine per existing patterns)

  **Recommended Agent Profile**:
  - **Category**: `quick`
    - Reason: Adding unit tests following exact existing patterns
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - None

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on Tasks 1-3 completing first)
  - **Parallel Group**: Wave 2 (with Task 6, but Task 6 depends on this)
  - **Blocks**: Task 6
  - **Blocked By**: Tasks 1, 2, 3 (the code changes these tests verify)

  **References**:

  **Pattern References**:
  - `src/mprime_config.rs:265-280` — `given_sse_mode_when_generating_config_then_disables_avx_flags` — test pattern to follow (Given/When/Then comments, assert!() with contains())
  - `src/mprime_config.rs:325-351` — `given_config_when_writing_then_creates_valid_prime_txt` — comprehensive key presence test to extend
  - `src/error_parser.rs:230-244` — `given_roundoff_error_line_when_parsing_then_detects_hardware_error` — error parser test pattern to follow
  - `src/error_parser.rs:408-427` — `given_case_insensitive_errors_when_parsing_then_all_detected` — pattern for testing multiple inputs

  **Acceptance Criteria**:
  - [ ] All new tests pass: `cargo test -- --nocapture`
  - [ ] All existing tests still pass (no regressions)
  - [ ] At least 3 new tests for mprime_config (default FFT, new keys, GUID)
  - [ ] At least 3 new tests for error_parser (torture failed, summary with errors, summary without errors)
  - [ ] `cargo clippy -- -D warnings` passes (no unused code warnings)

  **QA Scenarios:**

  ```
  Scenario: All tests pass after adding new test cases
    Tool: Bash
    Preconditions: Tasks 1-3 completed, new tests added
    Steps:
      1. Run: cargo test -- --nocapture 2>&1
      2. Assert: exit code 0
      3. Assert: output contains "test result: ok"
      4. Count test results: should show increased test count vs baseline
    Expected Result: All tests pass including new ones
    Failure Indicators: Any test failure or compile error
    Evidence: .sisyphus/evidence/task-5-all-tests.txt

  Scenario: Clippy and fmt pass with new test code
    Tool: Bash
    Preconditions: New tests written
    Steps:
      1. Run: cargo clippy -- -D warnings 2>&1
      2. Assert: exit code 0 (no warnings)
      3. Run: cargo fmt -- --check 2>&1
      4. Assert: exit code 0 (properly formatted)
    Expected Result: Clean clippy and fmt
    Failure Indicators: Clippy warning about unused import or dead code
    Evidence: .sisyphus/evidence/task-5-quality-check.txt
  ```

  **Commit**: YES (group with Tasks 1-4)
  - Message: `fix(mprime): resolve torture test execution failures`
  - Files: `src/mprime_config.rs`, `src/error_parser.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings`

- [ ] 6. Integration QA — A/B test Huge vs Small FFTs on core 2, validate instability detection

  **What to do**:
  - Build the project in release mode: `cargo build --release`
  - **Phase A — Huge FFTs (default)**: Run the tool targeting core 2 with default config (Huge FFTs, 8960-32768K). Record time-to-failure.
  - **Phase B — Small FFTs**: Temporarily modify `src/mprime_config.rs` to use `FftPreset::Small` as default, rebuild, run on core 2 again. Record time-to-failure.
  - **Compare results**: Which FFT preset detected core 2 instability faster? Record both times.
  - **Revert Phase B change**: After A/B test, revert mprime_config.rs back to `FftPreset::Huge` default. Commit must NOT include the Small FFT change.
  - Verify the complete flow (for BOTH runs):
    1. mprime actually starts and runs (no hang/deadlock)
    2. Torture test executes (results.txt gets written to)
    3. Error detected within <5 minutes (core 2 is known unstable)
    4. Report correctly identifies core 2 as unstable
  - If core 2 doesn't fail within 5 minutes on EITHER preset, this is a RED FLAG — investigate config
  - Capture full output, results.txt content, and any generated reports for BOTH runs

  **Must NOT do**:
  - Do NOT modify any code during this task — this is QA only
  - Do NOT run as root/sudo — the tool should work without it
  - Do NOT test on cores other than 2 unless core 2 validation succeeds first

  **Recommended Agent Profile**:
  - **Category**: `deep`
    - Reason: Real hardware interaction, needs monitoring over multiple minutes, may need to diagnose failures
  - **Skills**: []
  - **Skills Evaluated but Omitted**:
    - `playwright` — no browser interaction needed

  **Parallelization**:
  - **Can Run In Parallel**: NO (depends on all code changes being complete)
  - **Parallel Group**: Wave 2 (sequential after Task 5)
  - **Blocks**: Final verification wave
  - **Blocked By**: Tasks 1, 2, 3, 4, 5 (all code and test changes)

  **References**:

  **Context References**:
  - `src/main.rs` — CLI entry point; check available arguments for core selection
  - `src/coordinator.rs` — Orchestration logic; understand how cores are selected and tested
  - AGENTS.md — "CPU core 2 is known to be highly unstable" on the test machine (Ryzen 9 5900X)

  **External References**:
  - Core 2 on this Ryzen 9 5900X is known to fail mprime within <5 minutes (exact FFT preference unknown — A/B test will determine)
  - Expected errors: ROUNDOFF > 0.4, Hardware failure detected, TORTURE TEST FAILED

  **Acceptance Criteria**:
  - [ ] `cargo build --release` succeeds
  - [ ] Tool runs without hanging or deadlocking (both FFT presets)
  - [ ] mprime process starts and produces results.txt (both runs)
  - [ ] Core 2 instability detected within <5 minutes (at least one preset)
  - [ ] Report correctly flags core 2 as unstable (both runs)
  - [ ] Error messages match expected patterns (roundoff, hardware failure, torture test failed)
  - [ ] A/B comparison recorded: Huge FFT time-to-failure vs Small FFT time-to-failure
  - [ ] Code reverted to `FftPreset::Huge` default after A/B test

  **QA Scenarios:**

  ```
  Scenario: mprime torture test runs without deadlock
    Tool: interactive_bash (tmux)
    Preconditions: cargo build --release completed successfully
    Steps:
      1. Run: ./target/release/unstable-cpu-detector (with appropriate args for core 2)
      2. Wait 30 seconds
      3. Assert: process is still running (not hung)
      4. Check: results.txt exists in the temp working directory (path printed in tool output, typically `$TMPDIR/unstable-cpu-detector-*/iteration-*/core-*/results.txt` per coordinator.rs:289-292) and is growing
    Expected Result: mprime process alive and producing output after 30s
    Failure Indicators: Process exits immediately, or no results.txt created
    Evidence: .sisyphus/evidence/task-6-no-deadlock.txt

  Scenario: Phase A — Huge FFTs (default) detects core 2 instability
    Tool: interactive_bash (tmux)
    Preconditions: cargo build --release with default Huge FFT
    Steps:
      1. Run: ./target/release/unstable-cpu-detector (with appropriate args for core 2)
      2. Start a timer
      3. Wait up to 5 minutes for completion or error detection
      4. Record time-to-failure
      5. Check: results.txt exists in temp dir (path in tool output, per coordinator.rs:289-292)
      6. Verify error type matches known patterns (ROUNDOFF, Hardware failure, TORTURE TEST FAILED)
      7. Save full output
    Expected Result: Core 2 flagged as unstable with specific error details
    Failure Indicators: Core 2 reported as stable after 5+ minutes, process hangs, or no report
    Evidence: .sisyphus/evidence/task-6-phase-a-huge-fft.txt

  Scenario: Phase B — Small FFTs detects core 2 instability
    Tool: interactive_bash (tmux)
    Preconditions: mprime_config.rs temporarily changed to FftPreset::Small default, rebuilt
    Steps:
      1. Change FftPreset::Huge to FftPreset::Small in src/mprime_config.rs Default impl
      2. Run: cargo build --release
      3. Run: ./target/release/unstable-cpu-detector (same args as Phase A)
      4. Start a timer
      5. Wait up to 5 minutes for completion or error detection
      6. Record time-to-failure
      7. Save full output
      8. REVERT mprime_config.rs back to FftPreset::Huge
      9. Run: cargo build --release (rebuild with Huge default)
    Expected Result: Core 2 flagged as unstable; time-to-failure compared with Phase A
    Failure Indicators: Core 2 stable after 5 min, or forgot to revert
    Evidence: .sisyphus/evidence/task-6-phase-b-small-fft.txt

  Scenario: A/B comparison summary
    Tool: Bash
    Preconditions: Both Phase A and Phase B completed
    Steps:
      1. Compare time-to-failure: Huge FFT vs Small FFT
      2. Compare error types detected by each
      3. Write summary: "Huge FFT: {time}s, error: {type} | Small FFT: {time}s, error: {type}"
      4. Verify code is back to FftPreset::Huge default (grep src/mprime_config.rs)
    Expected Result: Clear comparison of both presets with recommendation
    Evidence: .sisyphus/evidence/task-6-ab-comparison.txt

  Scenario: Tool exits with non-zero code when instability found
    Tool: Bash
    Preconditions: Final run completed with core 2 failure detected (Huge FFT default)
    Steps:
      1. Check exit code of the tool run
      2. Assert: exit code is 1 (failures found)
    Expected Result: Exit code 1 indicating unstable cores detected
    Failure Indicators: Exit code 0 despite known instability
    Evidence: .sisyphus/evidence/task-6-exit-code.txt

  **Commit**: NO (QA task, no code changes)

---

## Final Verification Wave (MANDATORY — after ALL implementation tasks)

> 4 review agents run in PARALLEL. ALL must APPROVE. Rejection → fix → re-run.

- [ ] F1. **Plan Compliance Audit** — `oracle`
  Read the plan end-to-end. For each "Must Have": verify implementation exists (read file, run command). For each "Must NOT Have": search codebase for forbidden patterns — reject with file:line if found. Check evidence files exist in .sisyphus/evidence/. Compare deliverables against plan.
  Output: `Must Have [N/N] | Must NOT Have [N/N] | Tasks [N/N] | VERDICT: APPROVE/REJECT`

- [ ] F2. **Code Quality Review** — `unspecified-high`
  Run `cargo build`, `cargo clippy -- -D warnings`, `cargo fmt -- --check`, `cargo test`. Review all changed files for: `unwrap()` or `expect()` in non-test code, empty error handlers, commented-out code, unused imports. Check AI slop: excessive comments, over-abstraction, generic variable names.
  Output: `Build [PASS/FAIL] | Clippy [PASS/FAIL] | Fmt [PASS/FAIL] | Tests [N pass/N fail] | VERDICT`

- [ ] F3. **Real Manual QA** — `unspecified-high`
  Start from clean state (`cargo build --release`). Run the tool targeting core 2 specifically. Verify: mprime starts without hanging, torture test runs, errors detected within <5 minutes, report generated correctly. Save all output to `.sisyphus/evidence/final-qa/`.
  Output: `Scenarios [N/N pass] | Integration [N/N] | Edge Cases [N tested] | VERDICT`

- [ ] F4. **Scope Fidelity Check** — `deep`
  For each task: read "What to do", read actual diff (`git diff`). Verify 1:1 — everything in spec was built (no missing), nothing beyond spec was built (no creep). Check "Must NOT do" compliance: no coordinator test changes, no MCE changes, no CLI changes, no lifecycle changes. Flag unaccounted changes.
  Output: `Tasks [N/N compliant] | Contamination [CLEAN/N issues] | Unaccounted [CLEAN/N files] | VERDICT`

---

## Commit Strategy

- **Single commit** after all tasks pass: `fix(mprime): resolve torture test execution failures`
  - Files: `src/mprime_runner.rs`, `src/mprime_config.rs`, `src/coordinator.rs`, `src/error_parser.rs`
  - Pre-commit: `cargo test && cargo clippy -- -D warnings && cargo fmt -- --check`

---

## Success Criteria

### Verification Commands
```bash
cargo build                        # Expected: compiles without errors
cargo test                         # Expected: all tests pass (existing + new)
cargo clippy -- -D warnings        # Expected: no warnings
cargo fmt -- --check               # Expected: no formatting issues
cargo build --release              # Expected: release build succeeds
```

### Final Checklist
- [ ] All "Must Have" items present and verified
- [ ] All "Must NOT Have" items absent (no forbidden changes)
- [ ] All existing tests still pass
- [ ] New tests cover config key generation and error pattern detection
- [ ] Core 2 instability detected within <5 minutes on test machine (at least one FFT preset)
- [ ] A/B test results recorded: Huge FFT time-to-failure vs Small FFT time-to-failure
- [ ] No stdout pipe deadlock (mprime runs to completion or error)
- [ ] Code ships with FftPreset::Huge as default (reverted after A/B test)
