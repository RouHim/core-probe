use anyhow::Result;
use tracing::{debug, instrument};

/// Stress test mode controlling which CPU instruction sets to use.
///
/// Different instruction sets apply different load characteristics:
/// - SSE: Lower power draw, enables higher boost clocks (best for instability detection)
/// - AVX/AVX2: Higher power draw, lower boost clocks
/// - AVX512: Highest power draw, lowest boost clocks
/// - Custom: Manual override of CPU feature flags
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StressTestMode {
    /// SSE/SSE2 only (disables AVX/AVX2/AVX512) - default for instability detection
    #[default]
    SSE,
    /// AVX enabled (disables AVX2/AVX512)
    AVX,
    /// AVX2 enabled (disables AVX512)
    AVX2,
    /// AVX512 enabled
    AVX512,
    /// Custom CPU feature configuration (advanced users only)
    Custom {
        sse: bool,
        sse2: bool,
        avx: bool,
        avx2: bool,
        fma3: bool,
        avx512f: bool,
    },
}

/// FFT size presets matching CoreCycler configurations.
///
/// FFT sizes determine which CPU components are stressed:
/// - Smallest/Small: L1/L2 cache-focused
/// - Large/Huge: Memory-focused, best for instability detection
/// - Moderate/Heavy/HeavyShort: Various cache/memory balance points
///
/// Ranges are specified in KB.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FftPreset {
    /// 4K-21K: Primarily L1/L2 cache
    Smallest,
    /// 36K-248K: L2/L3 cache
    Small,
    /// 426K-8192K: L3 cache and memory
    Large,
    /// 8960K-32768K: Memory-focused (default, matches CoreCycler)
    #[default]
    Huge,
    /// 1344K-4096K: Balanced cache/memory
    Moderate,
    /// 4K-1344K: Full cache hierarchy
    Heavy,
    /// 4K-160K: Quick cache test
    HeavyShort,
}

impl FftPreset {
    /// Returns (min_fft_kb, max_fft_kb) for this preset.
    pub fn fft_range_kb(self) -> (u32, u32) {
        match self {
            Self::Smallest => (4, 21),
            Self::Small => (36, 248),
            Self::Large => (426, 8192),
            Self::Huge => (8960, 32768),
            Self::Moderate => (1344, 4096),
            Self::Heavy => (4, 1344),
            Self::HeavyShort => (4, 160),
        }
    }

    /// Returns the human-readable name of this preset.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Smallest => "Smallest",
            Self::Small => "Small",
            Self::Large => "Large",
            Self::Huge => "Huge",
            Self::Moderate => "Moderate",
            Self::Heavy => "Heavy",
            Self::HeavyShort => "HeavyShort",
        }
    }

    /// Returns a static slice of all 7 FFT presets in order.
    pub fn all_presets() -> &'static [FftPreset] {
        &[
            FftPreset::Smallest,
            FftPreset::Small,
            FftPreset::Large,
            FftPreset::Huge,
            FftPreset::Moderate,
            FftPreset::Heavy,
            FftPreset::HeavyShort,
        ]
    }
}

impl std::fmt::Display for FftPreset {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let (min, max) = self.fft_range_kb();
        write!(f, "{} ({}K-{}K)", self.name(), min, max)
    }
}

