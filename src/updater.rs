use self_update::backends::github::Update;
use self_update::cargo_crate_version;
use semver::Version;

/// Information about a GitHub release discovered during an update check.
#[derive(Debug, Clone)]
pub struct ReleaseInfo {
    /// The version string of the release (e.g. "0.1.0").
    pub version: String,
    /// The release body (markdown release notes).
    pub body: String,
}

/// Checks GitHub for a release newer than the currently-running version.
///
/// Returns `Ok(Some(ReleaseInfo))` if an update is available, `Ok(None)` if the
/// current version is already the latest, or `Err(String)` on network/parse errors.
pub fn check_update_available() -> Result<Option<ReleaseInfo>, String> {
    let current = Version::parse(cargo_crate_version!())
        .map_err(|e| format!("invalid current version: {e}"))?;

    let updater = Update::configure()
        .repo_owner("RouHim")
        .repo_name("core-probe")
        .bin_name("core-probe")
        .current_version(cargo_crate_version!())
        .no_confirm(true)
        .build()
        .map_err(|e| format!("failed to configure updater: {e}"))?;

    // Manually query releases so we can grab the body for display.
    let releases: Vec<self_update::update::Release> = updater
        .get_latest_releases(cargo_crate_version!())
        .map_err(|e| format!("failed to query releases: {e}"))?;

    // Find the highest semver release that is > current.
    let mut latest: Option<(Version, String)> = None;
    for release in &releases {
        let version = match Version::parse(release.version.trim_start_matches('v')) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if version > current {
            match &latest {
                Some((best, _)) if version <= *best => {}
                _ => {
                    latest = Some((version, release.body.clone().unwrap_or_default()));
                }
            }
        }
    }

    Ok(latest.map(|(v, body)| ReleaseInfo {
        version: v.to_string(),
        body,
    }))
}

/// Downloads and installs the latest GitHub release, replacing the current binary.
///
/// Returns `Ok(())` on success, or `Err(String)` with a human-readable error.
pub fn apply_update() -> Result<(), String> {
    let updater = Update::configure()
        .repo_owner("RouHim")
        .repo_name("core-probe")
        .bin_name("core-probe")
        .current_version(cargo_crate_version!())
        .no_confirm(true)
        .build()
        .map_err(|e| format!("failed to configure updater: {e}"))?;

    updater
        .update()
        .map_err(|e| format!("update failed: {e}"))?;

    Ok(())
}
