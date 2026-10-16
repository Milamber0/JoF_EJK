//! Multiplayer `CS_SHADERSTATE` and `remapShader` compatibility state.
use sjk_protocol::GameState;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicU64, Ordering};

/// The multiplayer shader-state configstring (`codemp/game/bg_public.h`).
pub const SHADER_STATE_CONFIG: usize = 24;
const MAX_REMAPS: usize = 1024;

/// Next stamp of the process-wide remap order. Every rd-vanilla remap source
/// writes the same `shader_t::remappedShader`, so the latest call wins; server
/// and local remaps take their stamps from this one counter. Worldspawn remaps
/// use 0: the renderer applies them while loading the world, before cgame.
pub fn next_remap_order() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Persistent, one-hop renderer aliases. Removing an entry from a configstring
/// does not undo it: stock cgame applies entries, and self-remaps undo aliases.
#[derive(Clone, Debug, PartialEq)]
pub struct ShaderRemaps {
    identity: u64,
    revision: u64,
    tables: [ShaderRemapTable; 2],
}
impl Default for ShaderRemaps {
    fn default() -> Self {
        static NEXT_ID: AtomicU64 = AtomicU64::new(1);
        Self {
            identity: NEXT_ID.fetch_add(1, Ordering::Relaxed),
            revision: 0,
            tables: Default::default(),
        }
    }
}
impl ShaderRemaps {
    /// Seed the renderer state of a new gamestate.
    pub fn from_game_state(game: &GameState) -> Self {
        let mut state = Self::default();
        state.apply_config(game.config_string(SHADER_STATE_CONFIG).unwrap_or_default());
        state
    }
    /// Session identity and revision, safe when a resident map is reused on reconnect.
    pub fn stamp(&self) -> (u64, u64) {
        (self.identity, self.revision)
    }
    /// Monotonic change counter within this session, including map resets.
    pub fn revision(&self) -> u64 {
        self.revision
    }
    /// Reset aliases for a replacement gamestate, then apply its initial state.
    pub(crate) fn reset(&mut self, game: &GameState) {
        let revision = self.revision.wrapping_add(1);
        *self = Self::from_game_state(game);
        self.revision = revision;
    }
    /// Tayst mode 1 excludes player-texture configstring remaps; mode 2 accepts
    /// all. Reliable remapShader commands are accepted by either nonzero mode.
    pub fn table(&self, mode: i64) -> Option<&ShaderRemapTable> {
        (mode != 0).then(|| &self.tables[usize::from(mode != 1)])
    }
    /// Apply complete `old=new:offset@` entries; a truncated tail is ignored.
    /// Every entry is applied again and becomes the latest remap of its shader,
    /// as `CG_ShaderStateChanged` calls `R_RemapShader` for each one.
    pub fn apply_config(&mut self, bytes: &[u8]) {
        let Ok(mut rest) = std::str::from_utf8(bytes) else {
            return;
        };
        while let Some((original, tail)) = rest.split_once('=') {
            let Some((target, tail)) = tail.split_once(':') else {
                break;
            };
            let Some((offset, tail)) = tail.split_once('@') else {
                break;
            };
            self.apply(original, target, offset, false);
            rest = tail;
        }
    }
    /// EternalJK's `clearRemaps` drops every alias until later entries arrive.
    /// Like its renderer, destination time offsets stay with their shaders.
    pub fn clear(&mut self) {
        let changed = self.tables.iter().any(|table| !table.aliases.is_empty());
        for table in &mut self.tables {
            table.aliases.clear();
        }
        self.revision = self.revision.wrapping_add(u64::from(changed));
    }
    /// Consume the already tokenized reliable cgame command (no wire changes).
    pub(crate) fn command(&mut self, arguments: &[Vec<u8>]) -> bool {
        if arguments
            .first()
            .is_none_or(|a| !a.eq_ignore_ascii_case(b"remapShader"))
        {
            return false;
        }
        if arguments.len() == 4 {
            if let (Ok(old), Ok(new), Ok(offset)) = (
                std::str::from_utf8(&arguments[1]),
                std::str::from_utf8(&arguments[2]),
                std::str::from_utf8(&arguments[3]),
            ) {
                self.apply(old, new, offset, true);
            }
        }
        true
    }
    fn apply(&mut self, original: &str, target: &str, offset: &str, command: bool) {
        let (Some(original), Some(target)) = (shader_name(original), shader_name(target)) else {
            return;
        };
        let offset = Some(atof(offset)).filter(|v| v.is_finite()).unwrap_or(0.);
        let order = next_remap_order();
        let mut changed = self.tables[1].apply(&original, &target, offset, order);
        if command || !original.starts_with("models/players/") {
            changed |= self.tables[0].apply(&original, &target, offset, order);
        }
        self.revision = self.revision.wrapping_add(u64::from(changed));
    }
}

