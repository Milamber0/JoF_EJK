//! An unarmed NPC's weapon (`NPC_CheckGetNewWeapon`, `NPC_SearchForWeapons`,
//! `NPC_SetPickUpGoal`, `NPC_combat.c:2946-3056`): in a fight with no weapon, once its
//! panic is over and nothing else is its goal, it looks for a dropped weapon it may take and
//! runs to it; and what it touches (`G_TouchTriggers`' items, `g_active.c:531-605`, with
//! `Touch_Item` and `CheckItemCanBePickedUpByNPC`, `g_items.c:2371-2681`): the pickup made
//! as a player's is (`Pickup_*`, the pickup event), its run for the weapon ended.
//!
//! The items are the host's ([`NpcHost::npc_item`]): the game reads them and asks the host
//! to take one ([`NpcHost::npc_took_item`]). A weapon taken is held (`STAT_WEAPONS`, its
//! ammo) but not switched to: nothing in the reference's multiplayer game changes an NPC's
//! weapon for a pickup, so the NPC stays unarmed and keeps looking. Team items (the flags)
//! are not taken by NPCs here (`Pickup_Team` reads a player's session).

use crate::items::{ITEMS, Kind};
use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_navigator::{NF_CLEAR_PATH, NavHolder, Q3_INFINITE};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;

/// `SQUAD_STAND_AND_SHOOT`, `SQUAD_TRANSITION`.
const SQUAD_STAND_AND_SHOOT: i32 = 1;
const SQUAD_TRANSITION: i32 = 4;
/// `BS_DEFAULT`.
const BS_DEFAULT: i32 = 0;
/// `SCF_FORCED_MARCH`.
const SCF_FORCED_MARCH: u32 = crate::npc_world::SCF_FORCED_MARCH;
/// `EF_NODRAW`, `EF_ITEMPLACEHOLDER`.
const EF_NODRAW: u32 = 0x100;
const EF_ITEMPLACEHOLDER: u32 = 1 << 26;
/// `CONTENTS_TRIGGER`.
const CONTENTS_TRIGGER: u32 = 0x400;
/// The classes that pick nothing up (`Touch_Item`, `g_items.c:2456-2475`): the AT-ST, the
/// gonk, the two Marks, the mouse, the probe, the protocol droid, R2 and R5, the seeker,
/// the remote, the rancor, the wampa, the ugnaught and the sentry.
const NEVER_PICK_UP: [i32; 15] = [1, 11, 23, 24, 29, 32, 33, 34, 35, 41, 39, 54, 55, 49, 42];
/// `fd.forceSide`; `FORCE_LIGHTSIDE`, `FORCE_DARKSIDE`; the enlightenments' tags.
const PS_FORCE_SIDE: usize = 61;
const FORCE_LIGHTSIDE: u32 = 1;
const FORCE_DARKSIDE: u32 = 2;
const PW_FORCE_ENLIGHTENED_LIGHT: i32 = 12;
const PW_FORCE_ENLIGHTENED_DARK: i32 = 13;
/// `AMMO_THERMAL`, `AMMO_TRIPMINE`, `AMMO_DETPACK` and the weapons they give.
const THROWN_AMMO: [(i32, u32); 3] = [(7, 12), (8, 13), (9, 14)];
/// `STAT_WEAPONS`.
const STAT_WEAPONS: usize = 4;

/// An item entity as an NPC's search and touch read it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcItem {
    /// Its entity number.
    pub number: u16,
    /// Which item it is (`bg_itemlist`'s row, [`crate::items::ITEMS`]).
    pub item: usize,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    /// Where its trajectory has it now (`BG_EvaluateTrajectory(&s.pos, level.time)`), which
    /// a touch is measured from (`BG_PlayerTouchesItem`).
    pub position: [f32; 3],
    /// `r.mins`, `r.maxs`.
    pub mins: [f32; 3],
    pub maxs: [f32; 3],
    /// `r.contents`: a trigger while it can be taken.
    pub contents: u32,
    /// `s.eFlags`.
    pub entity_flags: u32,
    /// `flags & FL_DROPPED_ITEM`.
    pub dropped: bool,
    /// `count`: its own quantity (0 for the item's).
    pub count: i32,
    /// `activator == &g_entities[0]`: the item belongs to client 0.
    pub belongs_to_client_zero: bool,
    /// `s.time`.
    pub time: i32,
    /// `spawnflags & ITMSF_ALLOWNPC`: a placed item NPCs may take.
    pub allow_npc: bool,
}

