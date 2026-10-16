//! Glyphs beyond the 256 code-page slots: Cyrillic, Japanese and any other
//! character a bundled font draws.
//!
//! Every UI font atlas holds the 256 characters of Windows-1252, one per byte, as
//! JKA's own fonts do, rasterized once. Text decoded from Windows-1251 or sent as
//! UTF-8 by another SJK player holds characters outside them (`привет`, `日本語`),
//! which used to draw as `?`. Those go through one shared "wide" atlas instead,
//! filled the first time each character is asked for:
//!
//! - A font face names the fonts that lend it wide glyphs, in order ([`Source`]):
//!   its own family when that has Cyrillic (Inter, Exo 2, JetBrains Mono), else the
//!   closest that does, then Noto Sans JP for kana and kanji, Noto Sans SC for the
//!   hanzi it lacks, Noto Sans KR for Hangul, then SJK Symbols for SJK's own symbols
//!   ([`crate::symbols`]). The first that has the character draws it; a character none
//!   has draws the face's `?`.
//! - Looking a character up ([`lookup`]) costs a hash lookup under a lock. The first
//!   time, it reads the advance from the font (a table read, no outline), so layout is
//!   right from the first frame, and queues the glyph. [`rasterize_pending`] outlines
//!   queued glyphs within a time budget once per frame and packs them into the atlas;
//!   until then the glyph has its advance but no ink, for about a frame.
//! - The atlas is one R8 texture, [`ATLAS_WIDTH`] by [`ATLAS_HEIGHT`] with
//!   [`super::ATLAS_MIP_LEVELS`] levels, kept on the CPU too ([`mirror`]) so menu
//!   snapshots draw wide glyphs and a new graphics context starts from it. Glyph cells
//!   sit on an 8-texel grid so each mip level's copy of a cell is whole texels, with
//!   the same gutter as the code-page atlases. When it is full it starts over: the
//!   glyphs on screen are asked for again and come back over the next frames.
//! - A text vertex samples the wide atlas when its `u` is [`U_OFFSET`] or more
//!   (`text.wgsl`), so every text batch keeps its own atlas and draw order and can
//!   still hold wide glyphs.
//!
//! Glyphs are outlined with `ab_glyph`, which reads outlines on demand: `fontdue`,
//! which the code-page atlases use, outlines every glyph of a font when it loads it,
//! seconds for Noto Sans JP's 16,000 glyphs.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont, point};
use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Raster em size of a wide glyph, in pixels: Inter's own atlas is rasterized at
/// 96 pixels per em at 100% scaling, so wide glyphs are as sharp as the text around them.
pub(crate) const RASTER_EM: f32 = 96.0;
/// The wide atlas's size in texels.
pub(crate) const ATLAS_WIDTH: u32 = 4_096;
/// The wide atlas's height in texels: room for about 600 kanji, or 1,500 Cyrillic letters.
pub(crate) const ATLAS_HEIGHT: u32 = 2_048;
/// Cell grid: a cell's copy in mip level 3 is whole texels.
const ALIGN: u32 = 1 << (super::ATLAS_MIP_LEVELS - 1);
/// Gutter around a glyph inside its cell, as the code-page atlases keep between glyphs.
const GUTTER: u32 = super::ATLAS_PADDING / 2;
/// Added to a wide glyph's `u` coordinates so `text.wgsl` samples the wide atlas.
pub(crate) const U_OFFSET: f32 = 2.0;
/// Most time [`rasterize_pending`] spends in one frame.
pub(crate) const FRAME_BUDGET: Duration = Duration::from_millis(2);

