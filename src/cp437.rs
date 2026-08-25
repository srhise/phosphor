//! Code page 437: the character set burned into the IBM VGA ROM.

/// CP437 byte -> Unicode. Index is the byte value.
pub const TABLE: [char; 256] = [
    ' ', '\u{263A}', '\u{263B}', '\u{2665}', '\u{2666}', '\u{2663}', '\u{2660}', '\u{2022}',
    '\u{25D8}', '\u{25CB}', '\u{25D9}', '\u{2642}', '\u{2640}', '\u{266A}', '\u{266B}', '\u{263C}',
    '\u{25BA}', '\u{25C4}', '\u{2195}', '\u{203C}', '\u{00B6}', '\u{00A7}', '\u{25AC}', '\u{21A8}',
    '\u{2191}', '\u{2193}', '\u{2192}', '\u{2190}', '\u{221F}', '\u{2194}', '\u{25B2}', '\u{25BC}',
    ' ', '!', '"', '#', '$', '%', '&', '\'', '(', ')', '*', '+', ',', '-', '.', '/', '0', '1', '2',
    '3', '4', '5', '6', '7', '8', '9', ':', ';', '<', '=', '>', '?', '@', 'A', 'B', 'C', 'D', 'E',
    'F', 'G', 'H', 'I', 'J', 'K', 'L', 'M', 'N', 'O', 'P', 'Q', 'R', 'S', 'T', 'U', 'V', 'W', 'X',
    'Y', 'Z', '[', '\\', ']', '^', '_', '`', 'a', 'b', 'c', 'd', 'e', 'f', 'g', 'h', 'i', 'j', 'k',
    'l', 'm', 'n', 'o', 'p', 'q', 'r', 's', 't', 'u', 'v', 'w', 'x', 'y', 'z', '{', '|', '}', '~',
    '\u{007F}', '\u{00C7}', '\u{00FC}', '\u{00E9}', '\u{00E2}', '\u{00E4}', '\u{00E0}', '\u{00E5}',
    '\u{00E7}', '\u{00EA}', '\u{00EB}', '\u{00E8}', '\u{00EF}', '\u{00EE}', '\u{00EC}', '\u{00C4}',
    '\u{00C5}', '\u{00C9}', '\u{00E6}', '\u{00C6}', '\u{00F4}', '\u{00F6}', '\u{00F2}', '\u{00FB}',
    '\u{00F9}', '\u{00FF}', '\u{00D6}', '\u{00DC}', '\u{00A2}', '\u{00A3}', '\u{00A5}', '\u{20A7}',
    '\u{0192}', '\u{00E1}', '\u{00ED}', '\u{00F3}', '\u{00FA}', '\u{00F1}', '\u{00D1}', '\u{00AA}',
    '\u{00BA}', '\u{00BF}', '\u{2310}', '\u{00AC}', '\u{00BD}', '\u{00BC}', '\u{00A1}', '\u{00AB}',
    '\u{00BB}', '\u{2591}', '\u{2592}', '\u{2593}', '\u{2502}', '\u{2524}', '\u{2561}', '\u{2562}',
    '\u{2556}', '\u{2555}', '\u{2563}', '\u{2551}', '\u{2557}', '\u{255D}', '\u{255C}', '\u{255B}',
    '\u{2510}', '\u{2514}', '\u{2534}', '\u{252C}', '\u{251C}', '\u{2500}', '\u{253C}', '\u{255E}',
    '\u{255F}', '\u{255A}', '\u{2554}', '\u{2569}', '\u{2566}', '\u{2560}', '\u{2550}', '\u{256C}',
    '\u{2567}', '\u{2568}', '\u{2564}', '\u{2565}', '\u{2559}', '\u{2558}', '\u{2552}', '\u{2553}',
    '\u{256B}', '\u{256A}', '\u{2518}', '\u{250C}', '\u{2588}', '\u{2584}', '\u{258C}', '\u{2590}',
    '\u{2580}', '\u{03B1}', '\u{00DF}', '\u{0393}', '\u{03C0}', '\u{03A3}', '\u{03C3}', '\u{00B5}',
    '\u{03C4}', '\u{03A6}', '\u{0398}', '\u{03A9}', '\u{03B4}', '\u{221E}', '\u{03C6}', '\u{03B5}',
    '\u{2229}', '\u{2261}', '\u{00B1}', '\u{2265}', '\u{2264}', '\u{2320}', '\u{2321}', '\u{00F7}',
    '\u{2248}', '\u{00B0}', '\u{2219}', '\u{00B7}', '\u{221A}', '\u{207F}', '\u{00B2}', '\u{25A0}',
    '\u{00A0}',
];

