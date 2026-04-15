# AGENTS.md - core-probe

## Project Purpose

**core-probe** is a Linux CLI tool designed to identify unstable AMD CPU cores using the mprime (Prime95) stress test, inspired by CoreCycler. The tool systematically cycles through each CPU core, runs mprime stress tests, monitors for failures via system error logs, and generates a report identifying cores that fail stability tests.

## Constraints & Technical Requirements

ALL agents MUST adhere to these constraints without exception:

### Platform Constraints
- **Linux only** — No Windows, macOS, or other platforms
- **AMD only** — Target AMD CPUs; abort if non-AMD CPU detected
- **64-bit only** — No 32-bit CPU support
- **No sudo** — If sudo is required, ask the user; never run sudo without explicit permission

### Programming Language & Tooling
- **Rust only** — All code must be written in Rust
- **CLI Framework** — Use `argh` for command-line argument parsing (NOT clap)
- **Logging** — Use `tracing` + `tracing_subscriber` with contextual spans; prefer structured logging over `println!`
- **Error Handling** — Use `anyhow` for errors; NO `unwrap()` or `expect()` in non-test code
- **HTTP Client** — Use `ureq` if HTTP is needed; NOT reqwest or hyper
- **TLS** — Use RustTLS only; NO OpenSSL
- **Async Runtime** — NO external async runtime (tokio, async-std). Iced's built-in executor is permitted for GUI event loop.
- **GUI Framework** — Use `iced` 0.14 for the graphical interface
- **Dependencies** — Keep external crates minimal; prefer standard Rust library features

### Code Structure & Style
- **No Rust modules** — Organize code into small, focused individual `.rs` files in `src/` directory
- **Single responsibility** — Each file encapsulates one responsibility or closely related functionalities
- **SOLID principles** — Follow single responsibility, open-closed, Liskov substitution, interface segregation, dependency inversion
- **YAGNI principle** — Don't add functionality until necessary; avoid writing unused code for future features
- **Code formatting** — Run `cargo fmt` and `cargo clippy` after each task
- **Linting** — Maintain clippy --D warnings compliance throughout

### Embedded Resources
- **Static embedding** — All external files (mprime binary, libraries) must be embedded in the binary at compile time
- **Binary size** — mprime v30.19 (~27MB) + libgmp.so (~706KB) embedded within release build

## Architecture Overview

### High-Level Flow

1. **CPU Topology Discovery**
   - Detect CPU model and core count
   - Verify AMD processor (abort if not)
   - Map logical core IDs to physical cores
   - Handle non-contiguous core layouts (e.g., 0-5, 8-13)
   - **BIOS Core Index Mapping**: All user-facing core numbers MUST use BIOS indices, not Linux physical core IDs. On multi-CCD AMD CPUs, physical IDs have gaps (e.g., 0-5, 8-13 on a 5900X) but the BIOS Curve Optimizer numbers cores sequentially (0-11). The mapping is: sort physical IDs → enumerate → enumeration index = BIOS index. `CpuTopology.bios_map` (physical→BIOS) and `physical_map` (BIOS→physical) handle this. Physical core IDs are internal only (CPU affinity, working dirs, tracing spans).

2. **Binary Extraction & Setup**
   - Extract embedded mprime binary to temporary working directory
   - Extract libgmp.so dependency
   - Prepare isolated working directories per core test run

3. **Per-Core mprime Cycling**
   - For each CPU core:
     - Set CPU affinity to target core
     - Launch mprime in stress mode with TUI menu navigation
     - Run with CoreCycler defaults: SSE mode, Huge FFT, 6 min/core, 1 thread
     - Monitor stdout/stderr for error patterns
     - Collect mprime output for analysis

4. **Error Detection**
   - Monitor for mprime error patterns:
     - "ROUND OFF > 0.40" (numerical instability)
     - "Hardware failure detected"
     - "FATAL ERROR"
     - "ILLEGAL SUMOUT"
   - Mark core as unstable on error detection
   - Continue testing remaining cores

5. **MCE/EDAC Monitoring (Parallel)**
   - Run `journalctl -k -b | grep -iE "mce|hardware error|edac"` in parallel
   - Correlate MCE events with core test timelines
   - May require fine-tuning of grep patterns based on kernel version

6. **Reporting**
   - Generate human-readable stability report
   - List stable cores
   - List unstable cores with failure details
   - Include MCE/EDAC events timeline
   - Exit with appropriate status code (0 = all stable, 1 = failures found)

## File Organization

### Key Files & Purposes

