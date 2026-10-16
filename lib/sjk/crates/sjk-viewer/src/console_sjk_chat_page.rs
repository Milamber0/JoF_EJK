//! Console side of the SJK chat page (see `sjk_chat_panel.rs`): opening it, sending
//! what it asks (messages to the hub, mutes on this PC through the mute list in
//! `player_mutes.rs`, staff requests), and routing keys and pointer events to it, as
//! for the Staff page. Also `sjksay` and `sjkgif`, which say something in the SJK chat
//! from a bind, a quick wheel choice or the console, in a game or not.

use super::sjk_chat_panel::{Inputs, PanelAction, StaffShown};
use super::*;
use crate::chat_gifs::GifId;
use sjk_ui::InputEvent;

/// The `sjkchat` command.
pub(crate) const SJK_CHAT_COMMAND: &str = "sjkchat";
pub(crate) const SJK_CHAT_HELP: &str =
    "Open the SJK chat, which every SJK player shares through the SJK hub";
/// `sjksay <text>`: say it in the SJK chat.
pub(crate) const SJK_SAY_COMMAND: &str = "sjksay";
pub(crate) const SJK_SAY_HELP: &str =
    "Say something in the SJK chat, as a bind can: sjksay <text> (for a GIF, sjkgif)";
/// `sjkgif <favourite number | GIPHY id> [text]`: send a GIF to the SJK chat.
pub(crate) const SJK_GIF_COMMAND: &str = "sjkgif";
pub(crate) const SJK_GIF_HELP: &str =
    "Send a GIF to the SJK chat: sjkgif <favourite number, 1 the newest, or GIPHY id> [text]";

/// The words of `args` as the SJK chat takes them ([`crate::sjk_chat_field::paste_into`]:
/// spaces made one, characters the chat refuses left out, 150 characters at most).
fn say_text(args: &[String]) -> String {
    let mut text = String::new();
    crate::sjk_chat_field::paste_into(&mut text, &args.join(" "));
    text.trim().to_owned()
}

/// The GIF `which` names: favourite number `which` (1 the newest) of `favourites`, a
/// GIPHY link or a GIF's id.
fn gif_for(which: &str, favourites: &[GifId]) -> Result<GifId, String> {
    if let Ok(number) = which.parse::<usize>()
        && which.len() <= 3
    {
        return number
            .checked_sub(1)
            .and_then(|at| favourites.get(at))
            .cloned()
            .ok_or_else(|| match favourites.len() {
                0 => {
                    "sjkgif: no favourite GIFs yet (star some in SJK chat's GIF picker)".to_owned()
                }
                count => format!("sjkgif: favourites go from 1 to {count}"),
            });
    }
    crate::chat_gifs::link::find(which)
        .map(|(id, _)| id)
        .or_else(|| GifId::new(which))
        .ok_or_else(|| format!("sjkgif: {which} is not a favourite's number or a GIPHY id"))
}

/// `text` with `id`'s link after it, if the two fit one message.
fn gif_message(text: &str, id: &GifId) -> Option<String> {
    let link = crate::chat_gifs::link::page_url(id);
    let message = if text.is_empty() {
        link
    } else {
        format!("{text} {link}")
    };
    (message.chars().count() <= crate::sjk_chat_field::MAX).then_some(message)
}

impl ViewerConsole {
    /// `sjkchat`: show the page, or close it.
    pub(crate) fn toggle_sjk_chat_panel(&mut self) {
        if self.open && self.sjk_chat_panel.is_open() {
            self.close_sjk_chat_panel();
            return;
        }
        self.open_sjk_chat_panel();
    }

    /// Show the page (the main page's docked chat, the in-game SJK menu).
    pub(crate) fn open_sjk_chat_panel(&mut self) {
        let owns_console = !self.open;
        if !self.open {
            self.set_open(true);
        }
        self.browser.close();
        self.debug_panel.close();
        self.changelog.close();
        self.update_panel.close();
        self.config_import.close();
        self.credits.close();
        self.identity_panel.close();
        self.profile_panel.close();
        self.collection_panel.close();
        self.holocrons_panel.close();
        self.staff_panel.close();
        self.dead_key.settle();
        self.sjk_chat_panel.open(owns_console);
    }

