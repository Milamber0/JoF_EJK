//! `CG_RegisterGraphics` sub-BSP registration (codemp/cgame/cg_main.c:1434).
//! All model numbers are allocated in server configstring order. Rendering and
//! collision share the composed asset; no per-frame loading or path resolution.
use super::{GpuWorldInput, load_bsp_from_vfs};
use sjk_scene::{MeshBuildOptions, StaticWorld};
use std::error::Error;

const CS_BSP_MODELS: usize = 1612; // codemp/game/bg_public.h
const MAX_SUB_BSP: usize = 32;

pub(crate) fn prepare(mut input: GpuWorldInput) -> Result<GpuWorldInput, Box<dyn Error>> {
    let game = input
        .live_session
        .as_ref()
        .map(|session| session.game_state())
        .or_else(|| {
            input
                .demo_session
                .as_ref()
                .map(|session| session.game_state())
        })
        .or(input.build_game_state.as_ref());
    let Some(game) = game else {
        return Ok(input);
    };
    let paths = paths(|index| game.config_string(index))?;
    if paths.is_empty() {
        return Ok(input);
    }
    for path in paths {
        let asset = load_bsp_from_vfs(&input.vfs, &path)?;
        let models = input.bsp.append_model_asset(asset)?;
        crate::log::progress(format_args!("BSP instance {path}: models {models:?}"));
    }
    input.scene = StaticWorld::build(&input.bsp, MeshBuildOptions::default().with_sky_surfaces())?;
    Ok(input)
}

fn paths<'a>(get: impl Fn(usize) -> Option<&'a [u8]>) -> Result<Vec<String>, Box<dyn Error>> {
    let mut paths = Vec::<String>::new();
    for slot in 1..MAX_SUB_BSP {
        let Some(bytes) = get(CS_BSP_MODELS + slot).filter(|b| !b.is_empty()) else {
            break;
        };
        let name = std::str::from_utf8(bytes)?;
        let stem = name
            .strip_prefix('#')
            .ok_or("sub-BSP model must begin with #")?;
        let path = sjk_vfs::VirtualPath::new(&format!("maps/{stem}.bsp"))?.to_string();
        // CM_LoadSubBSP registers each unique name once. Repeated placements
        // reference those existing numbers and never duplicate asset geometry.
        if !paths
            .iter()
            .any(|previous| previous.eq_ignore_ascii_case(&path))
        {
            paths.push(path);
        }
    }
    Ok(paths)
}
