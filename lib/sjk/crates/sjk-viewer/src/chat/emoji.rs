//! JoF EternalJK's chat emojis (`cg_chatBoxEmojis`): pictures in `gfx/emoji/*.png`
//! drawn in place of their names in chat messages.
//!
//! Names follow `CG_LoadEmojis` (JoF EternalJK `codemp/cgame/cg_main.c:2767-2827` at
//! bd5e202): the file name without `.png`, a `!` making the next letter upper case
//! (for archives built with lower-case names), and a backtick and `~` standing for
//! `:` and `>`, which file names cannot hold, so `` `poop`.png `` is `:poop:` and
//! `#~`!D.png` is `#>:D`. A file whose name is longer than 26 characters is skipped
//! with a warning, and at most 256 load (`MAX_LOADABLE_EMOJIS`). A message replaces
//! each name it holds, outside colour codes, with its picture, the first emoji in
//! the list that matches winning, up to 32 per message (`MAX_CHATBOX_ITEM_EMOJIS`;
//! `CG_ChatBox_AddString`, `cg_draw.c:10078-10204`), when the cvar is on as it arrives;
//! the pictures show while it stays on (`cg_draw.c:10288-10303`).
//!
//! Where this differs from JoF EternalJK:
//! - EternalJK reads the folder into a 2048-byte list (`cg_main.c:2770-2773`), which
//!   holds about 150 of JoF's 175 names, depending on the archive's order; the rest
//!   never load. Here all of them do.
//! - Its loader turns the cvar off and loads nothing when the cvar is on as a map
//!   loads and the folder has files (`if (!fileCnt < 1 && ...)`, a slip for
//!   `fileCnt < 1`). Here the list always loads and the cvar alone decides.
//! - It matches the whole chat line, sender's name included; here the name is drawn
//!   on its own line as text and only the message is matched.
//! - A name left empty (a file called `!.png`) ends its matching for every emoji
//!   listed after it (`cg_draw.c:10125-10127`); here that one file is skipped.
//! - A picture that does not load stops the drawing of it and every later picture
//!   of its message (`cg_draw.c:10294-10295`); here only its own place is blank.

use crate::decoded_image_cache::cached_decoded_image;
use crate::ui_renderer::{EMOJI_ICON_CELLS, EMOJI_ICON_FIRST, ICON_SIZE};
use sjk_shader::ShaderCatalog;
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;

/// The folder emoji pictures are read from (`CG_LoadEmojis`).
pub(crate) const FOLDER: &str = "gfx/emoji";
/// The cvar switching emojis on (EternalJK default 0, archived).
pub(crate) const CVAR: &str = "cg_chatBoxEmojis";
/// `MAX_LOADABLE_EMOJIS`.
const MAX_LOADABLE: usize = 256;
/// `MAX_EMOJI_LENGTH`: a file name (with `.png`) may be 4 characters longer, less 2.
const MAX_NAME: usize = 24;
/// `MAX_CHATBOX_ITEM_EMOJIS`.
pub(super) const MAX_PER_MESSAGE: usize = 32;
/// Stands for a message's `n`th emoji in its stored body: a private-use character,
/// which chat text decoded from the wire never holds.
const MARK_FIRST: u32 = 0xE000;
/// Pixel edge of one emoji: a quarter of an atlas cell, the size of JoF's pictures.
const PICTURE: u32 = ICON_SIZE / 2;

const _: () = assert!(MAX_LOADABLE as u32 <= EMOJI_ICON_CELLS * 4);

/// Edge of an emoji drawn in text of `size`: EternalJK draws 17 units for rows 20
/// apart (`CG_ChatBox_DrawStrings`, `cg_draw.c:10297-10301`), and rows here are one
/// text size apart.
pub(super) fn side(size: f32) -> f32 {
    size * 17.0 / 20.0
}

/// Room an emoji takes in a row of text of `size`: its edge and a gap of the
/// spaces EternalJK puts in place of the name.
pub(super) fn advance(size: f32) -> f32 {
    side(size) + size * 0.15
}

/// The `index`th emoji mark of a message.
fn mark(index: usize) -> char {
    char::from_u32(MARK_FIRST + index as u32).expect("private-use mark")
}

/// Which of a message's emojis `character` stands for, if it is a mark.
pub(super) fn mark_index(character: char) -> Option<usize> {
    let index = (character as u32).checked_sub(MARK_FIRST)? as usize;
    (index < MAX_PER_MESSAGE).then_some(index)
}