/// Configuration for mprime stress testing.
///
/// Generates prime.txt configuration file content for mprime v30.19.
/// Uses sensible defaults optimized for AMD CPU instability detection:
/// - SSE mode (lower power = higher boost = better instability detection)
/// - Huge FFT preset (8960K-32768K, memory-focused)
/// - 3 minutes per FFT size (internal mprime timing)
/// - Single thread (maximizes per-core boost frequency)
/// - Error checking enabled (catches numerical instability)
/// - Primenet disabled (no network usage)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MprimeConfig {
    /// CPU instruction set mode (default: SSE)
    mode: StressTestMode,
    /// FFT size preset (default: Huge)
    fft_preset: FftPreset,
    /// Minutes to run each FFT size (default: 3)
    torture_time: u32,
    /// Memory allocation in MB (default: 0 = in-place FFTs)
    memory: u32,
    /// Number of threads (default: 1)
    threads: u32,
    /// Enable error checking (default: true, always recommended)
    error_check: bool,
    /// Use Primenet (always false for this tool)
    use_primenet: bool,
    /// Computer GUID for mprime (32-char lowercase hex, no hyphens)
    computer_guid: String,
    /// When true, tells mprime NOT to manage CPU affinity internally via hwloc.
    /// Instead, the process relies on OS-level sched_setaffinity set in pre_exec.
    /// Also generates NumCores=1 to prevent mprime from spawning multiple workers.
    disable_internal_affinity: bool,
}

impl Default for MprimeConfig {
    fn default() -> Self {
        Self {
            mode: StressTestMode::SSE,
            fft_preset: FftPreset::Huge,
            torture_time: 3,
            memory: 0,
            threads: 1,
            error_check: true,
            use_primenet: false,
            disable_internal_affinity: false,
            computer_guid: uuid::Uuid::new_v4().simple().to_string(),
        }
    }
}

impl MprimeConfig {
    /// Creates a new configuration builder with default values.
    pub fn builder() -> Self {
        Self::default()
    }

    /// Sets the stress test mode (SSE, AVX, AVX2, AVX512, Custom).
    pub fn mode(mut self, mode: StressTestMode) -> Self {
        self.mode = mode;
        self
    }

    /// Sets the FFT size preset.
    pub fn fft_preset(mut self, preset: FftPreset) -> Self {
        self.fft_preset = preset;
        self
    }

    /// Sets torture time per FFT size in minutes.
    pub fn torture_time(mut self, minutes: u32) -> Self {
        self.torture_time = minutes;
        self
    }

    /// Sets memory allocation in MB (0 = in-place FFTs).
    pub fn memory(mut self, mb: u32) -> Self {
        self.memory = mb;
        self
    }

    /// Sets number of threads (1 recommended for per-core testing).
    pub fn threads(mut self, count: u32) -> Self {
        self.threads = count;
        self
    }

    /// Sets error checking (always recommended).
    pub fn error_check(mut self, enable: bool) -> Self {
        self.error_check = enable;
        self
    }

    /// Disables mprime's internal hwloc-based affinity management.
    ///
    /// When enabled, generates `EnableSetAffinity=0` and `NumCores=1` in prime.txt.
    /// mprime will not override the OS-level CPU affinity set via `sched_setaffinity`
    /// in the child process's `pre_exec` hook. This avoids the hwloc CPU numbering
    /// mismatch where mprime's Affinity=N uses hwloc PU# (not Linux logical CPU IDs).
    pub fn disable_internal_affinity(mut self) -> Self {
        self.disable_internal_affinity = true;
        self
    }

