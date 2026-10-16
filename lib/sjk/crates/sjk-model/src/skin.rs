//! Ghoul2 `.skin` files: which shader draws each named mesh surface.
//!
//! Skins are read the way rd-vanilla's `RE_RegisterIndividualSkin` reads them
//! (`codemp/rd-vanilla/tr_skin.cpp`): a stream of `CommaParse` tokens taken in
//! surface/shader pairs, so line breaks, missing commas, comments and stray
//! text change nothing about what loads. No file is ever refused; a file with
//! no usable pair gives an empty skin, which a caller treats as rd-vanilla
//! treats skin handle 0.

use std::collections::HashMap;

/// Skin shader name that hides a surface instead of texturing it.
pub const SKIN_SHADER_OFF: &str = "*off";

/// Surfaces one skin can name, counted over all parts (`skin_t::surfaces[128]`).
const MAX_SKIN_SURFACES: usize = 128;

/// Longest surface name `RE_RegisterIndividualSkin` keeps (`MAX_QPATH` - 1).
const MAX_SURFACE_NAME: usize = 63;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Skin {
    /// Lower-case surface name without `_off` to lower-case shader name.
    mappings: HashMap<String, String>,
    /// Surface entries read, duplicates included, as `skin_t::numSurfaces` counts them.
    entries: usize,
}

impl Skin {
    /// Read one `.skin` file. It never fails: whatever the tokenizer finds is
    /// what rd-vanilla would register.
    pub fn parse(bytes: &[u8]) -> Self {
        let mut skin = Self::default();
        skin.append(bytes);
        skin
    }

    /// Add one more file to this skin, as `RE_RegisterSkin` adds the torso and
    /// lower parts of a `head|torso|lower` skin to the head's. An entry for a
    /// surface already named keeps the earlier shader: the renderer takes the
    /// first match (`RenderSurfaces`, `tr_ghoul2.cpp`).
    pub fn append(&mut self, bytes: &[u8]) {
        let mut tokens = CommaParser { bytes, position: 0 };
        loop {
            let token = tokens.next_token();
            if token.is_empty() {
                break;
            }
            if tokens.peek() == b',' {
                tokens.position += 1;
            }
            // id-style tag entries; the token after one is read as a surface.
            if token.starts_with(b"tag_") {
                continue;
            }
            let mut surface = text(&token[..token.len().min(MAX_SURFACE_NAME)]);
            surface.make_ascii_lowercase();
            let shader = tokens.next_token();
            if let Some(stripped) = surface.strip_suffix("_off") {
                if shader == SKIN_SHADER_OFF.as_bytes() {
                    continue;
                }
                surface.truncate(stripped.len());
            }
            if self.entries >= MAX_SKIN_SURFACES {
                break;
            }
            self.entries += 1;
            let shader = text(&shader).replace('\\', "/").to_ascii_lowercase();
            self.mappings.entry(surface).or_insert(shader);
        }
    }

    /// The shader for a mesh surface, matched without case and without a
    /// trailing `_off`, which `R_LoadMDXM` strips from surface names.
    pub fn shader(&self, surface: &str) -> Option<&str> {
        let surface = surface.to_ascii_lowercase();
        let surface = surface.strip_suffix("_off").unwrap_or(&surface);
        self.mappings.get(surface).map(String::as_str)
    }

    /// Distinct surfaces the skin names.
    pub fn len(&self) -> usize {
        self.mappings.len()
    }

    /// True when the skin names no surface; rd-vanilla then returns skin
    /// handle 0 ("never let a skin have 0 shaders").
    pub fn is_empty(&self) -> bool {
        self.mappings.is_empty()
    }
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// `CommaParse` (`tr_skin.cpp`): whitespace, `//` and `/* */` comments separate
/// tokens; a token is a quoted string or runs to whitespace or a comma.
struct CommaParser<'a> {
    bytes: &'a [u8],
    position: usize,
}

