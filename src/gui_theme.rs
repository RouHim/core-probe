use iced::theme::Palette;
use iced::{Color, Theme};

use crate::coordinator::CoreStatus;
use crate::gui_events::LogLevel;

// ---------------------------------------------------------------------------
// ThemeMode
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeMode {
    Light,
    Dark,
    System,
}

// ---------------------------------------------------------------------------
// Dark palette colors (from wireframe CSS :root)
// ---------------------------------------------------------------------------

pub const DARK_BG_PRIMARY: Color = Color::from_rgb(
    0x12 as f32 / 255.0,
    0x12 as f32 / 255.0,
    0x12 as f32 / 255.0,
);
pub const DARK_BG_SECONDARY: Color = Color::from_rgb(
    0x1e as f32 / 255.0,
    0x1e as f32 / 255.0,
    0x1e as f32 / 255.0,
);
pub const DARK_BG_TERTIARY: Color = Color::from_rgb(
    0x2a as f32 / 255.0,
    0x2a as f32 / 255.0,
    0x2a as f32 / 255.0,
);

pub const DARK_TEXT_PRIMARY: Color = Color::WHITE;
pub const DARK_TEXT_SECONDARY: Color = Color::from_rgb(
    0xb3 as f32 / 255.0,
    0xb3 as f32 / 255.0,
    0xb3 as f32 / 255.0,
);
pub const DARK_TEXT_MUTED: Color = Color::from_rgb(
    0x80 as f32 / 255.0,
    0x80 as f32 / 255.0,
    0x80 as f32 / 255.0,
);

pub const DARK_BORDER: Color = Color::from_rgb(
    0x33 as f32 / 255.0,
    0x33 as f32 / 255.0,
    0x33 as f32 / 255.0,
);
pub const DARK_HEADER_BG: Color = Color::from_rgb(
    0x1a as f32 / 255.0,
    0x1a as f32 / 255.0,
    0x1a as f32 / 255.0,
);
pub const DARK_STATUS_BAR_BG: Color = Color::from_rgb(
    0x1a as f32 / 255.0,
    0x1a as f32 / 255.0,
    0x1a as f32 / 255.0,
);

pub const DARK_PROGRESS_FILL: Color = Color::from_rgb(
    0x4c as f32 / 255.0,
    0xaf as f32 / 255.0,
    0x50 as f32 / 255.0,
);
pub const DARK_PROGRESS_BG: Color = Color::from_rgb(
    0x33 as f32 / 255.0,
    0x33 as f32 / 255.0,
    0x33 as f32 / 255.0,
);

pub const DARK_LOG_BG: Color = Color::BLACK;

pub const DARK_BADGE_PBO_BG: Color = Color::from_rgb(
    0x31 as f32 / 255.0,
    0x1b as f32 / 255.0,
    0x92 as f32 / 255.0,
);
pub const DARK_BADGE_PBO_TEXT: Color = Color::from_rgb(
    0xb3 as f32 / 255.0,
    0x88 as f32 / 255.0,
    0xff as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// CO tier text colors
// ---------------------------------------------------------------------------

pub const DARK_CO_GOLD: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xc0 as f32 / 255.0,
    0x30 as f32 / 255.0,
);
pub const DARK_CO_SILVER: Color = Color::from_rgb(
    0xc8 as f32 / 255.0,
    0xc8 as f32 / 255.0,
    0xd0 as f32 / 255.0,
);
pub const DARK_CO_BRONZE: Color = Color::from_rgb(
    0xe0 as f32 / 255.0,
    0x8c as f32 / 255.0,
    0x40 as f32 / 255.0,
);
pub const DARK_CO_GOLD_BG: Color = Color::from_rgb(
    0x4d as f32 / 255.0,
    0x3a as f32 / 255.0,
    0x12 as f32 / 255.0,
);
pub const DARK_CO_GOLD_BORDER: Color = Color::from_rgb(
    0xc8 as f32 / 255.0,
    0x98 as f32 / 255.0,
    0x24 as f32 / 255.0,
);
pub const DARK_CO_SILVER_BG: Color = Color::from_rgb(
    0x34 as f32 / 255.0,
    0x37 as f32 / 255.0,
    0x40 as f32 / 255.0,
);
pub const DARK_CO_SILVER_BORDER: Color = Color::from_rgb(
    0x92 as f32 / 255.0,
    0x98 as f32 / 255.0,
    0xa4 as f32 / 255.0,
);
pub const DARK_CO_BRONZE_BG: Color = Color::from_rgb(
    0x4a as f32 / 255.0,
    0x2b as f32 / 255.0,
    0x17 as f32 / 255.0,
);
pub const DARK_CO_BRONZE_BORDER: Color = Color::from_rgb(
    0xb2 as f32 / 255.0,
    0x6a as f32 / 255.0,
    0x30 as f32 / 255.0,
);
pub const LIGHT_CO_GOLD: Color = Color::from_rgb(
    0xb8 as f32 / 255.0,
    0x86 as f32 / 255.0,
    0x0b as f32 / 255.0,
);
pub const LIGHT_CO_SILVER: Color = Color::from_rgb(
    0x60 as f32 / 255.0,
    0x60 as f32 / 255.0,
    0x68 as f32 / 255.0,
);
pub const LIGHT_CO_BRONZE: Color = Color::from_rgb(
    0x8b as f32 / 255.0,
    0x45 as f32 / 255.0,
    0x13 as f32 / 255.0,
);
pub const LIGHT_CO_GOLD_BG: Color = Color::from_rgb(
    0xf3 as f32 / 255.0,
    0xe7 as f32 / 255.0,
    0xbd as f32 / 255.0,
);
pub const LIGHT_CO_GOLD_BORDER: Color = Color::from_rgb(
    0xd2 as f32 / 255.0,
    0xb4 as f32 / 255.0,
    0x49 as f32 / 255.0,
);
pub const LIGHT_CO_SILVER_BG: Color = Color::from_rgb(
    0xe5 as f32 / 255.0,
    0xe7 as f32 / 255.0,
    0xec as f32 / 255.0,
);
pub const LIGHT_CO_SILVER_BORDER: Color = Color::from_rgb(
    0xb7 as f32 / 255.0,
    0xbd as f32 / 255.0,
    0xc8 as f32 / 255.0,
);
pub const LIGHT_CO_BRONZE_BG: Color = Color::from_rgb(
    0xef as f32 / 255.0,
    0xd9 as f32 / 255.0,
    0xc9 as f32 / 255.0,
);
pub const LIGHT_CO_BRONZE_BORDER: Color = Color::from_rgb(
    0xc3 as f32 / 255.0,
    0x91 as f32 / 255.0,
    0x6a as f32 / 255.0,
);