/// The emoji name `CG_LoadEmojis` makes of file `file` (`` `poop`.png ``), or
/// `None` when the name is too long to load.
pub(crate) fn name_from_file(file: &str) -> Option<String> {
    if file.len() + 2 > MAX_NAME + 4 {
        return None;
    }
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    let mut name = String::with_capacity(stem.len());
    let mut upper = false;
    for character in stem.chars() {
        let character = if upper {
            character.to_ascii_uppercase()
        } else {
            character
        };
        upper = character == '!';
        match character {
            '`' => name.push(':'),
            '~' => name.push('>'),
            '!' => {}
            other => name.push(other),
        }
    }
    Some(name)
}

/// The names of the emoji files `vfs` holds, in the order the game lists them, and
/// the files skipped for their length.
pub(crate) fn names(vfs: &VirtualFileSystem) -> (Vec<(String, String)>, Vec<String>) {
    let mut loaded = Vec::new();
    let mut too_long = Vec::new();
    for listed in vfs.list_files(FOLDER, ".png") {
        let path = format!("{FOLDER}/{listed}");
        // The name as stored, which `list_files` gives in lower case.
        let file = vfs
            .original_name(&path)
            .and_then(|stored| stored.rsplit('/').next().map(str::to_owned))
            .unwrap_or(listed);
        let Some(name) = name_from_file(&file) else {
            too_long.push(file);
            continue;
        };
        if name.is_empty() {
            continue;
        }
        if loaded.len() == MAX_LOADABLE {
            break;
        }
        loaded.push((name, path));
    }
    (loaded, too_long)
}

/// `CG_ListEmojis_f` (`cg_consolecmds.c:898-908`): one line of the loaded names in
/// yellow, each followed by a green comma, ending with the count; nothing when
/// there are none.
pub(crate) fn list_lines(vfs: &VirtualFileSystem) -> Vec<String> {
    let (loaded, _) = names(vfs);
    if loaded.is_empty() {
        return Vec::new();
    }
    let mut line = String::new();
    for (name, _) in &loaded {
        line.push_str("^3");
        line.push_str(name);
        line.push_str("^2, ");
    }
    line.push_str(&format!("({}) emojis", loaded.len()));
    vec![line]
}

/// Place `pictures` four to an atlas cell, one in each quarter, uploading each cell
/// once; returns each picture's cell and corner coordinates (top left, top right,
/// bottom right, bottom left), inset half a pixel so filtering never reaches the
/// neighbouring quarter. A missing picture keeps no place.
fn pack(
    pictures: Vec<Option<std::sync::Arc<image::RgbaImage>>>,
    mut upload: impl FnMut(TextureId, &[u8]),
) -> Vec<Option<(TextureId, [[f32; 2]; 4])>> {
    let mut cell = vec![0_u8; (ICON_SIZE * ICON_SIZE * 4) as usize];
    let mut slot = 0_u32;
    let mut placed = Vec::with_capacity(pictures.len());
    for picture in pictures {
        let Some(picture) = picture else {
            placed.push(None);
            continue;
        };
        let resized;
        let pixels = if picture.dimensions() == (PICTURE, PICTURE) {
            &*picture
        } else {
            resized = image::imageops::resize(
                &*picture,
                PICTURE,
                PICTURE,
                image::imageops::FilterType::Triangle,
            );
            &resized
        };
        let quarter = slot % 4;
        let (left, top) = ((quarter % 2) * PICTURE, (quarter / 2) * PICTURE);
        for (row, line) in pixels
            .as_raw()
            .chunks_exact((PICTURE * 4) as usize)
            .enumerate()
        {
            let start = (((top + row as u32) * ICON_SIZE + left) * 4) as usize;
            cell[start..start + line.len()].copy_from_slice(line);
        }
        let texture = TextureId(EMOJI_ICON_FIRST + slot / 4);
        let edge = |at: f32| at / ICON_SIZE as f32;
        let (low, high) = (
            [edge(left as f32 + 0.5), edge(top as f32 + 0.5)],
            [
                edge((left + PICTURE) as f32 - 0.5),
                edge((top + PICTURE) as f32 - 0.5),
            ],
        );
        placed.push(Some((
            texture,
            [low, [high[0], low[1]], high, [low[0], high[1]]],
        )));
        slot += 1;
        if quarter == 3 {
            upload(texture, &cell);
            cell.fill(0);
        }
    }
    if !slot.is_multiple_of(4) {
        upload(TextureId(EMOJI_ICON_FIRST + slot / 4), &cell);
    }
    placed
}

struct Emoji {
    name: String,
    /// The atlas picture; none when the file did not decode, which leaves a blank.
    picture: Option<(TextureId, [[f32; 2]; 4])>,
}

/// The emojis of the installed game data, uploaded into the UI atlas.
#[derive(Default)]
pub(crate) struct Emojis {
    entries: Vec<Emoji>,
}

