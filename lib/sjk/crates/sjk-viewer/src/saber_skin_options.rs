//! The local player's choice of a blade skin's options (`docs/unlockables.md`, "Options"):
//! the parts of a skin its wearer switched off, every one on until then. They are kept in
//! [`CVAR`], one `<skin>.<option>` word for each part switched off, so a skin keeps its
//! choice while another is worn; the look sent to the hub carries the worn skin's
//! (`saber_off`), and every other SJK player draws the skin that way.
//!
//! In a frame a choice is a set of 64 bits ([`OptionsOff`]), one per option id
//! ([`crate::blade_skin_file::extras::option_bit`]), so a look stays `Copy` and a skin's
//! colour is found without allocating.

use crate::blade_skin_file::extras::option_bit;

/// Archived: the blade skins' options switched off, `<skin id>.<option id>` words.
pub(crate) const CVAR: &str = "cg_saberSkinOptions";

/// A skin's options switched off, a bit per option id; empty with every part on.
pub(crate) type OptionsOff = u64;

/// The words of `setting` that name an option of `skin` switched off, the option ids.
pub(crate) fn off_ids<'a>(setting: &'a str, skin: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    setting.split_whitespace().filter_map(move |word| {
        let (named, option) = word.split_once('.')?;
        (named == skin && !option.is_empty()).then_some(option)
    })
}

/// The options of `skin` switched off in `setting`, as the look sends them: sorted, each
/// once, at most eight (the hub's limit).
pub(crate) fn off_list(setting: &str, skin: &str) -> Vec<String> {
    let mut ids: Vec<String> = off_ids(setting, skin).map(str::to_owned).collect();
    ids.sort();
    ids.dedup();
    ids.truncate(8);
    ids
}

/// The bits of the option ids `ids`.
pub(crate) fn bits<'a>(ids: impl IntoIterator<Item = &'a str>) -> OptionsOff {
    ids.into_iter().fold(0, |bits, id| bits | option_bit(id))
}

/// The options of `skin` switched off in `setting`, as bits.
pub(crate) fn off_bits(setting: &str, skin: &str) -> OptionsOff {
    bits(off_ids(setting, skin))
}

/// Whether `skin`'s option `option` is on in `setting`.
pub(crate) fn is_on(setting: &str, skin: &str, option: &str) -> bool {
    !off_ids(setting, skin).any(|id| id == option)
}

/// `setting` with `skin`'s option `option` switched on or off: its word gone, or there
/// once; the words sorted, so the same choice is always the same text.
pub(crate) fn with(setting: &str, skin: &str, option: &str, on: bool) -> String {
    let word = format!("{skin}.{option}");
    let mut words: Vec<&str> = setting
        .split_whitespace()
        .filter(|other| *other != word)
        .collect();
    if !on {
        words.push(&word);
    }
    words.sort_unstable();
    words.dedup();
    words.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_setting_names_each_skins_parts_switched_off() {
        let setting = "saber_sun.haze saber_spectral.wisps saber_sun.glint junk .x y.";
        assert_eq!(off_list(setting, "saber_sun"), ["glint", "haze"]);
        assert_eq!(off_list(setting, "saber_spectral"), ["wisps"]);
        assert!(off_list(setting, "saber_void").is_empty());
        assert!(is_on(setting, "saber_sun", "prominences"));
        assert!(!is_on(setting, "saber_sun", "haze"));
        assert_eq!(
            off_bits(setting, "saber_sun"),
            option_bit("glint") | option_bit("haze")
        );
        assert_eq!(off_bits("", "saber_sun"), 0);
    }

    #[test]
    fn switching_an_option_keeps_one_sorted_word_for_it() {
        let off = with("", "saber_sun", "haze", false);
        assert_eq!(off, "saber_sun.haze");
        let both = with(&off, "saber_spectral", "wisps", false);
        assert_eq!(both, "saber_spectral.wisps saber_sun.haze");
        assert_eq!(with(&both, "saber_sun", "haze", false), both, "already off");
        assert_eq!(
            with(&both, "saber_sun", "haze", true),
            "saber_spectral.wisps"
        );
        assert_eq!(with("saber_sun.glint", "saber_sun", "glint", true), "");
    }

    #[test]
    fn the_look_sends_at_most_eight_once_each() {
        let setting: String = (0..12)
            .map(|n| format!("saber_sun.o{n:02} saber_sun.o{n:02} "))
            .collect();
        let list = off_list(&setting, "saber_sun");
        assert_eq!(list.len(), 8);
        assert_eq!(list[0], "o00");
    }
}