pub const DARK_BUTTON_BG: Color = Color::from_rgb(
    0x33 as f32 / 255.0,
    0x33 as f32 / 255.0,
    0x33 as f32 / 255.0,
);
pub const DARK_BUTTON_TEXT: Color = Color::WHITE;

// ---------------------------------------------------------------------------
// Card border colors
// ---------------------------------------------------------------------------

pub const DARK_CARD_BORDER: Color = Color::from_rgb(
    0x3a as f32 / 255.0,
    0x3a as f32 / 255.0,
    0x3a as f32 / 255.0,
);
pub const LIGHT_CARD_BORDER: Color = Color::from_rgb(
    0xd0 as f32 / 255.0,
    0xd0 as f32 / 255.0,
    0xd0 as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// CCD container colors
// ---------------------------------------------------------------------------

pub const DARK_CCD_BG: Color = Color::from_rgb(
    0x18 as f32 / 255.0,
    0x18 as f32 / 255.0,
    0x18 as f32 / 255.0,
);
pub const LIGHT_CCD_BG: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
);
pub const DARK_CCD_BORDER: Color = Color::from_rgb(
    0x30 as f32 / 255.0,
    0x30 as f32 / 255.0,
    0x30 as f32 / 255.0,
);
pub const LIGHT_CCD_BORDER: Color = Color::from_rgb(
    0xcc as f32 / 255.0,
    0xcc as f32 / 255.0,
    0xcc as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// Light palette colors (from wireframe CSS [data-theme="light"])
// ---------------------------------------------------------------------------

pub const LIGHT_BG_PRIMARY: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
);
pub const LIGHT_BG_SECONDARY: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xff as f32 / 255.0,
    0xff as f32 / 255.0,
);
pub const LIGHT_BG_TERTIARY: Color = Color::from_rgb(
    0xe0 as f32 / 255.0,
    0xe0 as f32 / 255.0,
    0xe0 as f32 / 255.0,
);

pub const LIGHT_TEXT_PRIMARY: Color = Color::BLACK;
pub const LIGHT_TEXT_SECONDARY: Color = Color::from_rgb(
    0x42 as f32 / 255.0,
    0x42 as f32 / 255.0,
    0x42 as f32 / 255.0,
);
pub const LIGHT_TEXT_MUTED: Color = Color::from_rgb(
    0x75 as f32 / 255.0,
    0x75 as f32 / 255.0,
    0x75 as f32 / 255.0,
);

pub const LIGHT_BORDER: Color = Color::from_rgb(
    0xcc as f32 / 255.0,
    0xcc as f32 / 255.0,
    0xcc as f32 / 255.0,
);
pub const LIGHT_HEADER_BG: Color = Color::from_rgb(
    0xee as f32 / 255.0,
    0xee as f32 / 255.0,
    0xee as f32 / 255.0,
);
pub const LIGHT_STATUS_BAR_BG: Color = Color::from_rgb(
    0xee as f32 / 255.0,
    0xee as f32 / 255.0,
    0xee as f32 / 255.0,
);

pub const LIGHT_PROGRESS_FILL: Color = Color::from_rgb(
    0x4c as f32 / 255.0,
    0xaf as f32 / 255.0,
    0x50 as f32 / 255.0,
);
pub const LIGHT_PROGRESS_BG: Color = Color::from_rgb(
    0xe0 as f32 / 255.0,
    0xe0 as f32 / 255.0,
    0xe0 as f32 / 255.0,
);

pub const LIGHT_LOG_BG: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xff as f32 / 255.0,
    0xff as f32 / 255.0,
);