const INTER_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Inter-Regular.ttf");
const INTER_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Inter-SemiBold.ttf");
const EXO2_REGULAR: &[u8] = include_bytes!("../../assets/fonts/Exo2-Regular.ttf");
const EXO2_SEMIBOLD: &[u8] = include_bytes!("../../assets/fonts/Exo2-SemiBold.ttf");
const JETBRAINS_MONO: &[u8] = include_bytes!("../../assets/fonts/JetBrainsMono-Regular.ttf");
/// Noto Sans JP Regular, subset to kana, kanji, CJK punctuation and full-width forms.
const NOTO_SANS_JP: &[u8] = include_bytes!("../../assets/fonts/NotoSansJP-Regular.otf");
/// Noto Sans SC Regular, subset to the 2,215 GB 2312 hanzi Noto Sans JP lacks.
const NOTO_SANS_SC: &[u8] = include_bytes!("../../assets/fonts/NotoSansSC-Regular.otf");
/// Noto Sans KR Regular, subset to Hangul (syllables and jamo).
const NOTO_SANS_KR: &[u8] = include_bytes!("../../assets/fonts/NotoSansKR-Regular.otf");
/// SJK Symbols, SJK's own symbol font (`scripts/sjk_symbols.py`).
const SJK_SYMBOLS: &[u8] = include_bytes!("../../assets/fonts/SJKSymbols.ttf");

/// A font that lends wide glyphs.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum Source {
    InterRegular,
    InterSemibold,
    Exo2Regular,
    Exo2Semibold,
    JetBrainsMono,
    /// Kana and kanji (and the Chinese characters they share).
    NotoSansJp,
    /// The Simplified Chinese characters Noto Sans JP lacks.
    NotoSansSc,
    /// Hangul.
    NotoSansKr,
    /// SJK's own symbols, in the Private Use Area ([`crate::symbols`]).
    SjkSymbols,
}

impl Source {
    const ALL: [Self; 9] = [
        Self::InterRegular,
        Self::InterSemibold,
        Self::Exo2Regular,
        Self::Exo2Semibold,
        Self::JetBrainsMono,
        Self::NotoSansJp,
        Self::NotoSansSc,
        Self::NotoSansKr,
        Self::SjkSymbols,
    ];

    const fn index(self) -> usize {
        self as usize
    }

    const fn bytes(self) -> &'static [u8] {
        match self {
            Self::InterRegular => INTER_REGULAR,
            Self::InterSemibold => INTER_SEMIBOLD,
            Self::Exo2Regular => EXO2_REGULAR,
            Self::Exo2Semibold => EXO2_SEMIBOLD,
            Self::JetBrainsMono => JETBRAINS_MONO,
            Self::NotoSansJp => NOTO_SANS_JP,
            Self::NotoSansSc => NOTO_SANS_SC,
            Self::NotoSansKr => NOTO_SANS_KR,
            Self::SjkSymbols => SJK_SYMBOLS,
        }
    }

    /// Capital height as a fraction of the em (each font's OS/2 `sCapHeight` over its
    /// `unitsPerEm`). A wide glyph is scaled so this matches the capitals of the font it
    /// stands in for: Cyrillic letters then match the Latin ones beside them, and kanji
    /// take the size their font gives them next to its own Latin capitals.
    pub(crate) const fn cap_ratio(self) -> f32 {
        match self {
            Self::InterRegular | Self::InterSemibold => 1_490.0 / 2_048.0,
            Self::Exo2Regular | Self::Exo2Semibold => 0.69,
            Self::JetBrainsMono => 0.73,
            Self::NotoSansJp | Self::NotoSansSc | Self::NotoSansKr => 0.733,
            // Its capitals' height, which the symbols are drawn to.
            Self::SjkSymbols => 0.7,
        }
    }
}

