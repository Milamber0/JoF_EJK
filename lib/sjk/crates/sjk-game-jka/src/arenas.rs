//! The arena files (`scripts/*.arena`): which maps a server offers and for which game
//! types, as OpenJK's `codemp/game/g_bot.c` reads them — `G_LoadArenas`,
//! `G_GetMapTypeBits`, `G_DoesMapSupportGametype`, `G_RefreshNextMap` and
//! `G_GetArenaInfoByMap` — and `Cmd_MapList_f` (`g_cmds.c:1965`), which a bare
//! `callvote map` answers with.
//!
//! The reference caps the files at 256 and their text at 8 KiB each; this server reads
//! every file whole, as the rest of its limits are lifted.

use crate::bots::parse_infos;
use sjk_protocol::info_value;

/// `level.arenas`: every arena read, in the order the files were read.
#[derive(Clone, Debug, Default)]
pub struct Arenas {
    infos: Vec<Vec<u8>>,
}

/// `G_GetMapTypeBits`: an arena's `type` keywords as game-type bits (`strstr`, so
/// `powerduel` is also `duel`); no keywords is FFA and Jedi Master.
pub fn map_type_bits(types: &[u8]) -> u32 {
    let has = |word: &[u8]| types.windows(word.len()).any(|window| window == word);
    if types.is_empty() {
        return 1 << 0 | 1 << 2;
    }
    let mut bits = 0;
    if has(b"ffa") {
        bits |= 1 << 0 | 1 << 6 | 1 << 2;
    }
    if has(b"team") {
        bits |= 1 << 6;
    }
    if has(b"holocron") {
        bits |= 1 << 1;
    }
    if has(b"jedimaster") {
        bits |= 1 << 2;
    }
    if has(b"duel") || has(b"powerduel") {
        bits |= 1 << 3 | 1 << 4;
    }
    if has(b"siege") {
        bits |= 1 << 7;
    }
    if has(b"ctf") {
        bits |= 1 << 8 | 1 << 9;
    }
    if has(b"cty") {
        bits |= 1 << 9;
    }
    bits
}

impl Arenas {
    /// `G_LoadArenasFromFile` for each file, in order (`None`: "file not found").
    pub fn load<'a>(
        files: impl IntoIterator<Item = (&'a str, Option<&'a [u8]>)>,
        print: &mut dyn FnMut(&[u8]),
    ) -> Self {
        let mut infos = Vec::new();
        for (name, text) in files {
            match text {
                Some(text) => infos.extend(parse_infos(text, usize::MAX, print)),
                None => print(format!("^1file not found: {name}\n").as_bytes()),
            }
        }
        Self { infos }
    }

    /// How many arenas were read.
    pub fn len(&self) -> usize {
        self.infos.len()
    }

    /// Whether none were.
    pub fn is_empty(&self) -> bool {
        self.infos.is_empty()
    }

    fn value<'a>(&'a self, index: usize, key: &[u8]) -> &'a [u8] {
        info_value(&self.infos[index], key).unwrap_or_default()
    }

    /// The first arena whose `map` is `map` (`Q_stricmp`).
    fn find(&self, map: &[u8]) -> Option<usize> {
        (0..self.infos.len()).find(|&index| self.value(index, b"map").eq_ignore_ascii_case(map))
    }

    /// `G_GetArenaInfoByMap`: the arena for `map`, as its info string.
    pub fn info_for(&self, map: &[u8]) -> Option<&[u8]> {
        self.find(map).map(|index| self.infos[index].as_slice())
    }

    /// `G_DoesMapSupportGametype`.
    pub fn supports(&self, map: &[u8], gametype: i32) -> bool {
        if map.is_empty() {
            return false;
        }
        self.find(map)
            .is_some_and(|index| map_type_bits(self.value(index, b"type")) & (1 << gametype) != 0)
    }

    /// `G_RefreshNextMap(gametype, forced)`: the next arena after `mapname` (the first
    /// arena when `mapname` has none) that allows `gametype`, going round the list once.
    /// Returns what `nextmap` becomes — `map <name>`, or `map_restart 0` when no other
    /// map will do — and the map itself (this one again in the latter case). `None` when
    /// nothing is read, or when it is not forced and `g_autoMapCycle` is off.
    pub fn refresh_next_map(
        &self,
        mapname: &[u8],
        gametype: i32,
        forced: bool,
        auto_map_cycle: bool,
    ) -> Option<(Vec<u8>, Vec<u8>)> {
        if (!auto_map_cycle && !forced) || self.infos.is_empty() {
            return None;
        }
        let this = self.find(mapname).unwrap_or(0);
        let mut desired = this;
        let mut n = this + 1;
        let mut looping = false;
        while n != this {
            if n >= self.infos.len() {
                if looping {
                    break;
                }
                n = 0;
                looping = true;
            }
            if map_type_bits(self.value(n, b"type")) & (1 << gametype) != 0 {
                desired = n;
                break;
            }
            n += 1;
        }
        let map = self.value(desired, b"map").to_vec();
        let nextmap = if desired == this {
            b"map_restart 0".to_vec()
        } else {
            [b"map ".as_slice(), &map].concat()
        };
        Some((nextmap, map))
    }

    /// `Cmd_MapList_f`'s prints: "Map list:" and every map that allows `gametype`, green
    /// and yellow in turn (colours stripped from the names, 23 bytes at most), in prints
    /// of under 512 bytes.
    pub fn map_list(&self, gametype: i32) -> Vec<Vec<u8>> {
        let mut prints = Vec::new();
        let mut buffer = b"Map list:".to_vec();
        let mut toggle = 0;
        for index in 0..self.infos.len() {
            // `Q_strncpyz` into 24 bytes, then `Q_StripColor`.
            let value = self.value(index, b"map");
            let map = strip_colors(&value[..value.len().min(23)]);
            if !self.supports(&map, gametype) {
                continue;
            }
            toggle += 1;
            let colour = if toggle & 1 != 0 { b'2' } else { b'3' };
            let entry = [b" ^".as_slice(), &[colour], &map].concat();
            if buffer.len() + entry.len() >= 512 {
                prints.push([b"print \"".as_slice(), &buffer, b"\""].concat());
                buffer.clear();
            }
            buffer.extend_from_slice(&entry);
        }
        prints.push([b"print \"".as_slice(), &buffer, b"\n\""].concat());
        prints
    }
}

/// `Q_StripColor`: `^` and the digit after it removed, pass after pass until none is left.
fn strip_colors(text: &[u8]) -> Vec<u8> {
    let mut text = text.to_vec();
    loop {
        let mut out = Vec::with_capacity(text.len());
        let mut at = 0;
        while at < text.len() {
            if text[at] == b'^' && text.get(at + 1).is_some_and(u8::is_ascii_digit) {
                at += 2;
            } else {
                out.push(text[at]);
                at += 1;
            }
        }
        if out.len() == text.len() {
            return out;
        }
        text = out;
    }
}
