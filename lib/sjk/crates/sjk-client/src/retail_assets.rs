//! Which mounted assets are the retail Jedi Academy's own ("OG"), so the menus can
//! list them first and the community's after them (Sol's rule, 12/10/2026).
//!
//! Retail is told by the archive an asset comes from, never by the asset's name: the
//! mounts whose file is in [`RETAIL_ARCHIVES`] and sits in a directory named
//! [`RETAIL_DIRECTORY`]. The view of those mounts alone ([`retail_view`]) is
//! catalogued like the whole set, so an item is OG when the retail archives hold it,
//! even where a community archive replaces some of its files.

use sjk_vfs::{MountId, VirtualFileSystem};

/// Directory the retail archives live in (`GameData/base`).
pub const RETAIL_DIRECTORY: &str = "base";

/// The retail game's archives, with what each holds (the evidence: a stock
/// Steam install of Jedi Academy, whose `base` holds exactly these four).
pub const RETAIL_ARCHIVES: [(&str, &str); 4] = [
    ("assets0.pk3", "maps, textures, shaders, UI and sounds"),
    ("assets1.pk3", "player models, sabers, effects and weapons"),
    ("assets2.pk3", "skins, icons and sounds"),
    ("assets3.pk3", "the patched files of the 1.01 release"),
];

/// Whether the mount named `name` (an archive's path) is one of the retail ones.
pub fn is_retail_mount(name: &str) -> bool {
    // Either separator, whatever the platform that mounted it.
    let mut parts = name.rsplit(['/', '\\']);
    let (Some(file), Some(directory)) = (parts.next(), parts.next()) else {
        return false;
    };
    directory.eq_ignore_ascii_case(RETAIL_DIRECTORY)
        && RETAIL_ARCHIVES
            .iter()
            .any(|(archive, _)| archive.eq_ignore_ascii_case(file))
}

/// `vfs` as if only its retail archives were mounted; `None` when none is.
pub fn retail_view(vfs: &VirtualFileSystem) -> Option<VirtualFileSystem> {
    let others: Vec<MountId> = vfs
        .mounts()
        .filter(|mount| !is_retail_mount(&mount.name))
        .map(|mount| mount.id)
        .collect();
    let view = vfs.without_mounts(&others);
    (view.mounts().count() > 0).then_some(view)
}

/// Stable partition of `items` with the retail ones first; returns how many are.
pub fn retail_first<T>(items: &mut Vec<T>, is_retail: impl Fn(&T) -> bool) -> usize {
    let (mut retail, community): (Vec<T>, Vec<T>) = std::mem::take(items)
        .into_iter()
        .partition(|item| is_retail(item));
    let count = retail.len();
    retail.extend(community);
    *items = retail;
    count
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_the_four_archives_of_base_are_retail() {
        for name in [
            "C:/Games/GameData/base/assets0.pk3",
            "C:\\Games\\GameData\\base\\Assets3.PK3",
        ] {
            assert!(is_retail_mount(name), "{name}");
        }
        for name in [
            "C:/Games/GameData/base/zzz_models.pk3",
            "C:/Games/GameData/japlus/assets0.pk3",
            "C:/Games/GameData/base/assets4.pk3",
            "assets0.pk3",
        ] {
            assert!(!is_retail_mount(name), "{name}");
        }
    }

    #[test]
    fn retail_items_go_first_in_their_order() {
        let mut items = vec![1, 2, 3, 4, 5, 6];
        let count = retail_first(&mut items, |item| item % 2 == 0);
        assert_eq!((count, items), (3, vec![2, 4, 6, 1, 3, 5]));
    }
}
