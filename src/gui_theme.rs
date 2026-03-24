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
    0xf0 as f32 / 255.0,
    0xf0 as f32 / 255.0,
    0xf0 as f32 / 255.0,
);
pub const DARK_CCD_BORDER: Color = Color::from_rgb(
    0x30 as f32 / 255.0,
    0x30 as f32 / 255.0,
    0x30 as f32 / 255.0,
);
pub const LIGHT_CCD_BORDER: Color = Color::from_rgb(
    0xd8 as f32 / 255.0,
    0xd8 as f32 / 255.0,
    0xd8 as f32 / 255.0,
);

// ---------------------------------------------------------------------------
// Light palette colors (from wireframe CSS [data-theme="light"])
// ---------------------------------------------------------------------------

pub const LIGHT_BG_PRIMARY: Color = Color::from_rgb(
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
    0xf5 as f32 / 255.0,
);
pub const LIGHT_BG_SECONDARY: Color = Color::WHITE;
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

pub const LIGHT_LOG_BG: Color = Color::WHITE;

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

/// Interrupted has no dedicated text override in the wireframe — uses the
/// default text color for the active theme.
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
        (CoreStatus::Interrupted, true) => DARK_TEXT_PRIMARY,
        (CoreStatus::Interrupted, false) => LIGHT_TEXT_PRIMARY,
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

#[cfg(test)]
mod tests {
    use super::*;

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
            DARK_TEXT_PRIMARY
        );
        assert_eq!(
            status_text_color(&CoreStatus::Interrupted, false),
            LIGHT_TEXT_PRIMARY
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
}
