```
 ██████╗ ██████╗ ██████╗ ███████╗    ██████╗ ██████╗  ██████╗ ██████╗ ███████╗
██╔════╝██╔═══██╗██╔══██╗██╔════╝    ██╔══██╗██╔══██╗██╔═══██╗██╔══██╗██╔════╝
██║     ██║   ██║██████╔╝█████╗      ██████╔╝██████╔╝██║   ██║██████╔╝█████╗
██║     ██║   ██║██╔══██╗██╔══╝      ██╔═══╝ ██╔══██╗██║   ██║██╔══██╗██╔══╝
╚██████╗╚██████╔╝██║  ██║███████╗    ██║     ██║  ██║╚██████╔╝██████╔╝███████╗
 ╚═════╝ ╚═════╝ ╚═╝  ╚═╝╚══════╝    ╚═╝     ╚═╝  ╚═╝ ╚═════╝ ╚═════╝ ╚══════╝
```

[![Release](https://img.shields.io/github/v/release/RouHim/core-probe?label=release)](https://github.com/RouHim/core-probe/releases)
[![Release Date](https://img.shields.io/github/release-date/RouHim/core-probe)](https://github.com/RouHim/core-probe/releases)
[![CI](https://img.shields.io/github/actions/workflow/status/RouHim/core-probe/pipe.yaml?branch=main&label=ci)](https://github.com/RouHim/core-probe/actions)
[![License](https://img.shields.io/github/license/RouHim/core-probe)](LICENSE)

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

## Installation

### Pre-built binary

Grab the latest binary from the [releases page](https://github.com/RouHim/core-probe/releases):

```bash
curl -L -o core-probe.tar.gz https://github.com/RouHim/core-probe/releases/latest/download/core-probe-x86_64-linux.tar.gz
tar xzf core-probe.tar.gz
sudo install -m755 core-probe /usr/local/bin/core-probe
```

### AUR (Arch Linux)

```bash
# Source build
yay -S core-probe

# Pre-built binary
yay -S core-probe-bin
```

## Usage

### GUI (desktop mode)

Just run `core-probe` with no arguments — it launches a graphical desktop application:

```bash
core-probe
```

The GUI gives you a live dashboard with per-core progress, real-time CPU load graphs, pass/fail results per core, and a detailed report when testing completes. You can configure test duration, iterations, stress mode (SSE/AVX/AVX2), and select specific cores — all from the interface, no command-line flags needed.

### CLI (terminal mode)

For scripting, automation, or headless systems, core-probe also provides a command-line interface. Run `core-probe --help` to see all available flags:

```bash
# Test specific cores with AVX2, 1 minute each
core-probe -c 6,7,8 -m avx2 -d 1m

# Quick scan: stop on first failure
core-probe -d 1m -i 1 --bail

# Machine-readable output only
core-probe -q
```

Key flags: `-d`/`--duration`, `-i`/`--iterations`, `-c`/`--cores`, `-m`/`--mode` (sse/avx/avx2), `-b`/`--bail`, `-q`/`--quiet`. See `--help` for the full list.

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

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for build instructions, development setup, and guidance on adding support for new AGESA versions.