pub const LIGHT_BADGE_PBO_BG: Color = Color::from_rgb(
    0xed as f32 / 255.0,
    0xe7 as f32 / 255.0,
    0xf6 as f32 / 255.0,
);
pub const LIGHT_BADGE_PBO_TEXT: Color = Color::from_rgb(
    0x45 as f32 / 255.0,
    0x27 as f32 / 255.0,
    0xa0 as f32 / 255.0,
);

pub const LIGHT_BUTTON_BG: Color = Color::from_rgb(
    0xe0 as f32 / 255.0,
    0xe0 as f32 / 255.0,
    0xe0 as f32 / 255.0,
);
pub const LIGHT_BUTTON_TEXT: Color = Color::BLACK;

pub fn co_tier_color(tier: &crate::co_tier::CoTier, is_dark: bool) -> Color {
    use crate::co_tier::CoTier;

    match (tier, is_dark) {
        (CoTier::Gold, true) => DARK_CO_GOLD,
        (CoTier::Gold, false) => LIGHT_CO_GOLD,
        (CoTier::Silver, true) => DARK_CO_SILVER,
        (CoTier::Silver, false) => LIGHT_CO_SILVER,
        (CoTier::Bronze, true) => DARK_CO_BRONZE,
        (CoTier::Bronze, false) => LIGHT_CO_BRONZE,
        (CoTier::Neutral, true) => DARK_TEXT_SECONDARY,
        (CoTier::Neutral, false) => LIGHT_TEXT_SECONDARY,
    }
}

pub fn co_tier_badge_background(tier: &crate::co_tier::CoTier, is_dark: bool) -> Color {
    use crate::co_tier::CoTier;

    match (tier, is_dark) {
        (CoTier::Gold, true) => DARK_CO_GOLD_BG,
        (CoTier::Gold, false) => LIGHT_CO_GOLD_BG,
        (CoTier::Silver, true) => DARK_CO_SILVER_BG,
        (CoTier::Silver, false) => LIGHT_CO_SILVER_BG,
        (CoTier::Bronze, true) => DARK_CO_BRONZE_BG,
        (CoTier::Bronze, false) => LIGHT_CO_BRONZE_BG,
        (CoTier::Neutral, true) => DARK_BG_TERTIARY,
        (CoTier::Neutral, false) => LIGHT_BG_TERTIARY,
    }
}

pub fn co_tier_badge_border(tier: &crate::co_tier::CoTier, is_dark: bool) -> Color {
    use crate::co_tier::CoTier;

    match (tier, is_dark) {
        (CoTier::Gold, true) => DARK_CO_GOLD_BORDER,
        (CoTier::Gold, false) => LIGHT_CO_GOLD_BORDER,
        (CoTier::Silver, true) => DARK_CO_SILVER_BORDER,
        (CoTier::Silver, false) => LIGHT_CO_SILVER_BORDER,
        (CoTier::Bronze, true) => DARK_CO_BRONZE_BORDER,
        (CoTier::Bronze, false) => LIGHT_CO_BRONZE_BORDER,
        (CoTier::Neutral, true) => DARK_CARD_BORDER,
        (CoTier::Neutral, false) => LIGHT_CARD_BORDER,
    }
}

// ---------------------------------------------------------------------------
// Dark core status colors
// ---------------------------------------------------------------------------

const DARK_CORE_PASSED_BG: Color = Color::from_rgb(
    0x2d as f32 / 255.0,
    0x5a as f32 / 255.0,
    0x27 as f32 / 255.0,
);
const DARK_CORE_PASSED_TEXT: Color = Color::from_rgb(
    0xe8 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xe9 as f32 / 255.0,
);

const DARK_CORE_FAILED_BG: Color = Color::from_rgb(
    0x5a as f32 / 255.0,
    0x1a as f32 / 255.0,
    0x1a as f32 / 255.0,
);
const DARK_CORE_FAILED_TEXT: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xeb as f32 / 255.0,
    0xee as f32 / 255.0,
);

const DARK_CORE_TESTING_BG: Color = Color::from_rgb(
    0x1a as f32 / 255.0,
    0x3a as f32 / 255.0,
    0x5a as f32 / 255.0,
);
const DARK_CORE_TESTING_TEXT: Color = Color::from_rgb(
    0xe3 as f32 / 255.0,
    0xf2 as f32 / 255.0,
    0xfd as f32 / 255.0,
);

const DARK_CORE_SKIPPED_BG: Color = Color::from_rgb(
    0x5a as f32 / 255.0,
    0x4a as f32 / 255.0,
    0x00 as f32 / 255.0,
);
const DARK_CORE_SKIPPED_TEXT: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xfd as f32 / 255.0,
    0xe7 as f32 / 255.0,
);

const DARK_CORE_IDLE_BG: Color = Color::from_rgb(
    0x2a as f32 / 255.0,
    0x2a as f32 / 255.0,
    0x2a as f32 / 255.0,
);
const DARK_CORE_IDLE_TEXT: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
);

const DARK_CORE_INTERRUPTED_BG: Color = Color::from_rgb(
    0x5a as f32 / 255.0,
    0x2d as f32 / 255.0,
    0x00 as f32 / 255.0,
);

const DARK_CORE_INTERRUPTED_TEXT: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xff as f32 / 255.0,
    0xff as f32 / 255.0,
);
// ---------------------------------------------------------------------------
// Light core status colors
// ---------------------------------------------------------------------------

