//! What a downloaded pak may bring into a session: new content, never commands,
//! code or replacements. The archive stays as downloaded (its checksums are what
//! servers compare); refused entries are hidden when it is mounted.

use sjk_vfs::{VirtualFileSystem, VirtualPath};

/// Why an entry of a downloaded pak is hidden.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) enum Refusal {
    /// Native code or a host script (`cgamex86.dll`, `.exe`, `.bat`, `.qvm`).
    Program,
    /// Console commands (`autoexec.cfg`, any `.cfg` but model data) or ICARUS.
    Script,
    /// Menus and HUD definitions (`ui/`, `.menu`), which run commands.
    Menu,
    /// An archive inside the archive.
    Archive,
    /// A file the game or an installed pack already has.
    Replacement,
}

impl Refusal {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Program => "programs",
            Self::Script => "scripts",
            Self::Menu => "menus",
            Self::Archive => "archives",
            Self::Replacement => "replacements",
        }
    }
}

const PROGRAMS: [&str; 26] = [
    "dll", "so", "dylib", "exe", "com", "scr", "pif", "bat", "cmd", "ps1", "psm1", "vbs", "vbe",
    "js", "jse", "wsf", "wsh", "hta", "msi", "msp", "jar", "lnk", "reg", "cpl", "sh", "qvm",
];
const ARCHIVES: [&str; 5] = ["pk3", "pk3dir", "zip", "7z", "rar"];
/// Model data the game reads as tables, never executes.
const MODEL_TABLES: [&str; 3] = ["animation.cfg", "animevents.cfg", "sounds.cfg"];

/// Why `path` of a downloaded pak stays hidden when mounted over `base` (the
/// game data and installed packs), or `None` when it may load.
///
/// Shader scripts always mount: their definitions are screened one by one
/// (`shader_authority`), since a map's script mixes new shaders with copies.
/// A map file may replace an installed one of the same name, as the server
/// runs it; the session checks it against the server's map checksum.
pub(crate) fn refusal(path: &VirtualPath, base: &VirtualFileSystem) -> Option<Refusal> {
    let path = path.as_str();
    let name = path.rsplit('/').next().unwrap_or(path);
    let extension = name.rsplit_once('.').map_or("", |(_, extension)| extension);
    if PROGRAMS.contains(&extension) {
        return Some(Refusal::Program);
    }
    if ARCHIVES.contains(&extension) {
        return Some(Refusal::Archive);
    }
    if path.starts_with("ui/") || extension == "menu" {
        return Some(Refusal::Menu);
    }
    if extension == "ibi"
        || (extension == "cfg" && !(path.starts_with("models/") && MODEL_TABLES.contains(&name)))
    {
        return Some(Refusal::Script);
    }
    if extension != "shader" && extension != "bsp" && base.contains(path).unwrap_or(true) {
        return Some(Refusal::Replacement);
    }
    None
}

/// Counts of hidden entries, for one log line per pak.
#[derive(Default)]
pub(crate) struct Tally {
    counts: std::collections::BTreeMap<Refusal, usize>,
    examples: Vec<String>,
}

impl Tally {
    pub(crate) fn add(&mut self, refusal: Refusal, path: &VirtualPath) {
        *self.counts.entry(refusal).or_default() += 1;
        if refusal != Refusal::Replacement && self.examples.len() < 4 {
            self.examples.push(path.as_str().to_owned());
        }
    }

    /// `None` when nothing was hidden.
    pub(crate) fn summary(&self) -> Option<String> {
        if self.counts.is_empty() {
            return None;
        }
        let counts: Vec<_> = self
            .counts
            .iter()
            .map(|(refusal, count)| format!("{count} {}", refusal.label()))
            .collect();
        let mut text = counts.join(", ");
        if !self.examples.is_empty() {
            text.push_str(&format!(" ({})", self.examples.join(", ")));
        }
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> VirtualFileSystem {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "assets0.pk3",
            [
                ("maps/mp/ffa3.bsp", b"retail".to_vec()),
                ("levelshots/mp/ffa3.jpg", Vec::new()),
                ("shaders/bespin.shader", Vec::new()),
                ("sound/player/footsteps/boot1.wav", Vec::new()),
            ],
        )
        .unwrap();
        vfs
    }

    fn check(path: &str) -> Option<Refusal> {
        refusal(&VirtualPath::new(path).unwrap(), &base())
    }

    #[test]
    fn new_content_loads() {
        for path in [
            "maps/mp/ffa3night.bsp",
            "textures/jof/night.jpg",
            "models/players/hood/model.glm",
            "models/players/hood/animevents.cfg",
            "models/players/hood/sounds.cfg",
            "sound/jof/bell.wav",
            "scripts/ffa3night.arena",
            "rm.txt",
        ] {
            assert_eq!(check(path), None, "{path}");
        }
    }

    #[test]
    fn code_commands_and_menus_never_load() {
        for (path, refusal) in [
            ("cgamex86.dll", Refusal::Program),
            ("jampgamex86.dll", Refusal::Program),
            ("vm/cgame.qvm", Refusal::Program),
            ("install.bat", Refusal::Program),
            ("autoexec.cfg", Refusal::Script),
            ("mpdefault.cfg", Refusal::Script),
            ("configs/binds.cfg", Refusal::Script),
            ("models/players/hood/evil.cfg", Refusal::Script),
            ("scripts/ffa3_falcon.ibi", Refusal::Script),
            ("ui/main.menu", Refusal::Menu),
            ("ui/jahud.txt", Refusal::Menu),
            ("hud/custom.menu", Refusal::Menu),
            ("extra.pk3", Refusal::Archive),
        ] {
            assert_eq!(check(path), Some(refusal), "{path}");
        }
    }

    #[test]
    fn installed_files_are_not_replaced_but_maps_and_shader_scripts_mount() {
        assert_eq!(check("levelshots/mp/ffa3.jpg"), Some(Refusal::Replacement));
        assert_eq!(
            check("sound/player/footsteps/boot1.wav"),
            Some(Refusal::Replacement)
        );
        assert_eq!(check("maps/mp/ffa3.bsp"), None);
        assert_eq!(check("shaders/bespin.shader"), None);
    }

    #[test]
    fn the_tally_names_what_was_hidden() {
        let mut tally = Tally::default();
        assert_eq!(tally.summary(), None);
        let path = |p| VirtualPath::new(p).unwrap();
        tally.add(Refusal::Replacement, &path("levelshots/mp/ffa3.jpg"));
        tally.add(Refusal::Script, &path("autoexec.cfg"));
        assert_eq!(
            tally.summary().unwrap(),
            "1 scripts, 1 replacements (autoexec.cfg)"
        );
    }
}
