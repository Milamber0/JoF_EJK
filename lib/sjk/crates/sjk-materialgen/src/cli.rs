//! Command-line parsing and help text.

use crate::generate::Settings;
use crate::mount::{CLIENT_FOLDER, DEFAULT_FILE_NAME, default_output, find_game_data, is_inside};
use crate::overrides::Overrides;
use crate::overrides_file;
use crate::run::{Options, normalize_map};
use std::path::{Path, PathBuf};

/// `--help` output.
pub const USAGE: &str = "\
sjk-materialgen: generate rend2-style material maps for your installed Jedi Academy maps

USAGE:
    sjk-materialgen [OPTIONS]

OPTIONS:
    --game-data DIR   the game's GameData directory (default: JKA_GAME_DATA, the SJK
                      config's fs_gameData, or the usual Steam location)
    --fs-game NAME    also mount GameData/NAME after base, like fs_game
    --maps LIST       only textures of these maps, comma-separated (mp/ffa3,mp/duel1);
                      default: every installed map
    --limit N         only the N most-used textures
    --strength F      multiply every class's normal strength (default 1.0)
    --max-size N      halve textures larger than N texels before generating
                      (default: keep the source resolution)
    --overrides FILE  per-texture class, roughness, metalness, strength, height and
                      emission rules (see the crate documentation); default:
                      sjk-materialgen-overrides.txt next to the output pk3, else in the
                      client's generated folder (one left in an earlier version's folder,
                      %APPDATA%\\jkr\\generated, is copied there); the run says which
    --no-overrides    read no overrides file
    --dry-run         list what would be generated and what is skipped; write nothing
    --out FILE        output pk3 (default: generated/zzz_sjk_materials.pk3 in the client's
                      folder, GameData/SJK or %APPDATA%\\SJK, which SJK mounts above every
                      other pack); never elsewhere in the game installation
    -h, --help        print this help

For each world texture that installed maps draw on lightmapped surfaces, the tool
writes a normal map (<texture>_nh with height for parallax on stone, tiles and
ground, <texture>_n otherwise) and a packed <texture>_rmo map (roughness,
metalness, occlusion) into one pk3, plus sjk-materialgen/manifest.json listing
every source, output, skipped shader and setting. Textures that give light
(q3map_surfacelight, an authored _glow image, light, lamp or screen names) also get
an emission map <texture>_e of their luminous texels, unless their shader already
glows; emission=on|off|<strength> in the overrides file decides per texture.
Shaders with a tcGen environment stage are treated as polished (glossier). Skies, fog, liquids, system and
interface images, effects, glowing, animated and alpha-tested foliage stages, and
textures that already have rend2 maps get none. Game data is only read.

SJK loads a pack in its generated folder by itself, above every game, server and
downloaded pack, and leaves out older generated packs copied into a game folder.
Elsewhere, point the client at the output directory (SJK_CONTENT=<directory>). Enable
r_normalMapping, r_specularMapping and optionally r_parallaxMapping. Emission maps
show with r_emissiveMaps (on by default).

The generated images are derived from your retail textures. Keep them on your
machine: do not share, upload or commit them.
";

/// What the command line asks for.
#[derive(Debug)]
pub enum Command {
    Help,
    /// Boxed: the options are much larger than `Help`.
    Run(Box<Options>),
}