const LIGHT_CORE_PASSED_BG: Color = Color::from_rgb(
    0xc8 as f32 / 255.0,
    0xe6 as f32 / 255.0,
    0xc9 as f32 / 255.0,
);
const LIGHT_CORE_PASSED_TEXT: Color = Color::from_rgb(
    0x1b as f32 / 255.0,
    0x5e as f32 / 255.0,
    0x20 as f32 / 255.0,
);

const LIGHT_CORE_FAILED_BG: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xcd as f32 / 255.0,
    0xd2 as f32 / 255.0,
);
const LIGHT_CORE_FAILED_TEXT: Color = Color::from_rgb(
    0xb7 as f32 / 255.0,
    0x1c as f32 / 255.0,
    0x1c as f32 / 255.0,
);

const LIGHT_CORE_TESTING_BG: Color = Color::from_rgb(
    0xbb as f32 / 255.0,
    0xde as f32 / 255.0,
    0xfb as f32 / 255.0,
);
const LIGHT_CORE_TESTING_TEXT: Color = Color::from_rgb(
    0x0d as f32 / 255.0,
    0x47 as f32 / 255.0,
    0xa1 as f32 / 255.0,
);

const LIGHT_CORE_SKIPPED_BG: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xf9 as f32 / 255.0,
    0xc4 as f32 / 255.0,
);
const LIGHT_CORE_SKIPPED_TEXT: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0x7f as f32 / 255.0,
    0x17 as f32 / 255.0,
);

const LIGHT_CORE_IDLE_BG: Color = Color::from_rgb(
    0xee as f32 / 255.0,
    0xee as f32 / 255.0,
    0xee as f32 / 255.0,
);
const LIGHT_CORE_IDLE_TEXT: Color = Color::from_rgb(
    0x42 as f32 / 255.0,
    0x42 as f32 / 255.0,
    0x42 as f32 / 255.0,
);

const LIGHT_CORE_INTERRUPTED_BG: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xe0 as f32 / 255.0,
    0xb2 as f32 / 255.0,
);
const LIGHT_CORE_INTERRUPTED_TEXT: Color = Color::from_rgb(
    0x00 as f32 / 255.0,
    0x00 as f32 / 255.0,
    0x00 as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// Dark log-level colors
// ---------------------------------------------------------------------------

const DARK_LOG_ERROR: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0x52 as f32 / 255.0,
    0x52 as f32 / 255.0,
);
const DARK_LOG_MCE: Color = Color::from_rgb(
    0xff as f32 / 255.0,
    0xab as f32 / 255.0,
    0x40 as f32 / 255.0,
);
const DARK_LOG_STABLE: Color = Color::from_rgb(
    0x69 as f32 / 255.0,
    0xf0 as f32 / 255.0,
    0xae as f32 / 255.0,
);
const DARK_LOG_DEFAULT: Color = Color::from_rgb(
    0xb3 as f32 / 255.0,
    0xb3 as f32 / 255.0,
    0xb3 as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// Light log-level colors
// ---------------------------------------------------------------------------

const LIGHT_LOG_ERROR: Color = Color::from_rgb(
    0xd3 as f32 / 255.0,
    0x2f as f32 / 255.0,
    0x2f as f32 / 255.0,
);
const LIGHT_LOG_MCE: Color = Color::from_rgb(
    0xe6 as f32 / 255.0,
    0x51 as f32 / 255.0,
    0x00 as f32 / 255.0,
);
const LIGHT_LOG_STABLE: Color = Color::from_rgb(
    0x2e as f32 / 255.0,
    0x7d as f32 / 255.0,
    0x32 as f32 / 255.0,
);
const LIGHT_LOG_DEFAULT: Color = Color::from_rgb(
    0x42 as f32 / 255.0,
    0x42 as f32 / 255.0,
    0x42 as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// Theme constructors
// ---------------------------------------------------------------------------

pub fn dark_theme() -> Theme {
    Theme::custom(
        "core-probe Dark".to_string(),
        Palette {
            background: DARK_BG_PRIMARY,
            text: DARK_TEXT_PRIMARY,
            primary: DARK_PROGRESS_FILL,
            success: DARK_CORE_PASSED_BG,
            warning: DARK_CORE_SKIPPED_BG,
            danger: DARK_CORE_FAILED_BG,
        },
    )
}

pub fn light_theme() -> Theme {
    Theme::custom(
        "core-probe Light".to_string(),
        Palette {
            background: LIGHT_BG_PRIMARY,
            text: LIGHT_TEXT_PRIMARY,
            primary: LIGHT_PROGRESS_FILL,
            success: LIGHT_CORE_PASSED_BG,
            warning: LIGHT_CORE_SKIPPED_BG,
            danger: LIGHT_CORE_FAILED_BG,
        },
    )
}

// ---------------------------------------------------------------------------
// System theme detection
// ---------------------------------------------------------------------------

/// Detects the system color-scheme preference.
/// Falls back: gsettings → GTK_THEME env → Dark.
pub fn detect_system_theme() -> ThemeMode {
    // Try gsettings (GNOME / freedesktop portal)
    if let Ok(output) = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "color-scheme"])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout);
        if stdout.contains("prefer-dark") {
            return ThemeMode::Dark;
        }
        if stdout.contains("prefer-light") || stdout.contains("default") {
            return ThemeMode::Light;
        }
    }

    // Fallback: GTK_THEME env var
    if let Ok(gtk_theme) = std::env::var("GTK_THEME") {
        if gtk_theme.to_ascii_lowercase().contains("dark") {
            return ThemeMode::Dark;
        }
        return ThemeMode::Light;
    }

    // Final fallback: dark
    ThemeMode::Dark
}

