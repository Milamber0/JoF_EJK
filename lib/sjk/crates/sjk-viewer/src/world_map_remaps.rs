//! Worldspawn and local shader remaps owned by the loaded map, and the choice
//! between them and the session's server remaps.
//!
//! rd-vanilla writes every source into the same `shader_t::remappedShader`, so
//! the latest remap of a shader wins and remapping it to itself restores it.
//! Worldspawn keys apply while the world loads (order 0), before cgame reads its
//! shader state; server and local remaps carry [`sjk_client::next_remap_order`]
//! stamps. `cg_remaps` only selects the server table, so a server remap it
//! excludes reveals the remap that it hid.
use sjk_bsp::Bsp;
use sjk_client::ShaderRemapTable;
use sjk_shader::ShaderCatalog;
use sjk_vfs::VirtualFileSystem;
use std::collections::BTreeMap;

const MAX_LOCAL_REMAPS: usize = 1024;

/// One remap as [`MapRemaps::entries`] lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct RemapEntry<'a> {
    /// `map` (a worldspawn key), `server` or `local` (the console's `remapShader`).
    pub(crate) source: &'static str,
    pub(crate) from: &'a str,
    pub(crate) to: &'a str,
    /// Application order: worldspawn keys first, then by stamp.
    pub(crate) order: u64,
    /// No later remap of `from` hides it.
    pub(crate) active: bool,
}

/// Remaps that live as long as the loaded map: worldspawn keys and console overrides.
#[derive(Debug, Default)]
pub(crate) struct MapRemaps {
    worldspawn: BTreeMap<String, String>,
    local: BTreeMap<String, (String, u64)>,
}

impl MapRemaps {
    /// Read the map's worldspawn remaps; like `R_RemapShader`, drop and report
    /// a remap whose shaders do not register.
    pub(crate) fn load(bsp: &Bsp, vfs: &VirtualFileSystem, shaders: &ShaderCatalog) -> Self {
        let Ok(entities) = sjk_entity::parse_entity_lump(bsp.entities()) else {
            return Self::default();
        };
        // R_LoadEntities parses only the first entity, the worldspawn.
        let Some(world) = entities.first() else {
            return Self::default();
        };
        let fields = world.fields().iter().map(|(k, v)| (k.as_str(), v.as_str()));
        let mut worldspawn = BTreeMap::new();
        for (old, new) in worldspawn_remaps(fields) {
            match checked(vfs, shaders, old, new) {
                Ok((old, new)) => {
                    worldspawn.insert(old, new);
                }
                Err(error) => crate::log::progress(format_args!(
                    "worldspawn remap {old} -> {new} ignored: {error}"
                )),
            }
        }
        Self {
            worldspawn,
            local: BTreeMap::new(),
        }
    }
    /// Record a checked console remap as the latest remap of `old`.
    pub(crate) fn remap_local(&mut self, old: String, new: String) -> Result<(), &'static str> {
        if self.local.len() >= MAX_LOCAL_REMAPS && !self.local.contains_key(&old) {
            return Err("Local remap limit reached");
        }
        self.local
            .insert(old, (new, sjk_client::next_remap_order()));
        Ok(())
    }
    /// Forget console remaps when another session's state is displayed.
    pub(crate) fn clear_local(&mut self) {
        self.local.clear();
    }
    /// Forget every remap of the map, worldspawn keys included, for `clearRemaps`.
    pub(crate) fn clear(&mut self) {
        self.worldspawn.clear();
        self.local.clear();
    }
    /// Whether any source names this shader, so its material may need a refresh.
    pub(crate) fn affects(&self, name: &str, server: Option<&ShaderRemapTable>) -> bool {
        self.worldspawn.contains_key(name)
            || self.local.contains_key(name)
            || server.is_some_and(|s| s.affects(name))
    }
    /// Shader drawn for `name`: the target of its latest remap. One hop only.
    pub(crate) fn target<'a>(
        &'a self,
        name: &'a str,
        server: Option<&'a ShaderRemapTable>,
    ) -> &'a str {
        self.latest(name, server).map_or(name, |(target, _)| target)
    }
    /// Every remap with its source (map, server or local) in application order, each
    /// marked active unless a later remap of the same shader hides it.
    pub(crate) fn entries<'a>(
        &'a self,
        server: Option<&'a ShaderRemapTable>,
    ) -> Vec<RemapEntry<'a>> {
        let map = self
            .worldspawn
            .iter()
            .map(|(old, new)| ("map", old.as_str(), new.as_str(), 0));
        let served = server.into_iter().flat_map(|table| {
            table
                .entries()
                .map(|(old, new, order)| ("server", old, new, order))
        });
        let local = self
            .local
            .iter()
            .map(|(old, (new, order))| ("local", old.as_str(), new.as_str(), *order));
        let mut entries: Vec<_> = map
            .chain(served)
            .chain(local)
            .map(|(source, from, to, order)| RemapEntry {
                source,
                from,
                to,
                order,
                active: self.latest(from, server).is_some_and(|(_, o)| o == order),
            })
            .collect();
        entries.sort_by_key(|entry| entry.order);
        entries
    }
    /// `listRemaps` lines with their source (map, server or local) in application
    /// order; a remap hidden by a later one of the same shader is marked.
    pub(crate) fn listing(&self, server: Option<&ShaderRemapTable>) -> Vec<String> {
        self.entries(server)
            .into_iter()
            .map(|entry| {
                let time = match (entry.source, server) {
                    ("server", Some(table)) => format!(" (time {})", table.time_offset(entry.to)),
                    _ => String::new(),
                };
                let state = if entry.active { "" } else { " (overridden)" };
                format!(
                    "{}: {} -> {}{time}{state}",
                    entry.source, entry.from, entry.to
                )
            })
            .collect()
    }
    fn latest<'a>(
        &'a self,
        name: &str,
        server: Option<&'a ShaderRemapTable>,
    ) -> Option<(&'a str, u64)> {
        let map = self.worldspawn.get(name).map(|t| (t.as_str(), 0));
        let local = self.local.get(name).map(|(t, order)| (t.as_str(), *order));
        [map, server.and_then(|s| s.alias(name)), local]
            .into_iter()
            .flatten()
            .max_by_key(|(_, order)| *order)
    }
}

