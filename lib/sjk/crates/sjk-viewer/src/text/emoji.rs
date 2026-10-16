//! Colour emoji in text: Twemoji's pictures, drawn where text holds an emoji.
//!
//! An emoji is often several code points drawn as one picture: a skin tone after a
//! hand (`👍🏽`), people joined by U+200D into a family, two regional indicators into
//! a flag, a digit, U+FE0F and U+20E3 into a keycap. [`match_at`] finds the longest
//! sequence of the pack at a place in the text, so the layout walk
//! ([`super::glyph_at`]) takes the whole sequence as one glyph
//! ([`super::GlyphKey::Emoji`]). U+FE0F (emoji presentation) is ignored when matching,
//! as Twemoji's own lookup ignores it, and is taken into the glyph it follows. A
//! character with a Windows-1252 byte (`©`, `™`, a digit) is an emoji only when U+FE0F
//! or U+20E3 follows it, so plain text never turns into pictures.
//!
//! The pack ([`twemoji.txt`, `twemoji.bin`], built by `scripts/twemoji_pack.py`) holds
//! Twemoji's 72-pixel PNGs, under CC-BY 4.0. A picture is decoded the first time it is
//! shown ([`rasterize_pending`], within the frame budget) into an RGBA atlas
//! ([`ATLAS_WIDTH`] by [`ATLAS_HEIGHT`], [`super::ATLAS_MIP_LEVELS`] levels, kept on the
//! CPU too) like the wide glyphs ([`super::wide`]); until then its glyph has its
//! advance and no picture. A vertex whose `u` is [`U_OFFSET`] or more samples it
//! (`text.wgsl`): its own colours, faded by the text's alpha, or black for a shadow.

pub(crate) use super::wide::Cell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::Instant;

const INDEX: &str = include_str!("../../assets/emoji/twemoji.txt");
const PNGS: &[u8] = include_bytes!("../../assets/emoji/twemoji.bin");
/// Pixels on each side of a Twemoji picture.
pub(crate) const PICTURE: u32 = 72;
/// The emoji atlas's size in texels: 253 pictures.
pub(crate) const ATLAS_WIDTH: u32 = 2_048;
pub(crate) const ATLAS_HEIGHT: u32 = 1_024;
/// Gutter around a picture inside its cell; cells are whole texels at every mip level.
const GUTTER: u32 = super::ATLAS_PADDING / 2;
const CELL: u32 = PICTURE + 2 * GUTTER;
const COLUMNS: u32 = ATLAS_WIDTH / CELL;
const CELLS: u32 = COLUMNS * (ATLAS_HEIGHT / CELL);
/// Added to an emoji's `u` coordinates so `text.wgsl` samples the emoji atlas.
pub(crate) const U_OFFSET: f32 = 4.0;
/// Longest key of the pack, in bytes ("1f9d1-1f3fb-200d-..." of nine code points).
const KEY_BYTES: usize = 9 * 6;

/// The pack's index: each emoji's key and where its PNG is.
struct Index {
    ids: HashMap<&'static str, u16>,
    /// Every key's leading code points, so a match stops as soon as none can follow.
    prefixes: HashSet<&'static str>,
    pngs: Vec<(usize, usize)>,
}

fn index() -> &'static Index {
    static INDEX_ONCE: OnceLock<Index> = OnceLock::new();
    INDEX_ONCE.get_or_init(|| {
        let mut index = Index {
            ids: HashMap::new(),
            prefixes: HashSet::new(),
            pngs: Vec::new(),
        };
        for line in INDEX.lines() {
            let mut fields = line.trim_end_matches('\r').split(' ');
            let (Some(key), Some(offset), Some(length)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };
            let (Ok(offset), Ok(length)) = (offset.parse::<usize>(), length.parse::<usize>())
            else {
                continue;
            };
            if offset + length > PNGS.len() || index.pngs.len() >= usize::from(u16::MAX) {
                continue;
            }
            index.ids.insert(key, index.pngs.len() as u16);
            index.pngs.push((offset, length));
            for (at, _) in key.match_indices('-') {
                index.prefixes.insert(&key[..at]);
            }
        }
        index
    })
}