// ---------------------------------------------------------------------------
// Status color helpers
// ---------------------------------------------------------------------------

pub fn status_bg_color(status: &CoreStatus, is_dark: bool) -> Color {
    match (status, is_dark) {
        (CoreStatus::Passed, true) => DARK_CORE_PASSED_BG,
        (CoreStatus::Passed, false) => LIGHT_CORE_PASSED_BG,
        (CoreStatus::Failed, true) => DARK_CORE_FAILED_BG,
        (CoreStatus::Failed, false) => LIGHT_CORE_FAILED_BG,
        (CoreStatus::Testing, true) => DARK_CORE_TESTING_BG,
        (CoreStatus::Testing, false) => LIGHT_CORE_TESTING_BG,
        (CoreStatus::Skipped, true) => DARK_CORE_SKIPPED_BG,
        (CoreStatus::Skipped, false) => LIGHT_CORE_SKIPPED_BG,
        (CoreStatus::Idle, true) => DARK_CORE_IDLE_BG,
        (CoreStatus::Idle, false) => LIGHT_CORE_IDLE_BG,
        (CoreStatus::Interrupted, true) => DARK_CORE_INTERRUPTED_BG,
        (CoreStatus::Interrupted, false) => LIGHT_CORE_INTERRUPTED_BG,
    }
}

/// Returns the text color for a core status tile.
pub fn status_text_color(status: &CoreStatus, is_dark: bool) -> Color {
    match (status, is_dark) {
        (CoreStatus::Passed, true) => DARK_CORE_PASSED_TEXT,
        (CoreStatus::Passed, false) => LIGHT_CORE_PASSED_TEXT,
        (CoreStatus::Failed, true) => DARK_CORE_FAILED_TEXT,
        (CoreStatus::Failed, false) => LIGHT_CORE_FAILED_TEXT,
        (CoreStatus::Testing, true) => DARK_CORE_TESTING_TEXT,
        (CoreStatus::Testing, false) => LIGHT_CORE_TESTING_TEXT,
        (CoreStatus::Skipped, true) => DARK_CORE_SKIPPED_TEXT,
        (CoreStatus::Skipped, false) => LIGHT_CORE_SKIPPED_TEXT,
        (CoreStatus::Idle, true) => DARK_CORE_IDLE_TEXT,
        (CoreStatus::Idle, false) => LIGHT_CORE_IDLE_TEXT,
        (CoreStatus::Interrupted, true) => DARK_CORE_INTERRUPTED_TEXT,
        (CoreStatus::Interrupted, false) => LIGHT_CORE_INTERRUPTED_TEXT,
    }
}

// ---------------------------------------------------------------------------
// Greyed-out color helpers (for de-emphasized, non-selected cores)
// ---------------------------------------------------------------------------

pub fn greyed_bg_color(is_dark: bool) -> Color {
    if is_dark {
        DARK_BG_TERTIARY
    } else {
        LIGHT_BG_TERTIARY
    }
}

pub fn greyed_text_color(is_dark: bool) -> Color {
    if is_dark {
        DARK_TEXT_MUTED
    } else {
        LIGHT_TEXT_MUTED
    }
}

pub fn status_border_color(status: &CoreStatus, is_dark: bool) -> Color {
    match (status, is_dark) {
        (CoreStatus::Failed, true) => Color::from_rgb(0.7, 0.2, 0.2),
        (CoreStatus::Failed, false) => Color::from_rgb(0.8, 0.2, 0.2),
        (CoreStatus::Passed, true) => Color::from_rgb(0.2, 0.5, 0.2),
        (CoreStatus::Passed, false) => Color::from_rgb(0.2, 0.6, 0.2),
        (_, true) => DARK_CARD_BORDER,
        (_, false) => LIGHT_CARD_BORDER,
    }
}

// ---------------------------------------------------------------------------
// Log-level color helper
// ---------------------------------------------------------------------------

pub fn log_level_color(level: &LogLevel, is_dark: bool) -> Color {
    match (level, is_dark) {
        (LogLevel::Error, true) => DARK_LOG_ERROR,
        (LogLevel::Error, false) => LIGHT_LOG_ERROR,
        (LogLevel::Mce, true) => DARK_LOG_MCE,
        (LogLevel::Mce, false) => LIGHT_LOG_MCE,
        (LogLevel::Stable, true) => DARK_LOG_STABLE,
        (LogLevel::Stable, false) => LIGHT_LOG_STABLE,
        (LogLevel::Default, true) => DARK_LOG_DEFAULT,
        (LogLevel::Default, false) => LIGHT_LOG_DEFAULT,
    }
}

// ---------------------------------------------------------------------------
// Sparkline dimension constants
// ---------------------------------------------------------------------------

/// Width of a single sparkline bar in logical pixels.
pub const SPARKLINE_BAR_WIDTH: f32 = 6.0;

