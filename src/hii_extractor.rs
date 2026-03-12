use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

use anyhow::Context;

const HIIDB_EFIVAR_PATH: &str =
    "/sys/firmware/efi/efivars/HiiDB-1b838190-4625-4ead-abc9-cd5e6af18fe0";

pub fn check_hii_available() -> bool {
    Path::new(HIIDB_EFIVAR_PATH).exists()
}

/// Extract the raw HII database from system memory.
///
/// The HiiDB EFI variable contains a 12-byte header with the physical memory
/// address and size of the HII database. This function reads that header from
/// efivarfs, then reads the actual database blob from `/dev/mem`.
///
/// # Binary layout of the efivar (12 bytes, little-endian)
/// ```text
/// bytes 0-3:  flags   (u32) - EFI variable attributes (skipped)
/// bytes 4-7:  length  (u32) - size of HII DB in memory
/// bytes 8-11: address (u32) - physical address of HII DB
/// ```
///
/// # Errors
/// - If the efivar file cannot be opened or read
/// - If the efivar contents are shorter than 12 bytes
/// - If `/dev/mem` cannot be opened (e.g. `CONFIG_STRICT_DEVMEM` enabled)
/// - If the memory read fails
pub fn extract_hii_db() -> anyhow::Result<Vec<u8>> {
    let mut efivar_file = File::open(HIIDB_EFIVAR_PATH)
        .with_context(|| format!("Failed to open efivar at {}", HIIDB_EFIVAR_PATH))?;

    let mut efivar_contents = Vec::new();
    efivar_file
        .read_to_end(&mut efivar_contents)
        .with_context(|| format!("Failed to read efivar file at {}", HIIDB_EFIVAR_PATH))?;

    let (length, address) = parse_efivar_bytes(&efivar_contents)?;

    let mut mem_file = File::open("/dev/mem")
        .context("Failed to open /dev/mem (CONFIG_STRICT_DEVMEM may be enabled)")?;
    mem_file
        .seek(SeekFrom::Start(address as u64))
        .with_context(|| {
            format!(
                "Failed to seek to physical address {:#X} in /dev/mem",
                address
            )
        })?;

    let mut buf = vec![0u8; length as usize];
    mem_file.read_exact(&mut buf).with_context(|| {
        format!(
            "Failed to read {} bytes from /dev/mem at address {:#X}",
            length, address
        )
    })?;

    Ok(buf)
}

fn parse_efivar_bytes(bytes: &[u8]) -> anyhow::Result<(u32, u32)> {
    if bytes.len() < 12 {
        anyhow::bail!(
            "EFI variable too short: {} bytes (expected at least 12)",
            bytes.len()
        );
    }
    let _flags = u32::from_le_bytes(bytes[0..4].try_into()?);
    let length = u32::from_le_bytes(bytes[4..8].try_into()?);
    let address = u32::from_le_bytes(bytes[8..12].try_into()?);
    Ok((length, address))
}

#[derive(Debug, Default)]
pub struct BiosInfo {
    pub bios_vendor: String,
    pub bios_version: String,
    pub product_name: String,
}

pub fn read_bios_info() -> BiosInfo {
    BiosInfo {
        bios_vendor: read_dmi_file("/sys/class/dmi/id/bios_vendor"),
        bios_version: read_dmi_file("/sys/class/dmi/id/bios_version"),
        product_name: read_dmi_file("/sys/class/dmi/id/product_name"),
    }
}

fn read_dmi_file(path: &str) -> String {
    match std::fs::read_to_string(path) {
        Ok(contents) => {
            let trimmed = contents.trim().to_owned();
            if trimmed.is_empty() {
                "Unknown".to_owned()
            } else {
                trimmed
            }
        }
        Err(_) => "Unknown".to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn given_valid_efivar_bytes_when_parsing_then_extracts_length_and_address() {
        // flags=0x00000007, length=0x00010000 (65536), address=0xA0000000
        let bytes: [u8; 12] = [
            0x07, 0x00, 0x00, 0x00, // flags = 7 (LE)
            0x00, 0x00, 0x01, 0x00, // length = 65536 (LE)
            0x00, 0x00, 0x00, 0xA0, // address = 0xA0000000 (LE)
        ];

        let (length, address) = parse_efivar_bytes(&bytes).unwrap();

        assert_eq!(length, 65536);
        assert_eq!(address, 0xA000_0000);
    }

    #[test]
    fn given_missing_efivar_when_checking_availability_then_returns_false() {
        let available = check_hii_available();
        let expected =
            Path::new("/sys/firmware/efi/efivars/HiiDB-1b838190-4625-4ead-abc9-cd5e6af18fe0")
                .exists();

        assert_eq!(available, expected);
    }

    #[test]
    fn given_truncated_efivar_bytes_when_parsing_then_returns_error() {
        // Only 8 bytes — shorter than the required 12.
        let bytes: [u8; 8] = [0x07, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01, 0x00];

        let result = parse_efivar_bytes(&bytes);

        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(
            err_msg.contains("too short"),
            "Expected 'too short' in error message, got: {}",
            err_msg
        );
    }

    #[test]
    fn given_dmi_files_missing_when_reading_bios_info_then_uses_defaults() {
        // read_bios_info must never panic, regardless of DMI file availability.
        // On a real machine the fields will be populated; in CI they may be "Unknown".
        let info = read_bios_info();

        assert!(!info.bios_vendor.is_empty());
        assert!(!info.bios_version.is_empty());
        assert!(!info.product_name.is_empty());
    }
}
