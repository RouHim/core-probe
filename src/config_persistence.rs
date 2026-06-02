use std::path::Path;

use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::gui::TestConfig;
use crate::gui_theme::ThemeMode;

#[derive(Serialize, Deserialize)]
struct PersistedConfig {
    test: TestConfig,
    theme: ThemeMode,
}

/// Returns path: `$HOME/.config/core-probe/config.json`.
/// Returns `None` when `$HOME` is not set.
fn config_path() -> Option<std::path::PathBuf> {
    std::env::var_os("HOME").map(|home| {
        std::path::PathBuf::from(home)
            .join(".config")
            .join("core-probe")
            .join("config.json")
    })
}

/// Serializes the current test config and theme and writes them to disk.
///
/// Logs a warning on failure; never panics.
pub fn save(config: &TestConfig, theme: ThemeMode) {
    let Some(path) = config_path() else {
        warn!("cannot persist config: $HOME is not set");
        return;
    };
    save_at(&path, config, theme);
}

/// Reads the config file and returns `(TestConfig, ThemeMode)` if valid.
///
/// Returns `None` if the file does not exist, is corrupt, or cannot be read.
pub fn load() -> Option<(TestConfig, ThemeMode)> {
    let path = config_path()?;
    load_at(&path)
}

// ── internal helpers with explicit paths (testable without env vars) ──

fn save_at(path: &Path, config: &TestConfig, theme: ThemeMode) {
    let persisted = PersistedConfig {
        test: config.clone(),
        theme,
    };

    let json = match serde_json::to_string_pretty(&persisted) {
        Ok(json) => json,
        Err(error) => {
            warn!(%error, "failed to serialize config");
            return;
        }
    };

    if let Some(parent) = path.parent() {
        if let Err(error) = std::fs::create_dir_all(parent) {
            warn!(%error, path = %parent.display(), "failed to create config directory");
            return;
        }
    }

    if let Err(error) = std::fs::write(path, json) {
        warn!(%error, path = %path.display(), "failed to write config file");
    }
}

fn load_at(path: &Path) -> Option<(TestConfig, ThemeMode)> {
    let raw = std::fs::read_to_string(path).ok()?;
    match serde_json::from_str::<PersistedConfig>(&raw) {
        Ok(persisted) => Some((persisted.test, persisted.theme)),
        Err(error) => {
            warn!(%error, path = %path.display(), "failed to parse config file, using defaults");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        let original_config = TestConfig::default();
        let original_theme = ThemeMode::System;

        save_at(&path, &original_config, original_theme);

        let (loaded_config, loaded_theme) = load_at(&path).unwrap();
        assert_eq!(loaded_config.duration, original_config.duration);
        assert_eq!(loaded_config.iterations, original_config.iterations);
        assert_eq!(loaded_config.mode, original_config.mode);
        assert_eq!(loaded_config.cores, original_config.cores);
        assert_eq!(loaded_theme, original_theme);
    }

    #[test]
    fn missing_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("nonexistent.json");
        let result = load_at(&path);
        assert!(result.is_none());
    }

    #[test]
    fn corrupt_file_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("corrupt.json");
        std::fs::write(&path, "not valid json {{{").unwrap();
        let result = load_at(&path);
        assert!(result.is_none());
    }

    #[test]
    fn load_restores_exact_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");

        let mut config = TestConfig::default();
        config.duration = String::from("12m");
        config.iterations = 3;
        let theme = ThemeMode::Light;

        save_at(&path, &config, theme);
        let (loaded_config, loaded_theme) = load_at(&path).unwrap();

        assert_eq!(loaded_config.duration, "12m");
        assert_eq!(loaded_config.iterations, 3);
        assert_eq!(loaded_config.mode, config.mode);
        assert_eq!(loaded_config.cores, config.cores);
        assert_eq!(loaded_theme, ThemeMode::Light);
    }
}