/// Whether `character` can start an emoji of the pack without a U+FE0F after it.
fn may_start(character: char) -> bool {
    let code = u32::from(character);
    code >= 0x1F000
        || (0x2000..=0x2BFF).contains(&code)
        || matches!(code, 0x3030 | 0x303D | 0x3297 | 0x3299)
}

/// The emoji at the start of `text` and how many bytes it takes, when `text` starts
/// with one: a character with a Windows-1252 byte only when U+FE0F or U+20E3 follows.
pub(crate) fn match_at(text: &str) -> Option<(u16, usize)> {
    let mut chars = text.chars();
    let first = chars.next()?;
    let presented = matches!(chars.next(), Some('\u{FE0F}' | '\u{20E3}'));
    if sjk_protocol::windows_1252_byte(first).is_some() {
        if !presented {
            return None;
        }
    } else if !may_start(first) {
        return None;
    }
    let index = index();
    let mut key = [0_u8; KEY_BYTES];
    let mut length = 0;
    let mut best = None;
    for (at, character) in text.char_indices() {
        if character == '\u{FE0F}' {
            if let Some((id, end)) = best
                && end == at
            {
                best = Some((id, at + character.len_utf8()));
            }
            continue;
        }
        let hex = format_hex(u32::from(character));
        let needed = hex.len() + usize::from(length > 0);
        if length + needed > KEY_BYTES {
            break;
        }
        if length > 0 {
            key[length] = b'-';
            length += 1;
        }
        key[length..length + hex.len()].copy_from_slice(hex.as_bytes());
        length += hex.len();
        let Ok(so_far) = std::str::from_utf8(&key[..length]) else {
            break;
        };
        if let Some(&id) = index.ids.get(so_far) {
            best = Some((id, at + character.len_utf8()));
        }
        if !index.prefixes.contains(so_far) {
            break;
        }
    }
    best
}

/// `code` in lower-case hex without leading zeros, as Twemoji names its files.
fn format_hex(code: u32) -> HexDigits {
    let mut digits = HexDigits {
        bytes: [0; 8],
        length: 0,
    };
    let mut started = false;
    for shift in (0..8).rev() {
        let digit = (code >> (shift * 4)) & 0xF;
        if digit != 0 || started || shift == 0 {
            started = true;
            digits.bytes[digits.length] = b"0123456789abcdef"[digit as usize];
            digits.length += 1;
        }
    }
    digits
}

struct HexDigits {
    bytes: [u8; 8],
    length: usize,
}

impl HexDigits {
    fn as_bytes(&self) -> &[u8] {
        &self.bytes[..self.length]
    }

    fn len(&self) -> usize {
        self.length
    }
}

/// Where an emoji's picture is: `[u0, v0, u1, v1]` in the emoji atlas, `u` offset by
/// [`U_OFFSET`]; `None` until it is decoded.
pub(crate) fn picture(id: u16) -> Option<[f32; 4]> {
    let mut guard = atlas();
    let atlas = guard.get_or_insert_with(Atlas::default);
    if let Some(&cell) = atlas.placed.get(&id) {
        let (x, y) = atlas.origin(cell);
        let (width, height) = (ATLAS_WIDTH as f32, ATLAS_HEIGHT as f32);
        return Some([
            U_OFFSET + (x + GUTTER) as f32 / width,
            (y + GUTTER) as f32 / height,
            U_OFFSET + (x + GUTTER + PICTURE) as f32 / width,
            (y + GUTTER + PICTURE) as f32 / height,
        ]);
    }
    if atlas.queued.insert(id) {
        atlas.queue.push_back(id);
    }
    None
}

#[derive(Default)]
struct Atlas {
    placed: HashMap<u16, u32>,
    queued: HashSet<u16>,
    queue: VecDeque<u16>,
    next: u32,
    /// Level-0 RGBA, [`ATLAS_WIDTH`] by [`ATLAS_HEIGHT`]; empty until the first picture.
    pixels: Vec<u8>,
    dirty: Vec<Cell>,
}

static ATLAS: Mutex<Option<Atlas>> = Mutex::new(None);

