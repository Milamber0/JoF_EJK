//! What the settings' HUD picker offers: every game-data HUD installed (the
//! retail HUD, each HUD pack replacing `ui/hud.menu`, and other HUD lists
//! such as EternalJK's `ui/elegance_hud.txt`), the text-only HUD, and SJK's
//! own two layouts. A choice is the `cg_hudStyle`, `cg_hudFiles` and
//! `cg_hudPack` values that select it.

use super::layout::Layout;
use super::{
    DEFAULT_LIST, FILES_CVAR, HudStyle, PACK_CVAR, STYLE_CVAR, Source, hud_packs, mount_file_name,
    pack_view, read_menus, source,
};
use crate::console::ViewerConsole;
use sjk_vfs::VirtualFileSystem;

/// One HUD the picker lists.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct HudChoice {
    /// What the picker calls it.
    pub(crate) label: String,
    /// Where it comes from: its PK3 or list file, or SJK.
    pub(crate) origin: String,
    pub(crate) style: HudStyle,
    /// `cg_hudFiles` and `cg_hudPack` it sets (game-data HUDs only).
    pub(crate) files: String,
    pub(crate) pack: String,
}

impl HudChoice {
    fn game(label: String, origin: String, files: &str, pack: &str) -> Self {
        Self {
            label,
            origin,
            style: HudStyle::Game,
            files: files.to_owned(),
            pack: pack.to_owned(),
        }
    }

    fn own(label: &str, style: HudStyle) -> Self {
        Self {
            label: label.to_owned(),
            origin: "SJK's own layout".to_owned(),
            style,
            files: String::new(),
            pack: String::new(),
        }
    }

    /// Write the cvars that select this HUD. SJK's layouts leave the game
    /// HUD's files as they are.
    pub(crate) fn apply(&self, console: &mut ViewerConsole) {
        console.set_cvar(STYLE_CVAR, self.style.name());
        if self.style == HudStyle::Game {
            console.set_cvar(FILES_CVAR, &self.files);
            console.set_cvar(PACK_CVAR, &self.pack);
        }
    }

    /// Whether [`super::preview`] can draw it: the game-data HUDs. SJK's
    /// own layouts are drawn by the live HUD only.
    pub(crate) fn has_preview(&self) -> bool {
        self.style == HudStyle::Game
    }
}

/// The HUDs that can be picked with `vfs` mounted, in picker order: the
/// HUD packs in install order (the retail HUD first), other HUD lists, the
/// text HUD, then SJK's layouts.
pub(crate) fn list(vfs: &VirtualFileSystem) -> Vec<HudChoice> {
    let mut choices = Vec::new();
    for mount in hud_packs(vfs) {
        let file = mount_file_name(&mount.name);
        if pack_view(vfs, file).is_some_and(|view| usable(&view, DEFAULT_LIST)) {
            choices.push(HudChoice::game(
                pack_label(file),
                file.to_owned(),
                DEFAULT_LIST,
                file,
            ));
        }
    }
    for name in vfs.list_files("ui", ".txt") {
        let path = format!("ui/{name}");
        if name.contains('/')
            || !name.contains("hud")
            || path.eq_ignore_ascii_case(DEFAULT_LIST)
            || !usable(vfs, &path)
        {
            continue;
        }
        choices.push(HudChoice::game(list_label(&name), path.clone(), &path, ""));
    }
    choices.push(HudChoice::game(
        "Text only".to_owned(),
        "Numbers, no pictures".to_owned(),
        "1",
        "",
    ));
    choices.push(HudChoice::own("SJK radial", HudStyle::Radial));
    choices.push(HudChoice::own("SJK classic", HudStyle::Classic));
    choices
}

/// Whether `list` loads menus that draw a status HUD.
fn usable(vfs: &VirtualFileSystem, list: &str) -> bool {
    read_menus(vfs, list).is_some_and(|menus| Layout::resolve(&menus).is_usable())
}

