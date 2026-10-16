//! Looks (`docs/unlockables.md`, "Receiving"): what each player on the game server
//! wears that SJK draws, the blade skin and whether their Illuminate holocron is lit.
//! Other players' looks come through the SJK hub ([`sjk_identity::Look`]): from the
//! presence roster when it changes and from the feed's look events as they come. The
//! local player's own look comes from its own settings (`cg_saberSkin`, gated by the
//! unlocks of its hub profile, and its Illuminate), never from the hub. Blade skins are
//! named by their ids in the catalogue ([`crate::unlockables`]).
//!
//! A hub look counts only while the game shows, in its slot, the name the claim was
//! made under (the badges' and emotes' rule, [`sjk_identity::names_match`]). The
//! table is rebuilt when a roster or an event comes and twice a second, so a player
//! who leaves a slot loses the look at once for the renderer; a frame only reads the
//! fixed [`Worn`] table.
//!
//! The renderer reads [`Looks::saber_skin_id`], [`Looks::illuminated`] and
//! [`Looks::own_saber_skin`] on `GpuState::looks`, and [`Looks::revision`] tells it
//! when what is worn changed (`GpuState::sync_saber_skins`). A player muted on this PC
//! (`docs/hub-chat.md`, "Muting a player") wears nothing: the stock blade, its sounds,
//! no holocron ([`Looks::set_muted`]).

use sjk_identity::Look;
use sjk_protocol::{GameState, PlayerState};

/// Slots the table holds, as the emotes' do.
pub(crate) const SLOTS: usize = 64;

/// Whose slots a session's view involves: the local player's own (the game state's
/// `client_num`) and the one whose player state the snapshot carries. They differ
/// while the local player follows (spectates) someone: the server then sends the
/// followed player's state, flagged `PMF_FOLLOW`, under their `client_num`, and leaves
/// their entity out of the snapshot. The own look belongs in the own slot only; the
/// followed player wears their hub look.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct ViewSlots {
    /// The local player's own slot.
    pub(crate) own: Option<usize>,
    /// The slot whose player state the view shows.
    pub(crate) viewed: usize,
}

impl ViewSlots {
    /// The slots of a session with `game_state` whose latest player state is `player`.
    pub(crate) fn of(game_state: &GameState, player: &PlayerState) -> Self {
        Self {
            own: usize::try_from(game_state.client_num).ok(),
            viewed: usize::from(player.client_num()),
        }
    }

    /// The player the view follows, when it is not the local player's own.
    pub(crate) fn followed(self) -> Option<usize> {
        (Some(self.viewed) != self.own).then_some(self.viewed)
    }
}

/// The catalogue's id of the blade skin `id` names, if this client knows it; a look
/// naming any other id draws the stock blade.
fn blade_skin(id: &str) -> Option<&'static str> {
    crate::unlockables::blade_skin(id).map(|unlockable| unlockable.id)
}

/// What one player wears, as this client draws it.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Worn {
    /// The blade skin, a catalogue id; `None` for the stock blade.
    pub(crate) saber_skin: Option<&'static str>,
    /// The Illuminate holocron is lit.
    pub(crate) illuminate: bool,
    /// The blade skin's options its wearer switched off (`crate::saber_skin_options`).
    pub(crate) options_off: crate::saber_skin_options::OptionsOff,
}

impl Worn {
    /// What `look` draws as here: an unknown skin is the stock blade.
    pub(crate) fn of(look: &Look) -> Self {
        let saber_skin = blade_skin(&look.saber);
        Self {
            saber_skin,
            illuminate: look.illuminate,
            options_off: saber_skin.map_or(0, |_| {
                crate::saber_skin_options::bits(look.saber_off.iter().map(String::as_str))
            }),
        }
    }

    /// The look the hub is told the local player wears: the skin only when the own
    /// profile lists it (`owned`), with the options `options` (`cg_saberSkinOptions`)
    /// switches off for it, and the local holocron's lit state.
    pub(crate) fn own(
        skin_setting: &str,
        options: &str,
        owned: impl Fn(&str) -> bool,
        lit: bool,
    ) -> Self {
        let saber_skin = blade_skin(skin_setting).filter(|skin| owned(skin));
        Self {
            saber_skin,
            illuminate: lit,
            options_off: saber_skin
                .map_or(0, |skin| crate::saber_skin_options::off_bits(options, skin)),
        }
    }

