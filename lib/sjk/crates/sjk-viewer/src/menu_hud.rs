//! The status HUD described by the game's own menu files.
//!
//! Retail Jedi Academy draws its health, armor, Force and ammo display from
//! `.menu` files: `cg_hudFiles` (default `ui/jahud.txt`) lists them and
//! OpenJK codemp `CG_DrawHUD` (`cg_draw.c`) draws named items of their
//! `lefthud` and `righthud` menus. Loading those files gives the original
//! HUD, and any PK3 that replaces `ui/hud.menu` or names another list in
//! `cg_hudFiles` gives a custom HUD, with no SJK-specific format. A nonzero
//! integer `cg_hudFiles` selects the stock text-only HUD instead.
//!
//! `cg_hudStyle` chooses between the client's own classic and radial layouts and
//! this game-data HUD (`game`, SJK's default). In `game` mode the built-in status
//! widgets hide ([`crate::hud::HudVisibility::menu_hud`]) and everything else
//! the built-in HUD shows (crosshair, obituaries, timers, chat) stays. Files that
//! cannot be read, or that define no HUD menus, leave the built-in HUD in place.
//!
//! Several installed HUD packs replace the same `ui/hud.menu` (and often the
//! retail pictures too); the last one mounted wins, as in the game.
//! `cg_hudPack` names the PK3 whose HUD to use instead: the files are then read
//! as if the HUD packs mounted after it were not installed, so the retail HUD
//! (`assets1.pk3`) or any pack can be chosen without uninstalling the others.
//! [`choices`] lists what can be picked and [`preview`] draws a picture of each
//! for the settings' HUD picker.
//!
//! Files are parsed and pictures packed when the mode is first used and
//! again when `cg_hudFiles` or `cg_hudPack` changes; each frame only fills
//! fixed storage.

pub(crate) mod choices;
pub(crate) mod frame;
mod gpu;
mod layout;
mod parse;
pub(crate) mod preview;

use crate::GpuState;
use frame::{Frame, Readout, Timers};
use layout::{Layout, Side};
use sjk_shell::{CvarDefinition, CvarFlags, CvarRegistry};
use sjk_vfs::{MountId, MountSummary, VirtualFileSystem};

/// Which HUD the player sees.
pub(crate) const STYLE_CVAR: &str = "cg_hudStyle";
/// The menu list (or text-HUD switch) of the game-data HUD.
pub(crate) const FILES_CVAR: &str = "cg_hudFiles";
/// The PK3 whose HUD the game-data HUD uses; empty for the one installed last.
pub(crate) const PACK_CVAR: &str = "cg_hudPack";
/// The retail `cg_hudFiles` default.
pub(crate) const DEFAULT_LIST: &str = "ui/jahud.txt";
/// The menu a HUD pack replaces (`ui/jahud.txt` loads it).
pub(crate) const HUD_MENU: &str = "ui/hud.menu";
/// `gfx/2d/numbers/t_*`, the `NUM_FONT_SMALL` digits, then the minus sign.
const DIGITS: [&str; 11] = [
    "gfx/2d/numbers/t_zero",
    "gfx/2d/numbers/t_one",
    "gfx/2d/numbers/t_two",
    "gfx/2d/numbers/t_three",
    "gfx/2d/numbers/t_four",
    "gfx/2d/numbers/t_five",
    "gfx/2d/numbers/t_six",
    "gfx/2d/numbers/t_seven",
    "gfx/2d/numbers/t_eight",
    "gfx/2d/numbers/t_nine",
    "gfx/2d/numbers/t_minus",
];
/// `#include` nesting the loader follows.
const INCLUDE_DEPTH: usize = 4;

/// `cg_hudStyle` values.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum HudStyle {
    /// SJK's classic layout in either font.
    Classic,
    /// SJK's radial layout: health, armor, Force and ammunition as arcs around the
    /// crosshair, drawn by the engine (the TheRisqe Radial HUD, without its PK3).
    Radial,
    /// The game-data HUD of `cg_hudFiles`: SJK's default, the retail HUD
    /// unless a HUD pack is installed.
    Game,
}

