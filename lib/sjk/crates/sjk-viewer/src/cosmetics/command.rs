//! JoF EJK's `cosmetics` console command (`CG_Cosmetics_f`): list the
//! installed hats or capes, wear or take one off by number or name, take
//! both off, and set whose cosmetics are drawn.

use super::{Catalog, VISIBILITY_CVAR, Visibility};
use crate::console::ViewerConsole;
use sjk_client::CosmeticSlot;

/// Completion and help entry.
pub(crate) const COMMANDS: &[(&str, &str)] = &[(
    "cosmetics",
    "List, wear or take off hats and capes: cosmetics <hats|capes|clear|visibility> [value]",
)];

const USAGE: [&str; 5] = [
    "Usage: ^3cosmetics <hats|capes|clear|visibility> [value]^7",
    "  ^3hats^7 [num|name]    list hats, or wear one (again takes it off)",
    "  ^3capes^7 [num|name]   list capes, or wear one (again takes it off)",
    "  ^3clear^7              take off the hat and the cape",
    "  ^3visibility^7 [off|on|onlyme]   whose cosmetics are drawn",
];

/// Run `cosmetics` with `args` against the installed `catalog`.
pub(crate) fn run(
    console: &mut ViewerConsole,
    catalog: &Catalog,
    args: &[String],
) -> Result<Vec<String>, String> {
    let Some(category) = args.first() else {
        return Ok(USAGE.map(str::to_owned).to_vec());
    };
    match category.to_ascii_lowercase().as_str() {
        "hats" => wear(console, catalog, CosmeticSlot::Hat, args.get(1)),
        "capes" => wear(console, catalog, CosmeticSlot::Cape, args.get(1)),
        "clear" => {
            for slot in CosmeticSlot::ALL {
                set(console, slot, None);
            }
            Ok(vec!["Hat and cape removed.".to_owned()])
        }
        "visibility" => visibility(console, args.get(1)),
        _ => Err(format!(
            "Unknown category '{category}'. Run ^3cosmetics^7 for usage."
        )),
    }
}

fn wear(
    console: &mut ViewerConsole,
    catalog: &Catalog,
    slot: CosmeticSlot,
    argument: Option<&String>,
) -> Result<Vec<String>, String> {
    let category = match slot {
        CosmeticSlot::Hat => "hats",
        CosmeticSlot::Cape => "capes",
    };
    let pieces = catalog.pieces(slot);
    if pieces.is_empty() {
        return Ok(vec![format!(
            "No {category} installed. Install JoF EJK's cosmetics (GameData/EternalJK) or put a \
             pack with models/cosmetics/{category}/ in base, then restart."
        )]);
    }
    let worn = worn(console, slot);
    let Some(argument) = argument else {
        let mut lines = vec![format!("^5Available {category}:")];
        for (index, piece) in pieces.iter().enumerate() {
            let mark = if worn
                .as_deref()
                .is_some_and(|worn| worn.eq_ignore_ascii_case(&piece.name))
            {
                "^2[X]^7"
            } else {
                "[ ]"
            };
            lines.push(format!("{index:2} {mark} {}", piece.name));
        }
        lines.push(format!(
            "Wear one with ^3cosmetics {category} <num>^7, take it off with the same command."
        ));
        if catalog.has_missing(slot) {
            lines.push(format!(
                "^3Get more {category} from JoF Launcher or Cloud.^7"
            ));
        }
        return Ok(lines);
    };
    // A name as well as a number (`CG_CosmeticForName`).
    let index = match catalog.position(slot, argument) {
        Some(index) => index,
        None => match argument.parse::<usize>() {
            Ok(index) if index < pieces.len() => index,
            Ok(index) => {
                return Err(format!(
                    "cosmetics {category}: Invalid range: {index} [0, {}]",
                    pieces.len() - 1
                ));
            }
            Err(_) => {
                return Err(format!(
                    "No such {category}: '{argument}'. Run ^3cosmetics {category}^7 to see the list."
                ));
            }
        },
    };
    let name = &pieces[index].name;
    if worn
        .as_deref()
        .is_some_and(|worn| worn.eq_ignore_ascii_case(name))
    {
        set(console, slot, None);
        Ok(vec![format!("^3'{name}' ^1removed")])
    } else {
        set(console, slot, Some(name));
        Ok(vec![format!("^3'{name}' ^2equipped")])
    }
}