    /// The wire form, for `Service::set_look`, with the options `options` switches off
    /// for its skin.
    pub(crate) fn to_look(self, options: &str) -> Look {
        let saber = self.saber_skin.unwrap_or_default();
        Look {
            saber: saber.to_owned(),
            illuminate: self.illuminate,
            saber_off: if saber.is_empty() {
                Vec::new()
            } else {
                crate::saber_skin_options::off_list(options, saber)
            },
        }
    }
}

/// A look the hub relayed for a slot, with the name its claim was made under.
#[derive(Clone, Debug, PartialEq)]
struct Claimed {
    claimed_name: String,
    worn: Worn,
}

/// Every slot's look on the current game server.
pub(crate) struct Looks {
    /// What the hub said each slot wears.
    claimed: [Option<Claimed>; SLOTS],
    /// What each slot is drawn wearing: the claimed looks whose names match, and the
    /// local player's own.
    worn: [Worn; SLOTS],
    /// The local player's slot on the server, if in a game.
    own_slot: Option<u8>,
    /// The local player's own look, gated.
    own: Worn,
    /// The roster's revision last taken, so it is read again only when it changes.
    pub(crate) roster_revision: Option<u64>,
    /// The server the hub looks are for.
    pub(crate) server: Option<std::net::SocketAddr>,
    /// The feed's reading the hub looks are from (`ReceivedLooks::generation`).
    feed_generation: Option<u64>,
    /// Counts changes to [`Self::worn`] and [`Self::muted`], so a reader can follow
    /// it cheaply.
    revision: u64,
    /// Client slots muted on this PC (bits, `muted_players.rs`): their looks are not
    /// drawn.
    muted: u32,
    /// `cg_saberSkinOptions` as last read (twice a second), for the skins the
    /// Collection shows on the model.
    own_options: String,
    /// World shots own every unlock, having no hub.
    #[cfg(test)]
    pub(crate) shot_owns_unlocks: bool,
}

impl Default for Looks {
    fn default() -> Self {
        Self {
            claimed: std::array::from_fn(|_| None),
            worn: [Worn::default(); SLOTS],
            own_slot: None,
            own: Worn::default(),
            roster_revision: None,
            server: None,
            feed_generation: None,
            revision: 0,
            muted: 0,
            own_options: String::new(),
            #[cfg(test)]
            shot_owns_unlocks: false,
        }
    }
}