/// The choice the cvars select, if the list has it. An empty `cg_hudPack`
/// is the HUD installed last; a pack that is no longer installed is too.
pub(crate) fn current(
    choices: &[HudChoice],
    style: HudStyle,
    files: &str,
    pack: &str,
) -> Option<usize> {
    let position = |wanted: &dyn Fn(&HudChoice) -> bool| choices.iter().position(wanted);
    let game = |choice: &HudChoice| choice.style == HudStyle::Game;
    match style {
        HudStyle::Classic | HudStyle::Radial => position(&|choice| choice.style == style),
        HudStyle::Game => match source(files) {
            Source::Text => {
                position(&|choice| game(choice) && source(&choice.files) == Source::Text)
            }
            Source::Menus(list) if list.eq_ignore_ascii_case(DEFAULT_LIST) => {
                let packs = || {
                    choices.iter().enumerate().filter(|(_, choice)| {
                        game(choice) && choice.files.eq_ignore_ascii_case(DEFAULT_LIST)
                    })
                };
                let pack = pack.trim();
                packs()
                    .find(|(_, choice)| !pack.is_empty() && choice.pack.eq_ignore_ascii_case(pack))
                    .or_else(|| packs().last())
                    .map(|(index, _)| index)
            }
            Source::Menus(list) => {
                position(&|choice| game(choice) && choice.files.eq_ignore_ascii_case(list))
            }
        },
    }
}

/// What a HUD pack is called: the retail archives are "Jedi Academy", any
/// other its file name without the load-order prefix (`zz_`) and with spaces.
fn pack_label(file: &str) -> String {
    if is_retail_archive(file) {
        return "Jedi Academy".to_owned();
    }
    let stem = strip_suffix_ignore_case(file, ".pk3");
    let trimmed = stem.trim_start_matches(['z', 'Z']);
    let unprefixed = match trimmed.strip_prefix(['_', '-']) {
        Some(rest) if trimmed.len() < stem.len() && !rest.is_empty() => rest,
        _ => stem,
    };
    unprefixed.replace('_', " ").trim().to_owned()
}

/// Whether `file` is one of the game's own archives (`assets0.pk3` ...).
pub(super) fn is_retail_archive(file: &str) -> bool {
    let stem = strip_suffix_ignore_case(file, ".pk3");
    stem.len() < file.len()
        && stem
            .get(..6)
            .is_some_and(|start| start.eq_ignore_ascii_case("assets"))
        && stem[6..].bytes().all(|byte| byte.is_ascii_digit())
}

/// What a HUD list is called: `elegance_hud.txt` is "Elegance HUD".
fn list_label(name: &str) -> String {
    let stem = strip_suffix_ignore_case(name, ".txt");
    let words: Vec<String> = stem
        .split(['_', ' '])
        .filter(|word| !word.is_empty())
        .map(|word| {
            if word.eq_ignore_ascii_case("hud") {
                "HUD".to_owned()
            } else {
                let mut characters = word.chars();
                characters.next().map_or_else(String::new, |first| {
                    first.to_uppercase().chain(characters).collect()
                })
            }
        })
        .collect();
    words.join(" ")
}