fn visibility(
    console: &mut ViewerConsole,
    argument: Option<&String>,
) -> Result<Vec<String>, String> {
    let current = Visibility::from_cvar(console.integer_cvar(VISIBILITY_CVAR).unwrap_or(1));
    let Some(argument) = argument else {
        return Ok(vec![
            format!("Cosmetics visibility: ^3{}^7", current.label()),
            "Set it with ^3cosmetics visibility <off|on|onlyme>^7.".to_owned(),
        ]);
    };
    let next = match argument.to_ascii_lowercase().as_str() {
        "off" | "0" => Visibility::Off,
        "on" | "1" => Visibility::On,
        "onlyme" | "me" | "2" => Visibility::OnlyMe,
        _ => return Err("Use ^3cosmetics visibility <off|on|onlyme>^7.".to_owned()),
    };
    console.set_cvar(VISIBILITY_CVAR, next.cvar_value());
    Ok(vec![format!("Cosmetics visibility: ^3{}^7", next.label())])
}

fn worn(console: &ViewerConsole, slot: CosmeticSlot) -> Option<String> {
    let value = console.text_value(slot.cvar())?;
    sjk_client::split_color_value(value).1.map(str::to_owned)
}

fn set(console: &mut ViewerConsole, slot: CosmeticSlot, name: Option<&str>) {
    let colour = console
        .text_value(slot.cvar())
        .map_or(4, |value| sjk_client::split_color_value(value).0);
    console.set_cvar(slot.cvar(), &sjk_client::join_color_value(colour, name));
}

impl crate::GpuState {
    /// `cosmetics ...`, listing what the mounted game data holds.
    pub(crate) fn cosmetics_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let vfs = self.vfs.clone().ok_or("No game data mounted")?;
        let catalog = Catalog::scan(&vfs);
        let console = self.console.as_mut().ok_or("Console unavailable")?;
        run(console, &catalog, args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_vfs::VirtualFileSystem;

    fn catalog() -> Catalog {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "cosmetics",
            [
                ("models/cosmetics/hats/santahat.md3", b"x".to_vec()),
                ("models/cosmetics/hats/tophat.md3", b"x".to_vec()),
                ("models/cosmetics/capes/royalcape.md3", b"x".to_vec()),
            ],
        )
        .unwrap();
        Catalog::scan(&vfs)
    }

    #[test]
    fn wearing_by_number_or_name_keeps_the_saber_colour() {
        let directory = tempfile::tempdir().unwrap();
        let mut console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        let catalog = catalog();
        console.set_cvar("color1", "3");
        let args = |list: &[&str]| list.iter().map(|arg| (*arg).to_owned()).collect::<Vec<_>>();
        run(&mut console, &catalog, &args(&["hats", "1"])).unwrap();
        assert_eq!(console.text_value("color1"), Some("3tophat"));
        run(&mut console, &catalog, &args(&["hats", "SantaHat"])).unwrap();
        assert_eq!(console.text_value("color1"), Some("3santahat"));
        // The same piece again takes it off.
        run(&mut console, &catalog, &args(&["hats", "santahat"])).unwrap();
        assert_eq!(console.text_value("color1"), Some("3"));
        run(&mut console, &catalog, &args(&["capes", "0"])).unwrap();
        assert_eq!(console.text_value("color2"), Some("4royalcape"));
        let listed = run(&mut console, &catalog, &args(&["capes"])).unwrap();
        assert!(listed.iter().any(|line| line.contains("^2[X]^7 royalcape")));
        run(&mut console, &catalog, &args(&["clear"])).unwrap();
        assert_eq!(console.text_value("color2"), Some("4"));
        assert!(run(&mut console, &catalog, &args(&["hats", "9"])).is_err());
        assert!(run(&mut console, &catalog, &args(&["hats", "crown"])).is_err());
        run(&mut console, &catalog, &args(&["visibility", "onlyme"])).unwrap();
        assert_eq!(console.integer_cvar(VISIBILITY_CVAR), Some(2));
    }

    #[test]
    fn the_catalogue_lists_installed_pieces_and_knows_what_is_missing() {
        let catalog = catalog();
        let hats: Vec<&str> = catalog
            .pieces(CosmeticSlot::Hat)
            .iter()
            .map(|piece| piece.name.as_str())
            .collect();
        assert_eq!(hats, ["santahat", "tophat"]);
        assert!(catalog.has_missing(CosmeticSlot::Hat));
        assert_eq!(catalog.position(CosmeticSlot::Cape, "RoyalCape"), Some(0));
        // Older packs' folder counts when the new one is empty.
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "legacy",
            [("models/players/hats/fedora.md3", b"x".to_vec())],
        )
        .unwrap();
        let legacy = Catalog::scan(&vfs);
        assert_eq!(legacy.pieces(CosmeticSlot::Hat)[0].name, "fedora");
        assert_eq!(
            super::super::model_path(&vfs, CosmeticSlot::Hat, "fedora").as_deref(),
            Some("models/players/hats/fedora.md3")
        );
    }
}
