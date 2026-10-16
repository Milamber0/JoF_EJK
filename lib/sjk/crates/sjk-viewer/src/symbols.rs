//! SJK Symbols (`docs/sjk-ui.md`, "Symbols"): SJK's own symbol font, marks for qualities
//! drawn in the colour of the text around them. Each symbol is a character in Unicode's
//! Private Use Area, so any text can hold one ("\u{E003} Legendary"); the font
//! (`assets/fonts/SJKSymbols.ttf`, built by `scripts/sjk_symbols.py`) draws it through
//! the wide-glyph atlas ([`crate::text::wide`]) after every face's own fonts.
//!
//! A symbol keeps its code point for good: a new one takes the next free point of its
//! group in the script and gets its name here.

/// The rarity marks, Common to Mythical: one holocron octagon for every rank, the marks
/// inside it growing (empty, a core, an axis, a cross round a core, a second frame).
pub(crate) const RARITY: [char; 5] = ['\u{E000}', '\u{E001}', '\u{E002}', '\u{E003}', '\u{E004}'];

/// The cards' finishes: Normal (a card), Holo (a card with slanting bands), Solar (a
/// card with a sun at its heart).
pub(crate) const FINISH: [char; 3] = ['\u{E010}', '\u{E011}', '\u{E012}'];

/// The rarity mark of a card rarity (`crate::cards::RARITIES` order, Common to
/// Legendary).
pub(crate) fn card_rarity(index: usize) -> char {
    RARITY[index.min(3)]
}

/// The mark of a card finish (`crate::cards::FINISHES` order).
pub(crate) fn finish(index: usize) -> char {
    FINISH[index.min(FINISH.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use ab_glyph::{Font, FontRef};

    /// Every symbol the client names is in the font, and the font is the script's
    /// (`python scripts/sjk_symbols.py --check` checks the bytes).
    #[test]
    fn every_symbol_is_in_the_font() {
        let font = FontRef::try_from_slice(include_bytes!("../assets/fonts/SJKSymbols.ttf"))
            .expect("SJK Symbols reads");
        for symbol in RARITY.iter().chain(FINISH.iter()) {
            assert_ne!(font.glyph_id(*symbol).0, 0, "U+{:04X}", u32::from(*symbol));
        }
        // Not a letter the faces have: a symbol never stands in for one.
        assert_eq!(font.glyph_id('A').0, 0);
    }

    #[test]
    fn the_ranks_line_up() {
        assert_eq!(card_rarity(0), '\u{E000}');
        assert_eq!(card_rarity(3), '\u{E003}');
        use crate::unlockables::Rarity;
        assert_eq!(Rarity::Common.symbol(), '\u{E000}');
        assert_eq!(
            Rarity::Uncommon.symbol(),
            '\u{E001}',
            "an Uncommon holocron"
        );
        assert_eq!(Rarity::Mythical.symbol(), '\u{E004}', "a Mythical one");
        assert_eq!(finish(2), '\u{E012}');
    }
}