    /// Generates prime.txt configuration file content.
    ///
    /// Returns a string containing all required mprime configuration keys
    /// for stress testing, formatted for writing to prime.txt.
    #[instrument(skip(self), level = "debug")]
    pub fn generate(&self) -> Result<String> {
        let (min_fft, max_fft) = self.fft_preset.fft_range_kb();

        // Determine CPU feature flags based on mode
        let (sse, sse2, avx, avx2, fma3, avx512f) = match self.mode {
            StressTestMode::SSE => (1, 1, 0, 0, 0, 0),
            StressTestMode::AVX => (1, 1, 1, 0, 1, 0),
            StressTestMode::AVX2 => (1, 1, 1, 1, 1, 0),
            StressTestMode::AVX512 => (1, 1, 1, 1, 1, 1),
            StressTestMode::Custom {
                sse: c_sse,
                sse2: c_sse2,
                avx: c_avx,
                avx2: c_avx2,
                fma3: c_fma3,
                avx512f: c_avx512f,
            } => (
                c_sse as u8,
                c_sse2 as u8,
                c_avx as u8,
                c_avx2 as u8,
                c_fma3 as u8,
                c_avx512f as u8,
            ),
        };

        debug!(
            mode = ?self.mode,
            fft_range_kb = ?(min_fft, max_fft),
            torture_time_min = self.torture_time,
            threads = self.threads,
            "Generating mprime config"
        );

        // When internal affinity is disabled, we set:
        // - EnableSetAffinity=0: Prevents mprime from using hwloc to set CPU affinity
        // - NumCores=1: Tells mprime only one core is available, preventing multiple workers
        let affinity_lines = if self.disable_internal_affinity {
            "EnableSetAffinity=0\nNumCores=1\n"
        } else {
            ""
        };

        let config = format!(
            r#"V30OptionsConverted=1
StressTester=1
UsePrimenet=0
{affinity_lines}MinTortureFFT={min_fft}
MaxTortureFFT={max_fft}
TortureMem={memory}
TortureTime={torture_time}
CpuSupportsSSE={sse}
CpuSupportsSSE2={sse2}
CpuSupportsAVX={avx}
CpuSupportsAVX2={avx2}
CpuSupportsFMA3={fma3}
CpuSupportsAVX512F={avx512f}
ErrorCheck={error_check}
TortureHyperthreading=0
NumWorkers=1
CoresPerTest=1
TortureWeak=0
WorkPreference=0
ComputerGUID={computer_guid}
TortureThreads={threads}
ResultsFile=results.txt
"#,
            affinity_lines = affinity_lines,
            min_fft = min_fft,
            max_fft = max_fft,
            memory = self.memory,
            torture_time = self.torture_time,
            sse = sse,
            sse2 = sse2,
            avx = avx,
            avx2 = avx2,
            fma3 = fma3,
            avx512f = avx512f,
            error_check = self.error_check as u8,
            threads = self.threads,
            computer_guid = &self.computer_guid,
        );

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_sse_mode_when_generating_config_then_disables_avx_flags() {
        // Given: SSE mode configuration
        let config = MprimeConfig::builder().mode(StressTestMode::SSE);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: AVX/AVX2/FMA3/AVX512 are disabled
        assert!(result.contains("CpuSupportsSSE=1"));
        assert!(result.contains("CpuSupportsSSE2=1"));
        assert!(result.contains("CpuSupportsAVX=0"));
        assert!(result.contains("CpuSupportsAVX2=0"));
        assert!(result.contains("CpuSupportsFMA3=0"));
        assert!(result.contains("CpuSupportsAVX512F=0"));
    }

    #[test]
    fn given_avx2_mode_when_generating_then_enables_avx_and_avx2() {
        // Given: AVX2 mode configuration
        let config = MprimeConfig::builder().mode(StressTestMode::AVX2);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: SSE, AVX, AVX2, FMA3 enabled; AVX512 disabled
        assert!(result.contains("CpuSupportsSSE=1"));
        assert!(result.contains("CpuSupportsSSE2=1"));
        assert!(result.contains("CpuSupportsAVX=1"));
        assert!(result.contains("CpuSupportsAVX2=1"));
        assert!(result.contains("CpuSupportsFMA3=1"));
        assert!(result.contains("CpuSupportsAVX512F=0"));
    }

    #[test]
    fn given_huge_fft_preset_when_generating_then_sets_correct_fft_range() {
        // Given: Huge FFT preset
        let config = MprimeConfig::builder().fft_preset(FftPreset::Huge);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: FFT range is 8960K-32768K
        assert!(result.contains("MinTortureFFT=8960"));
        assert!(result.contains("MaxTortureFFT=32768"));
    }

    #[test]
    fn given_default_config_when_generating_then_disables_primenet() {
        // Given: Default configuration
        let config = MprimeConfig::default();

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: Primenet is disabled and stress tester enabled
        assert!(result.contains("UsePrimenet=0"));
        assert!(result.contains("StressTester=1"));
    }

    #[test]
    fn given_config_when_writing_then_creates_valid_prime_txt() {
        // Given: Configuration with explicit parameters
        let config = MprimeConfig::builder()
            .mode(StressTestMode::SSE)
            .fft_preset(FftPreset::Huge)
            .torture_time(3)
            .threads(1)
            .memory(0)
            .error_check(true);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: All required keys are present
        assert!(result.contains("V30OptionsConverted=1"));
        assert!(result.contains("StressTester=1"));
        assert!(result.contains("UsePrimenet=0"));
        assert!(result.contains("MinTortureFFT="));
        assert!(result.contains("MaxTortureFFT="));
        assert!(result.contains("TortureMem=0"));
        assert!(result.contains("TortureTime=3"));
        assert!(result.contains("ErrorCheck=1"));
        assert!(result.contains("TortureHyperthreading=0"));
        assert!(result.contains("TortureThreads=1"));
        assert!(result.contains("ResultsFile=results.txt"));
    }

    #[test]
    fn given_custom_fft_range_when_generating_then_uses_provided_values() {
        // Given: Small FFT preset (custom range)
        let config = MprimeConfig::builder().fft_preset(FftPreset::Small);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: FFT range matches Small preset (36K-248K)
        assert!(result.contains("MinTortureFFT=36"));
        assert!(result.contains("MaxTortureFFT=248"));
    }

    #[test]
    fn given_avx_mode_when_generating_then_enables_avx_but_not_avx2() {
        // Given: AVX mode configuration
        let config = MprimeConfig::builder().mode(StressTestMode::AVX);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: AVX and FMA3 enabled, AVX2 and AVX512 disabled
        assert!(result.contains("CpuSupportsAVX=1"));
        assert!(result.contains("CpuSupportsFMA3=1"));
        assert!(result.contains("CpuSupportsAVX2=0"));
        assert!(result.contains("CpuSupportsAVX512F=0"));
    }

    #[test]
    fn given_avx512_mode_when_generating_then_enables_all_features() {
        // Given: AVX512 mode configuration
        let config = MprimeConfig::builder().mode(StressTestMode::AVX512);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: All CPU features enabled
        assert!(result.contains("CpuSupportsSSE=1"));
        assert!(result.contains("CpuSupportsSSE2=1"));
        assert!(result.contains("CpuSupportsAVX=1"));
        assert!(result.contains("CpuSupportsAVX2=1"));
        assert!(result.contains("CpuSupportsFMA3=1"));
        assert!(result.contains("CpuSupportsAVX512F=1"));
    }

    #[test]
    fn given_all_fft_presets_when_queried_then_return_valid_ranges() {
        // Given/When/Then: All presets return valid (min < max) ranges
        let presets = [
            FftPreset::Smallest,
            FftPreset::Small,
            FftPreset::Large,
            FftPreset::Huge,
            FftPreset::Moderate,
            FftPreset::Heavy,
            FftPreset::HeavyShort,
        ];

        for preset in presets {
            let (min, max) = preset.fft_range_kb();
            assert!(
                min < max,
                "Preset {:?} has invalid range: {} >= {}",
                preset,
                min,
                max
            );
            assert!(min >= 4, "Preset {:?} min FFT too small: {}", preset, min);
            assert!(
                max <= 32768,
                "Preset {:?} max FFT too large: {}",
                preset,
                max
            );
        }
    }

    #[test]
    fn given_custom_mode_when_generating_then_uses_custom_flags() {
        // Given: Custom mode with specific flag configuration
        let config = MprimeConfig::builder().mode(StressTestMode::Custom {
            sse: true,
            sse2: true,
            avx: false,
            avx2: true, // Intentionally inconsistent for testing
            fma3: false,
            avx512f: false,
        });

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: Custom flags are respected exactly
        assert!(result.contains("CpuSupportsSSE=1"));
        assert!(result.contains("CpuSupportsSSE2=1"));
        assert!(result.contains("CpuSupportsAVX=0"));
        assert!(result.contains("CpuSupportsAVX2=1"));
        assert!(result.contains("CpuSupportsFMA3=0"));
        assert!(result.contains("CpuSupportsAVX512F=0"));
    }

    #[test]
    fn given_error_check_disabled_when_generating_then_sets_zero() {
        // Given: Configuration with error checking disabled
        let config = MprimeConfig::builder().error_check(false);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: ErrorCheck is set to 0
        assert!(result.contains("ErrorCheck=0"));
    }

    #[test]
    fn given_custom_torture_time_when_generating_then_uses_value() {
        // Given: Configuration with 10 minute torture time
        let config = MprimeConfig::builder().torture_time(10);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: TortureTime is set to 10
        assert!(result.contains("TortureTime=10"));
    }

    #[test]
    fn given_multiple_threads_when_generating_then_sets_count() {
        // Given: Configuration with 4 threads
        let config = MprimeConfig::builder().threads(4);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: TortureThreads is set to 4
        assert!(result.contains("TortureThreads=4"));
    }

    #[test]
    fn given_memory_allocation_when_generating_then_sets_value() {
        // Given: Configuration with 1024MB memory
        let config = MprimeConfig::builder().memory(1024);

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: TortureMem is set to 1024
        assert!(result.contains("TortureMem=1024"));
    }

    #[test]
    fn given_internal_affinity_disabled_when_generating_then_adds_enable_set_affinity_zero_and_num_cores(
    ) {
        // Given: Configuration with internal affinity disabled
        let config = MprimeConfig::builder().disable_internal_affinity();

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: EnableSetAffinity=0 is present
        assert!(
            result.contains("EnableSetAffinity=0"),
            "config should contain EnableSetAffinity=0 when internal affinity is disabled"
        );

        // Then: NumCores=1 is present
        assert!(
            result.contains("NumCores=1"),
            "config should contain NumCores=1 when internal affinity is disabled"
        );

        // Then: No [Worker #1] section (we rely on OS affinity, not mprime's)
        assert!(
            !result.contains("[Worker #1]"),
            "config should NOT contain [Worker #1] section — OS handles affinity"
        );
    }

    #[test]
    fn given_default_config_when_generating_then_omits_affinity_and_num_cores_settings() {
        // Given: Default configuration without affinity disabled
        let config = MprimeConfig::builder();

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: EnableSetAffinity is not present (mprime uses its default)
        assert!(
            !result.contains("EnableSetAffinity"),
            "config should not contain EnableSetAffinity when using defaults"
        );

        // Then: NumCores is not present
        assert!(
            !result.contains("NumCores"),
            "config should not contain NumCores when using defaults"
        );
    }
    #[test]
    fn given_default_config_when_generating_then_uses_huge_fft_range() {
        // Given: default configuration (no explicit FFT preset set)
        let config = MprimeConfig::builder();

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: Default uses Huge FFT range (CoreCycler default for PBO instability detection)
        assert!(
            result.contains("MinTortureFFT=8960"),
            "default should use Huge FFT min"
        );
        assert!(
            result.contains("MaxTortureFFT=32768"),
            "default should use Huge FFT max"
        );
    }

    #[test]
    fn given_config_when_generating_then_includes_all_required_keys() {
        // Given: default configuration
        let config = MprimeConfig::builder();

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: All required keys are present
        assert!(result.contains("NumWorkers=1"), "should include NumWorkers");
        assert!(
            result.contains("CoresPerTest=1"),
            "should include CoresPerTest"
        );
        assert!(
            result.contains("TortureWeak=0"),
            "should include TortureWeak"
        );
        assert!(
            result.contains("WorkPreference=0"),
            "should include WorkPreference"
        );
    }

    #[test]
    fn given_config_when_generating_then_includes_computer_guid() {
        // Given: default configuration
        let config = MprimeConfig::builder();

        // When: Generating configuration
        let result = config.generate().expect("should generate config");

        // Then: ComputerGUID is present and is 32 lowercase hex chars (no hyphens)
        assert!(
            result.contains("ComputerGUID="),
            "should include ComputerGUID key"
        );
        let guid_line = result
            .lines()
            .find(|l| l.starts_with("ComputerGUID="))
            .expect("ComputerGUID line must exist");
        let guid = guid_line.trim_start_matches("ComputerGUID=");
        assert_eq!(guid.len(), 32, "GUID must be exactly 32 chars, got: {guid}");
        assert!(
            guid.chars()
                .all(|c| c.is_ascii_hexdigit() && !c.is_uppercase()),
            "GUID must be lowercase hex, got: {guid}"
        );
    }

    #[test]
    fn given_two_configs_when_generating_then_guids_differ() {
        // Given: two independent config instances
        let config1 = MprimeConfig::builder();
        let config2 = MprimeConfig::builder();

        // When: Generating both configurations
        let result1 = config1.generate().expect("should generate config 1");
        let result2 = config2.generate().expect("should generate config 2");

        // Then: ComputerGUIDs are unique (each instance gets a fresh UUID)
        let guid1 = result1
            .lines()
            .find(|l| l.starts_with("ComputerGUID="))
            .expect("first config must have GUID")
            .trim_start_matches("ComputerGUID=")
            .to_string();
        let guid2 = result2
            .lines()
            .find(|l| l.starts_with("ComputerGUID="))
            .expect("second config must have GUID")
            .trim_start_matches("ComputerGUID=")
            .to_string();
        assert_ne!(guid1, guid2, "each config should have a unique GUID");
    }

    #[test]
    fn given_fft_preset_large_when_display_then_formats_with_range() {
        // Given: FftPreset::Large
        let preset = FftPreset::Large;

        // When: Converting to string
        let display_str = preset.to_string();

        // Then: Displays as "Large (426K-8192K)"
        assert_eq!(display_str, "Large (426K-8192K)");
    }

    #[test]
    fn given_fft_preset_huge_when_display_then_formats_correctly() {
        // Given: FftPreset::Huge
        let preset = FftPreset::Huge;

        // When: Converting to string
        let display_str = preset.to_string();

        // Then: Displays as "Huge (8960K-32768K)"
        assert_eq!(display_str, "Huge (8960K-32768K)");
    }

    #[test]
    fn given_fft_preset_when_name_called_then_returns_human_readable_name() {
        // Given: All 7 presets
        // When: Calling name() on each
        // Then: Returns correct human-readable names
        assert_eq!(FftPreset::Smallest.name(), "Smallest");
        assert_eq!(FftPreset::Small.name(), "Small");
        assert_eq!(FftPreset::Large.name(), "Large");
        assert_eq!(FftPreset::Huge.name(), "Huge");
        assert_eq!(FftPreset::Moderate.name(), "Moderate");
        assert_eq!(FftPreset::Heavy.name(), "Heavy");
        assert_eq!(FftPreset::HeavyShort.name(), "HeavyShort");
    }

    #[test]
    fn given_all_presets_when_calling_then_returns_7_variants() {
        // Given: FftPreset::all_presets()
        let presets = FftPreset::all_presets();

        // When: Checking length
        // Then: Returns exactly 7 presets
        assert_eq!(presets.len(), 7);
    }

    #[test]
    fn given_all_presets_when_iterating_then_preserves_order() {
        // Given: FftPreset::all_presets()
        let presets = FftPreset::all_presets();

        // When: Checking each preset
        // Then: Order is Smallest, Small, Large, Huge, Moderate, Heavy, HeavyShort
        assert_eq!(presets[0], FftPreset::Smallest);
        assert_eq!(presets[1], FftPreset::Small);
        assert_eq!(presets[2], FftPreset::Large);
        assert_eq!(presets[3], FftPreset::Huge);
        assert_eq!(presets[4], FftPreset::Moderate);
        assert_eq!(presets[5], FftPreset::Heavy);
        assert_eq!(presets[6], FftPreset::HeavyShort);
    }
}