fn strip_suffix_ignore_case<'a>(text: &'a str, suffix: &str) -> &'a str {
    let cut = text.len().saturating_sub(suffix.len());
    match text.get(cut..) {
        Some(end) if end.eq_ignore_ascii_case(suffix) => &text[..cut],
        _ => text,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A HUD menu file drawing a left frame `left` and a right frame `right`.
    pub(crate) fn hud_menu(left: &str, right: &str) -> Vec<u8> {
        format!(
            "menuDef {{ name lefthud rect 0 368 112 112 \
               itemDef {{ name frame background \"{left}\" rect 0 0 112 112 }} }}\n\
             menuDef {{ name righthud rect 640 368 -112 112 \
               itemDef {{ name frame background \"{right}\" rect 0 0 -112 112 }} }}\n"
        )
        .into_bytes()
    }

    /// Retail assets, a JoF assets pack and a radial HUD pack (both replacing
    /// `ui/hud.menu`), and a client folder adding EternalJK's elegance list.
    pub(crate) fn installed() -> VirtualFileSystem {
        let mut vfs = VirtualFileSystem::new();
        let base = "C:/Games/Jedi Academy/GameData/base";
        vfs.mount_memory(
            format!("{base}/assets1.pk3"),
            [
                ("ui/jahud.txt", b"{ loadMenu { \"ui/hud.menu\" } }".to_vec()),
                (
                    "ui/hud.menu",
                    hud_menu("gfx/hud/hudleft", "gfx/hud/hudright"),
                ),
                (
                    "ui/menus.txt",
                    b"{ loadMenu { \"ui/main.menu\" } }".to_vec(),
                ),
            ],
        )
        .unwrap();
        vfs.mount_memory(
            format!("{base}/JoF_AssetsExtra.pk3"),
            [(
                "ui/hud.menu",
                hud_menu("gfx/hud/jof_left", "gfx/hud/jof_right"),
            )],
        )
        .unwrap();
        vfs.mount_memory(
            format!("{base}\\zz_TheRisqe_Radial_HUD.pk3"),
            [("ui/hud.menu", hud_menu("gfx/hud/radial", "gfx/hud/radial"))],
        )
        .unwrap();
        vfs.mount_memory(
            format!("{base}/jofclient-assets.pk3"),
            [
                (
                    "ui/elegance_hud.txt",
                    b"{ loadMenu { \"ui/elegance_hud.menu\" } }".to_vec(),
                ),
                ("ui/elegance_hud.menu", hud_menu("e_left", "e_right")),
                (
                    "ui/broken_hud.txt",
                    b"{ loadMenu { \"ui/none.menu\" } }".to_vec(),
                ),
            ],
        )
        .unwrap();
        vfs
    }

    fn labels(choices: &[HudChoice]) -> Vec<&str> {
        choices.iter().map(|choice| choice.label.as_str()).collect()
    }

    #[test]
    fn every_installed_hud_is_listed_in_install_order() {
        let choices = list(&installed());
        assert_eq!(
            labels(&choices),
            [
                "Jedi Academy",
                "JoF AssetsExtra",
                "TheRisqe Radial HUD",
                "Elegance HUD",
                "Text only",
                "SJK radial",
                "SJK classic",
            ]
        );
        assert_eq!(choices[0].pack, "assets1.pk3");
        assert_eq!(choices[2].pack, "zz_TheRisqe_Radial_HUD.pk3");
        assert_eq!(choices[3].files, "ui/elegance_hud.txt");
        assert!(choices[3].pack.is_empty());
        assert!(choices[..5].iter().all(HudChoice::has_preview));
        assert!(choices[5..].iter().all(|choice| !choice.has_preview()));
    }

    #[test]
    fn the_cvars_find_their_choice() {
        let choices = list(&installed());
        let find = |style, files, pack| current(&choices, style, files, pack);
        // An empty pack is the HUD installed last, as is one no longer installed.
        assert_eq!(find(HudStyle::Game, "ui/jahud.txt", ""), Some(2));
        assert_eq!(find(HudStyle::Game, "0", "gone.pk3"), Some(2));
        assert_eq!(find(HudStyle::Game, "ui/jahud.txt", "ASSETS1.PK3"), Some(0));
        assert_eq!(
            find(HudStyle::Game, "ui/jahud.txt", "JoF_AssetsExtra.pk3"),
            Some(1)
        );
        // EternalJK's 3 is the elegance list; 1 and 2 are the text HUD.
        assert_eq!(find(HudStyle::Game, "3", ""), Some(3));
        assert_eq!(find(HudStyle::Game, "2", ""), Some(4));
        assert_eq!(find(HudStyle::Radial, "1", ""), Some(5));
        assert_eq!(find(HudStyle::Classic, "1", ""), Some(6));
        assert_eq!(find(HudStyle::Game, "ui/other.txt", ""), None);
    }

    #[test]
    fn a_pack_reads_as_if_the_later_packs_were_not_installed() {
        let vfs = installed();
        let menu = |view: &VirtualFileSystem| {
            String::from_utf8(view.read("ui/hud.menu").unwrap().unwrap().bytes).unwrap()
        };
        assert!(menu(&vfs).contains("radial"));
        assert!(menu(&pack_view(&vfs, "assets1.pk3").unwrap()).contains("hudleft"));
        assert!(menu(&pack_view(&vfs, "JoF_AssetsExtra.pk3").unwrap()).contains("jof_left"));
        assert!(pack_view(&vfs, "").is_none());
        assert!(pack_view(&vfs, "missing.pk3").is_none());
    }

    #[test]
    fn labels_drop_load_order_prefixes_and_extensions() {
        assert_eq!(pack_label("assets1.pk3"), "Jedi Academy");
        assert_eq!(pack_label("Assets2.PK3"), "Jedi Academy");
        assert_eq!(pack_label("assets_hd.pk3"), "assets hd");
        assert!(is_retail_archive("assets0.pk3") && !is_retail_archive("assets0"));
        assert!(!is_retail_archive("JoF_HDWideLevelshots.pk3"));
        assert_eq!(pack_label("zzz_my_hud.pk3"), "my hud");
        assert_eq!(pack_label("zelda.pk3"), "zelda");
        assert_eq!(pack_label("zz_.pk3"), "zz");
        assert_eq!(list_label("elegance_hud.txt"), "Elegance HUD");
        assert_eq!(list_label("jof_hud.txt"), "Jof HUD");
    }
}
