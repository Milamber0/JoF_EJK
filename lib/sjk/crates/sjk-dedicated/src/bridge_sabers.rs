//! Each player's sabers on the server: set from its userinfo through the saber
//! definitions its map's game data holds ([`sjk_game_jka::player_sabers`]), on its
//! first connect and at a spawn whose userinfo names others, with the sounds they name
//! registered as the game registers them.

use super::{NativeGame, Told};
use sjk_game_jka::player_death::Rng;
use sjk_game_jka::player_sabers::PlayerSabers;
use sjk_game_jka::registries::SoundTable;
use sjk_game_jka::saber_definition::{SaberParms, SaberParseHost};
use std::sync::Arc;

/// A parse's sounds registered in the server's table and told as configstrings; its
/// `random` colour drawn from the game's generator.
pub(super) struct SaberHost<'a> {
    pub(super) sounds: &'a mut SoundTable,
    pub(super) told: &'a mut Vec<Told>,
    pub(super) rng: &'a mut Rng,
}

impl SaberParseHost for SaberHost<'_> {
    fn sound_index(&mut self, name: &[u8]) -> u16 {
        let told = &mut *self.told;
        self.sounds.index(name, &mut |index, value| {
            told.push(Told::ConfigString {
                index,
                previous: Vec::new(),
                value: value.to_vec(),
            })
        })
    }

    fn irand(&mut self, low: i32, high: i32) -> i32 {
        self.rng.irand(low, high)
    }
}

/// The userinfo as `ClientUserinfoChanged` reads the sabers once they are set: the
/// names held (`pers.saber1`, `pers.saber2`), not the keys, which a spawn reads.
pub(super) fn with_saber_names(userinfo: &[u8], sabers: &PlayerSabers) -> Vec<u8> {
    let (first, second) = sabers.names();
    if first.is_empty() {
        return userinfo.to_vec();
    }
    let userinfo = sjk_protocol::info_set_value(userinfo, b"saber1", &first);
    sjk_protocol::info_set_value(&userinfo, b"saber2", &second)
}

impl NativeGame {
    /// The definitions set by the host, else those the running map's game data holds;
    /// none without either, and every saber is then the default one.
    pub(super) fn saber_parms(&self) -> Arc<SaberParms> {
        match (&self.saber_definitions, &self.map) {
            (Some(own), _) => Arc::clone(own),
            (None, Some(map)) => Arc::clone(&map.sabers),
            (None, None) => Arc::new(SaberParms::default()),
        }
    }

    /// Saber definitions to use in place of the game data's.
    pub fn set_saber_definitions(&mut self, definitions: SaberParms) {
        self.saber_definitions = Some(Arc::new(definitions));
    }

    /// `ClientUserinfoChanged` on `client`'s first connect: both hands from its
    /// userinfo.
    pub(super) fn connect_sabers(&mut self, client: usize) {
        let parms = self.saber_parms();
        let NativeGame {
            server,
            world,
            players,
            sounds,
            told,
            deaths,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return;
        };
        let value = |key: &[u8]| {
            sjk_protocol::info_value(&peer.userinfo, key)
                .unwrap_or_default()
                .to_vec()
        };
        let (first, second) = (value(b"saber1"), value(b"saber2"));
        let mut host = SaberHost {
            sounds,
            told,
            rng: &mut deaths.rng,
        };
        if let Err(error) = peer.sabers.connect(&parms, &first, &second, &mut host) {
            eprintln!("client {client}: {error}; the saber is refused");
        }
    }

    /// `ClientSpawn`'s saber check for `client`, before its begin or respawn: hands its
    /// userinfo names differently are set again, the stance they bring is left to the
    /// spawn, and the userinfo is rewritten to the names held.
    /// Whether the sabers changed (`changedSaber`), after which the reference runs
    /// `ClientUserinfoChanged` again.
    pub(super) fn spawn_sabers(&mut self, client: usize) -> bool {
        let parms = self.saber_parms();
        let NativeGame {
            server,
            world,
            players,
            sounds,
            told,
            deaths,
            ..
        } = self;
        let Some(peer) = players
            .at(client)
            .and_then(|handle| server.world_mut(*world)?.entity_mut(handle))
        else {
            return false;
        };
        let value = |key: &[u8]| {
            sjk_protocol::info_value(&peer.userinfo, key)
                .unwrap_or_default()
                .to_vec()
        };
        let (first, second) = (value(b"saber1"), value(b"saber2"));
        let mut host = SaberHost {
            sounds,
            told,
            rng: &mut deaths.rng,
        };
        match peer.sabers.spawn_check(&parms, &first, &second, &mut host) {
            Ok(Some(kit)) => {
                peer.session.sabers_set = false;
                peer.session.saber_kit = kit;
                // "they don't match up, force the user info"
                let (held_first, held_second) = peer.sabers.names();
                for (key, wanted, held) in [
                    (&b"saber1"[..], first, held_first),
                    (b"saber2", second, held_second),
                ] {
                    if !wanted.eq_ignore_ascii_case(&held) {
                        peer.userinfo = sjk_protocol::info_set_value(&peer.userinfo, key, &held);
                    }
                }
                true
            }
            Ok(None) => false,
            Err(error) => {
                eprintln!("client {client}: {error}; the saber is refused");
                false
            }
        }
    }
}
