//! Head icons for the classic scoreboard, one atlas cell per client slot.
//!
//! As `cg_drawScoreboardIcons` does with `clientInfo_t::modelIcon`, each row
//! shows `models/players/<model>/icon_<skin>` for the player's model. An icon is
//! decoded only when that slot's model changes, and at most
//! [`DECODES_PER_FRAME`] per frame so a full server does not stall the frame
//! the scoreboard first opens; unresolved icons draw nothing.

use crate::ui_renderer::SCOREBOARD_ICON_CELLS;
use sjk_protocol::GameState;
use sjk_ui::TextureId;

const CS_PLAYERS: usize = 1_131;
const SLOTS: usize = 32;
const DECODES_PER_FRAME: usize = 2;

/// Resolved head icons by client slot.
pub(crate) struct HeadIcons {
    models: [Vec<u8>; SLOTS],
    ready: [bool; SLOTS],
    /// The model bytes changed and the icon is not resolved yet.
    pending: [bool; SLOTS],
}

impl Default for HeadIcons {
    fn default() -> Self {
        Self {
            models: std::array::from_fn(|_| Vec::with_capacity(32)),
            ready: [false; SLOTS],
            pending: [false; SLOTS],
        }
    }
}

impl HeadIcons {
    /// The icon to draw for `client`, once resolved.
    pub(crate) fn texture(&self, client: u8) -> Option<TextureId> {
        let slot = usize::from(client);
        self.ready
            .get(slot)
            .copied()
            .unwrap_or(false)
            .then(|| TextureId(SCOREBOARD_ICON_CELLS + slot as u32))
    }

    /// Resolve `client`'s icon on its own (the player card wants one while the scoreboard
    /// is closed): note a model change, then decode it through `resolve` if pending.
    pub(crate) fn ensure(
        &mut self,
        game: &GameState,
        client: u8,
        mut resolve: impl FnMut(&str, TextureId) -> bool,
    ) {
        let slot = usize::from(client);
        if slot >= SLOTS {
            return;
        }
        let model = game
            .config_string(CS_PLAYERS + slot)
            .and_then(|bytes| sjk_client::LegacyClientInfo::new(bytes).bytes("model"))
            .unwrap_or_default();
        if self.models[slot] != model {
            self.models[slot].clear();
            self.models[slot].extend_from_slice(model);
            self.ready[slot] = false;
            self.pending[slot] = !model.is_empty();
        }
        if self.pending[slot] {
            self.pending[slot] = false;
            if let Some(path) = crate::hud::portrait::icon_path(&self.models[slot]) {
                self.ready[slot] = resolve(&path, TextureId(SCOREBOARD_ICON_CELLS + slot as u32));
            }
        }
    }

    /// Note model changes, then resolve up to [`DECODES_PER_FRAME`] pending
    /// slots through `resolve` (which uploads the icon to `texture` and says
    /// whether it resolved).
    pub(super) fn update(
        &mut self,
        game: &GameState,
        mut resolve: impl FnMut(&str, TextureId) -> bool,
    ) {
        for slot in 0..SLOTS {
            let model = game
                .config_string(CS_PLAYERS + slot)
                .and_then(|bytes| sjk_client::LegacyClientInfo::new(bytes).bytes("model"))
                .unwrap_or_default();
            if self.models[slot] != model {
                self.models[slot].clear();
                self.models[slot].extend_from_slice(model);
                self.ready[slot] = false;
                self.pending[slot] = !model.is_empty();
            }
        }
        let mut budget = DECODES_PER_FRAME;
        for slot in 0..SLOTS {
            if budget == 0 {
                break;
            }
            if !self.pending[slot] {
                continue;
            }
            self.pending[slot] = false;
            budget -= 1;
            if let Some(path) = crate::hud::portrait::icon_path(&self.models[slot]) {
                self.ready[slot] = resolve(&path, TextureId(SCOREBOARD_ICON_CELLS + slot as u32));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game_with_models(models: &[(usize, &str)]) -> GameState {
        let mut game = GameState::empty_local(0);
        for (slot, model) in models {
            game.replace_config_string(
                CS_PLAYERS + slot,
                format!("n\\P{slot}\\t\\0\\model\\{model}").into_bytes(),
            )
            .unwrap();
        }
        game
    }

    #[test]
    fn resolves_changed_models_a_few_per_frame() {
        let game = game_with_models(&[(0, "kyle"), (1, "jan"), (5, "luke/default")]);
        let mut icons = HeadIcons::default();
        let mut seen = Vec::new();
        icons.update(&game, |path, texture| {
            seen.push((path.to_owned(), texture));
            true
        });
        assert_eq!(
            seen,
            [
                (
                    "models/players/kyle/icon_default".to_owned(),
                    TextureId(SCOREBOARD_ICON_CELLS)
                ),
                (
                    "models/players/jan/icon_default".to_owned(),
                    TextureId(SCOREBOARD_ICON_CELLS + 1)
                ),
            ]
        );
        assert_eq!(icons.texture(1), Some(TextureId(SCOREBOARD_ICON_CELLS + 1)));
        assert_eq!(icons.texture(5), None);
        seen.clear();
        icons.update(&game, |path, _| {
            seen.push((path.to_owned(), TextureId(0)));
            false
        });
        // Only the remaining slot is decoded; a miss draws nothing.
        assert_eq!(seen.len(), 1);
        assert_eq!(icons.texture(5), None);
        // Unchanged models are never decoded again.
        icons.update(&game, |_, _| panic!("decoded an unchanged model"));
    }
}
