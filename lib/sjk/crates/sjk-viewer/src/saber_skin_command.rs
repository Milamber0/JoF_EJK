//! The `saberskin` console command (`docs/unlockables.md`, "Settings and commands"):
//! alone it lists the catalogue's blade skins ([`crate::unlockables`]), each owned or
//! locked, and which one is worn; `saberskin <id>` or `saberskin none` sets
//! `cg_saberSkin`. A locked skin is set all the same and shows once the player's own hub
//! profile lists it (the gate is `GpuState::local_saber_skin`'s). `saberskin <id> parts`
//! lists the parts a skin's file lets its wearer switch off, and `saberskin <id> <part>
//! on|off` switches one (`cg_saberSkinOptions`, [`crate::saber_skin_options`]).

use crate::console::ViewerConsole;
use crate::unlockables::{self, Holdings, SABER_SKIN_CVAR, Unlockable};

/// Console command name.
pub(crate) const COMMAND: &str = "saberskin";
/// Help text for completion and `cmdlist`.
pub(crate) const HELP: &str = "List your blade skins, wear one or switch its parts: saberskin <id> | none | <id> parts | <id> <part> on|off";

const USAGE: &str = "saberskin [<id> | none | <id> parts | <id> <part> on|off]";

/// The parts skin `skin` offers (option id and name), and the lines `saberskin <id> parts`
/// prints from them with `setting` (`cg_saberSkinOptions`).
pub(crate) fn parts_listing(
    skin: &Unlockable,
    parts: &[(String, String)],
    setting: &str,
) -> Vec<String> {
    if parts.is_empty() {
        return vec![format!(
            "The {} has no parts to switch (or its pack is not loaded).",
            skin.name
        )];
    }
    let mut lines = vec![format!(
        "{} parts (saberskin {} <part> on|off):",
        skin.name, skin.id
    )];
    for (id, name) in parts {
        let on = crate::saber_skin_options::is_on(setting, skin.id, id);
        lines.push(format!(
            "  {id}  {name}  {}",
            if on { "^2on^7" } else { "^3off^7" }
        ));
    }
    lines
}

/// `saberskin <id> <part> on|off`: the new `cg_saberSkinOptions` and the answer, or why
/// not.
pub(crate) fn switch_part(
    skin: &Unlockable,
    parts: &[(String, String)],
    setting: &str,
    part: &str,
    state: &str,
) -> Result<(String, String), String> {
    let on = match state.to_ascii_lowercase().as_str() {
        "on" | "1" => true,
        "off" | "0" => false,
        _ => return Err(format!("usage: {USAGE}")),
    };
    let part = part.to_ascii_lowercase();
    let Some((id, name)) = parts.iter().find(|(id, _)| *id == part) else {
        return Err(format!(
            "The {} has no part \"{part}\": saberskin {} parts lists them",
            skin.name, skin.id
        ));
    };
    let setting = crate::saber_skin_options::with(setting, skin.id, id, on);
    let answer = format!(
        "{} of the {} {}.",
        name,
        skin.name,
        if on { "on" } else { "off" }
    );
    Ok((setting, answer))
}

/// What `saberskin <word>` asks to wear.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Choice {
    /// The stock blade (`none`).
    Stock,
    Skin(&'static Unlockable),
}

impl Choice {
    /// The value `cg_saberSkin` takes.
    fn setting(self) -> &'static str {
        match self {
            Self::Stock => "",
            Self::Skin(unlockable) => unlockable.id,
        }
    }
}

/// Read the command's word.
pub(crate) fn parse(word: &str) -> Result<Choice, String> {
    if word.eq_ignore_ascii_case("none") {
        return Ok(Choice::Stock);
    }
    unlockables::blade_skin(word)
        .map(Choice::Skin)
        .ok_or_else(|| {
            format!("No blade skin \"{word}\": saberskin alone lists them (usage: {USAGE})")
        })
}

/// The lines `saberskin` alone prints: every blade skin with whether it is owned (and
/// since when) or locked (and how to get it), the worn one marked, and why ownership is
/// unknown when it is.
pub(crate) fn listing(setting: &str, holdings: Holdings<'_>) -> Vec<String> {
    let chosen = unlockables::blade_skin(setting);
    let mut lines =
        vec!["Blade skins (saberskin <id> wears one, saberskin none the stock blade):".to_owned()];
    for unlockable in unlockables::blade_skins() {
        let picked = chosen.is_some_and(|chosen| chosen.id == unlockable.id);
        let state = match holdings.unlock(unlockable.id) {
            Some(unlock) => {
                let since = crate::medals::date_text(unlock.granted);
                let owned = if since.is_empty() {
                    "^2owned".to_owned()
                } else {
                    format!("^2owned since {since}")
                };
                if picked {
                    format!("{owned}^7, worn")
                } else {
                    format!("{owned}^7")
                }
            }
            None if picked => format!(
                "^3locked^7, chosen: shows once unlocked ({})",
                unlockable.how_to_get
            ),
            None => format!("^3locked^7: {}", unlockable.how_to_get),
        };
        lines.push(format!("  {}  {}  {state}", unlockable.id, unlockable.name));
    }
    let setting = setting.trim();
    if chosen.is_none() && !setting.is_empty() {
        lines.push(format!(
            "cg_saberSkin names \"{setting}\", which this client does not know: the stock blade shows."
        ));
    }
    if let Some(reason) = holdings.reason() {
        lines.push(format!("^3{reason}"));
    }
    lines
}