impl Emojis {
    /// Load every emoji picture once per installed world. A name whose picture
    /// does not decode still matches and leaves a blank.
    pub(crate) fn load(
        vfs: &VirtualFileSystem,
        shaders: &ShaderCatalog,
        upload: impl FnMut(TextureId, &[u8]),
    ) -> Self {
        let (loaded, too_long) = names(vfs);
        for file in &too_long {
            crate::log::progress(format_args!(
                "Emoji [{file}] filename exceeded max length of {MAX_NAME}"
            ));
        }
        let mut absent = Vec::new();
        let pictures = loaded.iter().map(|(name, path)| {
            let picture = shaders
                .resolve_image(vfs, path)
                .ok()
                .flatten()
                .and_then(|image| cached_decoded_image(vfs, image.as_str()).ok().flatten());
            if picture.is_none() {
                absent.push(name.clone());
            }
            picture
        });
        let placed = pack(pictures.collect(), upload);
        if !absent.is_empty() {
            crate::log::progress(format_args!(
                "Emoji pictures that did not load: {}",
                absent.join(", ")
            ));
        }
        Self {
            entries: loaded
                .into_iter()
                .zip(placed)
                .map(|((name, _), picture)| Emoji { name, picture })
                .collect(),
        }
    }

    /// `body` with each emoji name outside colour codes replaced by its mark, and
    /// the emojis the marks stand for, in order (`CG_ChatBox_AddString`).
    pub(super) fn markup(&self, body: &str) -> (String, Vec<u16>) {
        let mut found = Vec::new();
        if self.entries.is_empty() {
            return (body.to_owned(), found);
        }
        let mut out = String::with_capacity(body.len());
        let mut rest = body;
        while let Some(character) = rest.chars().next() {
            let bytes = rest.as_bytes();
            if bytes[0] == b'^' && bytes.get(1).is_some_and(u8::is_ascii_digit) {
                out.push_str(&rest[..2]);
                rest = &rest[2..];
                continue;
            }
            if found.len() < MAX_PER_MESSAGE
                && let Some(index) = self
                    .entries
                    .iter()
                    .position(|emoji| rest.starts_with(emoji.name.as_str()))
            {
                out.push(mark(found.len()));
                rest = &rest[self.entries[index].name.len()..];
                found.push(index as u16);
                continue;
            }
            out.push(character);
            rest = &rest[character.len_utf8()..];
        }
        (out, found)
    }

    /// The atlas picture of emoji `index`.
    pub(super) fn picture(&self, index: u16) -> Option<(TextureId, [[f32; 2]; 4])> {
        self.entries.get(usize::from(index))?.picture
    }