fn atlas() -> MutexGuard<'static, Option<Atlas>> {
    ATLAS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Atlas {
    fn origin(&self, cell: u32) -> (u32, u32) {
        ((cell % COLUMNS) * CELL, (cell / COLUMNS) * CELL)
    }

    fn rasterize(&mut self, deadline: Instant) {
        while Instant::now() < deadline {
            let Some(id) = self.queue.pop_front() else {
                return;
            };
            self.queued.remove(&id);
            if self.placed.contains_key(&id) {
                continue;
            }
            if self.next >= CELLS {
                // Full: start over, the pictures on screen come back over the next frames.
                self.placed.clear();
                self.dirty.clear();
                self.next = 0;
            }
            let Some(&(offset, length)) = index().pngs.get(usize::from(id)) else {
                continue;
            };
            let Ok(picture) = image::load_from_memory_with_format(
                &PNGS[offset..offset + length],
                image::ImageFormat::Png,
            ) else {
                continue;
            };
            let mut picture = picture.to_rgba8();
            if picture.dimensions() != (PICTURE, PICTURE) {
                picture = image::imageops::resize(
                    &picture,
                    PICTURE,
                    PICTURE,
                    image::imageops::FilterType::Triangle,
                );
            }
            if self.pixels.is_empty() {
                self.pixels = vec![0; (ATLAS_WIDTH * ATLAS_HEIGHT * 4) as usize];
            }
            let cell = self.next;
            self.next += 1;
            let (x, y) = self.origin(cell);
            for row in 0..CELL {
                let start = (((y + row) * ATLAS_WIDTH + x) * 4) as usize;
                self.pixels[start..start + (CELL * 4) as usize].fill(0);
            }
            for row in 0..PICTURE {
                let start = (((y + GUTTER + row) * ATLAS_WIDTH + x + GUTTER) * 4) as usize;
                let source = (row * PICTURE * 4) as usize;
                self.pixels[start..start + (PICTURE * 4) as usize]
                    .copy_from_slice(&picture.as_raw()[source..source + (PICTURE * 4) as usize]);
            }
            self.placed.insert(id, cell);
            self.dirty.push(Cell {
                x,
                y,
                width: CELL,
                height: CELL,
            });
        }
    }
}

/// Decode queued pictures until `deadline`; whether any are still queued.
pub(crate) fn rasterize_pending(deadline: Instant) -> bool {
    let mut guard = atlas();
    let Some(atlas) = guard.as_mut() else {
        return false;
    };
    if atlas.queue.is_empty() {
        return false;
    }
    atlas.rasterize(deadline);
    !atlas.queue.is_empty()
}

/// Move the cells written since the last call into `cells`.
pub(crate) fn take_dirty(cells: &mut Vec<Cell>) {
    if let Some(atlas) = atlas().as_mut() {
        cells.append(&mut atlas.dirty);
    }
}

/// Forget every placed picture, for a GPU copy made empty.
pub(crate) fn start_over() {
    if let Some(atlas) = atlas().as_mut() {
        atlas.placed.clear();
        atlas.dirty.clear();
        atlas.next = 0;
    }
}

/// Call `read` with the level-0 RGBA (row-major, [`ATLAS_WIDTH`] wide), or nothing
/// before the first picture.
pub(crate) fn mirror<T>(read: impl FnOnce(&[u8]) -> T) -> T {
    let guard = atlas();
    read(guard.as_ref().map_or(&[][..], |atlas| &atlas.pixels))
}