/// What `saberskin <word>` answers once `cg_saberSkin` is set to `choice`.
pub(crate) fn answer(choice: Choice, holdings: Holdings<'_>) -> String {
    match choice {
        Choice::Stock => "You wear the stock blade.".to_owned(),
        Choice::Skin(unlockable) if holdings.unlock(unlockable.id).is_some() => {
            format!("You wear the {}.", unlockable.name)
        }
        Choice::Skin(unlockable) => match holdings.reason() {
            Some(reason) => format!(
                "Chose the {}; it shows only once your SJK profile holds it. {reason}",
                unlockable.name
            ),
            None => format!(
                "Chose the {}; it shows only once unlocked ({})",
                unlockable.name, unlockable.how_to_get
            ),
        },
    }
}

/// Run `saberskin` with `args` on `console`, `snapshot` being the identity's; `parts_of`
/// gives a skin's parts (option id and name) as its loaded file offers them.
pub(crate) fn run(
    console: &mut ViewerConsole,
    args: &[String],
    snapshot: Option<&sjk_identity::Snapshot>,
    parts_of: impl Fn(&str) -> Vec<(String, String)>,
) -> Result<Vec<String>, String> {
    let enabled = console.bool_cvar("cl_identity") == Some(true);
    let holdings = Holdings::of(enabled, snapshot);
    match args {
        [] => Ok(listing(
            console.text_cvar(SABER_SKIN_CVAR).unwrap_or_default(),
            holdings,
        )),
        [word] => {
            let choice = parse(word)?;
            if !console.set_cvar(SABER_SKIN_CVAR, choice.setting()) {
                return Err("cg_saberSkin could not be set".to_owned());
            }
            Ok(vec![answer(choice, holdings)])
        }
        [word, parts] if parts.eq_ignore_ascii_case("parts") => {
            let Choice::Skin(skin) = parse(word)? else {
                return Err(format!("usage: {USAGE}"));
            };
            let setting = console
                .text_cvar(crate::saber_skin_options::CVAR)
                .unwrap_or_default();
            Ok(parts_listing(skin, &parts_of(skin.id), setting))
        }
        [word, part, state] => {
            let Choice::Skin(skin) = parse(word)? else {
                return Err(format!("usage: {USAGE}"));
            };
            let setting = console
                .text_cvar(crate::saber_skin_options::CVAR)
                .unwrap_or_default()
                .to_owned();
            let (setting, answer) = switch_part(skin, &parts_of(skin.id), &setting, part, state)?;
            if !console.set_cvar(crate::saber_skin_options::CVAR, &setting) {
                return Err("cg_saberSkinOptions could not be set".to_owned());
            }
            Ok(vec![answer])
        }
        _ => Err(format!("usage: {USAGE}")),
    }
}

impl crate::GpuState {
    /// `saberskin [<id> | none]`.
    pub(crate) fn saber_skin_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let wanted = self
            .console
            .as_ref()
            .is_some_and(crate::debug_collectibles::wanted);
        let snapshot = crate::player_identity::collection_snapshot(wanted);
        let skins = &self.blade_skins;
        let parts_of = |id: &str| {
            skins.get(id).map_or_else(Vec::new, |skin| {
                skin.options()
                    .iter()
                    .map(|option| (option.id.clone(), option.name.clone()))
                    .collect()
            })
        };
        let Some(console) = self.console.as_mut() else {
            return Err("The console is not ready".to_owned());
        };
        run(console, args, snapshot.as_ref(), parts_of)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::Unlock;

    fn words(text: &str) -> Vec<String> {
        text.split_whitespace().map(str::to_owned).collect()
    }

    fn sun() -> Unlock {
        Unlock {
            id: "saber_sun".into(),
            granted: 1_791_336_225,
            note: String::new(),
            count: 1,
            medal: None,
        }
    }

