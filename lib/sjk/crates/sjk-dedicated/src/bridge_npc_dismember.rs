//! What `G_CheckForDismemberment` reads of the clients on this server
//! ([`sjk_game_jka::npc_dismember_check`]): `g_dismember`, a siege class's heavy melee, a
//! player's or an NPC's posed model given its death pose and read at a limb's bone and at
//! its hilt, the surface a blade struck on a player, and a player's blade readings. The
//! roster's [`NpcHost`] methods answer through these.

use super::*;
use sjk_game_jka::damage::HitLocation;
use sjk_game_jka::saber_clash::SaberStorage;

/// `CFL_HEAVYMELEE` (`bg_saga.h:61`).
const CFL_HEAVYMELEE: u32 = 4;

/// The level's rules for a cut: `g_dismember`, and the clients whose siege class has heavy
/// melee, a bit each (by wire client number).
#[derive(Clone, Copy, Debug, Default)]
pub(in super::super) struct LimbRules {
    pub(in super::super) dismember: i32,
    pub(in super::super) heavy_melee: u64,
}

impl NativeGame {
    /// The level's [`LimbRules`], as the NPCs' host is built.
    pub(in super::super) fn limb_rules(&self) -> LimbRules {
        let dismember = self.integer_cvar(b"g_dismember", 0);
        let heavy_melee = (0..self.players.places().min(64))
            .filter(|client| self.siege_class_flags(*client) & (1 << CFL_HEAVYMELEE) != 0)
            .fold(0, |mask, client| mask | 1 << client);
        LimbRules {
            dismember,
            heavy_melee,
        }
    }
}

impl ServerHost<'_> {
    /// `G_HeavyMelee(attacker)`.
    pub(super) fn heavy_melee_of(&self, attacker: u16) -> bool {
        attacker < 64 && self.limbs.heavy_melee & (1 << attacker) != 0
    }

    /// `G_UpdateClientAnims(npc, 1.0f)` on NPC `npc`'s model.
    pub(super) fn update_npc_skeleton(
        &mut self,
        npc: &sjk_game_jka::npc_spawn::NpcActor,
        level_time: i32,
    ) {
        if let Some(body) = self.bodies.get(npc.number)
            && let Err(error) = body.update_animations(npc, level_time)
        {
            eprintln!("npc {}'s skeleton: {error}", npc.number);
        }
    }

    /// A bone of client `number`'s posed model at `origin` turned by `angles`, at the Ghoul2
    /// clock: an NPC's own model, or a player's.
    pub(super) fn client_bone_matrix(
        &mut self,
        number: u16,
        bone: &str,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> Option<[[f32; 4]; 3]> {
        if let Some(matrix) = self.body_bolt_matrix_turned(number, bone, angles, origin) {
            return Some(matrix);
        }
        let ghoul2_time = self.ghoul2_time;
        let PlayerSkeleton {
            models, skeleton, ..
        } = self.peer_mut(number)?.skeleton.as_mut()?;
        skeleton
            .bolt_matrix_turned(models, bone, angles, origin, ghoul2_time)
            .ok()
            .flatten()
    }

    /// Where client `number`'s first hilt's blade points, its model at `origin` turned by
    /// `angles`, at the Ghoul2 clock.
    pub(super) fn client_hilt(
        &mut self,
        number: u16,
        angles: [f32; 3],
        origin: [f32; 3],
    ) -> Option<[f32; 3]> {
        let ghoul2_time = self.ghoul2_time;
        if let Some(body) = self.bodies.get(number) {
            return body
                .hilt_direction(angles, origin, ghoul2_time)
                .ok()
                .flatten();
        }
        let PlayerSkeleton {
            models, skeleton, ..
        } = self.peer_mut(number)?.skeleton.as_mut()?;
        Some(
            skeleton
                .blade_of(models, 0, 0, angles, origin, ghoul2_time)
                .ok()
                .flatten()?
                .direction,
        )
    }

    /// The part of player `player` a blade struck this frame.
    pub(super) fn player_struck_location(
        &mut self,
        player: u16,
        spot: [f32; 3],
        level_time: i32,
    ) -> Option<HitLocation> {
        let ghoul2_time = self.ghoul2_time;
        super::super::super::bridge_saber_damage::peer_surface_location(
            self.peer_mut(player)?,
            0,
            spot,
            level_time,
            ghoul2_time,
        )
    }

    /// Player `player`'s blade readings.
    pub(super) fn player_storage(&self, player: u16) -> Option<SaberStorage> {
        self.peer(player).map(|peer| peer.saber_cut.storage)
    }
}
