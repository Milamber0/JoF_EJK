//! What the Create game screen offers: the game types the native server
//! runs, the installed maps each can be played on (from the arena files, as
//! the stock screen lists them) and the bot definitions to fill a match with.

use sjk_game_jka::bots::BOTS_FILE;
use sjk_protocol::info_value;

/// Which score the mode's limit row edits.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ScoreLimit {
    /// `fraglimit`.
    Frags,
    /// `capturelimit`.
    Captures,
    /// None (siege ends on its objectives).
    None,
}

/// One game type the native server supports.
#[derive(Clone, Copy, Debug)]
pub(crate) struct GameMode {
    /// Name on the screen.
    pub(crate) label: &'static str,
    /// `sjk-server --gametype` value.
    pub(crate) server: &'static str,
    /// Keyword in an arena's `type` list.
    pub(crate) arena: &'static str,
    /// The limit the mode's score row sets.
    pub(crate) score: ScoreLimit,
}

/// Every mode, in the stock Create game order.
pub(crate) const MODES: [GameMode; 9] = [
    GameMode {
        label: "Free for all",
        server: "ffa",
        arena: "ffa",
        score: ScoreLimit::Frags,
    },
    GameMode {
        label: "Holocron FFA",
        server: "holocron",
        arena: "holocron",
        score: ScoreLimit::Frags,
    },
    GameMode {
        label: "Jedi Master",
        server: "jm",
        arena: "jedimaster",
        score: ScoreLimit::Frags,
    },
    GameMode {
        label: "Duel",
        server: "duel",
        arena: "duel",
        score: ScoreLimit::Frags,
    },
    GameMode {
        label: "Power duel",
        server: "powerduel",
        arena: "powerduel",
        score: ScoreLimit::Frags,
    },
    GameMode {
        label: "Team FFA",
        server: "team",
        arena: "team",
        score: ScoreLimit::Frags,
    },
    GameMode {
        label: "Siege",
        server: "siege",
        arena: "siege",
        score: ScoreLimit::None,
    },
    GameMode {
        label: "Capture the flag",
        server: "ctf",
        arena: "ctf",
        score: ScoreLimit::Captures,
    },
    GameMode {
        label: "Capture the ysalamiri",
        server: "cty",
        arena: "cty",
        score: ScoreLimit::Captures,
    },
];

/// Index of the mode whose server name is `server`, if any.
pub(crate) fn mode_index(server: &str) -> Option<usize> {
    MODES
        .iter()
        .position(|mode| mode.server.eq_ignore_ascii_case(server))
}

/// One installed map.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct MapEntry {
    /// `mp/ffa3`, as `--map` takes it.
    pub(crate) name: String,
    /// The arena's `longname`, or the name.
    pub(crate) title: String,
    /// Bit `i` set when `MODES[i]` can be played on it.
    modes: u16,
}

impl MapEntry {
    /// Whether mode `mode` is played on this map.
    pub(crate) fn supports(&self, mode: usize) -> bool {
        self.modes & (1 << mode) != 0
    }
}

/// The installed maps and bots.
#[derive(Clone, Debug, Default)]
pub(crate) struct Catalogue {
    maps: Vec<MapEntry>,
    bots: Vec<String>,
}

impl Catalogue {
    /// Read the maps (`maps/*.bsp`, `maps/mp/*.bsp`), arena files (`scripts/arenas.txt`,
    /// `scripts/*.arena`) and bot files (`botfiles/bots.txt`,
    /// `scripts/*.bot`) visible in `vfs`.
    pub(crate) fn from_vfs(vfs: &sjk_vfs::VirtualFileSystem) -> Self {
        let read = |path: &str| {
            vfs.read(path)
                .ok()
                .flatten()
                .map(|asset| asset.bytes.to_vec())
        };
        let bsps: Vec<String> = vfs.list_files("maps", ".bsp");
        let mut arenas: Vec<Vec<u8>> = read("scripts/arenas.txt").into_iter().collect();
        arenas.extend(
            vfs.list_files("scripts", ".arena")
                .iter()
                .filter_map(|name| read(&format!("scripts/{name}"))),
        );
        let mut bots: Vec<Vec<u8>> = read(BOTS_FILE).into_iter().collect();
        bots.extend(
            vfs.list_files("scripts", ".bot")
                .iter()
                .filter_map(|name| read(&format!("scripts/{name}"))),
        );
        Self::from_sources(bsps.iter().map(String::as_str), &arenas, &bots)
    }