/// Typographic characters macOS produces that CP437 lacks, and what we
/// store instead. Applied at the input boundary so the buffer can never
/// contain a character the screen cannot draw.
const SUBSTITUTIONS: [(char, &str); 9] = [
    ('\u{2018}', "'"),   // left single quote
    ('\u{2019}', "'"),   // right single quote / apostrophe
    ('\u{201A}', "'"),   // single low quote
    ('\u{201C}', "\""),  // left double quote
    ('\u{201D}', "\""),  // right double quote
    ('\u{201E}', "\""),  // double low quote
    ('\u{2013}', "-"),   // en dash
    ('\u{2014}', "--"),  // em dash
    ('\u{2026}', "..."), // ellipsis
];

/// CP437 byte -> Unicode character.
#[cfg_attr(not(test), allow(dead_code))]
pub fn decode(b: u8) -> char {
    TABLE[b as usize]
}

/// Unicode character -> CP437 byte, if the font has a glyph for it.
pub fn encode(c: char) -> Option<u8> {
    // 0x00 also maps to space; start at 0x20 so space encodes canonically.
    if c == ' ' {
        return Some(0x20);
    }
    TABLE
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, &t)| t == c)
        .map(|(i, _)| i as u8)
}

/// The input filter. Returns the text to insert for a typed character,
/// or `None` if it cannot be represented and should be discarded.
///
/// Returns an owned `String` because one input character can expand to
/// several stored ones: an em dash becomes two hyphens, an ellipsis three
/// periods.
pub fn accept(c: char) -> Option<String> {
    if let Some((_, replacement)) = SUBSTITUTIONS.iter().find(|(from, _)| *from == c) {
        return Some((*replacement).to_string());
    }
    // U+00A0 round-trips through 0xFF but should behave as a plain space.
    if c == '\u{00A0}' {
        return Some(" ".to_string());
    }
    if c.is_control() {
        return None;
    }
    encode(c)?;
    Some(c.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_ascii_unchanged() {
        assert_eq!(decode(b'A'), 'A');
        assert_eq!(decode(b' '), ' ');
    }

    #[test]
    fn decodes_low_range_as_graphics_not_controls() {
        // In the VGA font 0x01 is a smiley, not "start of heading".
        assert_eq!(decode(0x01), '\u{263A}');
        assert_eq!(decode(0x0F), '\u{263C}');
    }

    #[test]
    fn decodes_box_drawing_and_accents() {
        assert_eq!(decode(0xC9), '\u{2554}'); // double top-left corner
        assert_eq!(decode(0xE1), '\u{00DF}'); // sharp s
        assert_eq!(decode(0x82), '\u{00E9}'); // e acute
    }

    #[test]
    fn encode_is_the_inverse_of_decode_for_all_bytes() {
        for b in 0..=255u8 {
            // 0x00 and 0x20 both decode to space; skip the duplicate.
            if b == 0x00 {
                continue;
            }
            assert_eq!(
                encode(decode(b)),
                Some(b),
                "byte {b:#04X} failed round trip"
            );
        }
    }

    #[test]
    fn accept_passes_plain_text_through() {
        assert_eq!(accept('a').as_deref(), Some("a"));
        assert_eq!(accept('\u{00E9}').as_deref(), Some("\u{00E9}")); // e acute survives
    }

    #[test]
    fn accept_substitutes_typographic_characters() {
        assert_eq!(accept('\u{2019}').as_deref(), Some("'"));
        assert_eq!(accept('\u{201C}').as_deref(), Some("\""));
        assert_eq!(accept('\u{201D}').as_deref(), Some("\""));
        assert_eq!(accept('\u{2013}').as_deref(), Some("-"));
        assert_eq!(accept('\u{2014}').as_deref(), Some("--"));
        assert_eq!(accept('\u{2026}').as_deref(), Some("..."));
        assert_eq!(accept('\u{00A0}').as_deref(), Some(" "));
    }

    #[test]
    fn accept_rejects_unrepresentable_characters() {
        assert_eq!(accept('\u{3042}').as_deref(), None); // hiragana A
        assert_eq!(accept('\u{1F600}').as_deref(), None); // emoji
        assert_eq!(accept('\u{0416}').as_deref(), None); // cyrillic ZHE
    }

    #[test]
    fn accept_rejects_control_characters() {
        // Newlines and tabs are handled by the editor as commands, not text.
        assert_eq!(accept('\n').as_deref(), None);
        assert_eq!(accept('\t').as_deref(), None);
        assert_eq!(accept('\u{0007}').as_deref(), None);
    }
}
