//! Which skin a player appearance wears, chosen as the retail cgame chooses it
//! (`CG_RegisterClientModelname`, `codemp/cgame/cg_players.c`).
//!
//! A skin never makes a model fall back to Kyle there: when the requested skin
//! gives no skin handle (a file or part is missing, or it names no surface),
//! the cgame registers `model_default.skin` instead, and when that gives none
//! either the model draws the shaders its own surfaces name.

use sjk_model::Skin;
use sjk_vfs::VirtualFileSystem;
use std::error::Error;

/// The skin `directory` (`models/players/<name>`) wears for `variant`.
pub(crate) fn resolve(
    vfs: &VirtualFileSystem,
    directory: &str,
    variant: &str,
) -> Result<Skin, Box<dyn Error>> {
    if let Some(skin) = requested(vfs, directory, variant)? {
        return Ok(skin);
    }
    let fallback = read(vfs, &format!("{directory}/model_default.skin"))?
        .map(|bytes| Skin::parse(&bytes))
        .filter(|skin| !skin.is_empty());
    // `default` itself falls through to the surfaces' own shaders silently, as
    // machines such as the retail sentry ship no default skin.
    if variant != "default" {
        crate::log::progress(format_args!(
            "player skin {directory}/{variant} gives no skin; using {}",
            if fallback.is_some() {
                "model_default.skin"
            } else {
                "the model's own shaders"
            }
        ));
    }
    Ok(fallback.unwrap_or_default())
}

/// `RE_RegisterSkin` for the name the cgame builds; `None` stands for skin handle 0.
fn requested(
    vfs: &VirtualFileSystem,
    directory: &str,
    variant: &str,
) -> Result<Option<Skin>, Box<dyn Error>> {
    // The cgame takes a name as three parts only when it says which ones they are.
    let three_part = variant.contains('|')
        && variant.contains("head")
        && variant.contains("torso")
        && variant.contains("lower");
    if !three_part {
        let mut bytes = read(vfs, &format!("{directory}/model_{variant}.skin"))?;
        // BG_ValidateSkinForTeam (bg_misc.c:2687-2770) tries a custom team
        // suffix first, then the ordinary team skin if it is absent.
        if bytes.is_none() {
            let team = variant
                .strip_suffix("_red")
                .map(|_| "red")
                .or_else(|| variant.strip_suffix("_blue").map(|_| "blue"));
            if let Some(team) = team {
                bytes = read(vfs, &format!("{directory}/model_{team}.skin"))?;
            }
        }
        return Ok(bytes
            .map(|bytes| Skin::parse(&bytes))
            .filter(|skin| !skin.is_empty()));
    }
    // RE_SplitSkins: head and torso end at a `|`; the lower part is the rest.
    let parts = variant.splitn(3, '|').collect::<Vec<_>>();
    if parts.len() != 3 {
        return Ok(None);
    }
    let mut skin = Skin::default();
    for (index, part) in parts.iter().enumerate() {
        // A part named like an earlier one is not read twice.
        if parts[..index].contains(part) {
            continue;
        }
        // RE_RegisterIndividualSkin returns 0 for a missing part, and for a
        // skin that still names no surface; the remaining parts are skipped.
        let Some(bytes) = read(vfs, &format!("{directory}/{part}.skin"))? else {
            return Ok(None);
        };
        skin.append(&bytes);
        if skin.is_empty() {
            return Ok(None);
        }
    }
    Ok(Some(skin))
}

fn read(vfs: &VirtualFileSystem, path: &str) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
    Ok(vfs.read(path)?.map(|asset| asset.bytes))
}

#[cfg(test)]
mod tests {
    use super::resolve;
    use sjk_vfs::VirtualFileSystem;

    const MODEL: &str = "models/players/custom";

    fn vfs(files: &[(&str, &str)]) -> VirtualFileSystem {
        let mut vfs = VirtualFileSystem::new();
        vfs.mount_memory(
            "test",
            files
                .iter()
                .map(|(name, text)| (format!("{MODEL}/{name}"), text.as_bytes().to_vec())),
        )
        .expect("mount");
        vfs
    }

    fn shader(vfs: &VirtualFileSystem, variant: &str, surface: &str) -> Option<String> {
        resolve(vfs, MODEL, variant)
            .expect("resolve")
            .shader(surface)
            .map(str::to_owned)
    }

    #[test]
    fn a_named_skin_is_used() {
        let vfs = vfs(&[
            ("model_default.skin", "torso,plain"),
            ("model_blue.skin", "torso,blue"),
        ]);
        assert_eq!(shader(&vfs, "blue", "torso").as_deref(), Some("blue"));
    }

    #[test]
    fn a_missing_skin_falls_back_to_the_default_skin() {
        let vfs = vfs(&[("model_default.skin", "torso,plain")]);
        assert_eq!(shader(&vfs, "original", "torso").as_deref(), Some("plain"));
    }

    #[test]
    fn a_skin_naming_no_surface_falls_back_to_the_default_skin() {
        let vfs = vfs(&[
            ("model_default.skin", "torso,plain"),
            ("model_empty.skin", "// nothing yet\n"),
        ]);
        assert_eq!(shader(&vfs, "empty", "torso").as_deref(), Some("plain"));
    }

    #[test]
    fn without_any_skin_the_model_draws_its_own_shaders() {
        let vfs = vfs(&[]);
        assert!(resolve(&vfs, MODEL, "rgb").expect("resolve").is_empty());
        assert!(resolve(&vfs, MODEL, "default").expect("resolve").is_empty());
    }

    #[test]
    fn a_missing_team_skin_uses_the_plain_team_skin() {
        let vfs = vfs(&[
            ("model_default.skin", "torso,plain"),
            ("model_red.skin", "torso,red"),
        ]);
        assert_eq!(shader(&vfs, "custom_red", "torso").as_deref(), Some("red"));
    }

    #[test]
    fn three_parts_combine_with_the_head_first() {
        let vfs = vfs(&[
            ("head_b1.skin", "head,face\ntorso,from_head"),
            ("torso_a1.skin", "torso,body"),
            ("lower_a1.skin", "There's nothing here!"),
        ]);
        let skin = resolve(&vfs, MODEL, "head_b1|torso_a1|lower_a1").expect("resolve");
        assert_eq!(skin.shader("head"), Some("face"));
        assert_eq!(skin.shader("torso"), Some("from_head"));
    }

    #[test]
    fn a_missing_part_falls_back_to_the_default_skin() {
        let vfs = vfs(&[
            ("model_default.skin", "torso,plain"),
            ("head_b1.skin", "head,face"),
            ("torso_g1.skin", "torso,body"),
        ]);
        assert_eq!(
            shader(&vfs, "head_b1|torso_g1|lower_c2", "torso").as_deref(),
            Some("plain")
        );
        // The head part's surfaces are not kept either.
        assert!(shader(&vfs, "head_b1|torso_g1|lower_c2", "head").is_none());
    }

    #[test]
    fn a_name_without_head_torso_and_lower_is_one_skin_file() {
        let vfs = vfs(&[
            ("model_default.skin", "torso,plain"),
            ("a|b|c.skin", "torso,parts"),
            ("model_a|b|c.skin", "torso,whole"),
        ]);
        assert_eq!(shader(&vfs, "a|b|c", "torso").as_deref(), Some("whole"));
    }
}
