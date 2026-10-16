//! An NPC touched (`NPC_Touch`, `NPC_reactions.c:552-669`) — by another NPC's move
//! (`ClientImpacts` after its `Pmove`, `g_active.c:493-521`, `3387`) or by a player's
//! ([`crate::npc_roster::NpcRoster::player_touch`]): a living client toucher remembered
//! (`touchedByPlayer`), a goal reached by touch (`NPCAI_TOUCHED_GOAL`), and an enemy bumped
//! into made the NPC's enemy.
//!
//! Only the touches of NPCs are ported: an NPC's move walking into an item, a trigger or a
//! mover (their own `touch`) is not.

use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;

/// `NPCAI_TOUCHED_GOAL`.
const NPCAI_TOUCHED_GOAL: u32 = 0x8;
/// `FL_NOTARGET`.
const FL_NOTARGET: u32 = 0x20;

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `ClientImpacts` for the NPC at `me` after its move: every entity it touched, once,
    /// that is an NPC is touched by it (`other->touch(other, ent)`: players have no touch).
    pub fn client_impacts(&mut self, me: usize) {
        let touched = self.actors[me].movement.touched();
        let entities = touched.entities();
        let number = self.actors[me].number;
        for (index, &other) in entities.iter().enumerate() {
            if entities[..index].contains(&other) {
                continue;
            }
            if let Some(at) = self.actor_at(other).filter(|_| other != number) {
                self.npc_touch(at, number);
            }
        }
    }

    /// `NPC_Touch(npc, toucher)` for the NPC at `me`, touched by the client numbered
    /// `toucher` (a player or an NPC): a living toucher remembered, the goal reached, and —
    /// an enemy of its team bumped into while it hunts nobody by force — its enemy made so.
    pub fn npc_touch(&mut self, me: usize, toucher: u16) {
        self.host.noting_touch(self.actors[me].number, toucher);
        let Some(other) = self.body(toucher) else {
            return;
        };
        let npc = &mut self.actors[me];
        if other.health > 0 {
            npc.mind.touched_by = Some(toucher);
        }
        if npc.mind.goal == Some(toucher) {
            npc.ai_flags |= NPCAI_TOUCHED_GOAL;
        }
        if other.flags & FL_NOTARGET != 0
            || npc.enemy_team == 0
            || other.player_team != npc.enemy_team
        {
            return;
        }
        if npc.behavior_state != crate::npc_behavior::bstate::HUNT_AND_KILL
            && npc.mind.temp_behavior == 0
            && npc.mind.enemy != Some(toucher)
        {
            self.set_enemy(me, toucher);
        }
    }
}
