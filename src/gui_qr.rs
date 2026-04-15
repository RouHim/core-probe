use anyhow::{Context, Result};
use fast_qr::qr::QRBuilder;

pub struct QrMatrix {
    pub modules: Vec<Vec<bool>>,
    pub size: usize,
}

pub fn build_qr_content(failed_bios_indices: &[u32]) -> String {
    if failed_bios_indices.is_empty() {
        return "Failed BIOS cores: (none)".to_string();
    }

    let mut indices = failed_bios_indices.to_vec();
    indices.sort_unstable();

    let joined = indices
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(", ");

    format!("Failed BIOS cores: {joined}")
}

pub fn generate_qr_matrix(content: &str) -> Result<QrMatrix> {
    let qr_code = QRBuilder::new(content.as_bytes().to_vec())
        .build()
        .context("failed to build QR code")?;

    let size = qr_code.size;
    let modules = (0..size)
        .map(|row| {
            (0..size)
                .map(|col| qr_code[row][col].value())
                .collect::<Vec<bool>>()
        })
        .collect::<Vec<Vec<bool>>>();

    Ok(QrMatrix { modules, size })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_qr_content_single_core() {
        assert_eq!(build_qr_content(&[5]), "Failed BIOS cores: 5");
    }

    #[test]
    fn test_build_qr_content_multiple_cores() {
        assert_eq!(build_qr_content(&[2, 5, 8]), "Failed BIOS cores: 2, 5, 8");
    }

    #[test]
    fn test_build_qr_content_all_cores() {
        let cores: Vec<u32> = (0..12).collect();
        let content = build_qr_content(&cores);

        assert!(content.contains("0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11"));
    }

    #[test]
    fn test_build_qr_content_empty() {
        assert_eq!(build_qr_content(&[]), "Failed BIOS cores: (none)");
    }

    #[test]
    fn test_build_qr_content_sorted() {
        assert_eq!(build_qr_content(&[8, 2, 5]), "Failed BIOS cores: 2, 5, 8");
    }

    #[test]
    fn test_generate_qr_matrix_valid() {
        let matrix = generate_qr_matrix("Failed BIOS cores: 2, 5, 8")
            .expect("valid QR content should generate a matrix");

        assert!(matrix.size > 0);
    }

    #[test]
    fn test_generate_qr_matrix_dimensions() {
        let matrix = generate_qr_matrix("Failed BIOS cores: 2, 5, 8")
            .expect("valid QR content should generate a matrix");

        assert_eq!(matrix.modules.len(), matrix.size);
        assert!(matrix.modules.iter().all(|row| row.len() == matrix.size));
    }

    #[test]
    fn test_qr_roundtrip() {
        let content = "Failed BIOS cores: 2, 5, 8";
        let matrix =
            generate_qr_matrix(content).expect("valid QR content should generate a matrix");

        let grid = qr_code::decode::SimpleGrid::from_func(matrix.size, |x, y| matrix.modules[y][x]);
        let decoded = qr_code::decode::Grid::new(grid)
            .decode()
            .expect("QR matrix should decode")
            .1;

        assert_eq!(decoded, content);
    }

    #[test]
    fn test_qr_max_content() {
        let content = build_qr_content(&(0..16).collect::<Vec<_>>());
        let matrix = generate_qr_matrix(&content).expect("worst case content should generate");

        assert!(matrix.size <= 41);
    }
}
