//! Maps on which server shader remaps are ignored, as with `cg_remaps 0` there.
//!
//! `cg_remapsBlockedMaps` lists map names (`mp/ffa4 mp/duel6`), separated by spaces,
//! commas or semicolons (a console command needs the list in quotes to hold
//! semicolons). A listed map still applies its own worldspawn remaps and
//! local `remapShader` overrides; only the server's table is left out. The frame reads
//! a cached answer, refreshed when the cvar or the loaded map changes.

/// The list of maps, archived with the player's settings.
pub(crate) const CVAR: &str = "cg_remapsBlockedMaps";

/// The map a list entry or a loaded BSP path names: `maps/mp/ffa4.bsp` and
/// `mp/ffa4` are both `mp/ffa4`.
pub(crate) fn map_name(path: &str) -> &str {
    let name = path.trim().trim_start_matches(['/', '\\']);
    let name = name
        .get(..5)
        .filter(|prefix| {
            // Bytes, not str slices: a multi-byte character may straddle byte 4.
            let prefix = prefix.as_bytes();
            prefix[..4].eq_ignore_ascii_case(b"maps") && matches!(prefix[4], b'/' | b'\\')
        })
        .map_or(name, |_| &name[5..]);
    name.len()
        .checked_sub(4)
        .filter(|&end| {
            name.get(end..)
                .is_some_and(|ext| ext.eq_ignore_ascii_case(".bsp"))
        })
        .map_or(name, |end| &name[..end])
}

fn same_map(a: &str, b: &str) -> bool {
    let fold = |byte: u8| match byte {
        b'\\' => b'/',
        byte => byte.to_ascii_lowercase(),
    };
    a.len() == b.len() && a.bytes().zip(b.bytes()).all(|(a, b)| fold(a) == fold(b))
}

fn entries(list: &str) -> impl Iterator<Item = &str> {
    list.split(|c: char| c.is_whitespace() || c == ',' || c == ';')
        .map(map_name)
        .filter(|entry| !entry.is_empty())
}

/// Whether `list` names the map at `path`.
pub(crate) fn lists(list: &str, path: &str) -> bool {
    let map = map_name(path);
    !map.is_empty() && entries(list).any(|entry| same_map(entry, map))
}

/// `list` with `map` added; `None` when it is already there.
pub(crate) fn with(list: &str, map: &str) -> Option<String> {
    let map = map_name(map);
    (!lists(list, map)).then(|| entries(list).chain([map]).collect::<Vec<_>>().join(" "))
}

/// `list` without `map`; `None` when it was not there.
pub(crate) fn without(list: &str, map: &str) -> Option<String> {
    let map = map_name(map);
    lists(list, map).then(|| {
        entries(list)
            .filter(|entry| !same_map(entry, map))
            .collect::<Vec<_>>()
            .join(" ")
    })
}

/// Whether the loaded map is listed, as of the last cvar change and map path seen.
#[derive(Default)]
pub(crate) struct Cache {
    revision: Option<u64>,
    map: String,
    blocked: bool,
}

impl Cache {
    /// Answer for `map`, reading the list from `list` only after a change.
    pub(crate) fn blocked<'a>(
        &mut self,
        revision: u64,
        map: &str,
        list: impl FnOnce() -> Option<&'a str>,
    ) -> bool {
        if self.revision != Some(revision) || self.map != map {
            self.revision = Some(revision);
            self.map.clear();
            self.map.push_str(map);
            self.blocked = list().is_some_and(|list| lists(list, map));
        }
        self.blocked
    }
}

impl crate::GpuState {
    /// `cg_remaps` for the loaded map: 0 on a map in `cg_remapsBlockedMaps`.
    pub(crate) fn remap_mode(&mut self) -> i64 {
        let Some(console) = self.console.as_ref() else {
            return 1;
        };
        let blocked = self.remap_blocked_maps.blocked(
            console.remap_blocked_maps_revision(),
            &self.world_load_map,
            || console.text_cvar(CVAR).ok(),
        );
        if blocked { 0 } else { console.remap_mode() }
    }

