# core-probe

core-probe is a Linux CLI tool designed to identify unstable AMD CPU cores using the mprime (Prime95) stress test. Inspired by CoreCycler, it systematically cycles through each CPU core, runs mprime stress tests, monitors for failures via system error logs, and generates a report identifying cores that fail stability tests.

The tool is specifically built for AMD CPU owners using BIOS Curve Optimizer (PBO) to tune per-core voltages. It provides the exact core indices needed to adjust BIOS settings, bridging the gap between Linux physical core IDs and BIOS core numbering.

## BIOS Core Index Mapping

AMD multi-CCD CPUs expose non-contiguous physical core IDs in Linux. For example, on a Ryzen 9 5900X:
- CCD0 cores have physical IDs: 0, 1, 2, 3, 4, 5
- CCD1 cores have physical IDs: 8, 9, 10, 11, 12, 13
- IDs 6 and 7 are missing as they represent disabled cores in the 8-core CCD.

The BIOS Curve Optimizer shows cores as 0-11 sequentially. If core-probe reported Linux physical IDs, a user would attempt to adjust "core 8" in BIOS but only find cores 0-11, leading to incorrect tuning.

### Mapping Algorithm

core-probe solves this by mapping physical IDs to sequential BIOS indices:

```rust
let bios_map: BTreeMap<u32, u32> = core_map
    .keys()               // sorted physical IDs (BTreeMap guarantees order)
    .enumerate()          // 0, 1, 2, ... sequential = BIOS indices
    .map(|(idx, &phys)| (phys, idx as u32))
    .collect();
```

### Ryzen 9 5900X Example (12-core, 2 CCD)

| BIOS Index | Physical Core ID | CCD  |
|-----------|-----------------|------|
| 0         | 0               | CCD0 |
| 1         | 1               | CCD0 |
| 2         | 2               | CCD0 |
| 3         | 3               | CCD0 |
| 4         | 4               | CCD0 |
| 5         | 5               | CCD0 |
| 6         | 8               | CCD1 |
| 7         | 9               | CCD1 |
| 8         | 10              | CCD1 |
| 9         | 11              | CCD1 |
| 10        | 12              | CCD1 |
| 11        | 13              | CCD1 |

core-probe uses BIOS indices in all user-facing output, stability reports, and the `--cores` flag. Physical core IDs are handled internally.

## Requirements

- Linux (64-bit)
- AMD CPU (tool aborts if non-AMD detected)
- mprime v30.19 is embedded in the binary
- Root access is only required when using the `--uefi-only` flag to read UEFI settings

## Build

```bash
git clone <repo>
cd core-probe
cargo build --release
./target/release/core-probe --help
```

## Usage

```
Usage: core-probe [-d <duration>] [-i <iterations>] [-c <cores>] [-q] [-b] [-m <mode>] [--benchmark] [--uefi-only]

Options:
  -d, --duration    duration to test each core (default: 6m)
  -i, --iterations  number of full cycles through all cores (default: 3)
  -c, --cores       only test specific cores by BIOS index (comma-separated, e.g. "0,2,5")
  -q, --quiet       only output machine-readable RESULT line
  -b, --bail        stop testing immediately when the first core fails
  -m, --mode        stress test mode: sse, avx, avx2 (default: sse)
  --benchmark       run FFT preset benchmark
  --uefi-only       internal: read UEFI settings as root
  --help            display usage information
```

### Examples

```bash
# Test all cores with defaults (6 min each, 3 cycles, SSE)
./core-probe

# Test only BIOS cores 6, 7, 8 (CCD1 first 3 cores on 5900X)
./core-probe --cores 6,7,8

# Quick check: 1 minute per core, 1 cycle, bail on first failure
./core-probe -d 1m -i 1 --bail

# Heavy load with AVX2, quiet machine-readable output
./core-probe -m avx2 -q

# Run FFT benchmark to see throughput
./core-probe --benchmark
```

## Stress Test Modes

- **sse** (default): Recommended for general stability testing; works on all modern AMD CPUs.
- **avx**: Uses AVX instructions for a heavier load.
- **avx2**: Uses AVX2 instructions for the heaviest computational load.

## Output Format

The tool provides a stability report listing stable and unstable cores. In quiet mode (`-q`), it prints a machine-readable result line:

```
RESULT: PASS cores=0,1,2,3,4,5,6,7,8,9,10,11
```

Or on failure:

```
RESULT: FAIL unstable=6,9
```

All core numbers in the output are BIOS indices.