    /// Build from the `.bsp` paths under `maps` (`mp/ffa3.bsp`,
    /// `mprp1.bsp`), the arena files' text and the bot files' text. An arena
    /// naming a map that is not installed is skipped; a `maps/mp` map no arena
    /// lists is offered for free for all only, as any map with spawn points
    /// plays it. Maps elsewhere (community maps at `maps/*.bsp`, but also the
    /// single-player levels) are offered only when an arena lists them, as
    /// the stock screen does.
    pub(crate) fn from_sources<'a>(
        bsps: impl Iterator<Item = &'a str>,
        arenas: &[Vec<u8>],
        bots: &[Vec<u8>],
    ) -> Self {
        let mut maps: Vec<MapEntry> = bsps
            .filter_map(|file| {
                let stem = file
                    .strip_suffix(".bsp")
                    .or_else(|| file.strip_suffix(".BSP"))?;
                let name = stem.to_ascii_lowercase();
                Some(MapEntry {
                    title: name.clone(),
                    name,
                    modes: 0,
                })
            })
            .collect();
        let quiet = &mut |_: &[u8]| {};
        for info in arenas
            .iter()
            .flat_map(|text| sjk_game_jka::bots::parse_infos(text, 1_024, quiet))
        {
            let text = |key: &[u8]| {
                String::from_utf8_lossy(info_value(&info, key).unwrap_or_default()).into_owned()
            };
            let name = text(b"map").to_ascii_lowercase();
            let Some(entry) = maps.iter_mut().find(|entry| entry.name == name) else {
                continue;
            };
            let longname = text(b"longname");
            if !longname.is_empty() && longname != "<NULL>" {
                entry.title = longname;
            }
            let types = text(b"type").to_ascii_lowercase();
            for word in types.split_whitespace() {
                if let Some(index) = MODES.iter().position(|mode| mode.arena == word) {
                    entry.modes |= 1 << index;
                }
            }
        }
        for entry in &mut maps {
            if entry.modes == 0 && entry.name.starts_with("mp/") {
                entry.modes = 1;
            }
        }
        maps.retain(|entry| entry.modes != 0);
        maps.sort_by(|a, b| a.name.cmp(&b.name));
        maps.dedup_by(|a, b| a.name == b.name);
        let mut names: Vec<String> = Vec::new();
        for info in bots
            .iter()
            .flat_map(|text| sjk_game_jka::bots::parse_infos(text, 1_024, quiet))
        {
            let name = String::from_utf8_lossy(info_value(&info, b"name").unwrap_or_default())
                .into_owned();
            if !name.is_empty() && !names.iter().any(|known| known.eq_ignore_ascii_case(&name)) {
                names.push(name);
            }
        }
        Self { maps, bots: names }
    }

    /// Every installed map, by name.
    pub(crate) fn maps(&self) -> &[MapEntry] {
        &self.maps
    }

    /// Maps mode `mode` can be played on, by name.
    pub(crate) fn maps_for(&self, mode: usize) -> impl Iterator<Item = &MapEntry> {
        self.maps.iter().filter(move |entry| entry.supports(mode))
    }

    /// The map named `name` if mode `mode` is played on it.
    pub(crate) fn map(&self, mode: usize, name: &str) -> Option<&MapEntry> {
        self.maps_for(mode)
            .find(|entry| entry.name.eq_ignore_ascii_case(name))
    }

    /// The map `step` places from `name` among mode `mode`'s maps, wrapping;
    /// the first when `name` is not one of them.
    pub(crate) fn step_map(&self, mode: usize, name: &str, step: isize) -> Option<&MapEntry> {
        let maps: Vec<&MapEntry> = self.maps_for(mode).collect();
        if maps.is_empty() {
            return None;
        }
        let Some(at) = maps
            .iter()
            .position(|entry| entry.name.eq_ignore_ascii_case(name))
        else {
            return maps.first().copied();
        };
        let count = maps.len() as isize;
        Some(maps[(at as isize + step).rem_euclid(count) as usize])
    }

    /// Every bot definition's name, first definition first.
    pub(crate) fn bots(&self) -> &[String] {
        &self.bots
    }
}

/// `count` bot names drawn from `roster` in an order shuffled by `seed`,
/// each once before any repeats (a roster smaller than `count` wraps).
pub(crate) fn pick_bots(roster: &[String], count: usize, seed: u64) -> Vec<String> {
    if roster.is_empty() {
        return Vec::new();
    }
    let mut order: Vec<usize> = (0..roster.len()).collect();
    // xorshift64*: enough to vary the line-up; 0 is not a valid state.
    let mut state = seed | 1;
    for index in (1..order.len()).rev() {
        state ^= state >> 12;
        state ^= state << 25;
        state ^= state >> 27;
        let draw = state.wrapping_mul(0x2545_F491_4F6C_DD1D) >> 33;
        order.swap(index, (draw % (index as u64 + 1)) as usize);
    }
    order
        .iter()
        .cycle()
        .take(count)
        .map(|&index| roster[index].clone())
        .collect()
}