/// Gap between sparkline bars in logical pixels.
pub const SPARKLINE_BAR_GAP: f32 = 2.0;

/// Maximum bar height (100 % load) in logical pixels.
pub const SPARKLINE_BAR_MAX_HEIGHT: f32 = 12.0;

/// Minimum bar height (0 % load) in logical pixels.
pub const SPARKLINE_BAR_MIN_HEIGHT: f32 = 4.0;

/// Total height of the sparkline region in logical pixels.
pub const SPARKLINE_REGION_HEIGHT: f32 = 15.0;

// ---------------------------------------------------------------------------
// Sparkline colour / opacity helpers
// ---------------------------------------------------------------------------

/// Convert HSL colour components (each in range [0, 1]) to sRGB.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> (f32, f32, f32) {
    if s == 0.0 {
        return (l, l, l);
    }
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h * 6.0) % 2.0 - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h * 6.0).floor() as i32 % 6 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        5 => (c, 0.0, x),
        _ => (0.0, 0.0, 0.0),
    };
    (r + m, g + m, b + m)
}

/// Returns a thermal-gradient colour for a given CPU load percentage.
///
/// - 0–30 %:  green  (hue ≈ 0.33)
/// - 30–70 %: amber → orange (hue 0.17 → 0.08)
/// - 70–100 %: orange → red (hue 0.08 → 0.0)
///
/// Dark theme colours are brighter (lightness +0.1), light theme colours
/// are deeper (lightness −0.1).
pub fn sparkline_color(load_pct: f32, is_dark: bool) -> Color {
    let load = load_pct.clamp(0.0, 100.0) / 100.0;
    let saturation = 0.8;
    let lightness = if is_dark { 0.6 } else { 0.4 };

    let hue = if load <= 0.3 {
        0.33
    } else if load <= 0.7 {
        // 30–70 % → amber (0.17) to orange (0.08)
        let t = (load - 0.3) / 0.4;
        0.17 - t * (0.17 - 0.08)
    } else {
        // 70–100 % → orange (0.08) to red (0.0)
        let t = (load - 0.7) / 0.3;
        0.08 - t * 0.08
    };

    let (r, g, b) = hsl_to_rgb(hue, saturation, lightness);
    Color::from_rgb(r, g, b)
}