    /// `sjksay <text>`.
    pub(crate) fn sjk_say_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let text = say_text(args);
        if text.is_empty() {
            return Err(format!("usage: {SJK_SAY_COMMAND} <text>"));
        }
        self.say_in_sjk_chat(text)
    }

    /// `sjkgif <favourite number | GIPHY id> [text]`.
    pub(crate) fn sjk_gif_command(&mut self, args: &[String]) -> Result<Vec<String>, String> {
        let Some(which) = args.first() else {
            return Err(format!(
                "usage: {SJK_GIF_COMMAND} <favourite number, 1 the newest, or GIPHY id> [text]"
            ));
        };
        let id = gif_for(which, &crate::chat_gifs::favourites::list())?;
        let text = gif_message(&say_text(&args[1..]), &id)
            .ok_or("sjkgif: the text is too long to go with the GIF")?;
        self.say_in_sjk_chat(text)
    }

    /// Send `text` to the SJK chat, as the page's Send does.
    fn say_in_sjk_chat(&mut self, text: String) -> Result<Vec<String>, String> {
        if self.bool_cvar("cl_sjkChat") == Some(false) {
            return Err("SJK chat is off (cl_sjkChat 1 turns it on)".to_owned());
        }
        if !crate::player_identity::chat(text) {
            return Err("the SJK identity is off (cl_identity 1)".to_owned());
        }
        Ok(Vec::new())
    }

    fn close_sjk_chat_panel(&mut self) {
        if self.sjk_chat_panel.close() {
            self.set_open(false);
        }
    }

    fn sjk_chat_panel_action(&mut self, action: PanelAction) {
        match action {
            PanelAction::None => {}
            PanelAction::Close => self.close_sjk_chat_panel(),
            PanelAction::Send(text) => {
                if self.bool_cvar("cl_sjkChat") == Some(false) {
                    self.push_log("^3sjkchat: SJK chat is off (cl_sjkChat 1 turns it on)");
                } else if !crate::player_identity::chat(text) {
                    self.push_log("^3sjkchat: the SJK identity is off (cl_identity 1)");
                }
            }
            PanelAction::Mute {
                key_id,
                name,
                muted,
            } => crate::player_mutes::set_muted(Some(&key_id), &name, muted),
            PanelAction::GiphyKey(key) => {
                // Saved with the profile; never logged.
                self.set_cvars([(crate::chat_gifs::search::KEY_CVAR, key.as_str())]);
            }
            PanelAction::Staff(request) => {
                let serial = crate::player_identity::staff_state().map_or(0, |state| state.serial);
                if crate::player_identity::staff(request) {
                    self.sjk_chat_panel.staff_sent(serial);
                } else {
                    self.push_log("^3sjkchat: the identity is not running");
                }
            }
        }
    }

    /// Give a pressed key to the open page; false when the page is closed.
    pub(super) fn sjk_chat_panel_key(&mut self, event: &KeyEvent) -> bool {
        if !self.sjk_chat_panel.is_open() {
            return false;
        }
        let action = self
            .sjk_chat_panel
            .handle_key(event, self.shift, self.control);
        self.sjk_chat_panel_action(action);
        true
    }

    /// Give a pointer event to the open page; false when the page is closed.
    pub(super) fn sjk_chat_panel_pointer(&mut self, event: InputEvent) -> bool {
        if !self.sjk_chat_panel.is_open() {
            return false;
        }
        let action = self.sjk_chat_panel.handle_pointer(event);
        self.sjk_chat_panel_action(action);
        true
    }

    /// Draw the page with its text to `target`.
    pub(super) fn append_sjk_chat_panel(
        &mut self,
        target: crate::menu::sjk::TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        self.sjk_chat_panel.place_card(
            crate::player_mutes::place,
            crate::player_identity::avatar_version,
        );
        self.sjk_chat_panel
            .find_pictures(crate::player_identity::avatar_version);
        let muted = crate::player_mutes::muted_keys();
        let staff = crate::player_identity::is_staff();
        let staff_state = staff.then(crate::player_identity::staff_state).flatten();
        let staff_shown = staff_state.as_ref().map(|state| StaffShown {
            serial: state.serial,
            message: &state.message,
            failed: state.failed,
            busy: state.busy,
        });
        let enabled = self.bool_cvar("cl_sjkChat") != Some(false);
        let now = crate::menu::sjk::recent::now();
        // A copy, drawn once the lock is let go: drawing asks the identity for the
        // player's own key (to keep it off the chosen message and the sender card),
        // which takes that lock again and would wait on itself for ever.
        // `None`: the identity is not running, and the page says so.
        let chat = crate::player_identity::with_chat(Clone::clone);
        #[cfg(test)]
        let chat = self.sjk_chat_panel.preview.clone().or(chat);
        let giphy_key = self
            .text_cvar(crate::chat_gifs::search::KEY_CVAR)
            .map(str::to_owned)
            .unwrap_or_default();
        let inputs = Inputs {
            chat: chat.as_ref(),
            muted: &muted,
            staff,
            enabled,
            now,
            staff_state: staff_shown,
            giphy_key: &giphy_key,
        };
        self.sjk_chat_panel.append_sjk(&inputs, target, viewport);
    }

    /// Open the page on `chat` in place of the live one, `chosen` the message chosen,
    /// for a world shot.
    #[cfg(test)]
    pub(crate) fn preview_sjk_chat(&mut self, chat: sjk_identity::ChatState, chosen: Option<u64>) {
        self.open_sjk_chat_panel();
        self.sjk_chat_panel.preview = Some(chat);
        if let Some(id) = chosen {
            self.sjk_chat_panel.choose_for_shot(id);
        }
    }

    /// Open the page on `chat` with the GIF picker on tab `tab`, `typed` searched, for a
    /// world shot.
    #[cfg(test)]
    pub(crate) fn preview_sjk_chat_gifs(
        &mut self,
        chat: sjk_identity::ChatState,
        tab: usize,
        typed: &str,
    ) {
        self.preview_sjk_chat(chat, None);
        self.sjk_chat_panel.open_gifs_for_shot(tab, typed);
    }

    /// Open the page on `chat` with the GIF picker's key card, `typed` in its field, for
    /// a world shot.
    #[cfg(test)]
    pub(crate) fn preview_sjk_chat_key_card(&mut self, chat: sjk_identity::ChatState, typed: &str) {
        self.preview_sjk_chat(chat, None);
        self.sjk_chat_panel.open_key_card_for_shot(typed);
    }

    /// Answer the open picker's search with `ids` of `total`, for a world shot.
    #[cfg(test)]
    pub(crate) fn answer_sjk_chat_gif_search(
        &mut self,
        ids: Vec<crate::chat_gifs::GifId>,
        total: usize,
    ) {
        self.sjk_chat_panel.answer_gif_search_for_shot(ids, total);
    }

    /// Rest the pointer on a player of the open page's who-is-online window, for a world
    /// shot.
    #[cfg(test)]
    pub(crate) fn sjk_chat_hover_person(&mut self, index: Option<u16>) {
        self.sjk_chat_panel.hover_person_for_shot(index);
    }

    /// The page's draw list while it is shown.
    pub(super) fn sjk_chat_panel_draw_list(&self) -> Option<&sjk_ui::DrawList> {
        (self.open && self.sjk_chat_panel.is_open()).then(|| self.sjk_chat_panel.draw_list())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(line: &str) -> Vec<String> {
        line.split(' ').map(str::to_owned).collect()
    }

    #[test]
    fn sjksay_says_its_words_as_the_chat_takes_them() {
        assert_eq!(say_text(&words("gg   wp")), "gg wp");
        assert_eq!(say_text(&[]), "");
        assert_eq!(say_text(&words(&"a".repeat(200))).chars().count(), 150);
    }

    #[test]
    fn sjkgif_takes_a_favourites_number_a_link_or_an_id() {
        let favourites: Vec<GifId> = ["Newest01xyz", "Older002xyz"]
            .iter()
            .map(|id| GifId::new(id).unwrap())
            .collect();
        assert_eq!(gif_for("1", &favourites).unwrap(), favourites[0]);
        assert_eq!(gif_for("2", &favourites).unwrap(), favourites[1]);
        assert!(gif_for("3", &favourites).unwrap_err().contains("1 to 2"));
        assert!(gif_for("0", &favourites).is_err());
        assert!(gif_for("1", &[]).unwrap_err().contains("no favourite"));
        let id = GifId::new("3o7TKSjRrfIPjeiVyM").unwrap();
        assert_eq!(gif_for("3o7TKSjRrfIPjeiVyM", &favourites).unwrap(), id);
        assert_eq!(
            gif_for("https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM", &favourites).unwrap(),
            id
        );
        assert_eq!(
            gif_message("gg", &id).as_deref(),
            Some("gg https://giphy.com/gifs/3o7TKSjRrfIPjeiVyM")
        );
        assert_eq!(gif_message(&"a".repeat(140), &id), None);
    }
}