impl HudStyle {
    /// Settings choices, in [`HudStyle`] order.
    pub(crate) const NAMES: [&'static str; 3] = ["classic", "game", "radial"];
    /// The style of a fresh profile.
    pub(crate) const DEFAULT: Self = Self::Game;

    /// The cvar value naming this style.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Classic => Self::NAMES[0],
            Self::Game => Self::NAMES[1],
            Self::Radial => Self::NAMES[2],
        }
    }

    /// Read the cvar; anything unknown, `modern` (a retired layout) among it,
    /// is the default.
    pub(crate) fn from_cvar(value: Option<&str>) -> Self {
        match value.map(str::trim) {
            Some(text) if text.eq_ignore_ascii_case("classic") => Self::Classic,
            Some(text) if text.eq_ignore_ascii_case("game") => Self::Game,
            Some(text) if text.eq_ignore_ascii_case("radial") => Self::Radial,
            _ => Self::DEFAULT,
        }
    }

    pub(crate) fn read(console: Option<&crate::console::ViewerConsole>) -> Self {
        Self::from_cvar(console.and_then(|console| console.text_value(STYLE_CVAR)))
    }
}

/// What `cg_hudFiles` selects.
#[derive(Debug, Eq, PartialEq)]
pub(crate) enum Source<'a> {
    /// The stock text HUD (`cg_hudFiles.integer` nonzero in `CG_DrawHUD`).
    Text,
    /// A menu list or menu file.
    Menus(&'a str),
}

pub(crate) fn source(value: &str) -> Source<'_> {
    let value = value.trim();
    match value.parse::<i64>() {
        // EternalJK reads 0 as the default list and 3/4 as its bundled
        // elegance and JoF HUDs; 1 and 2 are its (and stock's) text HUDs.
        Ok(0) => Source::Menus(DEFAULT_LIST),
        Ok(3) => Source::Menus("ui/elegance_hud.txt"),
        Ok(4) => Source::Menus("ui/jof_hud.txt"),
        Ok(_) => Source::Text,
        Err(_) if value.is_empty() => Source::Menus(DEFAULT_LIST),
        Err(_) => Source::Menus(value),
    }
}

pub(crate) fn register(cvars: &mut CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        STYLE_CVAR,
        HudStyle::DEFAULT.name(),
        CvarFlags::ARCHIVE,
        "HUD: game (the game's menu-file HUD, cg_hudFiles), classic or radial (SJK layouts)",
    ))?;
    cvars.register(CvarDefinition::new(
        FILES_CVAR,
        DEFAULT_LIST,
        CvarFlags::ARCHIVE,
        "Game HUD menu list (ui/jahud.txt); 1 or 2 is the text HUD, 3/4 EternalJK's HUDs",
    ))?;
    cvars.register(CvarDefinition::new(
        PACK_CVAR,
        "",
        CvarFlags::ARCHIVE,
        "PK3 whose ui/hud.menu the game HUD uses (assets1.pk3 = retail); empty: the last installed",
    ))?;
    Ok(())
}

/// Mounts that ship [`HUD_MENU`], lowest priority first: the game's own
/// assets, then each HUD pack.
pub(crate) fn hud_packs(vfs: &VirtualFileSystem) -> Vec<MountSummary> {
    vfs.mounts()
        .filter(|mount| matches!(vfs.read_from_mount(mount.id, HUD_MENU), Ok(Some(_))))
        .collect()
}