/// Linear opacity from 0.3 (idle) to 1.0 (full load).
pub fn sparkline_opacity(load_pct: f32) -> f32 {
    0.3 + load_pct.clamp(0.0, 100.0) / 100.0 * 0.7
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::co_tier::CoTier;

    #[test]
    fn given_dark_theme_when_created_then_uses_wireframe_background() {
        let theme = dark_theme();
        let palette = theme.palette();
        assert_eq!(palette.background, DARK_BG_PRIMARY);
        assert_eq!(palette.text, DARK_TEXT_PRIMARY);
    }

    #[test]
    fn given_light_theme_when_created_then_uses_wireframe_background() {
        let theme = light_theme();
        let palette = theme.palette();
        assert_eq!(palette.background, LIGHT_BG_PRIMARY);
        assert_eq!(palette.text, LIGHT_TEXT_PRIMARY);
    }

    #[test]
    fn given_each_core_status_when_querying_dark_bg_then_returns_distinct_color() {
        let statuses = [
            CoreStatus::Idle,
            CoreStatus::Testing,
            CoreStatus::Passed,
            CoreStatus::Failed,
            CoreStatus::Skipped,
            CoreStatus::Interrupted,
        ];
        let colors: Vec<_> = statuses.iter().map(|s| status_bg_color(s, true)).collect();

        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "dark bg colors for {:?} and {:?} should differ",
                    statuses[i], statuses[j]
                );
            }
        }
    }

    #[test]
    fn given_each_core_status_when_querying_light_bg_then_returns_distinct_color() {
        let statuses = [
            CoreStatus::Idle,
            CoreStatus::Testing,
            CoreStatus::Passed,
            CoreStatus::Failed,
            CoreStatus::Skipped,
            CoreStatus::Interrupted,
        ];
        let colors: Vec<_> = statuses.iter().map(|s| status_bg_color(s, false)).collect();

        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "light bg colors for {:?} and {:?} should differ",
                    statuses[i], statuses[j]
                );
            }
        }
    }

    #[test]
    fn given_interrupted_status_when_querying_text_then_uses_default_text_color() {
        assert_eq!(
            status_text_color(&CoreStatus::Interrupted, true),
            DARK_CORE_INTERRUPTED_TEXT
        );
        assert_eq!(
            status_text_color(&CoreStatus::Interrupted, false),
            LIGHT_CORE_INTERRUPTED_TEXT
        );
    }

    #[test]
    fn given_all_log_levels_when_querying_dark_colors_then_returns_distinct_colors() {
        let levels = [
            LogLevel::Error,
            LogLevel::Mce,
            LogLevel::Stable,
            LogLevel::Default,
        ];
        let colors: Vec<_> = levels.iter().map(|l| log_level_color(l, true)).collect();

        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "dark log colors for {:?} and {:?} should differ",
                    levels[i], levels[j]
                );
            }
        }
    }

    #[test]
    fn given_system_detection_when_called_then_returns_valid_theme_mode() {
        let mode = detect_system_theme();
        assert!(matches!(mode, ThemeMode::Dark | ThemeMode::Light));
    }

    /// BDD: Given dark and light themes, when comparing palettes, then backgrounds differ
    #[test]
    fn given_dark_and_light_themes_when_compared_then_backgrounds_differ() {
        let dark = dark_theme();
        let light = light_theme();
        assert_ne!(dark.palette().background, light.palette().background);
    }

    /// BDD: Given all log levels, when querying light colors, then returns distinct colors
    #[test]
    fn given_all_log_levels_when_querying_light_colors_then_returns_distinct_colors() {
        let levels = [
            LogLevel::Error,
            LogLevel::Mce,
            LogLevel::Stable,
            LogLevel::Default,
        ];
        let colors: Vec<_> = levels.iter().map(|l| log_level_color(l, false)).collect();

        for i in 0..colors.len() {
            for j in (i + 1)..colors.len() {
                assert_ne!(
                    colors[i], colors[j],
                    "light log colors for {:?} and {:?} should differ",
                    levels[i], levels[j]
                );
            }
        }
    }

    /// BDD: Given each core status, when querying text color for both themes, then dark and light differ
    #[test]
    fn given_each_status_when_querying_text_for_both_themes_then_dark_and_light_differ() {
        let statuses = [
            CoreStatus::Idle,
            CoreStatus::Testing,
            CoreStatus::Passed,
            CoreStatus::Failed,
            CoreStatus::Skipped,
            CoreStatus::Interrupted,
        ];
        for status in &statuses {
            let dark = status_text_color(status, true);
            let light = status_text_color(status, false);
            assert_ne!(
                dark, light,
                "text colors for {:?} should differ between dark and light",
                status
            );
        }
    }

    /// BDD: Given ThemeMode variants, when matched, then all three are distinct
    #[test]
    fn given_theme_mode_variants_when_matched_then_all_three_are_distinct() {
        let modes = [ThemeMode::Dark, ThemeMode::Light, ThemeMode::System];
        assert!(!matches!(modes[0], ThemeMode::Light | ThemeMode::System));
        assert!(!matches!(modes[1], ThemeMode::Dark | ThemeMode::System));
        assert!(!matches!(modes[2], ThemeMode::Dark | ThemeMode::Light));
    }

    /// BDD: Given Failed status, when querying border color, then returns red-tinted color
    #[test]
    fn given_failed_status_when_querying_border_color_then_returns_red_tinted() {
        let dark_border = status_border_color(&CoreStatus::Failed, true);
        assert_ne!(dark_border, DARK_CARD_BORDER);
        assert!(dark_border.r > dark_border.g);
        assert!(dark_border.r > dark_border.b);
    }

    /// BDD: Given Passed status, when querying border color, then returns green-tinted color
    #[test]
    fn given_passed_status_when_querying_border_color_then_returns_green_tinted() {
        let dark_border = status_border_color(&CoreStatus::Passed, true);
        assert_ne!(dark_border, DARK_CARD_BORDER);
        assert!(dark_border.g > dark_border.r);
        assert!(dark_border.g > dark_border.b);
    }

    /// BDD: Given Idle status, when querying border color, then returns default card border
    #[test]
    fn given_idle_status_when_querying_border_color_then_returns_default() {
        assert_eq!(
            status_border_color(&CoreStatus::Idle, true),
            DARK_CARD_BORDER
        );
        assert_eq!(
            status_border_color(&CoreStatus::Idle, false),
            LIGHT_CARD_BORDER
        );
    }

    #[test]
    fn given_light_surface_hierarchy_when_compared_then_all_surfaces_are_distinct() {
        // All four primary surfaces must be pairwise distinct
        assert_ne!(
            LIGHT_BG_PRIMARY, LIGHT_BG_SECONDARY,
            "BG_PRIMARY and BG_SECONDARY must differ"
        );
        assert_ne!(
            LIGHT_BG_PRIMARY, LIGHT_BG_TERTIARY,
            "BG_PRIMARY and BG_TERTIARY must differ"
        );
        assert_ne!(
            LIGHT_BG_PRIMARY, LIGHT_HEADER_BG,
            "BG_PRIMARY and HEADER_BG must differ"
        );
        assert_ne!(
            LIGHT_BG_SECONDARY, LIGHT_BG_TERTIARY,
            "BG_SECONDARY and BG_TERTIARY must differ"
        );
        assert_ne!(
            LIGHT_BG_SECONDARY, LIGHT_HEADER_BG,
            "BG_SECONDARY and HEADER_BG must differ"
        );
        assert_ne!(
            LIGHT_BG_TERTIARY, LIGHT_HEADER_BG,
            "BG_TERTIARY and HEADER_BG must differ"
        );
    }

    #[test]
    fn co_tier_color_gold_dark_is_distinct() {
        let gold = co_tier_color(&CoTier::Gold, true);
        let silver = co_tier_color(&CoTier::Silver, true);
        let bronze = co_tier_color(&CoTier::Bronze, true);
        assert_ne!(gold, silver);
        assert_ne!(gold, bronze);
        assert_ne!(silver, bronze);
    }

    #[test]
    fn co_tier_color_neutral_dark_matches_secondary() {
        assert_eq!(co_tier_color(&CoTier::Neutral, true), DARK_TEXT_SECONDARY);
    }

    #[test]
    fn co_tier_color_neutral_light_matches_secondary() {
        assert_eq!(co_tier_color(&CoTier::Neutral, false), LIGHT_TEXT_SECONDARY);
    }

    #[test]
    fn co_tier_badge_background_gold_dark_is_distinct() {
        let gold = co_tier_badge_background(&CoTier::Gold, true);
        let silver = co_tier_badge_background(&CoTier::Silver, true);
        let bronze = co_tier_badge_background(&CoTier::Bronze, true);
        assert_ne!(gold, silver);
        assert_ne!(gold, bronze);
        assert_ne!(silver, bronze);
    }

    #[test]
    fn co_tier_badge_background_neutral_dark_matches_tertiary() {
        assert_eq!(
            co_tier_badge_background(&CoTier::Neutral, true),
            DARK_BG_TERTIARY
        );
    }

    #[test]
    fn co_tier_badge_background_neutral_light_matches_tertiary() {
        assert_eq!(
            co_tier_badge_background(&CoTier::Neutral, false),
            LIGHT_BG_TERTIARY
        );
    }

    #[test]
    fn co_tier_badge_border_gold_dark_is_distinct() {
        let gold = co_tier_badge_border(&CoTier::Gold, true);
        let silver = co_tier_badge_border(&CoTier::Silver, true);
        let bronze = co_tier_badge_border(&CoTier::Bronze, true);
        assert_ne!(gold, silver);
        assert_ne!(gold, bronze);
        assert_ne!(silver, bronze);
    }

    #[test]
    fn co_tier_badge_border_neutral_dark_matches_card_border() {
        assert_eq!(
            co_tier_badge_border(&CoTier::Neutral, true),
            DARK_CARD_BORDER
        );
    }

    #[test]
    fn co_tier_badge_border_neutral_light_matches_card_border() {
        assert_eq!(
            co_tier_badge_border(&CoTier::Neutral, false),
            LIGHT_CARD_BORDER
        );
    }

    // -----------------------------------------------------------------------
    // Sparkline colour / opacity tests
    // -----------------------------------------------------------------------

    /// BDD: Given zero load, when querying sparkline colour, then green
    ///      channel dominates over red and blue.
    #[test]
    fn given_zero_load_when_querying_sparkline_color_then_green_dominant() {
        let c = sparkline_color(0.0, true);
        assert!(
            c.g > c.r,
            "green ({}) should exceed red ({}) at 0% load",
            c.g,
            c.r
        );
        assert!(
            c.g > c.b,
            "green ({}) should exceed blue ({}) at 0% load",
            c.g,
            c.b
        );
    }

    /// BDD: Given full load, when querying sparkline colour, then red
    ///      channel dominates over green and blue.
    #[test]
    fn given_full_load_when_querying_sparkline_color_then_red_dominant() {
        let c = sparkline_color(100.0, true);
        assert!(
            c.r > c.g,
            "red ({}) should exceed green ({}) at 100% load",
            c.r,
            c.g
        );
        assert!(
            c.r > c.b,
            "red ({}) should exceed blue ({}) at 100% load",
            c.r,
            c.b
        );
    }

    /// BDD: Given the same sparkline load, when comparing dark and light theme
    ///      colours, then the dark theme variant is brighter (higher luma).
    #[test]
    fn given_same_sparkline_load_when_comparing_dark_and_light_then_dark_is_brighter() {
        let dark = sparkline_color(50.0, true);
        let light = sparkline_color(50.0, false);
        let dark_luma = dark.r + dark.g + dark.b;
        let light_luma = light.r + light.g + light.b;
        assert!(
            dark_luma > light_luma,
            "dark luma ({dark_luma}) should exceed light luma ({light_luma})"
        );
    }

    /// BDD: Given 50 % load (amber zone), when computing the sparkline colour,
    ///      then both red and green channels are present.
    #[test]
    fn given_fifty_percent_load_when_querying_sparkline_color_then_has_both_red_and_green() {
        let c = sparkline_color(50.0, true);
        assert!(c.r > 0.0, "red should be present at 50% load");
        assert!(c.g > 0.0, "green should be present at 50% load");
    }

    /// BDD: Given zero load, when computing opacity, then returns 0.3 (minimum).
    #[test]
    fn given_zero_load_when_querying_sparkline_opacity_then_returns_minimum() {
        let op = sparkline_opacity(0.0);
        assert!((op - 0.3).abs() < 1e-6, "expected ~0.3, got {op}");
    }

    /// BDD: Given full load, when computing opacity, then returns 1.0 (maximum).
    #[test]
    fn given_full_load_when_querying_sparkline_opacity_then_returns_maximum() {
        let op = sparkline_opacity(100.0);
        assert!((op - 1.0).abs() < 1e-6, "expected ~1.0, got {op}");
    }

    /// BDD: Given 50 % load, when computing opacity, then returns 0.65 (midpoint).
    #[test]
    fn given_mid_load_when_querying_sparkline_opacity_then_returns_midpoint() {
        let op = sparkline_opacity(50.0);
        assert!((op - 0.65).abs() < 1e-6, "expected ~0.65, got {op}");
    }
}