/// Parse `arguments` (without the program name).
pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Command, String> {
    let mut arguments = arguments.into_iter();
    let mut game_data = None;
    let mut fs_game = None;
    let mut maps = None;
    let mut limit = None;
    let mut settings = Settings::default();
    let mut dry_run = false;
    let mut out = None;
    let mut overrides_path = None;
    let mut no_overrides = false;
    while let Some(argument) = arguments.next() {
        let mut value = |name: &str| {
            arguments
                .next()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match argument.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--game-data" => game_data = Some(PathBuf::from(value("--game-data")?)),
            "--fs-game" => fs_game = Some(value("--fs-game")?),
            "--maps" => {
                let list: Vec<String> = value("--maps")?
                    .split(',')
                    .map(str::trim)
                    .filter(|map| !map.is_empty())
                    .map(normalize_map)
                    .collect();
                if list.is_empty() {
                    return Err("--maps needs at least one map name".into());
                }
                maps = Some(list);
            }
            "--limit" => {
                limit = Some(
                    value("--limit")?
                        .parse::<usize>()
                        .map_err(|_| "--limit needs a whole number")?,
                )
            }
            "--strength" => {
                let strength = value("--strength")?
                    .parse::<f32>()
                    .ok()
                    .filter(|s| s.is_finite() && *s > 0.0 && *s <= 10.0)
                    .ok_or("--strength needs a number above 0 and at most 10")?;
                settings.strength = strength;
            }
            "--max-size" => {
                let size = value("--max-size")?
                    .parse::<u32>()
                    .ok()
                    .filter(|size| *size >= 16)
                    .ok_or("--max-size needs a whole number of at least 16")?;
                settings.max_size = Some(size);
            }
            "--overrides" => overrides_path = Some(PathBuf::from(value("--overrides")?)),
            "--no-overrides" => no_overrides = true,
            "--dry-run" => dry_run = true,
            "--out" => out = Some(PathBuf::from(value("--out")?)),
            other => return Err(format!("unknown argument {other:?}; see --help")),
        }
    }
    let game_data = find_game_data(game_data.as_deref())?;
    let out = match out {
        Some(out) if out.is_dir() => out.join(DEFAULT_FILE_NAME),
        Some(out) => out,
        None => default_output(&game_data)?,
    };
    check_output(&out, &game_data)?;
    let found = overrides_file::find(
        overrides_path.as_deref(),
        no_overrides,
        &out,
        overrides_file::canonical(&game_data).as_deref(),
        &overrides_file::legacy(),
        !dry_run,
    )?;
    let overrides = match found.path() {
        Some(path) => {
            let text = std::fs::read_to_string(path)
                .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
            Overrides::parse(&text).map_err(|error| format!("{}: {error}", path.display()))?
        }
        None => Overrides::default(),
    };
    Ok(Command::Run(Box::new(Options {
        game_data,
        fs_game,
        maps,
        limit,
        settings,
        overrides,
        overrides_found: found,
        dry_run,
        out,
    })))
}

/// The overrides file read by default ([`overrides_file`]).
pub const OVERRIDES_FILE_NAME: &str = overrides_file::FILE_NAME;

/// The output must be a `.pk3` outside the game installation, or in the client's
/// own folder there (`GameData/SJK`, where the default output goes).
fn check_output(out: &Path, game_data: &Path) -> Result<(), String> {
    let is_pk3 = out
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("pk3"));
    if !is_pk3 {
        return Err(format!(
            "--out must name a .pk3 file, not {}",
            out.display()
        ));
    }
    let install = game_data.parent().unwrap_or(game_data);
    let client = game_data.join(CLIENT_FOLDER);
    if !is_inside(out, &client) && (is_inside(out, game_data) || is_inside(out, install)) {
        return Err(format!(
            "refusing to write into the game installation ({}) outside {}: leave --out \
             out to write where SJK loads it",
            install.display(),
            client.display()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_must_be_a_pk3_outside_the_game() {
        let root = std::env::temp_dir().join("sjk-materialgen-cli");
        let game_data = root.join("Jedi Academy/GameData");
        assert!(check_output(&root.join("out/x.pk3"), &game_data).is_ok());
        assert!(check_output(&root.join("out/x.zip"), &game_data).is_err());
        assert!(check_output(&game_data.join("base/zzz.pk3"), &game_data).is_err());
        assert!(check_output(&game_data.join("SJK/generated/zzz.pk3"), &game_data).is_ok());
        assert!(check_output(&root.join("Jedi Academy/zzz.pk3"), &game_data).is_err());
    }

    #[test]
    fn help_and_bad_arguments() {
        assert!(matches!(parse(["--help".to_owned()]), Ok(Command::Help)));
        assert!(parse(["--bogus".to_owned()]).is_err());
        assert!(parse(["--strength".to_owned(), "0".to_owned()]).is_err());
        assert!(parse(["--limit".to_owned()]).is_err());
    }

    #[test]
    fn usage_carries_the_notice() {
        assert!(USAGE.contains("do not share, upload or commit them"));
    }
}