impl CommaParser<'_> {
    fn at(&self, index: usize) -> u8 {
        // The C string ends at its first NUL as well as at the buffer's end.
        self.bytes.get(index).copied().unwrap_or(0)
    }

    fn peek(&self) -> u8 {
        self.at(self.position)
    }

    fn next_token(&mut self) -> Vec<u8> {
        loop {
            while self.peek() != 0 && self.peek() <= b' ' {
                self.position += 1;
            }
            if self.peek() == b'/' && self.at(self.position + 1) == b'/' {
                while self.peek() != 0 && self.peek() != b'\n' {
                    self.position += 1;
                }
            } else if self.peek() == b'/' && self.at(self.position + 1) == b'*' {
                while self.peek() != 0
                    && !(self.peek() == b'*' && self.at(self.position + 1) == b'/')
                {
                    self.position += 1;
                }
                if self.peek() != 0 {
                    self.position += 2;
                }
            } else {
                break;
            }
        }
        let mut token = Vec::new();
        if self.peek() == 0 {
            return token;
        }
        if self.peek() == b'"' {
            self.position += 1;
            loop {
                let character = self.peek();
                if character != 0 {
                    self.position += 1;
                }
                if character == b'"' || character == 0 {
                    return token;
                }
                token.push(character);
            }
        }
        loop {
            token.push(self.peek());
            self.position += 1;
            // `c > 32` on a signed char: a byte from 0x80 up also ends the word.
            let next = self.peek();
            if next <= b' ' || next == b',' || next >= 0x80 {
                return token;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{SKIN_SHADER_OFF, Skin};

    #[test]
    fn plain_lines_map_surfaces_to_shaders() {
        let skin =
            Skin::parse(b"head,models/players/a/head\r\nTorso,Models\\Players\\A\\Torso.tga\r\n");
        assert_eq!(skin.shader("head"), Some("models/players/a/head"));
        assert_eq!(skin.shader("TORSO"), Some("models/players/a/torso.tga"));
        assert_eq!(skin.len(), 2);
    }

    #[test]
    fn comments_blank_lines_and_spacing_are_ignored() {
        let skin = Skin::parse(
            b"// header\n\n  head,  shader/a // trailing\n/* block\n torso,nope */ torso,shader/b\n",
        );
        assert_eq!(skin.shader("head"), Some("shader/a"));
        assert_eq!(skin.shader("torso"), Some("shader/b"));
        assert_eq!(skin.len(), 2);
    }

    #[test]
    fn a_comma_after_a_space_is_read_as_the_shader() {
        // CommaParse only skips a comma right after the surface name, so
        // rd-vanilla gives `head` the shader "," and pairs the rest anew.
        let skin = Skin::parse(b"head , shader/a\ntorso,shader/b\n");
        assert_eq!(skin.shader("head"), Some(","));
        assert_eq!(skin.shader("shader/a"), Some("torso"));
    }

    #[test]
    fn a_missing_comma_still_pairs_tokens() {
        let skin = Skin::parse(b"head shader/a\ntorso\tshader/b");
        assert_eq!(skin.shader("head"), Some("shader/a"));
        assert_eq!(skin.shader("torso"), Some("shader/b"));
    }

    #[test]
    fn stray_text_is_read_as_pairs_instead_of_refused() {
        // A three-part pack ships this as its lower_a1.skin.
        let skin = Skin::parse(b"There's nothing here!");
        assert_eq!(skin.shader("there's"), Some("nothing"));
        assert_eq!(skin.shader("here!"), Some(""));
        assert!(skin.shader("hips").is_none());
    }

    #[test]
    fn quoted_tokens_keep_their_spaces() {
        let skin = Skin::parse(b"\"head\",\"models/my skins/head\"");
        assert_eq!(skin.shader("head"), Some("models/my skins/head"));
    }

    #[test]
    fn tag_entries_are_skipped() {
        let skin = Skin::parse(b"tag_head,\ntag_weapon,\nhead,shader/a\n");
        assert_eq!(skin.shader("head"), Some("shader/a"));
        assert_eq!(skin.len(), 1);
    }

    #[test]
    fn off_suffixes_match_surfaces_like_the_renderer() {
        let skin = Skin::parse(b"head_cap_torso_off,caps\nhips_cap_l_leg_off,*off\n");
        // R_LoadMDXM strips _off from surface names; the skin entry is stored without it.
        assert_eq!(skin.shader("head_cap_torso_off"), Some("caps"));
        assert_eq!(skin.shader("head_cap_torso"), Some("caps"));
        // An `_off` surface switched off again is dropped as a double off.
        assert!(skin.shader("hips_cap_l_leg_off").is_none());
        assert_eq!(skin.len(), 1);
    }

    #[test]
    fn star_off_hides_ordinary_surfaces() {
        let skin = Skin::parse(b"helmet,*off");
        assert_eq!(skin.shader("helmet"), Some(SKIN_SHADER_OFF));
    }

    #[test]
    fn the_first_entry_for_a_surface_wins() {
        let mut skin = Skin::parse(b"head,first\nhead,second\n");
        assert_eq!(skin.shader("head"), Some("first"));
        skin.append(b"head,third\ntorso,body\n");
        assert_eq!(skin.shader("head"), Some("first"));
        assert_eq!(skin.shader("torso"), Some("body"));
    }

    #[test]
    fn bytes_outside_utf8_do_not_refuse_the_file() {
        let skin = Skin::parse(b"// caf\xe9\nhead,shader/a\n");
        assert_eq!(skin.shader("head"), Some("shader/a"));
        // CommaParse compares a signed char: a high byte ends a word.
        let skin = Skin::parse(b"head,ab\xe9cd\n");
        assert_eq!(skin.shader("head"), Some("ab"));
    }

    #[test]
    fn an_empty_or_comment_only_file_names_nothing() {
        assert!(Skin::parse(b"").is_empty());
        assert!(Skin::parse(b"// nothing\r\n\r\n").is_empty());
    }

    #[test]
    fn a_skin_stops_at_128_entries_over_all_parts() {
        let lines = |prefix: &str, count: usize| {
            (0..count)
                .map(|index| format!("{prefix}{index},shader\n"))
                .collect::<String>()
        };
        let mut skin = Skin::parse(lines("head", 100).as_bytes());
        skin.append(lines("torso", 40).as_bytes());
        assert_eq!(skin.len(), 128);
        assert!(skin.shader("torso27").is_some());
        assert!(skin.shader("torso28").is_none());
    }
}
