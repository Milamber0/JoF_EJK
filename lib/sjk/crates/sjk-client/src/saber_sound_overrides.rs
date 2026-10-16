//! Per-player additions to the stock saber sounds: a blade skin (an SJK unlockable,
//! drawn by the viewer) brings its own ignition, switching off, hum and swings, heard for
//! every player wearing it and only for them, on top of the stock sounds, which keep
//! playing below.
//!
//! The viewer registers its skins' sound sets once per gamestate
//! ([`LegacySoundAdapter::register_saber_sound_sets`]) and tells the adapter every frame
//! which client slot wears which set
//! ([`LegacySoundAdapter::set_saber_sound_overrides`]). With no set worn, every path
//! below is the compatibility one, unchanged. Added, for a client wearing a set (each
//! on a channel of its own, so it does not cut the stock sound):
//!
//! - `EV_SABER_UNHOLSTER` (the ignition the game predicts) plays the set's `on`;
//! - cgame's own ignition and switching off as the player draws the saber from another
//!   weapon or puts a lit one away ([`super::saber_switch`]) play `on` or `off`, once;
//! - `EV_GENERAL_SOUND` of a stock `saberon*`/`saberoff*` sound, or of any `.sab`
//!   `soundOn`/`soundOff`, plays `on`/`off`. The game's `G_Sound` puts these on a
//!   temporary entity with no owner (`g_utils.c`), so the sound is taken as the
//!   player's whose origin is nearest the event's, within [`TOGGLE_REACH`];
//! - `EV_SABER_ATTACK` plays one of the three `swings` (as a `.sab` with
//!   `swingSound1`-`3` does, `cg_event.c`);
//! - the hum loop ([`crate::loop_sounds`]) plays `hum` once, whichever sabers are lit,
//!   and for the saber while it is thrown (over the flying saber entity's own
//!   `loopSound`).
//!
//! Animation swing cues are the viewer's (`actor_sounds.rs`).

use super::{LegacySoundAdapter, RegisteredLegacySound, intern_sound};
use sjk_audio::SoundHandle;
use sjk_protocol::Snapshot;
use sjk_vfs::VirtualFileSystem;

/// Client slots a set can be worn in (`MAX_CLIENTS`).
pub const SABER_SOUND_CLIENTS: usize = 32;
/// How far from a player a saber on/off sound may start and still be that player's:
/// the game sounds it from where the player stood when the command ran, which the
/// rest of the server frame can move by a few units.
pub const TOGGLE_REACH: f32 = 64.0;
const ET_PLAYER: u8 = 1;

/// The game paths of one set of replacement saber sounds.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SaberSoundSet<'a> {
    /// Ignition.
    pub on: &'a str,
    /// Switching off.
    pub off: &'a str,
    /// The hum, looped while a blade is lit.
    pub hum: &'a str,
    /// Swings, chosen like `.sab` `swingSound1`-`3`.
    pub swings: [&'a str; 3],
}

/// A set's sounds in the event adapter's table.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct EventSet {
    on: u16,
    off: u16,
    swings: [u16; 3],
}

/// Whether a sound switches a saber on or off.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Toggle {
    On,
    Off,
}

/// Which client wears which set, and each set's event sounds.
#[derive(Clone, Debug, Default)]
pub(crate) struct SaberSoundOverrides {
    sets: Vec<EventSet>,
    clients: [Option<u8>; SABER_SOUND_CLIENTS],
    /// Whether any client wears a set, so stock events skip every check.
    active: bool,
    /// The `.sab` definitions' own on/off sounds, lowercase and without extension.
    toggles: Vec<(Box<str>, Toggle)>,
}