    #[test]
    fn the_listing_says_owned_locked_and_worn() {
        let owned = [sun()];
        let lines = listing("saber_sun", Holdings::Known(&owned));
        // A line, then one per blade skin of the catalogue.
        let skins = crate::unlockables::blade_skins().count();
        assert_eq!(lines.len(), 1 + skins);
        assert!(lines[1].contains("saber_sun  Sun blade"), "{}", lines[1]);
        assert!(
            lines[2].contains("saber_storm  Storm blade"),
            "{}",
            lines[2]
        );
        assert!(lines[2].contains("locked"), "{}", lines[2]);
        assert!(
            lines[1].contains("owned since 07/10/2026^7, worn"),
            "{}",
            lines[1]
        );
        let lines = listing("", Holdings::Known(&owned));
        assert!(!lines[1].contains("worn"));
        let lines = listing(" SABER_SUN ", Holdings::Known(&[]));
        assert!(
            lines[1].contains("locked^7, chosen: shows once unlocked (Given by the SJK team.)"),
            "{}",
            lines[1]
        );
        let lines = listing("saber_moon", Holdings::IdentityOff);
        assert!(lines[1].contains("locked^7: Given by the SJK team."));
        assert!(lines[1 + skins].contains("\"saber_moon\", which this client does not know"));
        assert!(
            lines[2 + skins].contains("need the SJK identity"),
            "{}",
            lines[2 + skins]
        );
    }

    #[test]
    fn words_choose_a_skin_or_the_stock_blade() {
        assert_eq!(parse("none"), Ok(Choice::Stock));
        assert_eq!(parse("NONE"), Ok(Choice::Stock));
        assert_eq!(
            parse("Saber_Sun").map(Choice::setting),
            Ok("saber_sun"),
            "the catalogue's spelling is kept"
        );
        assert!(
            parse("saber_moon")
                .unwrap_err()
                .contains("saberskin alone lists them")
        );
        let owned = [sun()];
        let sun = parse("saber_sun").unwrap();
        assert_eq!(
            answer(sun, Holdings::Known(&owned)),
            "You wear the Sun blade."
        );
        assert!(answer(sun, Holdings::Known(&[])).contains("shows only once unlocked"));
        assert!(answer(sun, Holdings::IdentityOff).contains("need the SJK identity"));
        assert_eq!(
            answer(Choice::Stock, Holdings::IdentityOff),
            "You wear the stock blade."
        );
    }

    #[test]
    fn the_command_sets_cg_saber_skin_even_for_a_locked_skin() {
        let directory = tempfile::tempdir().unwrap();
        let mut console =
            ViewerConsole::new(directory.path().join("config.cfg")).expect("a console");
        let lines = run(&mut console, &words("saber_SUN"), None, no_parts).unwrap();
        assert_eq!(console.text_cvar(SABER_SKIN_CVAR).unwrap(), "saber_sun");
        assert!(lines[0].contains("only once your SJK profile holds it"));
        let lines = run(&mut console, &[], None, no_parts).unwrap();
        assert!(lines[1].contains("chosen"));
        run(&mut console, &words("none"), None, no_parts).unwrap();
        assert_eq!(console.text_cvar(SABER_SKIN_CVAR).unwrap(), "");
        assert!(run(&mut console, &words("saber_moon"), None, no_parts).is_err());
        assert_eq!(console.text_cvar(SABER_SKIN_CVAR).unwrap(), "");
        assert!(run(&mut console, &words("a b"), None, no_parts).is_err());
    }

    fn no_parts(_: &str) -> Vec<(String, String)> {
        Vec::new()
    }

    fn sun_parts(id: &str) -> Vec<(String, String)> {
        if id != "saber_sun" {
            return Vec::new();
        }
        [("haze", "Heat haze"), ("glint", "Tip glint")]
            .map(|(id, name)| (id.to_owned(), name.to_owned()))
            .to_vec()
    }

    #[test]
    fn the_command_lists_and_switches_a_skins_parts() {
        let directory = tempfile::tempdir().unwrap();
        let mut console =
            ViewerConsole::new(directory.path().join("config.cfg")).expect("a console");
        let lines = run(&mut console, &words("saber_sun parts"), None, sun_parts).unwrap();
        assert_eq!(lines.len(), 3);
        assert!(
            lines[1].contains("haze  Heat haze") && lines[1].contains("on"),
            "{}",
            lines[1]
        );
        let lines = run(&mut console, &words("saber_sun HAZE off"), None, sun_parts).unwrap();
        assert_eq!(lines, ["Heat haze of the Sun blade off."]);
        let options = crate::saber_skin_options::CVAR;
        assert_eq!(console.text_cvar(options).unwrap(), "saber_sun.haze");
        let lines = run(&mut console, &words("saber_sun parts"), None, sun_parts).unwrap();
        assert!(lines[1].contains("off"), "{}", lines[1]);
        run(&mut console, &words("saber_sun haze on"), None, sun_parts).unwrap();
        assert_eq!(console.text_cvar(options).unwrap(), "");
        // An unknown part, a bad state, the stock blade: refused, nothing set.
        assert!(run(&mut console, &words("saber_sun wings off"), None, sun_parts).is_err());
        assert!(
            run(
                &mut console,
                &words("saber_sun haze maybe"),
                None,
                sun_parts
            )
            .is_err()
        );
        assert!(run(&mut console, &words("none haze off"), None, sun_parts).is_err());
        assert_eq!(console.text_cvar(options).unwrap(), "");
        // A skin without parts says so.
        let lines = run(&mut console, &words("saber_void parts"), None, sun_parts).unwrap();
        assert!(lines[0].contains("no parts"), "{}", lines[0]);
    }
}
