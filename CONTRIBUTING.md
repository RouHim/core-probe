# Contributing to core-probe

## Building from source

```bash
git clone https://github.com/RouHim/core-probe.git
cd core-probe
cargo build --release
```

The binary ends up at `target/release/core-probe`.

## Development setup

core-probe is written in Rust. Standard workflow:

```bash
cargo build        # compile
cargo test         # run tests
cargo clippy       # lint (must be warning-clean)
cargo fmt          # format
```

Key conventions:

- **Error handling**: Use `anyhow::Result<T>`. No `unwrap()` or `expect()` in non-test code.
- **Logging**: Use `tracing` + `tracing_subscriber` with contextual spans. Avoid bare `println!`.
- **CLI parsing**: Uses `argh`, not clap.
- **No Rust modules**: Code is organized into individual `.rs` files in `src/`, each with a single responsibility.
- **GUI**: Uses `iced` 0.14. No external async runtime — Iced's built-in executor is used for GUI event loops.

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
