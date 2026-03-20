use anyhow::{Context, Result};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use tracing::{debug, info};

/// Embedded mprime binary (27MB)
const MPRIME_BINARY: &[u8] = include_bytes!("../mprime-latest/mprime");

/// Embedded libgmp shared library (706KB)
const LIBGMP_BINARY: &[u8] = include_bytes!("../mprime-latest/libgmp.so.10.4.1");

/// Paths to extracted binaries
#[derive(Debug, Clone)]
pub struct ExtractedBinaries {
    /// Temporary directory containing all extracted files
    pub temp_dir: PathBuf,
    /// Path to the mprime executable
    pub mprime_path: PathBuf,
    /// Directory containing libgmp shared libraries
    pub lib_dir: PathBuf,
}

impl ExtractedBinaries {
    /// Extract embedded binaries to a temporary directory
    ///
    /// Creates a temporary directory with a random suffix, extracts mprime and libgmp.so,
    /// sets proper permissions, and creates required symlinks.
    ///
    /// # Returns
    /// - `Ok(ExtractedBinaries)` with paths to the extracted files
    /// - `Err` if extraction fails
    ///
    /// # Example
    /// ```no_run
    /// use core_probe::embedded::ExtractedBinaries;
    ///
    /// let binaries = ExtractedBinaries::extract()?;
    /// // Use binaries.mprime_path...
    /// binaries.cleanup()?;
    /// # Ok::<(), anyhow::Error>(())
    /// ```
    pub fn extract() -> Result<Self> {
        // Create temp directory with random suffix
        let temp_dir = Self::create_temp_dir()?;
        info!(
            path = %temp_dir.display(),
            "Created temporary directory for embedded binaries"
        );

        // Extract mprime binary
        let mprime_path = temp_dir.join("mprime");
        Self::extract_file(&mprime_path, MPRIME_BINARY, "mprime")?;
        Self::make_executable(&mprime_path)?;
        debug!(
            path = %mprime_path.display(),
            size = MPRIME_BINARY.len(),
            "Extracted mprime binary"
        );

        // Create lib directory
        let lib_dir = temp_dir.join("lib");
        fs::create_dir(&lib_dir).context("Failed to create lib directory")?;

        // Extract libgmp.so.10.4.1
        let libgmp_actual = lib_dir.join("libgmp.so.10.4.1");
        Self::extract_file(&libgmp_actual, LIBGMP_BINARY, "libgmp.so.10.4.1")?;
        debug!(
            path = %libgmp_actual.display(),
            size = LIBGMP_BINARY.len(),
            "Extracted libgmp shared library"
        );

        // Create symlink chain: libgmp.so.10 → libgmp.so.10.4.1
        let libgmp_10 = lib_dir.join("libgmp.so.10");
        symlink("libgmp.so.10.4.1", &libgmp_10).context("Failed to create libgmp.so.10 symlink")?;
        debug!(target = "libgmp.so.10.4.1", link = %libgmp_10.display(), "Created symlink");

        // Create symlink chain: libgmp.so → libgmp.so.10
        let libgmp = lib_dir.join("libgmp.so");
        symlink("libgmp.so.10", &libgmp).context("Failed to create libgmp.so symlink")?;
        debug!(target = "libgmp.so.10", link = %libgmp.display(), "Created symlink");

        Ok(ExtractedBinaries {
            temp_dir,
            mprime_path,
            lib_dir,
        })
    }

    /// Remove the temporary directory and all extracted files
    ///
    /// # Returns
    /// - `Ok(())` if cleanup succeeded
    /// - `Err` if removal failed
    pub fn cleanup(&self) -> Result<()> {
        if self.temp_dir.exists() {
            fs::remove_dir_all(&self.temp_dir).with_context(|| {
                format!("Failed to remove temp dir: {}", self.temp_dir.display())
            })?;
            info!(path = %self.temp_dir.display(), "Cleaned up temporary directory");
        }
        Ok(())
    }

    /// Create a temporary directory with a random suffix
    fn create_temp_dir() -> Result<PathBuf> {
        let base = std::env::temp_dir();
        let suffix = uuid::Uuid::new_v4().to_string();
        let temp_dir = base.join(format!("core-probe-{}", suffix));

        fs::create_dir(&temp_dir)
            .with_context(|| format!("Failed to create temp directory: {}", temp_dir.display()))?;

        Ok(temp_dir)
    }

    /// Extract a file from embedded bytes
    fn extract_file(path: &Path, bytes: &[u8], name: &str) -> Result<()> {
        let mut file = File::create(path)
            .with_context(|| format!("Failed to create file: {}", path.display()))?;

        file.write_all(bytes)
            .with_context(|| format!("Failed to write {} bytes to {}", bytes.len(), name))?;

        file.sync_all()
            .with_context(|| format!("Failed to sync {} to disk", name))?;

        Ok(())
    }

