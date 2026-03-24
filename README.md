# core-probe

A Linux tool that finds unstable CPU cores on AMD systems. Built for anyone tuning per-core Curve Optimizer (CO) values in BIOS — it tells you exactly which cores are failing so you know where to back off.

It uses mprime (Prime95) under the hood, the same stress test trusted by overclockers for decades. mprime is bundled inside the binary, so there's nothing extra to download.

## Why this exists

If you're running PBO with aggressive Curve Optimizer offsets, some cores will be unstable. The hard part is figuring out *which* ones. Running Prime95 manually on each core is tedious — core-probe automates the whole process and gives you a clear pass/fail per core.

## Core numbering matches your BIOS

This is the important part. Linux numbers your CPU cores differently than your BIOS does.

On multi-CCD chips (like the Ryzen 9 5900X), Linux skips numbers between CCDs. Your 12 cores might show up as 0–5 and 8–13 internally, with a gap where the disabled cores on each CCD would be. But your BIOS Curve Optimizer just lists them as Core 0 through Core 11, no gaps.

core-probe always uses the **BIOS numbering**. When it says Core 6 failed, that's Core 6 in your BIOS Curve Optimizer — no mental translation needed.

Here's what the mapping looks like on a 5900X:

| Core (BIOS / core-probe) | CCD |
|--------------------------|-----|
| 0                        | 0   |
| 1                        | 0   |
| 2                        | 0   |
| 3                        | 0   |
| 4                        | 0   |
| 5                        | 0   |
| 6                        | 1   |
| 7                        | 1   |
| 8                        | 1   |
| 9                        | 1   |
| 10                       | 1   |
| 11                       | 1   |

## Requirements

- Linux (64-bit)
- AMD CPU (the tool checks this and stops if it detects something else)
- Root is only needed if you want to read UEFI/BIOS settings directly (`--uefi-only`)

## Build

```bash
cargo build --release
```

The binary ends up at `target/release/core-probe`.

## Usage

Just run it:

```bash
./core-probe
```

By default it tests every core for 6 minutes each, repeating 3 full cycles, using SSE workloads. That's usually enough to catch instability.

### Common scenarios

```bash
# Only test specific cores (by BIOS number)
./core-probe --cores 6,7,8

# Quick scan: 1 minute per core, 1 cycle, stop on first failure
./core-probe -d 1m -i 1 --bail

# Heavier workload using AVX2
./core-probe -m avx2

# Machine-readable output only (for scripting)
./core-probe -q
```

### All options

| Flag | What it does | Default |
|------|-------------|---------|
| `-d, --duration` | How long to test each core | 6 minutes |
| `-i, --iterations` | How many full cycles through all cores | 3 |
| `-c, --cores` | Only test these cores (comma-separated BIOS numbers) | all |
| `-m, --mode` | Stress test type: `sse`, `avx`, or `avx2` | `sse` |
| `-b, --bail` | Stop immediately when any core fails | off |
| `-q, --quiet` | Only print the machine-readable result line | off |
| `--benchmark` | Run an FFT benchmark instead of stability test | — |

### Stress test modes

- **SSE** — Standard workload. Works on every modern AMD chip. Start here.
- **AVX** — Heavier. Draws more power, generates more heat, finds more issues.
- **AVX2** — Heaviest. If a core passes AVX2, it's solid.

## Output

After testing, you get a report showing which cores passed and which failed. If you use `-q` (quiet mode), you get a single machine-readable line:

```
RESULT: PASS cores=0,1,2,3,4,5,6,7,8,9,10,11
```

Or if something failed:

```
RESULT: FAIL unstable=6,9
```

All core numbers are always BIOS indices — the same numbers you see in your Curve Optimizer.

## What to do with the results

If core-probe reports a core as unstable, reduce that core's Curve Optimizer offset in BIOS. For example, if Core 6 fails, go to your BIOS CO settings, find Core 6, and reduce the negative offset (e.g., from -30 to -20). Then re-run core-probe to verify.

## Adding support for new AGESA versions

core-probe can read Curve Optimizer settings directly from your UEFI firmware variables. This works by knowing where CO data is stored inside the AMD Overclocking (AOD) UEFI variable — but the exact byte layout depends on your BIOS's AGESA version.

Currently supported AGESA versions are defined in `src/co_offsets.rs`. If your AGESA version isn't listed, core-probe falls back to a heuristic scanner that tries to find CO patterns automatically. You can add explicit support for your version:

### How CO data is stored

The AOD UEFI variable (GUID `5ed15dc0-edef-4161-9151-6014c4cc630c`) contains a binary blob with CO settings at fixed offsets. The layout is described by `CoByteLayout`:

```rust
pub struct CoByteLayout {
    pub mode_offset: usize,        // CO mode: 0=Disabled, 1=AllCore, 2=PerCore
    pub signs_offset: usize,       // One u8 per core: 0=positive, 1=negative
    pub magnitudes_offset: usize,  // One u16 LE per core: magnitude value (0-30)
    pub max_cores: usize,          // Maximum cores this layout supports
}
```

Between the mode byte and the signs region there's typically a 4-byte gap. Between signs and magnitudes there's usually a 0x40-byte gap, though this can vary by AGESA version.

### Adding a new layout

1. **Find your AGESA version** — check your BIOS settings or run `dmidecode -t bios` and look for the AGESA string.

2. **Dump the AOD variable** — read the raw bytes from `/sys/firmware/efi/efivars/` using the AOD GUID above. You'll need root access.

3. **Locate the offsets** — set known CO values in BIOS (e.g., PerCore mode, Core 0 = -15), dump the variable, and search for the corresponding byte patterns:
   - Mode byte: `0x02` for PerCore
   - Signs: `0x01` for negative
   - Magnitudes: `0x0F 0x00` for value 15 (u16 LE)

4. **Add the entry** in `src/co_offsets.rs`:

```rust
let known_layouts: &[(_, CoByteLayout)] = &[
    // existing entries...
    (
        "1.2.0.7",  // your AGESA version substring
        CoByteLayout {
            mode_offset: 0x174,
            signs_offset: 0x178,
            magnitudes_offset: 0x1B8,
            max_cores: 16,
        },
    ),
];
```

The version string is matched with `contains()`, so `"1.2.0.7"` matches both `"1.2.0.7"` and `"AGESA V2 PI 1.2.0.7 Patch C"`.

### Heuristic fallback

When no known layout matches, `src/co_heuristic.rs` scans the entire AOD blob for CO-like patterns. It looks for mode bytes (0x01 or 0x02) followed by valid sign regions (all bytes 0x00 or 0x01) and plausible magnitude regions (all u16 LE values ≤ 30 with at least half non-zero). Candidates are ranked by confidence (High/Medium/Low) and proximity to the data's center.

The heuristic works well for standard layouts but adding an explicit entry is more reliable if you've confirmed the offsets.