/// Resolved remaps for one client policy. This is material data, not wire state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ShaderRemapTable {
    aliases: BTreeMap<String, (String, u64)>,
    offsets: BTreeMap<String, f32>,
}
impl ShaderRemapTable {
    /// Shader used for this original name. Aliases deliberately do not chain.
    pub fn destination<'a>(&'a self, original: &'a str) -> &'a str {
        self.alias(original).map_or(original, |(target, _)| target)
    }
    /// Latest server remap of a name and its [`next_remap_order`] stamp.
    pub fn alias(&self, original: &str) -> Option<(&str, u64)> {
        let (target, order) = self.aliases.get(original)?;
        Some((target, *order))
    }
    /// Stock stores timeOffset on the destination shader, shared by its users.
    pub fn time_offset(&self, destination: &str) -> f32 {
        self.offsets.get(destination).copied().unwrap_or(0.)
    }
    /// Whether this state has an explicit alias or clock update for a name.
    pub fn affects(&self, original: &str) -> bool {
        self.aliases.contains_key(original) || self.offsets.contains_key(original)
    }
    /// Original and replacement names with their order stamps, for listRemaps.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str, u64)> {
        self.aliases
            .iter()
            .map(|(a, (b, order))| (a.as_str(), b.as_str(), *order))
    }
    /// A repeated remap still changes the table: it becomes the latest one.
    fn apply(&mut self, original: &str, target: &str, offset: f32, order: u64) -> bool {
        if (!self.aliases.contains_key(original) && self.aliases.len() >= MAX_REMAPS)
            || (!self.offsets.contains_key(target) && self.offsets.len() >= MAX_REMAPS * 2)
        {
            return false;
        }
        self.aliases
            .insert(original.to_owned(), (target.to_owned(), order));
        self.offsets.insert(target.to_owned(), offset);
        true
    }
}

/// Renderer shader identity: case-insensitive virtual path without its extension.
pub fn shader_name(name: &str) -> Option<String> {
    if name.is_empty()
        || name.len() >= 64
        || name
            .bytes()
            .any(|b| b < 32 || matches!(b, b'=' | b':' | b'@'))
    {
        return None;
    }
    let mut name = name.replace('\\', "/").to_ascii_lowercase();
    let start = name.rfind('/').map_or(0, |i| i + 1);
    if let Some(dot) = name[start..].rfind('.') {
        name.truncate(start + dot);
    }
    (!name.is_empty() && !name.split('/').any(|part| part == "..")).then_some(name)
}

