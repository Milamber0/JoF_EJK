//! A player's limbs on this server (`G_CheckForDismemberment` for a client that is no NPC,
//! [`sjk_game_jka::npc_dismember_check`]): cut by the blade that killed it (`player_die`,
//! `g_combat.c:2788-2792`), and a lost lock's loser's sword hand (`g_active.c:3091-3095`).
//! The limb is the level's, run with the NPCs' limbs ([`sjk_game_jka::npc_dismember`]).
//! `g_dismember` 0, a retail server's, cuts nothing and costs nothing.

use super::*;
use sjk_game_jka::npc_dismember_check::{AvoidDismember, DismemberCheck};

/// `ENTITYNUM_WORLD`: a death by nobody.
const ENTITY_WORLD: u16 = 1_022;

impl NativeGame {
    /// `player_die`'s cut for player `client`, killed by `request`'s blow into the death
    /// pose `death_animation` (none: no cut): after a saber's kill, or a heavy-melee siege
    /// class's, the death pose is given to the model at once, and a limb perhaps comes off
    /// — none while a lost lock's finishing blow lands (`gGAvoidDismember` 1).
    pub(crate) fn dismember_on_death(
        &mut self,
        client: usize,
        request: &DeathRequest,
        death_animation: Option<u16>,
    ) {
        let Some(death_anim) = death_animation else {
            return;
        };
        let rules = self.limb_rules();
        let enemy = request.attacker.unwrap_or(ENTITY_WORLD);
        let heavy_melee = request.means == sjk_game_jka::means_of_death::MOD_MELEE
            && enemy < 64
            && rules.heavy_melee & (1 << enemy) != 0;
        if !(request.means == sjk_game_jka::means_of_death::MOD_SABER || heavy_melee) {
            return;
        }
        self.update_player_skeleton(client, request.level_time);
        if rules.dismember == 0 {
            return;
        }
        let avoid = if self.avoid_dismember {
            AvoidDismember::Always
        } else {
            AvoidDismember::No
        };
        let check = DismemberCheck {
            victim: client as u16,
            enemy,
            point: request.point,
            damage: request.damage,
            death_anim,
            post_death: false,
            avoid,
        };
        self.cut_player(check, request.level_time);
    }

    /// The loser `loser` of a lock `winner` won, killed by its finishing blow: its sword
    /// hand off, whatever the chance (`gGAvoidDismember` 2).
    pub(crate) fn dismember_lock_loser(&mut self, loser: usize, winner: u16, level_time: i32) {
        let Some(peer) = self.peer(loser).filter(|peer| peer.health < 1) else {
            return;
        };
        let (point, death_anim) = (peer.state.origin(), peer.state.leg_animation());
        if self.limb_rules().dismember == 0 {
            return;
        }
        let check = DismemberCheck {
            victim: loser as u16,
            enemy: winner,
            point,
            damage: 999,
            death_anim,
            post_death: false,
            avoid: AvoidDismember::RightHand,
        };
        self.cut_player(check, level_time);
    }

    /// The check through the NPCs' level, which keeps the limbs.
    fn cut_player(&mut self, check: DismemberCheck, level_time: i32) {
        let _ = self
            .with_roster(|roster, _, host| roster.check_for_dismemberment(check, level_time, host));
    }

    /// `G_UpdateClientAnims(self, 1.0f)` on player `client`'s model.
    fn update_player_skeleton(&mut self, client: usize, level_time: i32) {
        let Some(peer) = self.peer_mut(client) else {
            return;
        };
        let animation = super::super::bridge_saber::animation_inputs(peer);
        if let Some(PlayerSkeleton {
            models, skeleton, ..
        }) = peer.skeleton.as_mut()
            && let Err(error) = skeleton.update_animations(models, &animation, level_time)
        {
            eprintln!("client {client}'s skeleton: {error}");
        }
    }
}
