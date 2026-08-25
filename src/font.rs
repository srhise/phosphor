//! The IBM VGA character generator, embedded in the binary.
//!
//! Each byte is one row of one glyph, one bit per pixel, MSB leftmost.
//! Glyph N of an 8x16 face occupies bytes `N*16 .. N*16+16`.

/// 256 glyphs x 16 rows.
pub const DATA_8X16: &[u8; 4096] = include_bytes!("../assets/fonts/IBM_VGA_8x16.bin");
/// 256 glyphs x 8 rows.
pub const DATA_8X8: &[u8; 2048] = include_bytes!("../assets/fonts/IBM_VGA_8x8.bin");

/// A character cell is 9 pixels wide even though the font is 8. See
/// `glyph_row` for what fills the 9th column.
pub const CELL_WIDTH: usize = 9;

/// Glyphs in this range are box-drawing characters whose 9th column
/// repeats the 8th, so that horizontal rules connect across cells.
const BOX_DRAWING: std::ops::RangeInclusive<u8> = 0xC0..=0xDF;

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Face {
    W8x16,
    W8x8,
}

impl Face {
    pub fn cell_height(self) -> usize {
        match self {
            Face::W8x16 => 16,
            Face::W8x8 => 8,
        }
    }

    fn data(self) -> &'static [u8] {
        match self {
            Face::W8x16 => DATA_8X16.as_slice(),
            Face::W8x8 => DATA_8X8.as_slice(),
        }
    }
}

/// One row of one glyph as 9 bits: bit 8 is the leftmost pixel, bit 0 the
/// rightmost. Rows past the end of the cell read as blank.
pub fn glyph_row(face: Face, glyph: u8, row: usize) -> u16 {
    let height = face.cell_height();
    if row >= height {
        return 0;
    }
    let bits = face.data()[glyph as usize * height + row];
    let ninth = if BOX_DRAWING.contains(&glyph) { bits & 1 } else { 0 };
    (u16::from(bits) << 1) | u16::from(ninth)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_data_is_the_expected_size() {
        assert_eq!(DATA_8X16.len(), 4096);
        assert_eq!(DATA_8X8.len(), 2048);
    }

    #[test]
    fn capital_a_has_the_expected_bitmap() {
        // Rows 2..12 of glyph 0x41 in the IBM VGA 8x16 face.
        let expected: [u8; 10] = [
            0b00010000, 0b00111000, 0b01101100, 0b11000110, 0b11000110,
            0b11111110, 0b11000110, 0b11000110, 0b11000110, 0b11000110,
        ];
        for (i, want) in expected.iter().enumerate() {
            let got = glyph_row(Face::W8x16, b'A', i + 2);
            assert_eq!(got >> 1, u16::from(*want), "row {} of 'A'", i + 2);
        }
    }

    #[test]
    fn blank_rows_are_empty() {
        assert_eq!(glyph_row(Face::W8x16, b'A', 0), 0);
        assert_eq!(glyph_row(Face::W8x16, b'A', 15), 0);
    }

    #[test]
    fn ninth_column_repeats_for_box_drawing_glyphs() {
        // 0xC9 is the double top-left corner; its horizontal arm runs to
        // column 7, so the 9th column must continue it or the box breaks.
        // Row 5 is the arm; rows 6+ are the vertical stem only.
        let row = glyph_row(Face::W8x16, 0xC9, 5);
        assert_eq!(row & 1, (row >> 1) & 1, "9th column must repeat column 7");
        assert_eq!(row & 1, 1, "the arm of 0xC9 does reach column 7");

        // The stem rows stop short, so their 9th column stays blank.
        let stem = glyph_row(Face::W8x16, 0xC9, 6);
        assert_eq!(stem & 1, 0, "nothing to continue on this row");
    }

    #[test]
    fn ninth_column_is_blank_for_ordinary_glyphs() {
        // Without this rule, wide letters would smear into the next cell.
        for row in 0..16 {
            assert_eq!(glyph_row(Face::W8x16, b'M', row) & 1, 0);
        }
    }

    #[test]
    fn small_face_has_eight_rows() {
        assert_eq!(Face::W8x8.cell_height(), 8);
        assert_eq!(Face::W8x16.cell_height(), 16);
        let _ = glyph_row(Face::W8x8, 0xFF, 7);
    }

    #[test]
    fn out_of_range_rows_are_blank_rather_than_panicking() {
        assert_eq!(glyph_row(Face::W8x16, b'A', 99), 0);
        assert_eq!(glyph_row(Face::W8x8, b'A', 8), 0);
    }
}