/// C `atof` for decimal text: leading whitespace, then the longest decimal
/// prefix with an optional exponent; 0 when there is none.
fn atof(text: &str) -> f32 {
    let text = text.trim_start_matches([' ', '\t', '\n', '\r', '\x0b', '\x0c']);
    let bytes = text.as_bytes();
    let digits = |from: usize| {
        from + bytes
            .get(from..)
            .unwrap_or_default()
            .iter()
            .take_while(|b| b.is_ascii_digit())
            .count()
    };
    let sign = usize::from(matches!(bytes.first(), Some(b'+' | b'-')));
    let mut end = digits(sign);
    let mut mantissa = end - sign;
    if bytes.get(end) == Some(&b'.') {
        let fraction = digits(end + 1);
        mantissa += fraction - end - 1;
        end = fraction;
    }
    if mantissa == 0 {
        return 0.;
    }
    if matches!(bytes.get(end), Some(b'e' | b'E')) {
        let sign = end + 1 + usize::from(matches!(bytes.get(end + 1), Some(b'+' | b'-')));
        let exponent = digits(sign);
        if exponent > sign {
            end = exponent;
        }
    }
    text[..end].parse().unwrap_or(0.)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table(state: &ShaderRemaps, mode: i64) -> &ShaderRemapTable {
        state.table(mode).unwrap()
    }

    #[test]
    fn atof_matches_the_c_library() {
        for (text, value) in [
            (" 5.20", 5.2),
            ("\t-3", -3.),
            ("1.5x", 1.5),
            ("1e2x", 100.),
            ("1e", 1.),
            ("1e+", 1.),
            ("2E-1", 0.2),
            (".5", 0.5),
            ("7.", 7.),
            ("+.", 0.),
            (".", 0.),
            ("abc", 0.),
            ("", 0.),
        ] {
            assert_eq!(atof(text), value, "{text:?}");
        }
    }

    #[test]
    fn offsets_parse_like_atof_and_reject_nonfinite_values() {
        let mut state = ShaderRemaps::default();
        state.apply_config(b"a=b: 1.5x@c=d:1e999@e=f:junk@");
        let table = table(&state, 2);
        assert_eq!(table.time_offset("b"), 1.5);
        assert_eq!(table.time_offset("d"), 0.);
        assert_eq!(table.time_offset("f"), 0.);
    }

    #[test]
    fn reapplied_entries_become_the_latest_remaps() {
        let mut state = ShaderRemaps::default();
        state.apply_config(b"textures/a=textures/b:0@");
        let (_, first) = table(&state, 1).alias("textures/a").unwrap();
        let revision = state.revision();
        // An unchanged configstring still re-asserts its entries over later
        // remaps from other sources, as CG_ShaderStateChanged does.
        let between = next_remap_order();
        state.apply_config(b"textures/a=textures/b:0@");
        let (target, again) = table(&state, 1).alias("textures/a").unwrap();
        assert_eq!(target, "textures/b");
        assert!(first < between && between < again);
        assert_ne!(state.revision(), revision);
    }

    #[test]
    fn player_entries_stay_out_of_the_default_table_but_commands_do_not() {
        let mut state = ShaderRemaps::default();
        state.apply_config(b"models/players/kyle/body=textures/x:0@");
        assert!(table(&state, 1).alias("models/players/kyle/body").is_none());
        assert!(table(&state, 2).alias("models/players/kyle/body").is_some());
        let command = ["remapShader", "models/players/kyle/head", "textures/y", "2"]
            .map(|a| a.as_bytes().to_vec());
        assert!(state.command(&command));
        assert_eq!(
            table(&state, 1).destination("models/players/kyle/head"),
            "textures/y"
        );
        assert_eq!(table(&state, 1).time_offset("textures/y"), 2.);
        assert!(state.table(0).is_none());
    }

    #[test]
    fn clear_drops_aliases_until_the_server_sends_them_again() {
        let mut remaps = ShaderRemaps::default();
        let state = b"textures/a=textures/b:1.5@models/players/x/y=textures/c:0@";
        remaps.apply_config(state);
        let (identity, revision) = remaps.stamp();
        remaps.clear();
        assert_eq!(remaps.stamp(), (identity, revision + 1));
        for mode in [1, 2] {
            let table = remaps.table(mode).unwrap();
            assert_eq!(table.entries().count(), 0);
            assert_eq!(table.destination("textures/a"), "textures/a");
            assert_eq!(table.time_offset("textures/b"), 1.5);
        }
        remaps.clear();
        assert_eq!(remaps.revision(), revision + 1);
        remaps.apply_config(state);
        let all = remaps.table(2).unwrap();
        assert_eq!(all.destination("textures/a"), "textures/b");
        assert_eq!(all.entries().count(), 2);
        assert_eq!(remaps.table(1).unwrap().entries().count(), 1);
    }
}