    /// `blockRemaps` and `unblockRemaps`: list or unlist a map, the loaded one by default.
    pub(crate) fn remap_block_command(
        &mut self,
        block: bool,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        let usage = if block {
            "usage: blockRemaps [map]"
        } else {
            "usage: unblockRemaps [map]"
        };
        let map = match args {
            [] => map_name(&self.world_load_map).to_owned(),
            [map] => map_name(map).to_owned(),
            _ => return Err(usage.into()),
        };
        if map.is_empty() {
            return Err(format!("No loaded map; {usage}"));
        }
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        let list = console.text_cvar(CVAR).map_err(|error| error.to_string())?;
        let (changed, message) = if block {
            (with(list, &map), "Server shader remaps ignored on")
        } else {
            (without(list, &map), "Server shader remaps allowed again on")
        };
        let Some(changed) = changed else {
            let state = if block { "already" } else { "not" };
            return Ok(vec![format!("{map} is {state} in {CVAR}")]);
        };
        if !console.set_cvar(CVAR, &changed) {
            return Err(format!("could not set {CVAR}"));
        }
        Ok(vec![format!("{message} {map}")])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn map_names_drop_the_folder_and_extension() {
        assert_eq!(map_name("maps/mp/ffa4.bsp"), "mp/ffa4");
        assert_eq!(map_name("MAPS\\mp/ffa4.BSP"), "mp/ffa4");
        assert_eq!(map_name("/maps/mp/ffa4.bsp"), "mp/ffa4");
        assert_eq!(map_name("mp/ffa4"), "mp/ffa4");
        assert_eq!(map_name("mapsmp/ffa4"), "mapsmp/ffa4");
        assert_eq!(map_name(""), "");
        // A multi-byte character across byte 4 (the cvar is archived: this must not panic).
        assert_eq!(map_name("mp/\u{e9}t\u{e9}"), "mp/\u{e9}t\u{e9}");
    }

    #[test]
    fn listed_maps_match_regardless_of_case_and_separators() {
        let list = "mp/duel6, MP/FFA4;mp/ctf1";
        assert!(lists(list, "maps/mp/ffa4.bsp"));
        assert!(lists(list, "maps\\mp\\duel6.bsp"));
        assert!(lists(list, "mp/ctf1"));
        assert!(!lists(list, "maps/mp/ffa3.bsp"));
        assert!(!lists(list, "maps/mp/ffa.bsp"));
        assert!(!lists("", "maps/mp/ffa4.bsp"));
        assert!(!lists(list, ""));
    }

    #[test]
    fn adding_and_removing_keep_the_other_maps() {
        assert_eq!(with("", "maps/mp/ffa4.bsp").as_deref(), Some("mp/ffa4"));
        assert_eq!(
            with("mp/duel6", "mp/ffa4").as_deref(),
            Some("mp/duel6 mp/ffa4")
        );
        assert_eq!(with("mp/duel6 MP/FFA4", "mp/ffa4"), None);
        assert_eq!(
            without("mp/duel6,MP/FFA4 mp/ctf1", "maps/mp/ffa4.bsp").as_deref(),
            Some("mp/duel6 mp/ctf1")
        );
        assert_eq!(without("mp/duel6", "mp/ffa4"), None);
    }

    #[test]
    fn the_cache_reads_the_list_only_after_a_change() {
        let mut cache = Cache::default();
        assert!(cache.blocked(1, "maps/mp/ffa4.bsp", || Some("mp/ffa4")));
        assert!(cache.blocked(1, "maps/mp/ffa4.bsp", || panic!("read again")));
        assert!(!cache.blocked(1, "maps/mp/ffa3.bsp", || Some("mp/ffa4")));
        assert!(!cache.blocked(2, "maps/mp/ffa3.bsp", || Some("")));
        assert!(cache.blocked(3, "maps/mp/ffa3.bsp", || Some("mp/ffa3")));
    }
}