/// Inter's faces, then kana and kanji, the hanzi Japanese lacks, Hangul and SJK's
/// symbols.
pub(crate) const INTER: [&[Source]; 2] = [
    &[
        Source::InterRegular,
        Source::NotoSansJp,
        Source::NotoSansSc,
        Source::NotoSansKr,
        Source::SjkSymbols,
    ],
    &[
        Source::InterSemibold,
        Source::NotoSansJp,
        Source::NotoSansSc,
        Source::NotoSansKr,
        Source::SjkSymbols,
    ],
];
/// Exo 2's faces, then the CJK fonts; also Rajdhani's, which has no Cyrillic and
/// borrows Exo 2 for what it lacks already.
pub(crate) const EXO2: [&[Source]; 2] = [
    &[
        Source::Exo2Regular,
        Source::NotoSansJp,
        Source::NotoSansSc,
        Source::NotoSansKr,
        Source::SjkSymbols,
    ],
    &[
        Source::Exo2Semibold,
        Source::NotoSansJp,
        Source::NotoSansSc,
        Source::NotoSansKr,
        Source::SjkSymbols,
    ],
];
/// JetBrains Mono, then the CJK fonts: the console's, and SJK Chat's (OCR-A, which
/// has no Cyrillic; a monospace face stands in best).
pub(crate) const MONO: &[Source] = &[
    Source::JetBrainsMono,
    Source::NotoSansJp,
    Source::NotoSansSc,
    Source::NotoSansKr,
    Source::SjkSymbols,
];
/// For a face with no wide glyphs (test fonts): every wide character draws `?`.
#[cfg(test)]
pub(crate) const NONE: [&[Source]; 2] = [&[], &[]];

/// A wide glyph in raster pixels ([`RASTER_EM`] per em of its [`Source`]).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct WideGlyph {
    /// The font it is drawn from, whose [`Source::cap_ratio`] scales it.
    pub(crate) source: Option<Source>,
    /// Pen advance.
    pub(crate) advance: f32,
    /// Ink left edge from the pen.
    pub(crate) left: f32,
    /// Ink top edge above the baseline.
    pub(crate) top: f32,
    /// Ink width; 0 until it is rasterized.
    pub(crate) width: f32,
    /// Ink height; 0 until it is rasterized.
    pub(crate) height: f32,
    /// Rectangle `[u0, v0, u1, v1]` in the wide atlas, `u` offset by [`U_OFFSET`].
    pub(crate) uv: [f32; 4],
}

/// What the atlas knows of one character in one font.
#[derive(Clone, Copy, Debug)]
enum Entry {
    /// The font has no glyph for it.
    Missing,
    /// Queued for [`rasterize_pending`].
    Pending(WideGlyph),
    /// In the atlas.
    Ready(WideGlyph),
}

/// A packed cell in level-0 texels: where the glyph's ink and gutter were written.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct Cell {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Default)]
struct Atlas {
    fonts: [Option<FontRef<'static>>; Source::ALL.len()],
    entries: HashMap<(Source, char), Entry>,
    queue: VecDeque<(Source, char)>,
    /// Next free cell position, and the tallest cell of its row.
    shelf: [u32; 3],
    /// Level-0 coverage, [`ATLAS_WIDTH`] by [`ATLAS_HEIGHT`]; empty until the first glyph.
    pixels: Vec<u8>,
    /// Cells written since the GPU copy last took them ([`take_dirty`]).
    dirty: Vec<Cell>,
    /// Counts the times the atlas started over.
    generation: u64,
}

static ATLAS: Mutex<Option<Atlas>> = Mutex::new(None);

fn atlas() -> MutexGuard<'static, Option<Atlas>> {
    ATLAS
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

impl Atlas {
    fn font(&mut self, source: Source) -> Option<&FontRef<'static>> {
        let slot = &mut self.fonts[source.index()];
        if slot.is_none() {
            *slot = FontRef::try_from_slice(source.bytes()).ok();
        }
        slot.as_ref()
    }

    /// The entry for `character` in `source`, read from the font the first time.
    fn entry(&mut self, source: Source, character: char) -> Entry {
        if let Some(entry) = self.entries.get(&(source, character)) {
            return *entry;
        }
        let entry = match self.font(source) {
            Some(font) if font.glyph_id(character).0 != 0 => {
                let id = font.glyph_id(character);
                Entry::Pending(WideGlyph {
                    source: Some(source),
                    advance: font.as_scaled(scale(font)).h_advance(id),
                    ..WideGlyph::default()
                })
            }
            _ => Entry::Missing,
        };
        if matches!(entry, Entry::Pending(_)) {
            self.queue.push_back((source, character));
        }
        self.entries.insert((source, character), entry);
        entry
    }