/// The file name of mount `name` (`zz_hud.pk3`), which `cg_hudPack` holds.
pub(crate) fn mount_file_name(name: &str) -> &str {
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

/// What the game HUD reads for `cg_hudPack` value `pack`: the files without
/// the HUD packs mounted after it, so its menu and pictures win as if it had
/// been installed last. `None` for an empty value or a pack that is not
/// mounted: the files as installed.
pub(crate) fn pack_view(vfs: &VirtualFileSystem, pack: &str) -> Option<VirtualFileSystem> {
    let pack = pack.trim();
    if pack.is_empty() {
        return None;
    }
    let packs = hud_packs(vfs);
    let chosen = packs
        .iter()
        .position(|mount| mount_file_name(&mount.name).eq_ignore_ascii_case(pack))?;
    let hidden: Vec<MountId> = packs[chosen + 1..].iter().map(|mount| mount.id).collect();
    Some(vfs.without_mounts(&hidden))
}

enum Loaded {
    /// Nothing loaded, or the files were unusable.
    None,
    Text,
    Menus {
        layout: Box<Layout>,
        /// Picture index of the first digit.
        digits: u16,
    },
}

/// Game-data HUD state owned by [`GpuState`].
pub(crate) struct MenuHud {
    gpu: gpu::Gpu,
    loaded: Loaded,
    /// `cg_hudFiles` and `cg_hudPack` the current load was made for; `None`
    /// before any.
    loaded_for: Option<(String, String)>,
    frame: Frame,
    timers: Timers,
    score_label: String,
    /// Whether the game-data HUD stands in for the built-in status widgets.
    active: bool,
    /// Whether this frame's pictures and text are drawn.
    drawn: bool,
}

impl MenuHud {
    pub(crate) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
        Self {
            gpu: gpu::Gpu::new(device, format),
            loaded: Loaded::None,
            loaded_for: None,
            frame: Frame::default(),
            timers: Timers::default(),
            score_label: String::from("Score"),
            active: false,
            drawn: false,
        }
    }

    /// Whether the built-in status widgets should hide this frame.
    pub(crate) fn active(&self) -> bool {
        self.active
    }

    /// Follow `cg_hudStyle`, `cg_hudFiles` and `cg_hudPack`, loading on first
    /// use or change.
    pub(crate) fn sync(gpu: &mut GpuState) {
        let style = HudStyle::read(gpu.console.as_ref());
        if style != HudStyle::Game {
            gpu.menu_hud.active = false;
            return;
        }
        let text = |name, fallback| {
            gpu.console
                .as_ref()
                .and_then(|console| console.text_value(name))
                .unwrap_or(fallback)
        };
        let (files, pack) = (text(FILES_CVAR, DEFAULT_LIST), text(PACK_CVAR, ""));
        let current =
            gpu.menu_hud
                .loaded_for
                .as_ref()
                .is_some_and(|(loaded_files, loaded_pack)| {
                    loaded_files == files && loaded_pack == pack
                });
        if !current {
            let (files, pack) = (files.to_owned(), pack.to_owned());
            let Some(vfs) = gpu.vfs.clone() else {
                return;
            };
            gpu.menu_hud
                .load(&gpu.device, &gpu.queue, &vfs, &gpu.shaders, &files, &pack);
            if let Some(label) = gpu.localization.strings.get("SCORE") {
                gpu.menu_hud.score_label.clone_from(label);
            }
            gpu.menu_hud.loaded_for = Some((files, pack));
        }
        gpu.menu_hud.active = !matches!(gpu.menu_hud.loaded, Loaded::None);
    }

    #[allow(clippy::too_many_arguments)]
    fn load(
        &mut self,
        device: &wgpu::Device,
        queue: &crate::frame_queue::FrameQueue,
        vfs: &VirtualFileSystem,
        shaders: &sjk_shader::ShaderCatalog,
        files: &str,
        pack: &str,
    ) {
        let view = pack_view(vfs, pack);
        if view.is_none() && !pack.trim().is_empty() {
            crate::log::progress(format_args!(
                "warning: game HUD pack {pack} is not installed; using the HUD installed last"
            ));
        }
        let vfs = view.as_ref().unwrap_or(vfs);
        self.timers = Timers::default();
        self.gpu.unload();
        self.loaded = Loaded::None;
        let list = match source(files) {
            Source::Text => {
                self.loaded = Loaded::Text;
                crate::log::progress(format_args!("game HUD: text HUD (cg_hudFiles {files})"));
                return;
            }
            Source::Menus(list) => list,
        };
        // CG_LoadMenus falls back to the default list when a file is missing.
        let menus = read_menus(vfs, list)
            .filter(|menus| !menus.is_empty())
            .or_else(|| (list != DEFAULT_LIST).then(|| read_menus(vfs, DEFAULT_LIST))?);
        let mut layout = Layout::resolve(menus.as_deref().unwrap_or_default());
        if !layout.is_usable() {
            crate::log::progress(format_args!(
                "warning: game HUD {list} defines no lefthud/righthud menu; keeping the SJK HUD"
            ));
            return;
        }
        let digits = layout.pictures.len() as u16;
        let mut largest = vec![0.0_f32; layout.pictures.len()];
        layout.for_each_piece(|piece| {
            if let Some(picture) = piece.picture {
                let [_, _, w, h] = piece.rect;
                let side = &mut largest[usize::from(picture)];
                *side = side.max(w.abs()).max(h.abs());
            }
        });
        let digit_side = [&layout.health_amount, &layout.armor_amount]
            .into_iter()
            .chain([&layout.force_amount, &layout.ammo_amount])
            .flatten()
            .map(|piece| piece.rect[2].abs().max(piece.rect[3].abs()))
            .fold(12.0_f32, f32::max);
        layout
            .pictures
            .extend(DIGITS.iter().map(|name| (*name).to_owned()));
        largest.extend([digit_side; DIGITS.len()]);
        let found = self
            .gpu
            .load(device, queue, vfs, shaders, &layout.pictures, &largest);
        crate::log::progress(format_args!(
            "game HUD: {list}{}{}, {found} of {} pictures",
            if view.is_some() { " from " } else { "" },
            if view.is_some() { pack.trim() } else { "" },
            layout.pictures.len()
        ));
        self.loaded = Loaded::Menus {
            layout: Box::new(layout),
            digits,
        };
    }

    /// Build and upload this frame. `visible` is retail's condition for
    /// `CG_DrawStats`: a living, non-spectating player with the status HUD
    /// on and no scoreboard.
    pub(crate) fn prepare(
        &mut self,
        queue: &crate::frame_queue::FrameQueue,
        readout: Option<&Readout>,
        viewport: [f32; 2],
        scale: f32,
    ) {
        self.drawn = false;
        let readout = readout.filter(|_| self.active);
        let Some(readout) = readout else {
            self.gpu.prepare(queue, None, viewport, scale);
            return;
        };
        match &self.loaded {
            Loaded::None => {}
            Loaded::Text => {
                self.frame.simple_hud(readout, &mut self.timers);
                self.drawn = true;
            }
            Loaded::Menus { layout, digits } => {
                self.frame.menu_hud(
                    layout,
                    readout,
                    &mut self.timers,
                    *digits,
                    &self.score_label,
                );
                self.drawn = true;
            }
        }
        self.gpu
            .prepare(queue, self.drawn.then_some(&self.frame), viewport, scale);
    }

    /// Append this frame's text runs (score line, infinite-ammo mark, text
    /// HUD) in the HUD font.
    pub(crate) fn append_text(
        &self,
        vertices: &mut Vec<crate::text::TextVertex>,
        font: &crate::text::UiFont,
        viewport: [f32; 2],
        scale: f32,
    ) {
        if !self.drawn {
            return;
        }
        for run in &self.frame.texts[..self.frame.text_count] {
            let [x, y, _, size] = gpu::to_pixels(
                [run.x, run.y, 0.0, run.size],
                run.side.unwrap_or(Side::Left),
                viewport,
                scale,
            );
            let glyph_scale = size / font.height.max(1.0);
            let face = crate::text::TextFace::Regular;
            let x = if run.centred {
                x - crate::text::visible_text_width_style(font, &run.text, glyph_scale, face, 0.0)
                    * 0.5
            } else {
                x
            };
            crate::text::append_text_style(
                vertices,
                font,
                &run.text,
                [x, y],
                glyph_scale,
                viewport,
                face,
                run.color,
                0.0,
            );
        }
    }

    /// Draw this frame's pictures (before the retained UI shapes and text).
    pub(crate) fn draw(&self, pass: &mut wgpu::RenderPass<'_>) {
        if self.drawn {
            self.gpu.draw(pass);
        }
    }
}