impl SaberSoundOverrides {
    fn register(
        &mut self,
        sets: &[SaberSoundSet<'_>],
        sounds: &mut Vec<RegisteredLegacySound>,
        vfs: &VirtualFileSystem,
        register: &mut impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        let mut intern = |path: &str| intern_sound(sounds, vfs, path, register);
        self.sets = sets
            .iter()
            .map(|set| EventSet {
                on: intern(set.on),
                off: intern(set.off),
                swings: set.swings.map(&mut intern),
            })
            .collect();
        self.toggles.clear();
        let definitions = crate::shared_saber_definitions(vfs).unwrap_or_default();
        for definition in definitions.values() {
            for (path, toggle) in [
                (definition.sound_on.as_deref(), Toggle::On),
                (definition.sound_off.as_deref(), Toggle::Off),
            ] {
                let Some(stem) = path.map(stem) else {
                    continue;
                };
                if !stem.is_empty()
                    && !self
                        .toggles
                        .iter()
                        .any(|(known, _)| known.eq_ignore_ascii_case(stem))
                {
                    self.toggles
                        .push((stem.to_ascii_lowercase().into(), toggle));
                }
            }
        }
        let clients = self.clients;
        self.set_clients(&clients);
    }

    fn set_clients(&mut self, clients: &[Option<u8>; SABER_SOUND_CLIENTS]) {
        self.clients = clients.map(|set| set.filter(|&set| usize::from(set) < self.sets.len()));
        self.active = self.clients.iter().any(Option::is_some);
    }

    /// The set entity `source` wears: a client's body (`source` < 32) with one.
    fn for_source(&self, source: u16) -> Option<EventSet> {
        if !self.active {
            return None;
        }
        let set = (*self.clients.get(usize::from(source))?)?;
        self.sets.get(usize::from(set)).copied()
    }

    /// The set's swing for `variant`, as `ci->saber[0].swingSound[Q_irand(0, 2)]`.
    pub(crate) fn swing(&self, source: u16, variant: usize) -> Option<u16> {
        self.for_source(source).map(|set| set.swings[variant % 3])
    }

    /// The set's ignition.
    pub(crate) fn on(&self, source: u16) -> Option<u16> {
        self.for_source(source).map(|set| set.on)
    }

    /// The set's switching off.
    pub(crate) fn off(&self, source: u16) -> Option<u16> {
        self.for_source(source).map(|set| set.off)
    }

    /// The replacement for a general sound `sound` started at `origin`: the set's on or
    /// off when `sound` switches a saber and the nearest player wears a set.
    pub(crate) fn general(
        &self,
        sound: Option<u16>,
        sounds: &[RegisteredLegacySound],
        origin: [f32; 3],
        snapshot: &Snapshot,
    ) -> Option<u16> {
        if !self.active {
            return None;
        }
        let path = &sounds.get(usize::from(sound?))?.path;
        let toggle = self.toggle(path)?;
        let set = self.for_source(nearest_player(snapshot, origin)?)?;
        Some(match toggle {
            Toggle::On => set.on,
            Toggle::Off => set.off,
        })
    }

    fn toggle(&self, path: &str) -> Option<Toggle> {
        let stem = stem(path);
        let file = stem.rsplit('/').next().unwrap_or(stem);
        let stock = stem.len() > file.len()
            && stem[..stem.len() - file.len()].eq_ignore_ascii_case("sound/weapons/saber/");
        let starts = |prefix: &str| {
            file.get(..prefix.len())
                .is_some_and(|start| start.eq_ignore_ascii_case(prefix))
        };
        if stock && starts("saberon") {
            return Some(Toggle::On);
        }
        if stock && starts("saberoff") {
            return Some(Toggle::Off);
        }
        self.toggles
            .iter()
            .find(|(known, _)| known.eq_ignore_ascii_case(stem))
            .map(|(_, toggle)| *toggle)
    }
}

/// `path` without its extension.
fn stem(path: &str) -> &str {
    match path.rsplit_once('.') {
        Some((stem, extension)) if !extension.contains('/') => stem,
        _ => path,
    }
}

/// The client whose body is nearest `origin`, within [`TOGGLE_REACH`]: the local
/// player from its own state, others from their entities.
fn nearest_player(snapshot: &Snapshot, origin: [f32; 3]) -> Option<u16> {
    let distance = |at: [f32; 3]| {
        (0..3)
            .map(|axis| (at[axis] - origin[axis]).powi(2))
            .sum::<f32>()
    };
    let local = (
        snapshot.player.client_num(),
        distance(snapshot.player.origin()),
    );
    snapshot
        .entities
        .iter()
        .filter(|state| {
            state.entity_type() == ET_PLAYER && usize::from(state.number()) < SABER_SOUND_CLIENTS
        })
        .map(|state| (state.number(), distance(state.trajectory_base())))
        .chain(std::iter::once(local))
        .filter(|(_, squared)| *squared <= TOGGLE_REACH * TOGGLE_REACH)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(client, _)| client)
}

impl LegacySoundAdapter {
    /// Register replacement saber sound sets; a set is then worn by its index here.
    /// Call once after [`Self::new`], with the same `register`, before snapshots are
    /// observed (registration reads the files and the `.sab` definitions).
    pub fn register_saber_sound_sets(
        &mut self,
        sets: &[SaberSoundSet<'_>],
        vfs: &VirtualFileSystem,
        mut register: impl FnMut(&str, &[u8]) -> Option<SoundHandle>,
    ) {
        self.saber_overrides
            .register(sets, &mut self.sounds, vfs, &mut register);
        let hums: Vec<&str> = sets.iter().map(|set| set.hum).collect();
        self.loops.register_saber_hums(&hums, vfs, &mut register);
    }

    /// Which registered set each client slot wears (`None`: the stock sounds). Cheap
    /// (two 32-entry copies); the viewer calls it every frame.
    pub fn set_saber_sound_overrides(&mut self, clients: &[Option<u8>; SABER_SOUND_CLIENTS]) {
        if *clients == self.saber_overrides.clients {
            return;
        }
        self.saber_overrides.set_clients(clients);
        self.loops
            .set_saber_hum_overrides(&self.saber_overrides.clients);
    }

    /// The set each client slot wears, after registration's checks.
    pub fn saber_sound_overrides(&self) -> &[Option<u8>; SABER_SOUND_CLIENTS] {
        &self.saber_overrides.clients
    }
}

#[cfg(test)]
#[path = "saber_sound_overrides_tests.rs"]
mod tests;