    /// Set executable permissions (0o755) on a file
    fn make_executable(path: &Path) -> Result<()> {
        let mut perms = fs::metadata(path)
            .with_context(|| format!("Failed to read metadata: {}", path.display()))?
            .permissions();

        perms.set_mode(0o755);

        fs::set_permissions(path, perms)
            .with_context(|| format!("Failed to set permissions: {}", path.display()))?;

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn given_embedded_binaries_when_extracting_then_creates_temp_directory() {
        // given: embedded binaries are available

        // when: extracting binaries
        let result = ExtractedBinaries::extract();

        // then: extraction succeeds and temp directory exists
        assert!(result.is_ok(), "Extraction should succeed");
        let binaries = result.unwrap();
        assert!(
            binaries.temp_dir.exists(),
            "Temp directory should exist: {}",
            binaries.temp_dir.display()
        );
        assert!(
            binaries
                .temp_dir
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
                .starts_with("core-probe-"),
            "Temp directory should have correct prefix"
        );

        // cleanup
        let _ = binaries.cleanup();
    }

    #[test]
    fn given_extracted_mprime_when_checking_permissions_then_is_executable() {
        // given: binaries are extracted
        let binaries = ExtractedBinaries::extract().expect("Extraction should succeed");

        // when: checking mprime permissions
        let metadata =
            fs::metadata(&binaries.mprime_path).expect("Should be able to read mprime metadata");
        let permissions = metadata.permissions();
        let mode = permissions.mode();

        // then: mprime is executable (0o755 = user:rwx, group:rx, other:rx)
        assert_eq!(
            mode & 0o777,
            0o755,
            "mprime should have 0o755 permissions, got 0o{:o}",
            mode & 0o777
        );
        assert!(
            binaries.mprime_path.exists(),
            "mprime file should exist at {}",
            binaries.mprime_path.display()
        );
        assert_eq!(
            metadata.len(),
            MPRIME_BINARY.len() as u64,
            "mprime should have correct size"
        );

        // cleanup
        let _ = binaries.cleanup();
    }

    #[test]
    fn given_extracted_libgmp_when_checking_symlinks_then_has_correct_links() {
        // given: binaries are extracted
        let binaries = ExtractedBinaries::extract().expect("Extraction should succeed");

        // when: checking libgmp symlinks
        let libgmp_actual = binaries.lib_dir.join("libgmp.so.10.4.1");
        let libgmp_10 = binaries.lib_dir.join("libgmp.so.10");
        let libgmp = binaries.lib_dir.join("libgmp.so");

        // then: actual library file exists with correct size
        assert!(
            libgmp_actual.exists(),
            "libgmp.so.10.4.1 should exist at {}",
            libgmp_actual.display()
        );
        let metadata = fs::metadata(&libgmp_actual).expect("Should read libgmp metadata");
        assert_eq!(
            metadata.len(),
            LIBGMP_BINARY.len() as u64,
            "libgmp should have correct size"
        );

        // then: symlinks exist and point to correct targets
        assert!(
            libgmp_10.exists(),
            "libgmp.so.10 symlink should exist at {}",
            libgmp_10.display()
        );
        let link_target_10 = fs::read_link(&libgmp_10).expect("Should read libgmp.so.10 symlink");
        assert_eq!(
            link_target_10.to_str().unwrap(),
            "libgmp.so.10.4.1",
            "libgmp.so.10 should point to libgmp.so.10.4.1"
        );

        assert!(
            libgmp.exists(),
            "libgmp.so symlink should exist at {}",
            libgmp.display()
        );
        let link_target = fs::read_link(&libgmp).expect("Should read libgmp.so symlink");
        assert_eq!(
            link_target.to_str().unwrap(),
            "libgmp.so.10",
            "libgmp.so should point to libgmp.so.10"
        );

        // cleanup
        let _ = binaries.cleanup();
    }

    #[test]
    fn given_temp_dir_when_cleanup_called_then_directory_removed() {
        // given: binaries are extracted to a temp directory
        let binaries = ExtractedBinaries::extract().expect("Extraction should succeed");
        let temp_dir = binaries.temp_dir.clone();
        assert!(
            temp_dir.exists(),
            "Temp directory should exist before cleanup"
        );

        // when: cleanup is called
        let result = binaries.cleanup();

        // then: cleanup succeeds and temp directory is removed
        assert!(result.is_ok(), "Cleanup should succeed");
        assert!(
            !temp_dir.exists(),
            "Temp directory should be removed after cleanup: {}",
            temp_dir.display()
        );
    }
}
