//! Pictures of the bindable actions (classic+): the retail HUD icon of the
//! weapon, item or Force power an action selects or uses, read from the
//! player's game data and never bundled (but for SJK's Illuminate, whose
//! picture comes with the client). The classic key-binding panel draws
//! them beside the rows and in the detail box; an action without one, or a
//! picture the game data lacks, is simply text.

use super::ACTIONS;
use crate::player_menu::icons::{IconLoader, IconRequest};
use crate::ui_renderer::{BIND_ICON_CELLS, BIND_ICON_FIRST};
use sjk_ui::TextureId;
use sjk_vfs::VirtualFileSystem;
use std::sync::Arc;

/// Console command and icon (extension included, as shipped) of every
/// action with a picture. `weapon N` selects weapon `N + 2` of
/// `weapon_t` (`weapon 1` the saber, else melee; `weapon 10` cycles the
/// explosives), as `legacy_direct_weapon` reads it.
const ICONS: [(&str, &str); 39] = [
    // INTERACTION
    ("sv_saberswitch", "gfx/hud/w_icon_lightsaber.tga"),
    ("saberAttackCycle", "gfx/hud/saber_med.tga"),
    ("zoom", "gfx/hud/i_icon_zoom.tga"),
    ("use_bacta", "gfx/hud/i_icon_bacta.tga"),
    ("use_seeker", "gfx/hud/i_icon_seeker.tga"),
    ("use_sentry", "gfx/hud/i_icon_sentrygun.tga"),
    ("use_field", "gfx/hud/i_icon_shieldwall.tga"),
    ("use_electrobinoculars", "gfx/hud/i_icon_zoom.tga"),
    // SJK's Illuminate holocron, which the client bundles (`crate::illuminate`).
    ("toy_illuminate", "gfx/sjk/toy_illuminate.png"),
    // WEAPONS
    ("weapon 1", "gfx/hud/w_icon_lightsaber.tga"),
    ("weapmelee", "gfx/hud/w_icon_melee.tga"),
    ("weapon 2", "gfx/hud/w_icon_blaster_pistol.tga"),
    ("weapon 3", "gfx/hud/w_icon_blaster.tga"),
    ("weapon 4", "gfx/hud/w_icon_disruptor.tga"),
    ("weapon 5", "gfx/hud/w_icon_bowcaster.tga"),
    ("weapon 6", "gfx/hud/w_icon_repeater.tga"),
    ("weapon 7", "gfx/hud/w_icon_demp2.tga"),
    ("weapon 8", "gfx/hud/w_icon_flechette.tga"),
    ("weapon 13", "gfx/hud/w_icon_c_rifle.tga"),
    ("weapon 9", "gfx/hud/w_icon_merrsonn.tga"),
    ("weapon 10", "gfx/hud/w_icon_thermal.tga"),
    // FORCE
    ("force_throw", "gfx/mp/f_icon_push.tga"),
    ("force_pull", "gfx/mp/f_icon_pull.tga"),
    ("force_speed", "gfx/mp/f_icon_speed.tga"),
    ("force_seeing", "gfx/mp/f_icon_sight.tga"),
    ("force_heal", "gfx/mp/f_icon_lt_heal.tga"),
    ("force_protect", "gfx/mp/f_icon_lt_protect.tga"),
    ("force_absorb", "gfx/mp/f_icon_lt_absorb.tga"),
    ("force_distract", "gfx/mp/f_icon_lt_telepathy.tga"),
    ("+force_grip", "gfx/mp/f_icon_dk_grip.tga"),
    ("+force_lightning", "gfx/mp/f_icon_dk_l1.tga"),
    ("force_rage", "gfx/mp/f_icon_dk_rage.tga"),
    ("+force_drain", "gfx/mp/f_icon_dk_drain.tga"),
    ("force_healother", "gfx/mp/f_icon_lt_healother.tga"),
    ("force_forcepowerother", "gfx/mp/f_icon_dk_forceother.tga"),
    // JoF EJK's Force wheel pictures (`jofclient-assets.pk3`); Stasis has none
    // of its own and borrows Jump's, as on the wheel.
    ("force_dash", "gfx/jof/force_dash.tga"),
    ("+force_stasis", "gfx/mp/f_icon_levitation.tga"),
    ("force_repulse", "gfx/jof/force_repulse.tga"),
    // OTHER
    ("teammenu", "gfx/hud/mpi_rflag.tga"),
];