/// Renderer names of a remap whose shaders both register, by definition or image.
pub(crate) fn checked(
    vfs: &VirtualFileSystem,
    shaders: &ShaderCatalog,
    old: &str,
    new: &str,
) -> Result<(String, String), String> {
    let old = sjk_client::shader_name(old).ok_or("Invalid source shader name")?;
    let new = sjk_client::shader_name(new).ok_or("Invalid destination shader name")?;
    for name in [&old, &new] {
        if shaders.get(name).is_none()
            && shaders
                .resolve_image(vfs, name)
                .map_err(|e| e.to_string())?
                .is_none()
        {
            return Err(format!("Shader not found: {name}"));
        }
    }
    Ok((old, new))
}

/// Worldspawn remaps in the order rd-vanilla `R_LoadEntities` applies them:
/// keys with the case-sensitive prefix `remapshader` and an `old;new` value,
/// split at the first `;`. Like C, the scan stops at an empty key or value, at
/// one starting with `}`, or at a remap value without `;`. `vertexremapshader`
/// keys apply only under `r_vertexLight`, which SJK does not have.
pub(crate) fn worldspawn_remaps<'a>(
    fields: impl IntoIterator<Item = (&'a str, &'a str)>,
) -> Vec<(&'a str, &'a str)> {
    let mut remaps = Vec::new();
    for (key, value) in fields {
        if [key, value]
            .iter()
            .any(|token| token.is_empty() || token.starts_with('}'))
        {
            break;
        }
        let vertex = key.starts_with("vertexremapshader");
        if !vertex && !key.starts_with("remapshader") {
            continue;
        }
        let Some(remap) = value.split_once(';') else {
            break;
        };
        if !vertex {
            remaps.push(remap);
        }
    }
    remaps
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_client::ShaderRemaps;

    fn map(remaps: &[(&str, &str)]) -> MapRemaps {
        MapRemaps {
            worldspawn: remaps
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
            local: BTreeMap::new(),
        }
    }
    fn local(remaps: &mut MapRemaps, old: &str, new: &str) {
        remaps.remap_local(old.into(), new.into()).unwrap();
    }

    #[test]
    fn worldspawn_keys_follow_r_load_entities() {
        let fields = [
            ("classname", "worldspawn"),
            ("remapshader", "textures/a;textures/b"),
            ("remapshader2", "textures/c;textures/d;e"),
            ("RemapShader", "textures/x;textures/y"),
            ("vertexremapshader", "textures/v;textures/w"),
            ("remapshader3", "textures/a;textures/a"),
        ];
        assert_eq!(
            worldspawn_remaps(fields),
            [
                ("textures/a", "textures/b"),
                ("textures/c", "textures/d;e"),
                ("textures/a", "textures/a"),
            ]
        );
    }

    #[test]
    fn worldspawn_scan_stops_where_c_breaks() {
        let after = ("remapshader9", "textures/c;textures/d");
        for stop in [
            ("remapshader1", "no-separator"),
            ("vertexremapshader", "no-separator"),
            ("message", ""),
            ("", "value"),
            ("}", "value"),
        ] {
            let fields = [("remapshader", "textures/a;textures/b"), stop, after];
            assert_eq!(
                worldspawn_remaps(fields),
                [("textures/a", "textures/b")],
                "{stop:?}"
            );
        }
    }

    #[test]
    fn latest_remap_wins_across_sources() {
        let mut remaps = map(&[("textures/a", "textures/map")]);
        let mut server = ShaderRemaps::default();
        assert_eq!(remaps.target("textures/a", None), "textures/map");
        server.apply_config(b"textures/a=textures/server:0@");
        assert_eq!(
            remaps.target("textures/a", server.table(1)),
            "textures/server"
        );
        local(&mut remaps, "textures/a", "textures/local");
        assert_eq!(
            remaps.target("textures/a", server.table(1)),
            "textures/local"
        );
        // Each configstring update applies every entry again, as C does.
        server.apply_config(b"textures/a=textures/server:0@");
        assert_eq!(
            remaps.target("textures/a", server.table(1)),
            "textures/server"
        );
        assert!(remaps.affects("textures/a", None));
        assert!(!remaps.affects("textures/other", server.table(1)));
        assert_eq!(remaps.target("textures/other", None), "textures/other");
    }

    #[test]
    fn self_restore_yields_to_a_later_server_remap() {
        let mut remaps = map(&[("textures/a", "textures/map")]);
        local(&mut remaps, "textures/a", "textures/a");
        assert_eq!(remaps.target("textures/a", None), "textures/a");
        let mut server = ShaderRemaps::default();
        server.apply_config(b"textures/a=textures/server:0@");
        assert_eq!(
            remaps.target("textures/a", server.table(1)),
            "textures/server"
        );
    }

    #[test]
    fn cg_remaps_levels_reveal_the_remap_a_server_remap_hid() {
        let player = "models/players/kyle/body";
        let mut remaps = map(&[(player, "textures/map")]);
        local(&mut remaps, "textures/wall", "textures/local");
        let mut server = ShaderRemaps::default();
        server.apply_config(
            b"models/players/kyle/body=textures/server:0@textures/wall=textures/x:0@",
        );
        assert_eq!(remaps.target(player, server.table(2)), "textures/server");
        assert_eq!(remaps.target(player, server.table(1)), "textures/map");
        assert_eq!(
            remaps.target("textures/wall", server.table(1)),
            "textures/x"
        );
        assert_eq!(
            remaps.target("textures/wall", server.table(0)),
            "textures/local"
        );
        assert_eq!(remaps.target(player, server.table(0)), "textures/map");
    }

    #[test]
    fn listing_shows_sources_in_order_and_marks_overridden_remaps() {
        let mut remaps = map(&[("textures/a", "textures/map")]);
        let mut server = ShaderRemaps::default();
        server.apply_config(b"textures/a=textures/server: 2.50@");
        local(&mut remaps, "textures/b", "textures/b");
        assert_eq!(
            remaps.listing(server.table(1)),
            [
                "map: textures/a -> textures/map (overridden)",
                "server: textures/a -> textures/server (time 2.5)",
                "local: textures/b -> textures/b",
            ]
        );
        assert_eq!(
            remaps.listing(server.table(0)),
            [
                "map: textures/a -> textures/map",
                "local: textures/b -> textures/b",
            ]
        );
        remaps.clear_local();
        assert!(MapRemaps::default().listing(None).is_empty());
    }
}