    #[cfg(test)]
    pub(super) fn from_names(names: &[&str]) -> Self {
        Self {
            entries: names
                .iter()
                .map(|name| Emoji {
                    name: (*name).to_owned(),
                    picture: Some((TextureId(EMOJI_ICON_FIRST), [[0.0; 2]; 4])),
                })
                .collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn file_names_become_emoji_names_as_eternaljk_makes_them() {
        assert_eq!(name_from_file("`poop`.png").as_deref(), Some(":poop:"));
        assert_eq!(name_from_file("#~`!d.png").as_deref(), Some("#>:D"));
        assert_eq!(name_from_file("#`(.png").as_deref(), Some("#:("));
        assert_eq!(name_from_file("#^_^.png").as_deref(), Some("#^_^"));
        // 26 characters load, 27 do not.
        assert!(name_from_file(&format!("{}.png", "a".repeat(22))).is_some());
        assert!(name_from_file(&format!("{}.png", "a".repeat(23))).is_none());
    }

    #[test]
    fn names_are_replaced_outside_colour_codes() {
        let emojis = Emojis::from_names(&[":poop:", ":fire:"]);
        let (body, found) = emojis.markup("^2hi :fire: and :poop::poop:");
        assert_eq!(body, format!("^2hi {} and {}{}", mark(0), mark(1), mark(2)));
        assert_eq!(found, [1, 0, 0]);
        // A colour code is skipped whole, never matched into a name.
        let emojis = Emojis::from_names(&["^1x"]);
        assert_eq!(emojis.markup("^1x").0, "^1x");
    }

    #[test]
    fn the_first_listed_name_wins_and_at_most_32_are_replaced() {
        let emojis = Emojis::from_names(&[":p", ":poop:"]);
        let (body, found) = emojis.markup(":poop:");
        assert_eq!(body, format!("{}oop:", mark(0)));
        assert_eq!(found, [0]);
        let emojis = Emojis::from_names(&[":x:"]);
        let (body, found) = emojis.markup(&":x:".repeat(33));
        assert_eq!(found.len(), MAX_PER_MESSAGE);
        assert!(body.ends_with(":x:"));
    }

    #[test]
    fn marks_round_trip_and_ordinary_text_is_untouched() {
        assert_eq!(mark_index(mark(5)), Some(5));
        assert_eq!(mark_index('a'), None);
        let emojis = Emojis::from_names(&[":x:"]);
        assert_eq!(emojis.markup("Æ ok").0, "Æ ok");
        assert_eq!(Emojis::default().markup(":x:").0, ":x:");
    }

    #[test]
    fn listing_names_every_loaded_emoji_and_the_count() {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "emoji",
            [
                ("gfx/emoji/`poop`.png", Vec::new()),
                ("gfx/emoji/#~`!d.png", Vec::new()),
                ("gfx/emoji/this_name_is_far_too_long.png", Vec::new()),
                ("gfx/emoji/readme.txt", Vec::new()),
                ("gfx/emoji/!.png", Vec::new()),
            ],
        )
        .expect("mount");
        let lines = list_lines(&vfs);
        // The folder's order is the archive's; the names and count are what matter.
        assert_eq!(lines.len(), 1);
        assert!(lines[0].contains("^3:poop:^2, ") && lines[0].contains("^3#>:D^2, "));
        assert!(lines[0].ends_with("^2, (2) emojis"));
        assert!(list_lines(&VirtualFileSystem::new()).is_empty());
    }

    #[test]
    fn pictures_are_packed_four_to_a_cell() {
        let picture = |shade: u8| {
            Some(std::sync::Arc::new(image::RgbaImage::from_pixel(
                PICTURE,
                PICTURE,
                image::Rgba([shade, 0, 0, 255]),
            )))
        };
        let mut uploads = Vec::new();
        let placed = pack(
            vec![
                picture(1),
                None,
                picture(2),
                picture(3),
                picture(4),
                picture(5),
            ],
            |texture, rgba| uploads.push((texture, rgba.to_vec())),
        );
        // Five pictures: one full cell and one with a single quarter.
        assert_eq!(uploads.len(), 2);
        assert_eq!(uploads[0].0, TextureId(EMOJI_ICON_FIRST));
        assert_eq!(uploads[1].0, TextureId(EMOJI_ICON_FIRST + 1));
        assert!(placed[1].is_none());
        let pixel = |cell: &[u8], x: u32, y: u32| cell[((y * ICON_SIZE + x) * 4) as usize];
        // Quarters in reading order: top left, top right, bottom left, bottom right.
        assert_eq!(pixel(&uploads[0].1, 0, 0), 1);
        assert_eq!(pixel(&uploads[0].1, PICTURE, 0), 2);
        assert_eq!(pixel(&uploads[0].1, 0, PICTURE), 3);
        assert_eq!(pixel(&uploads[0].1, PICTURE, PICTURE), 4);
        // The last cell was cleared before its one picture.
        assert_eq!(pixel(&uploads[1].1, 0, 0), 5);
        assert_eq!(pixel(&uploads[1].1, PICTURE, 0), 0);
        // The top right quarter's corners, inset half a pixel.
        let (texture, uv) = placed[2].unwrap();
        assert_eq!(texture, TextureId(EMOJI_ICON_FIRST));
        assert_eq!(uv[0], [64.5 / 128.0, 0.5 / 128.0]);
        assert_eq!(uv[2], [127.5 / 128.0, 63.5 / 128.0]);
    }

    #[test]
    fn a_name_without_a_picture_still_matches() {
        let emojis = Emojis {
            entries: vec![Emoji {
                name: ":x:".to_owned(),
                picture: None,
            }],
        };
        let (body, found) = emojis.markup("a:x:");
        assert_eq!(body, format!("a{}", mark(0)));
        assert_eq!(emojis.picture(found[0]), None);
    }

    #[test]
    fn clean_chat_keeps_messages_that_differ_only_in_their_emojis() {
        use crate::chat::ChatOverlay;
        use sjk_client::ServerEventKind;
        let mut chat = ChatOverlay::with_emojis(Emojis::from_names(&[":poop:", ":fire:"]));
        chat.options.clean = 1;
        chat.options.emojis = true;
        let now = std::time::Instant::now();
        for text in ["gg :poop:", "gg :fire:", "gg :fire:"] {
            chat.receive(ServerEventKind::Chat, text.to_owned(), None, now);
        }
        // Both marks are the message's first, so only the emojis tell them apart;
        // the repeated last message is still dropped.
        assert_eq!(chat.lines.len(), 2);
        assert_eq!(chat.lines[0].emojis, [0]);
        assert_eq!(chat.lines[1].emojis, [1]);
    }
}
