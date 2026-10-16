//! The per-frame hook of player identity (`player_identity.rs`): twice a second,
//! hand the service the player's settings and where they are playing, the new
//! medal pop-up the medals of the player's own profile (`medal_popup.rs`), the
//! achievements their counts (`achievements_frame.rs`), the picture cache where
//! pictures come from (`avatars.rs`) and the profile card what it shows
//! (`profile_card.rs`), and the hub whether the player is actively playing
//! (`holocrons_frame.rs`), and the holocron odds the hub's (`loot_odds.rs`). Each frame it also lets the chat feed follow the SJK chat
//! (`sjk_chat_frame.rs`), keeps crash reports up to date and sent
//! (`crash_report.rs`), starts and ends emotes (`emotes_frame.rs`), shows the pings
//! relayed for this server (`world_ping_frame.rs`), keeps the
//! players' looks (`looks_frame.rs`), follows the players muted on this PC
//! (`muted_players_frame.rs`), notes the Profile screen's tab on show
//! (`profile_hub.rs`), hands the hub the player models the game has
//! (`installed_models.rs`), puts the pictures that finished loading into the UI's atlas and
//! gives the UI the frames of the SJK chat's GIFs on screen (`chat_gifs.rs`).

use super::*;
use sjk_identity::Settings;

impl GpuState {
    /// Tell the identity service about the settings and the live session. Does
    /// nothing between its twice-a-second turns.
    pub(crate) fn update_identity(&mut self) {
        self.remember_profile_hub_tab();
        self.update_sjk_chat();
        self.update_emotes();
        self.update_world_pings();
        self.update_muted_players();
        avatars::service(&self.ui_shapes, &self.queue);
        if let Some(console) = self.console.as_ref() {
            chat_gifs::set_enabled(console.bool_cvar(chat_gifs::CVAR) != Some(false));
            chat_gifs::favourites::load(console.config_directory());
            crate::jof_tag::set_shown(console.bool_cvar(crate::jof_tag::CVAR) != Some(false));
        }
        chat_gifs::service(&mut self.ui_shapes, &self.device, &self.queue);
        let due = player_identity::due();
        self.update_looks(due);
        if !due {
            return;
        }
        let Some(console) = self.console.as_ref() else {
            return;
        };
        let settings = Settings {
            enabled: console.bool_cvar("cl_identity") == Some(true),
            hub_url: console
                .text_cvar("cl_hubUrl")
                .unwrap_or_default()
                .trim()
                .to_owned(),
        };
        let name = console
            .text_cvar("name")
            .unwrap_or_default()
            .trim()
            .to_owned();
        let location = self.live_session.as_ref().and_then(|session| {
            player_identity::location(session.server(), session.is_local(), session.game_state())
        });
        let chat = console.bool_cvar("cl_sjkChat") != Some(false);
        // Pictures are read from the hub the identity talks to, and only while it is on.
        let pictures_from = if settings.enabled {
            settings.hub_url.as_str()
        } else {
            ""
        };
        avatars::configure(Some(console.config_directory()), pictures_from);
        player_identity::apply(console.config_directory(), settings, name, location, chat);
        self.offer_installed_models();
        let (packs_revision, assets_note) = player_identity::packs();
        crate::sjk_packs::follow_identity(packs_revision, assets_note);
        profile_card::refresh(console);
        self.offer_medals();
        self.offer_holocrons();
        let _ = player_identity::with_snapshot(|snapshot| crate::loot_odds::follow(&snapshot.loot));
        self.update_achievements();
        self.update_holocron_activity();
        self.follow_crash_reports();
    }

    /// Hand the identity the player models the game has and the one the player wears,
    /// for the player card's model line (`installed_models.rs`).
    fn offer_installed_models(&self) {
        if let Some(vfs) = self.vfs.as_ref() {
            installed_models::follow(vfs);
        }
        let (Some(models), Some(console)) = (installed_models::models(), self.console.as_ref())
        else {
            return;
        };
        let share = console.bool_cvar(installed_models::CVAR) != Some(false);
        player_identity::offer_models(&models, share);
        // What the server published for the player wins over the setting (a forced
        // team model, a mod's own).
        let published = self.live_session.as_ref().and_then(|session| {
            let game = session.game_state();
            let slot = usize::try_from(game.client_num).ok()?;
            let info = game.config_string(player_identity::CS_PLAYERS + slot)?;
            sjk_client::LegacyClientInfo::new(info)
                .text("model")
                .map(str::to_owned)
        });
        let model = published.or_else(|| console.text_cvar("model").ok().map(str::to_owned));
        let own = model
            .as_deref()
            .and_then(installed_models::folder)
            .filter(|folder| !models.retail.contains(folder));
        player_identity::set_own_model(own);
    }
}