impl Looks {
    /// The blade skin the player in `client` wears, a catalogue id; `None` for the
    /// stock blade (and for a slot out of range or muted on this PC).
    pub(crate) fn saber_skin_id(&self, client: usize) -> Option<&'static str> {
        self.shown(client)?.saber_skin
    }

    /// The options the player in `client` switched off on their blade skin.
    pub(crate) fn options_off(&self, client: usize) -> crate::saber_skin_options::OptionsOff {
        self.shown(client).map_or(0, |worn| worn.options_off)
    }

    /// Whether the player in `client` has their Illuminate holocron lit (and is not
    /// muted on this PC).
    pub(crate) fn illuminated(&self, client: usize) -> bool {
        self.shown(client).is_some_and(|worn| worn.illuminate)
    }

    /// What the player in `client` is drawn wearing: nothing while they are muted on
    /// this PC, which never mutes the local player's own slot.
    fn shown(&self, client: usize) -> Option<&Worn> {
        let muted = client < 32
            && self.muted & (1 << client) != 0
            && self.own_slot.map(usize::from) != Some(client);
        self.worn.get(client).filter(|_| !muted)
    }

    /// The client slots muted on this PC changed (or not: this is called every frame):
    /// their looks are no longer drawn, or are again.
    pub(crate) fn set_muted(&mut self, slots: u32) {
        if self.muted != slots {
            self.muted = slots;
            self.revision += 1;
        }
    }

    /// The local player's own blade skin, gated by its profile's unlocks: for the
    /// first-person blade and the Character page's preview, in a game or not.
    pub(crate) fn own_saber_skin(&self) -> Option<&'static str> {
        self.own.saber_skin
    }

    /// The local player's own look.
    pub(crate) fn own(&self) -> Worn {
        self.own
    }

    /// Keep `cg_saberSkinOptions` as read; a change counts as one to what is worn, so
    /// the skins are taken again.
    pub(crate) fn set_own_options(&mut self, options: String) {
        if options != self.own_options {
            self.own_options = options;
            self.revision += 1;
        }
    }

    /// The options the local player switched off on blade skin `skin`, worn or only
    /// shown (the Collection's preview on the model).
    pub(crate) fn own_options_off(&self, skin: &str) -> crate::saber_skin_options::OptionsOff {
        crate::saber_skin_options::off_bits(&self.own_options, skin)
    }

    /// Changes whenever what a slot is drawn wearing (or the own look) changes.
    pub(crate) fn revision(&self) -> u64 {
        self.revision
    }

    /// Forget every hub look: another server, or none.
    pub(crate) fn clear(&mut self, server: Option<std::net::SocketAddr>) {
        self.claimed = std::array::from_fn(|_| None);
        self.roster_revision = None;
        self.server = server;
        self.rebuild(|_| None);
    }

    /// Follow the feed's reading: when its `generation` changes (the identity went off
    /// or on, another hub or server), every hub look goes and the roster is read
    /// again, so no look from before, a late poll's included, stays. True when the
    /// looks were cleared.
    pub(crate) fn follow_feed(&mut self, generation: u64) -> bool {
        if self.feed_generation == Some(generation) {
            return false;
        }
        self.feed_generation = Some(generation);
        self.clear(self.server);
        true
    }

    /// Take the presence roster's looks in place of every hub look: `(slot, claimed
    /// name, look)` for each live claim, a claim without a look wearing none.
    pub(crate) fn replace_roster<'a>(
        &mut self,
        roster: impl IntoIterator<Item = (u8, &'a str, Option<&'a Look>)>,
    ) {
        self.claimed = std::array::from_fn(|_| None);
        for (slot, claimed_name, look) in roster {
            if let Some(look) = look {
                self.set(slot, claimed_name, look);
            }
        }
    }

    /// Take a look event from the feed: newer than the roster, so it wins until the
    /// roster changes again.
    pub(crate) fn apply_event(&mut self, slot: u8, claimed_name: &str, look: &Look) {
        self.set(slot, claimed_name, look);
    }

    fn set(&mut self, slot: u8, claimed_name: &str, look: &Look) {
        if let Some(entry) = self.claimed.get_mut(usize::from(slot)) {
            *entry = Some(Claimed {
                claimed_name: claimed_name.to_owned(),
                worn: Worn::of(look),
            });
        }
    }

    /// The local player's slot on the server, if in a game.
    pub(crate) fn own_slot(&self) -> Option<u8> {
        self.own_slot
    }

    /// The local player's slot (`None` out of a game) and own look. A slot it left
    /// wears nothing until the next [`Looks::rebuild`].
    pub(crate) fn set_own(&mut self, slot: Option<u8>, own: Worn) {
        if (slot, own) != (self.own_slot, self.own) {
            self.revision += 1;
        }
        if slot != self.own_slot
            && let Some(left) = self
                .own_slot
                .and_then(|left| self.worn.get_mut(usize::from(left)))
        {
            *left = Worn::default();
        }
        self.own_slot = slot;
        self.own = own;
        if let Some(entry) = slot.and_then(|slot| self.worn.get_mut(usize::from(slot))) {
            *entry = own;
        }
    }

    /// Rebuild what each slot is drawn wearing; `shown` is the name the game shows in
    /// a slot. Only slots with a hub look ask for it.
    pub(crate) fn rebuild(&mut self, shown: impl Fn(u8) -> Option<String>) {
        let before = self.worn;
        for (slot, (worn, claimed)) in self.worn.iter_mut().zip(&self.claimed).enumerate() {
            *worn = claimed
                .as_ref()
                .filter(|claimed| {
                    u8::try_from(slot)
                        .ok()
                        .and_then(&shown)
                        .is_some_and(|shown| {
                            sjk_identity::names_match(&claimed.claimed_name, &shown)
                        })
                })
                .map_or_else(Worn::default, |claimed| claimed.worn);
        }
        let (slot, own) = (self.own_slot, self.own);
        self.set_own(slot, own);
        if self.worn != before {
            self.revision += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn look(saber: &str, illuminate: bool) -> Look {
        Look {
            saber: saber.to_owned(),
            illuminate,
            saber_off: Vec::new(),
        }
    }

    /// The game shows `^2Sol` in slot 3 and `Fox` in slot 5.
    fn shown(slot: u8) -> Option<String> {
        match slot {
            3 => Some("^2Sol".to_owned()),
            5 => Some("Fox".to_owned()),
            _ => None,
        }
    }

    #[test]
    fn a_hub_look_counts_only_under_the_name_the_game_shows() {
        let mut looks = Looks::default();
        let sun = look("saber_sun", true);
        looks.replace_roster([
            (3, "sol", Some(&sun)),
            (5, "Wolf", Some(&sun)),
            (7, "Kit", Some(&sun)),
            (9, "Ren", None),
        ]);
        looks.rebuild(shown);
        assert_eq!(looks.saber_skin_id(3), Some("saber_sun"));
        assert!(looks.illuminated(3));
        assert_eq!(looks.saber_skin_id(5), None, "another name in the slot");
        assert!(!looks.illuminated(5));
        assert!(!looks.illuminated(7), "nobody in the slot");
        assert!(!looks.illuminated(9));
        assert!(!looks.illuminated(200), "no such slot");
        // Someone else takes slot 3: the look goes with the next rebuild.
        looks.rebuild(|slot| (slot == 3).then(|| "Fox".to_owned()));
        assert!(!looks.illuminated(3));
    }

    #[test]
    fn following_someone_the_view_is_theirs_and_the_own_slot_stays() {
        // `PMF_FOLLOW`: the server sends the followed player's state.
        const PMF_FOLLOW: u16 = 0x1000;
        let game_state = GameState::empty_local(2);
        let mut player = PlayerState::zero();
        player.set_client_num(2);
        let playing = ViewSlots::of(&game_state, &player);
        assert_eq!((playing.own, playing.viewed), (Some(2), 2));
        assert_eq!(playing.followed(), None);
        player.set_client_num(5);
        player.set_movement_flags(PMF_FOLLOW);
        let following = ViewSlots::of(&game_state, &player);
        assert_eq!(following.own, Some(2), "the game state's, not the state's");
        assert_eq!(following.followed(), Some(5));
    }

    #[test]
    fn an_unknown_blade_skin_is_the_stock_blade() {
        let mut looks = Looks::default();
        looks.apply_event(3, "Sol", &look("saber_from_the_future", true));
        looks.rebuild(shown);
        assert_eq!(looks.saber_skin_id(3), None);
        assert!(looks.illuminated(3), "the rest of the look still counts");
    }

    #[test]
    fn feed_events_win_until_the_roster_changes() {
        let mut looks = Looks::default();
        looks.replace_roster([(3, "Sol", Some(&look("", false)))]);
        looks.rebuild(shown);
        assert!(!looks.illuminated(3));
        // The feed's event is newer than the roster.
        looks.apply_event(3, "Sol", &look("saber_sun", true));
        looks.apply_event(5, "Fox", &look("", true));
        looks.rebuild(shown);
        assert_eq!(looks.saber_skin_id(3), Some("saber_sun"));
        assert!(looks.illuminated(5));
        // A changed roster replaces every hub look: slot 5's claim has none now.
        looks.replace_roster([(3, "Sol", Some(&look("", true)))]);
        looks.rebuild(shown);
        assert_eq!(looks.saber_skin_id(3), None);
        assert!(looks.illuminated(3));
        assert!(!looks.illuminated(5));
    }

    #[test]
    fn another_server_clears_the_hub_looks_but_not_the_own_one() {
        let mut looks = Looks::default();
        looks.apply_event(3, "Sol", &look("saber_sun", true));
        looks.roster_revision = Some(4);
        let own = Worn {
            saber_skin: Some("saber_sun"),
            illuminate: true,
            options_off: 0,
        };
        looks.set_own(Some(1), own);
        looks.rebuild(shown);
        assert!(looks.illuminated(3));
        let elsewhere = "5.6.7.8:29070".parse().ok();
        looks.clear(elsewhere);
        assert!(!looks.illuminated(3));
        assert_eq!(looks.roster_revision, None);
        assert_eq!(looks.server, elsewhere);
        assert_eq!(looks.own(), own);
        assert!(looks.illuminated(1), "the local player's own slot");
    }

    #[test]
    fn a_new_feed_reading_clears_the_hub_looks() {
        let mut looks = Looks::default();
        assert!(looks.follow_feed(1), "the first reading");
        assert!(!looks.follow_feed(1));
        looks.roster_revision = Some(4);
        looks.apply_event(3, "Sol", &look("saber_sun", true));
        looks.rebuild(shown);
        assert!(looks.illuminated(3));
        // The identity went off: a look applied before must not stay.
        assert!(looks.follow_feed(2));
        looks.rebuild(shown);
        assert!(!looks.illuminated(3));
        assert_eq!(looks.saber_skin_id(3), None);
        assert_eq!(looks.roster_revision, None, "the roster is read again");
    }

    #[test]
    fn the_own_look_comes_from_the_settings_not_the_hub() {
        let mut looks = Looks::default();
        // The hub's roster lists the local player's own claim too.
        looks.replace_roster([(3, "Sol", Some(&look("saber_sun", true)))]);
        looks.set_own(Some(3), Worn::default());
        looks.rebuild(shown);
        assert_eq!(looks.saber_skin_id(3), None);
        assert!(!looks.illuminated(3));
        // Out of a game the own look still holds, for the Character page.
        let own = Worn::own("saber_sun", "", |_| true, false);
        looks.set_own(None, own);
        assert_eq!(looks.own_saber_skin(), Some("saber_sun"));
    }

    #[test]
    fn a_muted_players_look_is_not_drawn() {
        let mut looks = Looks::default();
        looks.replace_roster([
            (3, "Sol", Some(&look("saber_sun", true))),
            (5, "Fox", Some(&look("saber_sun", true))),
        ]);
        looks.set_own(Some(1), Worn::own("saber_sun", "", |_| true, true));
        looks.rebuild(shown);
        let before = looks.revision();
        // Slot 3 muted on this PC: the stock blade (so no skin sounds) and no holocron.
        looks.set_muted(1 << 3 | 1 << 1);
        assert!(looks.revision() > before, "the blades follow");
        assert_eq!(looks.saber_skin_id(3), None);
        assert!(!looks.illuminated(3));
        assert_eq!(looks.saber_skin_id(5), Some("saber_sun"), "nobody else");
        assert!(looks.illuminated(5));
        // The local player's own slot is never muted.
        assert_eq!(looks.saber_skin_id(1), Some("saber_sun"));
        assert!(looks.illuminated(1));
        let muted = looks.revision();
        looks.set_muted(1 << 3 | 1 << 1);
        assert_eq!(looks.revision(), muted, "no change, no new revision");
        looks.set_muted(0);
        assert_eq!(looks.saber_skin_id(3), Some("saber_sun"));
        assert!(looks.illuminated(3));
    }

    #[test]
    fn the_revision_follows_what_is_worn() {
        let mut looks = Looks::default();
        let start = looks.revision();
        looks.rebuild(shown);
        assert_eq!(looks.revision(), start, "nothing worn changed");
        looks.apply_event(3, "Sol", &look("saber_sun", false));
        assert_eq!(looks.revision(), start, "not drawn before the rebuild");
        looks.rebuild(shown);
        let worn = looks.revision();
        assert!(worn > start);
        looks.rebuild(shown);
        assert_eq!(looks.revision(), worn);
        looks.set_own(Some(1), Worn::own("saber_sun", "", |_| true, false));
        assert!(looks.revision() > worn);
        let own = looks.revision();
        looks.set_own(Some(1), Worn::own("saber_sun", "", |_| true, false));
        assert_eq!(looks.revision(), own);
    }

    #[test]
    fn the_own_blade_skin_needs_the_unlock_and_a_known_id() {
        let owned = |id: &str| id == "saber_sun";
        assert_eq!(
            Worn::own(" saber_sun ", "", owned, true),
            Worn {
                saber_skin: Some("saber_sun"),
                illuminate: true,
                options_off: 0,
            }
        );
        assert_eq!(
            Worn::own("saber_sun", "", |_| false, false).saber_skin,
            None
        );
        assert_eq!(
            Worn::own("saber_moon", "", |_| true, false).saber_skin,
            None
        );
        assert_eq!(Worn::own("", "", owned, true).to_look(""), look("", true));
        assert_eq!(
            Worn::own("saber_sun", "", owned, false).to_look(""),
            look("saber_sun", false)
        );
    }

    #[test]
    fn the_own_look_carries_the_worn_skins_options_switched_off() {
        let options = "saber_sun.haze saber_spectral.wisps saber_sun.glint";
        let own = Worn::own("saber_sun", options, |_| true, false);
        assert_eq!(
            own.options_off,
            crate::saber_skin_options::bits(["glint", "haze"])
        );
        let sent = own.to_look(options);
        assert_eq!(
            sent.saber_off,
            ["glint", "haze"],
            "sorted, the worn skin's only"
        );
        // Read back from the hub, the same choice.
        assert_eq!(Worn::of(&sent), own);
        // The stock blade sends none, and an unowned skin is the stock blade.
        let stock = Worn::own("saber_sun", options, |_| false, false);
        assert_eq!(stock.options_off, 0);
        assert!(stock.to_look(options).saber_off.is_empty());
        // A slot shows its wearer's choice; a muted one none.
        let mut looks = Looks::default();
        looks.apply_event(3, "Sol", &sent);
        looks.rebuild(shown);
        assert_eq!(looks.options_off(3), own.options_off);
        looks.set_muted(1 << 3);
        assert_eq!(looks.options_off(3), 0);
    }
}