/// The values the HUD shows, or `None` when retail would not draw it.
pub(crate) fn readout(gpu: &GpuState, time: i32) -> Option<Readout> {
    let (snapshot, game) = if let Some(session) = &gpu.live_session {
        (session.latest_snapshot(), session.game_state())
    } else if let Some(session) = &gpu.demo_session {
        (session.snapshot_at_or_before(time), session.game_state())
    } else {
        return None;
    };
    let player = &snapshot.player;
    // CG_Draw2D: no status for spectators, the dead or an open scoreboard;
    // CG_DrawHUD: none for PM_SPECTATOR.
    if player.team() == 3
        || player.is_spectator()
        || player.health() <= 0
        || gpu.gameplay_input.held(crate::input::GameButton::Scores)
        || gpu.quick_wheel.hides_hud()
        // CG_DrawVehicleHud: no player HUD in a vehicle that hides its rider.
        || gpu.hud.vehicle.hides_player()
    {
        return None;
    }
    let console = gpu.console.as_ref();
    let enabled = |name, fallback| {
        console
            .and_then(|console| console.bool_cvar(name))
            .unwrap_or(fallback)
    };
    if !(enabled("cg_draw2D", true)
        && enabled("cg_drawHud", true)
        && enabled("cg_drawStatus", true))
    {
        return None;
    }
    let predicted = gpu.local_prediction.predicted_state();
    let info = sjk_client::LegacyClientInfo::new(game.config_string(0).unwrap_or_default());
    let gametype = info.integer("g_gametype").unwrap_or(0);
    let mut ammo = [0; 10];
    for (slot, value) in ammo.iter_mut().enumerate() {
        *value = predicted.map_or(player.ammo[slot] as i32, |state| state.ammo[slot]);
    }
    Some(Readout {
        health: player.health(),
        max_health: player.max_health(),
        armor: player.armor(),
        force: i32::from(player.force_power()),
        weapon: predicted.map_or(player.weapon(), |state| state.weapon),
        ammo,
        saber_style: player.saber_draw_style(),
        weapon_state: predicted.map_or(player.weapon_state(), |state| state.weapon_state),
        weapon_time: predicted.map_or(player.weapon_time(), |state| state.weapon_time),
        double_ammo: player.entity_flags() & (1 << 20) != 0,
        score: player.persistent[0] as i32,
        duel: gametype == 3,
        power_duel: gametype == 4,
        fraglimit: info.integer("fraglimit").unwrap_or(0),
        time,
    })
}