/// The atlas cell of picture `path`: one per distinct picture, in order of
/// first use, so the saber and the binoculars are decoded once. `None` past
/// the reserved cells.
fn cell(path: &str) -> Option<TextureId> {
    let slot = ICONS
        .iter()
        .enumerate()
        .filter(|(index, (_, other))| ICONS[..*index].iter().all(|(_, seen)| seen != other))
        .position(|(_, (_, other))| *other == path)?;
    (slot < BIND_ICON_CELLS as usize).then(|| TextureId(BIND_ICON_FIRST + slot as u32))
}

/// The atlas cell of action `action`'s picture, if it has one.
pub(super) fn texture(action: usize) -> Option<TextureId> {
    let command = ACTIONS.get(action)?.command;
    let (_, path) = ICONS.iter().find(|(name, _)| *name == command)?;
    cell(path)
}

/// Each distinct picture and its cell.
pub(super) fn requests() -> Vec<IconRequest> {
    let mut requests: Vec<IconRequest> = Vec::with_capacity(ICONS.len());
    for (_, path) in ICONS {
        let Some(texture) = cell(path) else { continue };
        if requests.iter().all(|(other, _)| *other != texture) {
            requests.push((texture, vec![path.to_owned()]));
        }
    }
    requests
}

/// Whether any of `ACTIONS[rows]` has a picture.
pub(super) fn any(rows: std::ops::Range<usize>) -> bool {
    rows.into_iter().any(|action| texture(action).is_some())
}

/// The pictures, decoded once the game data is known and uploaded while the
/// key-binding panel is up.
pub(super) struct BindIcons {
    loader: IconLoader,
    vfs: Option<Arc<VirtualFileSystem>>,
}

impl BindIcons {
    pub(super) fn new() -> Self {
        Self {
            loader: IconLoader::new(),
            vfs: None,
        }
    }

    pub(super) fn attach_vfs(&mut self, vfs: Arc<VirtualFileSystem>) {
        self.vfs = Some(vfs);
    }

    /// Start decoding (once) and collect what the worker finished.
    pub(super) fn poll(&mut self) {
        if self.loader.is_idle()
            && let Some(vfs) = &self.vfs
        {
            self.loader.request_paths(Arc::clone(vfs), requests());
        }
        self.loader.poll();
    }

    /// Forget the uploaded pictures (the atlas is another world's): the
    /// next poll decodes them again.
    pub(super) fn forget_uploads(&mut self) {
        self.loader = IconLoader::new();
    }

    /// Count every picture as uploaded (snapshots draw without a GPU).
    #[cfg(test)]
    pub(super) fn assume_ready(&mut self) {
        for (texture, _) in requests() {
            self.loader.mark_ready(texture);
        }
    }

    pub(super) fn upload(
        &mut self,
        renderer: &crate::ui_renderer::ShapeRenderer,
        queue: &crate::frame_queue::FrameQueue,
    ) {
        self.loader.upload_batch(renderer, queue, 12);
    }

    /// Action `action`'s picture, once it is in the atlas.
    pub(super) fn ready(&self, action: usize) -> Option<TextureId> {
        texture(action).filter(|texture| self.loader.is_texture_ready(*texture))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_picture_names_a_bindable_action_once() {
        for (index, (command, _)) in ICONS.iter().enumerate() {
            assert!(
                ACTIONS.iter().any(|action| action.command == *command),
                "{command}"
            );
            assert!(
                ICONS[index + 1..].iter().all(|(other, _)| other != command),
                "{command}"
            );
        }
    }

    #[test]
    fn every_weapon_slot_and_force_power_has_a_picture() {
        for (index, action) in ACTIONS.iter().enumerate() {
            let pictured = action.command.starts_with("weapon ")
                || action.command == "weapmelee"
                || action.command.contains("force_");
            if pictured {
                assert!(texture(index).is_some(), "{}", action.command);
            }
        }
    }

    #[test]
    fn a_picture_shared_by_two_actions_takes_one_cell() {
        let action = |command: &str| {
            ACTIONS
                .iter()
                .position(|action| action.command == command)
                .unwrap()
        };
        assert_eq!(
            texture(action("weapon 1")),
            texture(action("sv_saberswitch"))
        );
        assert_ne!(texture(action("weapon 1")), texture(action("weapon 2")));
    }

    #[test]
    fn pictures_stay_in_their_cells() {
        let end = BIND_ICON_FIRST + BIND_ICON_CELLS;
        assert!(end <= crate::ui_renderer::ATLAS_CELLS);
        for action in 0..ACTIONS.len() {
            if let Some(texture) = texture(action) {
                assert!((BIND_ICON_FIRST..end).contains(&texture.0));
            }
        }
        // Every picture has a cell.
        assert!(ICONS.iter().all(|(_, path)| cell(path).is_some()));
    }
}