    /// Outline and pack the queued glyphs until `deadline`.
    fn rasterize(&mut self, deadline: Instant) {
        while Instant::now() < deadline {
            let Some((source, character)) = self.queue.pop_front() else {
                return;
            };
            let Some(Entry::Pending(glyph)) = self.entries.get(&(source, character)).copied()
            else {
                continue;
            };
            let Some(font) = self.font(source).cloned() else {
                continue;
            };
            let id = font.glyph_id(character);
            let outlined =
                font.outline_glyph(id.with_scale_and_position(scale(&font), point(0.0, 0.0)));
            let Some(outlined) = outlined else {
                // A space: an advance and no ink.
                self.entries
                    .insert((source, character), Entry::Ready(glyph));
                continue;
            };
            let bounds = outlined.px_bounds();
            let (width, height) = (bounds.width() as u32, bounds.height() as u32);
            let Some(cell) = self.place(width, height) else {
                self.start_over();
                return;
            };
            if self.pixels.is_empty() {
                self.pixels = vec![0; (ATLAS_WIDTH * ATLAS_HEIGHT) as usize];
            }
            for row in cell.y..cell.y + cell.height {
                let start = (row * ATLAS_WIDTH + cell.x) as usize;
                self.pixels[start..start + cell.width as usize].fill(0);
            }
            let (ink_x, ink_y) = (cell.x + GUTTER, cell.y + GUTTER);
            let pixels = &mut self.pixels;
            outlined.draw(|x, y, coverage| {
                if x < width && y < height {
                    let at = ((ink_y + y) * ATLAS_WIDTH + ink_x + x) as usize;
                    pixels[at] = (coverage.clamp(0.0, 1.0) * 255.0).round() as u8;
                }
            });
            self.dirty.push(cell);
            let (u, v) = (ATLAS_WIDTH as f32, ATLAS_HEIGHT as f32);
            self.entries.insert(
                (source, character),
                Entry::Ready(WideGlyph {
                    left: bounds.min.x,
                    top: -bounds.min.y,
                    width: width as f32,
                    height: height as f32,
                    uv: [
                        U_OFFSET + ink_x as f32 / u,
                        ink_y as f32 / v,
                        U_OFFSET + (ink_x + width) as f32 / u,
                        (ink_y + height) as f32 / v,
                    ],
                    ..glyph
                }),
            );
        }
    }

    /// A free cell for ink `width` by `height`, on the [`ALIGN`] grid; `None` when the
    /// atlas is full.
    fn place(&mut self, width: u32, height: u32) -> Option<Cell> {
        let round = |size: u32| (size + 2 * GUTTER).div_ceil(ALIGN) * ALIGN;
        let (cell_width, cell_height) = (round(width), round(height));
        let [mut x, mut y, mut row] = self.shelf;
        if x + cell_width > ATLAS_WIDTH {
            (x, y, row) = (0, y + row, 0);
        }
        if cell_width > ATLAS_WIDTH || y + cell_height > ATLAS_HEIGHT {
            return None;
        }
        self.shelf = [x + cell_width, y, row.max(cell_height)];
        Some(Cell {
            x,
            y,
            width: cell_width,
            height: cell_height,
        })
    }

    /// Empty the atlas: every glyph is asked for, and rasterized, again.
    fn start_over(&mut self) {
        self.entries.clear();
        self.queue.clear();
        self.dirty.clear();
        self.shelf = [0; 3];
        self.generation += 1;
    }
}