/// `BG_EvaluateTrajectory(&item->s.pos, level_time)`: where an item's wire trajectory has it
/// ([`NpcItem::position`]).
pub fn item_position(state: &sjk_protocol::EntityState, level_time: i32) -> [f32; 3] {
    let read = |index: usize| state.raw_field(index).unwrap_or(0);
    let float = |index: usize| f32::from_bits(read(index));
    let base = [2, 1, 4].map(float);
    let delta = [6, 7, 10].map(float);
    crate::trajectory::legacy_evaluate_trajectory(
        base,
        delta,
        read(23) as u8,
        read(0) as i32,
        read(20) as i32,
        level_time,
    )
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_CheckGetNewWeapon` (`NPC_combat.c:3020-3056`): an unarmed NPC in a fight
    /// forgets a weapon someone else took, and — its panic over, no other goal — goes for
    /// the nearest one it may take.
    pub fn check_get_new_weapon(&mut self, me: usize) {
        if self.npc(me).weapon != 0 || self.actors[me].mind.enemy.is_none() {
            return;
        }
        if self.goal_is_temp(me)
            && let Some(target) = self.actors[me].mind.tactics.temp_goal.target
            && !self.host.in_use(target)
            && self.actor_at(target).is_none()
        {
            // "maybe was running at a weapon that was picked up".
            self.actors[me].mind.goal = None;
        }
        if self.actors[me].mind.timers.done("panic", self.level_time)
            && self.actors[me].mind.goal.is_none()
            && let Some((item, waypoint)) = self.search_for_weapons(me)
        {
            self.set_pick_up_goal(me, &item, waypoint);
        }
    }

    /// [`Self::check_get_new_weapon`], or — where the replay's driver stood it in
    /// ([`crate::npc_groups::NpcLevel::states_stood_in`]) — its stub, told for an unarmed
    /// NPC only (the driver's stand-in runs the reference's for an armed one, which does
    /// nothing).
    pub(crate) fn check_get_new_weapon_or_stub(&mut self, me: usize) {
        if !self.level.states_stood_in() && !self.level.stub_weapon_search {
            self.check_get_new_weapon(me);
        } else if self.npc(me).weapon == 0 {
            self.host
                .stub(self.actors[me].number, "NPC_CheckGetNewWeapon");
        }
    }

    /// Whether entity `number` is an item the host keeps (`s.eType == ET_ITEM`).
    pub(crate) fn is_item(&self, number: u16) -> bool {
        let mut index = 0;
        while let Some(item) = self.host.npc_item(index) {
            if item.number == number {
                return true;
            }
            index += 1;
        }
        false
    }

    /// Whether the NPC at `me` runs for an item (`goalEntity == tempGoal` and
    /// `goalEntity->enemy->s.eType == ET_ITEM`, `NPC_AI_Stormtrooper.c:1928-1934`).
    pub(crate) fn running_for_item(&self, me: usize) -> bool {
        self.goal_is_temp(me)
            && self.actors[me]
                .mind
                .tactics
                .temp_goal
                .target
                .is_some_and(|target| self.is_item(target))
    }

    /// `CheckItemCanBePickedUpByNPC` (`g_items.c:2371-2390`): a dropped item not client 0's,
    /// three seconds on the floor, for an unarmed NPC in a fight that is not in pain, not
    /// surrendering and not marched.
    pub fn npc_may_take(&self, me: usize, item: &NpcItem) -> bool {
        let npc = &self.actors[me];
        let level_time = self.level_time;
        item.dropped
            && !item.belongs_to_client_zero
            && npc.number != 0
            && self.npc(me).weapon == 0
            && npc.mind.enemy.is_some()
            && npc.mind.fight.pain_debounce_time < level_time
            && npc.mind.surrender_time < level_time
            && npc.script_flags & SCF_FORCED_MARCH == 0
            && level_time - item.time >= 3_000
    }

    /// `NPC_SearchForWeapons` (`NPC_combat.c:2947-3005`): the nearest weapon it may take in
    /// its potentially visible set that the waypoints lead to — or, without a route, that
    /// it can walk straight to — with the waypoint the item's route query left it.
    pub fn search_for_weapons(&mut self, me: usize) -> Option<(NpcItem, i32)> {
        let mut best: Option<(NpcItem, i32)> = None;
        let mut best_distance = Q3_INFINITE as f32;
        let mut index = 0;
        while let Some(item) = self.host.npc_item(index) {
            index += 1;
            if ITEMS[item.item].kind != Kind::Weapon
                || item.entity_flags & EF_NODRAW != 0
                || !self.npc_may_take(me, &item)
            {
                continue;
            }
            let origin = self.actors[me].current_origin;
            if !self.host.in_pvs(item.origin, origin) {
                continue;
            }
            let distance = crate::npc_senses::distance_squared(item.origin, origin);
            if distance >= best_distance {
                continue;
            }
            let holder = NavHolder::Marker {
                number: item.number,
                origin: item.origin,
                mins: item.mins,
                maxs: item.maxs,
            };
            self.level.navigator.marker_waypoint = WAYPOINT_NONE;
            let path = self.best_path_between(NavHolder::Actor(me), holder, NF_CLEAR_PATH);
            let waypoint = self.level.navigator.marker_waypoint;
            let routed = path != 0 && {
                let mut cost = 0;
                let mine = self.actors[me].mind.tactics.waypoint;
                self.level.navigator.graph.best_node_alt_route(
                    mine,
                    waypoint,
                    &mut cost,
                    sjk_nav::NODE_NONE,
                    false,
                ) != WAYPOINT_NONE
            };
            if !routed {
                // "can't possibly have a route to this one": a clear straight path, then.
                let npc = &self.actors[me];
                let (mins, maxs, clip) = (npc.mins, npc.maxs, npc.clip_mask);
                let entity = self.nav_entity(NavHolder::Actor(me));
                if !self.nav_clear_path(
                    &entity,
                    mins,
                    maxs,
                    item.origin,
                    clip,
                    crate::npc_spawn::ENTITYNUM_NONE,
                ) {
                    continue;
                }
            }
            best_distance = distance;
            best = Some((item, waypoint));
        }
        best
    }

    /// `NPC_SetPickUpGoal` (`NPC_combat.c:3007-3018`): the goal entity at the item's foot
    /// (as far above its bottom as an NPC stands above its own), within three quarters of
    /// its width, the item its target; the flight given up for it.
    pub fn set_pick_up_goal(&mut self, me: usize, item: &NpcItem, waypoint: i32) {
        let mut spot = item.origin;
        spot[2] += 24.0 - item.mins[2] * -1.0;
        let radius = (f64::from(item.maxs[0]) * 0.75) as i32;
        self.set_move_goal(me, spot, radius, false, -1, Some(item.number));
        let npc = &mut self.actors[me];
        npc.mind.tactics.temp_goal.waypoint = waypoint;
        npc.mind.temp_behavior = BS_DEFAULT;
        npc.mind.tactics.squad_state = SQUAD_TRANSITION;
    }

    /// `G_TouchTriggers`' items (`g_active.c:531-605`) for the living NPC at `me`, from
    /// where its move left it: each item whose trigger it stands in is touched
    /// (`Touch_Item`).
    pub fn touch_items(&mut self, me: usize) {
        if self.actors[me].player.stats[0] as i32 <= 0 {
            return;
        }
        let mut index = 0;
        while let Some(item) = self.host.npc_item(index) {
            index += 1;
            let origin = self.actors[me].player.origin();
            if item.contents & CONTENTS_TRIGGER == 0
                || !crate::items::touches(origin, item.position)
            {
                continue;
            }
            self.touch_item(me, &item);
        }
    }

    /// `Touch_Item` (`g_items.c:2397-2681`) with the NPC at `me` touching `item`.
    fn touch_item(&mut self, me: usize, item: &NpcItem) {
        let level_time = self.level_time;
        let number = self.actors[me].number;
        if self.host.npc_item_refuses(item.number, number, level_time)
            || item.entity_flags & (EF_ITEMPLACEHOLDER | EF_NODRAW) != 0
        {
            return;
        }
        let npc = &self.actors[me];
        if npc.health < 1 {
            return;
        }
        let row = ITEMS[item.item];
        if row.kind == Kind::Powerup {
            let side = npc.player.raw_field(PS_FORCE_SIDE).unwrap_or(0);
            if (row.tag == PW_FORCE_ENLIGHTENED_LIGHT && side != FORCE_LIGHTSIDE)
                || (row.tag == PW_FORCE_ENLIGHTENED_DARK && side != FORCE_DARKSIDE)
            {
                return;
            }
        }
        if row.kind == Kind::Team
            || !crate::items::can_be_grabbed(item.item, &npc.player, item.dropped)
            || NEVER_PICK_UP.contains(&npc.definition.client_class)
        {
            return;
        }
        if self.npc_may_take(me, item) {
            let npc = &mut self.actors[me];
            if npc.mind.goal.is_some()
                && npc.mind.goal == npc.goal
                && npc.mind.tactics.temp_goal.target == Some(item.number)
            {
                // "they were running to pick me up, they did, so clear goal".
                npc.mind.goal = None;
                npc.mind.tactics.squad_state = SQUAD_STAND_AND_SHOOT;
            }
        } else if !item.allow_npc {
            // An AT-ST's healing by ammo (`VH_WALKER`) is the vehicles'; nothing else.
            return;
        }
        self.host
            .log(&format!("Item: {number} {}\n", row.classname));
        let npc = &mut self.actors[me];
        let taken = crate::items::pick_up(
            item.item,
            &mut npc.player,
            &mut npc.health,
            item.count,
            item.dropped,
        );
        if row.kind == Kind::Ammo
            && let Some(&(_, weapon)) = THROWN_AMMO.iter().find(|(ammo, _)| *ammo == row.tag)
            && npc.player.ammo[crate::weapon_data::LEGACY_WEAPON_DATA[weapon as usize].ammo_index]
                > 0
        {
            npc.player.stats[STAT_WEAPONS] |= 1 << weapon;
        }
        if taken.respawn == 0 {
            return;
        }
        // An NPC's own `pers.predictItemPickup` is not set: a holdable's pickup is told.
        if taken.predicted && row.kind != Kind::Holdable {
            crate::items::add_predictable_event(
                &mut self.actors[me].player,
                crate::items::EV_ITEM_PICKUP,
                u32::from(item.number),
            );
        } else {
            self.add_event(me, crate::items::EV_ITEM_PICKUP, u32::from(item.number));
        }
        self.host
            .npc_took_item(item.number, number, taken.respawn, level_time);
    }
}
