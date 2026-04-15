use anyhow::{Context, Result};
use fast_qr::qr::QRBuilder;
use iced::widget::{column, container, row, text, Space};
use iced::{Color, Element, Length};

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

pub fn qr_code_view<'a>(
    qr_content: &str,
    is_dark: bool,
    module_size: f32,
) -> Element<'a, crate::gui::Message> {
    let matrix = match generate_qr_matrix(qr_content) {
        Ok(m) => m,
        Err(_) => return text("QR code unavailable").into(),
    };

    let (dark_module_color, light_module_color) = if is_dark {
        (Color::WHITE, Color::from_rgb(0.1, 0.1, 0.1))
    } else {
        (Color::BLACK, Color::WHITE)
    };

    let quiet_zone_color = light_module_color;

    let rows: Vec<Element<'a, crate::gui::Message>> = matrix
        .modules
        .iter()
        .map(|module_row| {
            let cells: Vec<Element<'a, crate::gui::Message>> = module_row
                .iter()
                .map(|&is_dark_module| {
                    let color = if is_dark_module {
                        dark_module_color
                    } else {
                        light_module_color
                    };
                    container(Space::new())
                        .width(Length::Fixed(module_size))
                        .height(Length::Fixed(module_size))
                        .style(move |_theme: &iced::Theme| container::Style {
                            background: Some(color.into()),
                            ..container::Style::default()
                        })
                        .into()
                })
                .collect();
            row(cells).spacing(0).into()
        })
        .collect();

    let grid = column(rows).spacing(0);

    let padding = module_size as u16;
    container(grid)
        .padding(padding)
        .style(move |_theme: &iced::Theme| container::Style {
            background: Some(quiet_zone_color.into()),
            ..container::Style::default()
        })
        .into()
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

    #[test]
    fn test_qr_code_view_returns_element() {
        let _elem = qr_code_view("Failed BIOS cores: 2, 5, 8", true, 6.0);
    }
}