/// `cell`'s RGBA texels at mip `level` from the level-0 `pixels`: each level averages
/// 2x2 texels of the one above, colours weighted by their alpha so transparent texels
/// do not darken the edges.
pub(crate) fn cell_level(pixels: &[u8], cell: Cell, level: u32) -> Vec<u8> {
    let (mut width, mut height) = (cell.width, cell.height);
    let mut data: Vec<u8> = (cell.y..cell.y + cell.height)
        .flat_map(|row| {
            let start = ((row * ATLAS_WIDTH + cell.x) * 4) as usize;
            pixels[start..start + (cell.width * 4) as usize]
                .iter()
                .copied()
        })
        .collect();
    for _ in 0..level {
        let (half_width, half_height) = (width / 2, height / 2);
        let mut half = Vec::with_capacity((half_width * half_height * 4) as usize);
        for row in 0..half_height {
            for column in 0..half_width {
                let mut sum = [0_u32; 4];
                for (dx, dy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                    let at = (((row * 2 + dy) * width + column * 2 + dx) * 4) as usize;
                    let alpha = u32::from(data[at + 3]);
                    for channel in 0..3 {
                        sum[channel] += u32::from(data[at + channel]) * alpha;
                    }
                    sum[3] += alpha;
                }
                for channel in 0..3 {
                    let colour = (sum[channel] + sum[3] / 2).checked_div(sum[3]);
                    half.push(colour.unwrap_or(0) as u8);
                }
                half.push(((sum[3] + 2) / 4) as u8);
            }
        }
        (data, width, height) = (half, half_width, half_height);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(key: &str) -> u16 {
        index().ids[key]
    }

    #[test]
    fn sequences_match_whole_and_fe0f_is_taken_along() {
        assert_eq!(match_at("😀 hi"), Some((id("1f600"), 4)));
        // A thumb with a skin tone is one picture.
        assert_eq!(match_at("👍🏽"), Some((id("1f44d-1f3fd"), 8)));
        // A family joined by zero-width joiners.
        let family = "👨\u{200D}👩\u{200D}👧";
        assert_eq!(
            match_at(family),
            Some((id("1f468-200d-1f469-200d-1f467"), family.len()))
        );
        // A flag from two regional indicators.
        assert_eq!(match_at("🇫🇷!"), Some((id("1f1eb-1f1f7"), 8)));
        // A red heart with its presentation selector, which the glyph takes.
        assert_eq!(match_at("❤\u{FE0F}x"), Some((id("2764"), 6)));
        assert_eq!(match_at("❤x"), Some((id("2764"), 3)));
        // A keycap, and plain text that stays text.
        assert_eq!(match_at("1\u{FE0F}\u{20E3}"), Some((id("31-20e3"), 7)));
        assert_eq!(match_at("1 2"), None);
        assert_eq!(match_at("© 2026"), None);
        assert_eq!(match_at("™"), None);
        assert_eq!(match_at("привет"), None);
        assert_eq!(match_at("日本"), None);
        // A joiner after the last emoji is not taken.
        assert_eq!(match_at("😀\u{200D}"), Some((id("1f600"), 4)));
    }

    #[test]
    fn the_pack_holds_every_picture_it_lists() {
        let index = index();
        assert!(index.ids.len() > 3_900, "{}", index.ids.len());
        let (offset, length) = index.pngs[usize::from(id("1f600"))];
        assert_eq!(&PNGS[offset..offset + 8], b"\x89PNG\r\n\x1a\n");
        assert!(length > 0);
    }

    #[test]
    fn a_picture_is_decoded_into_the_atlas() {
        let mut atlas = Atlas::default();
        let grin = id("1f600");
        atlas.queue.push_back(grin);
        atlas.rasterize(Instant::now() + std::time::Duration::from_secs(5));
        assert!(atlas.placed.contains_key(&grin));
        let cell = atlas.dirty[0];
        let align = 1 << (super::super::ATLAS_MIP_LEVELS - 1);
        assert_eq!(
            [cell.x % align, cell.y % align, cell.width % align],
            [0, 0, 0]
        );
        // Opaque yellow in the face's middle; the gutter stays clear.
        let middle = (((cell.y + CELL / 2) * ATLAS_WIDTH + cell.x + CELL / 2) * 4) as usize;
        assert_eq!(atlas.pixels[middle + 3], 255);
        assert!(atlas.pixels[middle] > 200 && atlas.pixels[middle + 2] < 120);
        assert_eq!(
            atlas.pixels[((cell.y * ATLAS_WIDTH + cell.x) * 4 + 3) as usize],
            0
        );
        let level3 = cell_level(&atlas.pixels, cell, 3);
        assert_eq!(level3.len(), ((CELL / 8) * (CELL / 8) * 4) as usize);
    }
}