/// Parse `list` and every menu file it loads or includes.
fn read_menus(vfs: &VirtualFileSystem, list: &str) -> Option<Vec<parse::MenuDef>> {
    let mut menus = Vec::new();
    read_file(vfs, list, 0, &mut menus)?;
    Some(menus)
}

fn read_file(
    vfs: &VirtualFileSystem,
    path: &str,
    depth: usize,
    menus: &mut Vec<parse::MenuDef>,
) -> Option<()> {
    let asset = vfs.read(path).ok().flatten()?;
    let parsed = parse::parse(&String::from_utf8_lossy(&asset.bytes));
    for include in &parsed.includes {
        if depth < INCLUDE_DEPTH {
            // Headers such as ui/menudef.h only define constants.
            let _ = read_file(vfs, include, depth + 1, menus);
        }
    }
    menus.extend(parsed.menus);
    for file in &parsed.load {
        if depth < INCLUDE_DEPTH && read_file(vfs, file, depth + 1, menus).is_none() {
            crate::log::progress(format_args!("warning: game HUD menu {file} not found"));
        }
    }
    Some(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hud_files_values_follow_retail_and_eternaljk() {
        assert_eq!(source(""), Source::Menus(DEFAULT_LIST));
        assert_eq!(source("0"), Source::Menus(DEFAULT_LIST));
        assert_eq!(source("ui/jahud.txt"), Source::Menus("ui/jahud.txt"));
        assert_eq!(
            source(" ui/elegance_hud.txt "),
            Source::Menus("ui/elegance_hud.txt")
        );
        assert_eq!(source("1"), Source::Text);
        assert_eq!(source("2"), Source::Text);
        assert_eq!(source("3"), Source::Menus("ui/elegance_hud.txt"));
        assert_eq!(source("4"), Source::Menus("ui/jof_hud.txt"));
    }

    #[test]
    fn style_defaults_to_the_game_hud() {
        assert_eq!(HudStyle::from_cvar(None), HudStyle::Game);
        assert_eq!(HudStyle::from_cvar(Some("GAME")), HudStyle::Game);
        assert_eq!(HudStyle::from_cvar(Some("classic")), HudStyle::Classic);
        assert_eq!(HudStyle::from_cvar(Some(" Radial")), HudStyle::Radial);
        assert_eq!(HudStyle::from_cvar(Some("retro")), HudStyle::Game);
        assert_eq!(HudStyle::from_cvar(Some(" Modern ")), HudStyle::Game);
        let parsed = HudStyle::NAMES.map(|name| HudStyle::from_cvar(Some(name)));
        assert_eq!(
            parsed,
            [HudStyle::Classic, HudStyle::Game, HudStyle::Radial]
        );
        for style in parsed {
            assert_eq!(HudStyle::from_cvar(Some(style.name())), style);
        }
    }
}