/// The scale that rasterizes `font` at [`RASTER_EM`] pixels per em (`ab_glyph` scales
/// by line height).
fn scale(font: &FontRef<'_>) -> PxScale {
    let em = font.units_per_em().unwrap_or(1_000.0);
    PxScale::from(RASTER_EM * font.height_unscaled() / em)
}

/// `character` from the first of `sources` that has it, in raster pixels; `None` when
/// none does. The first time, the glyph is queued and comes back with its advance and
/// no ink until [`rasterize_pending`] has drawn it.
pub(crate) fn lookup(sources: &[Source], character: char) -> Option<WideGlyph> {
    if sources.is_empty() {
        return None;
    }
    let mut guard = atlas();
    let atlas = guard.get_or_insert_with(Atlas::default);
    sources
        .iter()
        .find_map(|&source| match atlas.entry(source, character) {
            Entry::Missing => None,
            Entry::Pending(glyph) | Entry::Ready(glyph) => Some(glyph),
        })
}

/// Rasterize queued glyphs for up to `budget`; whether any are still queued.
pub(crate) fn rasterize_pending(budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
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

/// The cells written since the last call, and the atlas's generation: a GPU copy
/// made for another generation holds nothing that is still asked for.
pub(crate) fn take_dirty(cells: &mut Vec<Cell>) -> u64 {
    let mut guard = atlas();
    let Some(atlas) = guard.as_mut() else {
        return 0;
    };
    cells.append(&mut atlas.dirty);
    atlas.generation
}

/// Forget every packed glyph, for a GPU copy made empty: what is on screen is asked
/// for again and drawn into it over the next frames.
pub(crate) fn start_over() {
    if let Some(atlas) = atlas().as_mut() {
        atlas.start_over();
    }
}

/// Call `read` with the level-0 coverage (row-major, [`ATLAS_WIDTH`] wide), or nothing
/// before the first glyph.
pub(crate) fn mirror<T>(read: impl FnOnce(&[u8]) -> T) -> T {
    let guard = atlas();
    read(guard.as_ref().map_or(&[][..], |atlas| &atlas.pixels))
}

/// `cell`'s texels at mip `level` from the level-0 `pixels`: each level averages 2x2
/// texels of the one above, as a GPU mip chain would. The cell is on the [`ALIGN`] grid,
/// so its copy at every level is whole texels.
pub(crate) fn cell_level(pixels: &[u8], cell: Cell, level: u32) -> Vec<u8> {
    let mut width = cell.width;
    let mut data: Vec<u8> = (cell.y..cell.y + cell.height)
        .flat_map(|row| {
            let start = (row * ATLAS_WIDTH + cell.x) as usize;
            pixels[start..start + cell.width as usize].iter().copied()
        })
        .collect();
    let mut height = cell.height;
    for _ in 0..level {
        let (half_width, half_height) = (width / 2, height / 2);
        let mut half = Vec::with_capacity((half_width * half_height) as usize);
        for row in 0..half_height {
            for column in 0..half_width {
                let at = |x: u32, y: u32| u32::from(data[(y * width + x) as usize]);
                let (x, y) = (column * 2, row * 2);
                let sum = at(x, y) + at(x + 1, y) + at(x, y + 1) + at(x + 1, y + 1);
                half.push(((sum + 2) / 4) as u8);
            }
        }
        (data, width, height) = (half, half_width, half_height);
    }
    data
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_ratios_match_each_fonts_capital() {
        for source in [
            Source::InterRegular,
            Source::InterSemibold,
            Source::Exo2Regular,
            Source::Exo2Semibold,
            Source::JetBrainsMono,
        ] {
            let font = FontRef::try_from_slice(source.bytes()).unwrap();
            let em = font.units_per_em().unwrap();
            let outline = font.outline(font.glyph_id('H')).unwrap();
            // `ab_glyph` keeps the top (`y_max`, y up) in `min.y`.
            let cap = outline.bounds.min.y / em;
            assert!((cap - source.cap_ratio()).abs() < 0.01, "{source:?}: {cap}");
        }
    }

    #[test]
    fn each_script_has_a_font_and_emoji_has_none() {
        let has = |sources: &[Source], character: char| {
            let mut atlas = Atlas::default();
            sources
                .iter()
                .find(|&&source| !matches!(atlas.entry(source, character), Entry::Missing))
                .copied()
        };
        assert_eq!(has(INTER[0], 'Ж'), Some(Source::InterRegular));
        assert_eq!(has(EXO2[1], 'ё'), Some(Source::Exo2Semibold));
        assert_eq!(has(MONO, 'Я'), Some(Source::JetBrainsMono));
        for character in ['日', 'あ', 'カ', '。', '漢'] {
            assert_eq!(has(INTER[0], character), Some(Source::NotoSansJp));
        }
        // Simplified hanzi Japanese lacks, and Hangul (syllables and compatibility jamo).
        assert_eq!(has(INTER[0], '们'), Some(Source::NotoSansSc));
        assert_eq!(has(INTER[0], '这'), Some(Source::NotoSansSc));
        assert_eq!(has(MONO, '한'), Some(Source::NotoSansKr));
        assert_eq!(has(EXO2[0], 'ㅋ'), Some(Source::NotoSansKr));
        assert_eq!(has(INTER[0], '😀'), None);
    }

    #[test]
    fn glyphs_pack_on_the_mip_grid_and_start_over_when_full() {
        let mut atlas = Atlas::default();
        assert_eq!(
            atlas.entry(Source::NotoSansJp, '漢').source(),
            Some(Source::NotoSansJp)
        );
        atlas.rasterize(Instant::now() + Duration::from_secs(5));
        let Entry::Ready(glyph) = atlas.entries[&(Source::NotoSansJp, '漢')] else {
            panic!("not rasterized");
        };
        // About an em wide and tall, sitting on the baseline.
        assert!(glyph.width > 70.0 && glyph.width <= RASTER_EM, "{glyph:?}");
        assert!(glyph.top > 60.0 && glyph.top < RASTER_EM, "{glyph:?}");
        assert!(glyph.uv[0] >= U_OFFSET);
        let cell = atlas.dirty[0];
        assert_eq!([cell.x % ALIGN, cell.y % ALIGN], [0, 0]);
        assert_eq!([cell.width % ALIGN, cell.height % ALIGN], [0, 0]);
        let ink: u32 = atlas.pixels.iter().map(|&texel| u32::from(texel)).sum();
        assert!(ink > 0);
        // Fill the atlas: the glyph that does not fit empties it.
        let mut filled = 0;
        while atlas.place(RASTER_EM as u32, RASTER_EM as u32).is_some() {
            filled += 1;
        }
        assert!(filled > 500, "{filled}");
        atlas.entry(Source::NotoSansJp, '字');
        let generation = atlas.generation;
        atlas.rasterize(Instant::now() + Duration::from_secs(5));
        assert_eq!(atlas.generation, generation + 1);
        assert!(atlas.entries.is_empty() && atlas.dirty.is_empty());
    }

    #[test]
    fn mip_levels_average_their_texels() {
        let mut pixels = vec![0; (ATLAS_WIDTH * ATLAS_HEIGHT) as usize];
        let cell = Cell {
            x: 8,
            y: 16,
            width: 16,
            height: 8,
        };
        pixels[(16 * ATLAS_WIDTH + 8) as usize] = 255;
        assert_eq!(cell_level(&pixels, cell, 0).len(), 128);
        let level1 = cell_level(&pixels, cell, 1);
        assert_eq!(level1.len(), 32);
        assert_eq!(level1[0], 64);
        let level3 = cell_level(&pixels, cell, 3);
        assert_eq!(level3, [4, 0]);
    }

    impl Entry {
        fn source(self) -> Option<Source> {
            match self {
                Self::Missing => None,
                Self::Pending(glyph) | Self::Ready(glyph) => glyph.source,
            }
        }
    }
}