- **src/main.rs** — Entry point; argument parsing via argh; orchestrates top-level flow
- **src/cpu_detector.rs** — CPU topology detection; AMD verification; core ID mapping
- **src/mprime_runner.rs** — mprime process management; TUI menu navigation; error parsing
- **src/test_executor.rs** — Per-core test cycling; CPU affinity management
- **src/mce_monitor.rs** — System log monitoring via journalctl; MCE event correlation
- **src/reporter.rs** — Report generation; formatting; output to stdout/file
- **mprime-latest/** — Embedded mprime v30.19 binary + libgmp.so (gitignored, embedded at build)

### Build Artifacts
- **target/release/core-probe** — Final binary (stripped, LTO-optimized)

## mprime Control Approach

### Configuration Strategy
mprime v30.19 has limited CLI options; interactive TUI menu control required:

1. **Startup Configuration**
   - Launch mprime with minimal CLI args (stress test mode)
   - Immediately navigate TUI menu via stdin key sequences
   - Configure: SSE mode, Huge FFT size, 6 min iterations, 1 thread per core
   - Write temporary `prime.ini` config file per core test run
   - Use isolated working directories to prevent state pollution

2. **Error Pattern Monitoring**
   - Parse stdout/stderr for error messages in real-time
   - Errors indicate core instability; stop that core's test
   - Log errors with timestamps for correlation with MCE events

3. **Working Directory Isolation**
   - Each core test runs in separate `$TMPDIR/mprime-core-N/` directory
   - Prevents cross-core state leakage
   - Simplifies cleanup after test completion

4. **Sensible Defaults (No User Config)**
   - 6 minutes per core (pragmatic balance between detection & runtime)
   - Huge FFT (catches most CPU issues)
   - SSE mode (works on all modern AMD)
   - Single thread per core (cores tested in isolation)
   - No user-customizable options (CoreCycler-inspired simplicity)

## Testing & Quality Standards

### Test-Driven Development (TDD)
- Write tests before implementation
- Tests drive design decisions
- Ensure robust feature implementation

### Behavior-Driven Development (BDD)
- Write tests in BDD style: scenarios, actions, expected results
- Focus on behavior outcomes, not implementation details
- Structure tests for clarity and communication

### Quality Verification
- **cargo build**: No compile errors
- **cargo test**: All tests pass (BDD-style)
- **cargo clippy**: No warnings (-D warnings)
- **cargo fmt**: Code properly formatted
- All code paths covered with tests before merge

## Error Handling Strategy

- Use `anyhow::Result<T>` for error propagation
- Surface actionable error messages to user (not debug dumps)
- Log detailed context via tracing spans
- Gracefully handle missing mprime, invalid CPU, permission issues
- Never panic; always return Result with clear error messages

## References

- **CoreCycler** — https://github.com/sp00n/CoreCycler (inspiration)
- **Prime95/GIMPS** — https://www.mersenne.org/
- **mprime v30.19** — https://download.mersenne.ca/gimps/v30/30.19/p95v3019b20.linux64.tar.gz
- **Prime95 Wiki** — https://en.wikipedia.org/wiki/Prime95
- **mprime Documentation** — See `mprime-latest/readme.txt`, `stress.txt`, `undoc.txt`
- **CPU Stability Testing** — https://github.com/sp00n/CoreCycler (methodology reference)

## Development Notes

### Machine-Specific Context
- Primary test machine: AMD Ryzen 9 5900X
- Core layout: Non-contiguous physical core IDs (0-5, 8-13 — 12 cores total)
- PBO enabled on all cores; some cores unstable under stress
- Target: Identify which cores are unstable

### Alternative Approaches Evaluated
Task specification requests evaluation of alternatives to mprime for Linux CPU stability testing:
- **linpack** — CPU-intensive; different error patterns
- **stress-ng** — Broad stress testing; less focused on CPU computation errors
- **sysbench** — Benchmark-oriented; not primarily stability-focused
- **mprime** — Industry standard for CPU stability; proven error detection; CoreCycler uses it

Recommendation: mprime is the proven standard for CPU stability detection. Alternatives should be evaluated in future iterations if mprime limitations are encountered.

---

## Learnings

**Iced modal pattern:** Use `stack! + opaque() + mouse_area() + center()` for blocking modal dialogs in Iced 0.14 (see `src/gui_modal.rs`). The `float` widget is for non-blocking overlays (badges/tooltips) — NOT modals.

**fast_qr API:** `QRBuilder::new(content.as_bytes().to_vec()).build()` returns `QRCode`; access modules as `qr_code[row][col].value()` (bool). Add `qr_code` crate as dev-dep for round-trip decode tests (`qr_code::decode::SimpleGrid` + `Grid::new(grid).decode()`).

**`build_modal_content` topology fallback:** Signature is `topology: Option<&CpuTopology>`. When `None`, physical core IDs are used as `bios_index` and `" (physical ID)"` is appended to `error_summary`. MCE-only failures use `build_error_summary()` helper (not `format_error_summary`) to show real MCE type labels.

