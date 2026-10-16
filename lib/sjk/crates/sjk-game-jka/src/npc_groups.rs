//! The squads NPCs fight in (`codemp/game/NPC_AI_Utils.c`): a group of NPCs of one team
//! after one enemy (`AI_GetGroup`), kept up every frame (`AI_UpdateGroups`,
//! `AI_RefreshGroup`: members that died or no longer belong dropped, groups with one enemy
//! merged, the commander by rank, the morale by ranks and the enemy's health and weapon),
//! and what a member's death does to it (`AI_GroupMemberKilled`, `AI_DeleteSelfFromGroup`).
//!
//! The level has 32 groups of at most 31 members each (`MAX_FRAME_GROUPS`,
//! `MAX_GROUP_MEMBERS - 1`): rules of the game's squad tactics, kept as the reference keeps
//! them because which slot a squad takes decides who commands and who moves first. A
//! group's members are kept as the reference's array is: a member deleted shifts the rest
//! down and leaves the last slot as it was, which the reference then reads
//! (`AI_ValidateNoEnemyGroupMember` of an emptied group).
//!
//! `AI_SortGroupByPathCostToEnemy` looks every member's waypoint up at the enemy (the
//! reference's own slip), so every member's cost is the same and nothing is reordered.
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`).

use crate::npc_mind::WAYPOINT_NONE;
use crate::npc_senses::{Body, distance_squared};
use crate::npc_spawn::{ENTITYNUM_NONE, NpcHost};
use crate::npc_world::NpcWorld;

/// `MAX_FRAME_GROUPS`: the level's groups.
pub const MAX_FRAME_GROUPS: usize = 32;
/// `MAX_GROUP_MEMBERS`: a group is full one short of it.
const MAX_GROUP_MEMBERS: usize = 32;
/// `NUM_SQUAD_STATES`.
pub const NUM_SQUAD_STATES: usize = 7;
use crate::npc_navigator::Q3_INFINITE;
/// `RANK_ENSIGN`.
pub(crate) const RANK_ENSIGN: i32 = 2;
/// `SCF_NO_GROUPS`.
const SCF_NO_GROUPS: u32 = 0x2_0000;
/// The weapons whose carriers never join a squad (`AI_ValidateGroupMember`): the stun
/// baton, the saber, the disruptor, the thermal, the emplaced gun and the turret.
const LONERS: [u8; 6] = [1, 3, 6, 12, 17, 18];
/// The classes that never use the squad AI: the AT-ST, the probe, the seeker, the remote,
/// the sentry, the interrogator, the mine monster, the howler and the Marks.
const LONE_CLASSES: [i32; 10] = [1, 32, 41, 39, 42, 16, 26, 13, 23, 24];

/// `AIGroupMember_t`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GroupMember {
    pub number: u16,
    pub waypoint: i32,
    pub path_cost_to_enemy: i32,
    pub closest_buddy: u16,
}

impl GroupMember {
    fn new(number: u16) -> Self {
        Self {
            number,
            waypoint: 0,
            path_cost_to_enemy: 0,
            closest_buddy: 0,
        }
    }
}

/// `AIGroupInfo_t`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AiGroup {
    /// The member slots ever written, and how many are members (`numGroup`).
    slots: Vec<GroupMember>,
    count: usize,
    pub processed: bool,
    pub team: i32,
    pub enemy: Option<u16>,
    pub enemy_waypoint: i32,
    pub speech_debounce_time: i32,
    pub last_clear_shot_time: i32,
    pub last_seen_enemy_time: i32,
    pub morale: i32,
    pub morale_adjust: i32,
    pub morale_debounce: i32,
    pub member_validate_time: i32,
    pub active_member_num: i32,
    pub commander: Option<u16>,
    pub enemy_last_seen_pos: [f32; 3],
    pub num_state: [i32; NUM_SQUAD_STATES],
}

impl AiGroup {
    /// `numGroup`.
    pub fn len(&self) -> usize {
        self.count
    }

    /// Whether it has no members.
    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    /// Its members, in order.
    pub fn members(&self) -> &[GroupMember] {
        &self.slots[..self.count]
    }

    /// `member[0]` as the reference reads it: the first slot, stale or never written.
    fn first_slot(&self) -> GroupMember {
        self.slots.first().copied().unwrap_or(GroupMember::new(0))
    }

    /// `memset(group, 0)`, keeping the slots' storage.
    fn clear(&mut self) {
        let mut slots = std::mem::take(&mut self.slots);
        slots.clear();
        *self = Self {
            slots,
            ..Self::default()
        };
    }

    /// `AI_GroupContainsEntNum`.
    pub fn contains(&self, number: u16) -> bool {
        self.members().iter().any(|member| member.number == number)
    }
}

/// What the level keeps for its NPCs' tactics beyond each NPC: the groups, the combat
/// points, the stormtroopers' speech debounce by team, the players' waypoint times, the
/// last move's navigation (`frameNavInfo`), and whether the class AI runs.
#[derive(Clone, Debug)]
pub struct NpcLevel {
    /// Opt-in native sand-creature AI; false preserves stock multiplayer dispatch.
    pub native_sand_creatures: bool,
    /// `level.groups`.
    pub groups: Vec<AiGroup>,
    /// `level.combatPoints`.
    pub combat_points: Vec<crate::npc_combat_points::CombatPoint>,
    /// `groupSpeechDebounceTime[TEAM_NUM_TEAMS]`.
    pub group_speech: [i32; 4],
    /// Each player's navigation (`waypoint`, `noWaypointTime`, ...), by entity number.
    pub player_navs: Vec<(u16, crate::npc_navigator::NavState)>,
    /// The level's navigator: the map's waypoint graph.
    pub navigator: crate::npc_navigator::Navigator,
    /// `frameNavInfo`: the last move's.
    pub nav: crate::npc_nav::NavInfo,
    /// The stormtroopers' AI stood in for by stubs (`stub NUMBER NAME`), for the replays of
    /// drivers that stub it (npcthink.c, npccombat.c).
    pub stub_class_ai: bool,
    /// The Jedi's AI (`NPC_BSJedi_*`, `NPC_Jedi_Pain`, `NPC_Jedi_RateNewEnemy`) stood in
    /// for by stubs, for the replays of drivers that run the stormtroopers' AI but stub the
    /// Jedi's (npcst.c, npcnav.c, npcmix.c). [`Self::stub_class_ai`] stands it in too.
    pub stub_jedi_ai: bool,
    /// The default set's states (`NPC_BSFlee`, `NPC_BSSearch`, ...) and
    /// `NPC_CheckGetNewWeapon` stood in for by stubs, for the replays of drivers that stub
    /// them ([`crate::npc_states`]). [`Self::stub_class_ai`] stands them in too.
    pub stub_states: bool,
    /// `NPC_CheckGetNewWeapon` alone stood in for (an unarmed NPC's stub), for a replay whose
    /// driver runs the states but keeps the weapon search's stand-in (npcsand.c).
    pub stub_weapon_search: bool,
    /// `jediSpeechDebounceTime[TEAM_NUM_TEAMS]` (`NPC_AI_Jedi.c:99`): no Jedi of a team
    /// speaks before this time.
    pub jedi_speech_debounce: [i32; 4],
    /// The limbs `G_Dismember` cut off that are still flying or lying about
    /// ([`crate::npc_dismember`]).
    pub limbs: Vec<crate::npc_dismember::Limb>,
    /// `enemyDist` (`NPC_AI_Wampa.c:36`): the wampas' distance to their enemy, a global every
    /// wampa's think reads and writes ([`crate::npc_wampa`]).
    pub wampa_enemy_distance: f32,
    /// The C library's `rand()` (glibc's, seeded 1 as a process that never seeds it), which
    /// the interrogator's strafe draws from (`NPC_AI_Interrogator.c:271`).
    pub crt: crate::crt_rand::CrtRand,
    /// Scratch for what may come at an NPC ([`crate::npc_missile_block`]).
    pub incoming: Vec<crate::npc_missile_block::IncomingEntity>,
    /// `gGAvoidDismember` while a lost lock's finishing blow lands on an NPC
    /// ([`crate::npc_dismember_check`]).
    pub avoid_dismember: crate::npc_dismember_check::AvoidDismember,
}

impl Default for NpcLevel {
    fn default() -> Self {
        Self {
            native_sand_creatures: false,
            groups: vec![AiGroup::default(); MAX_FRAME_GROUPS],
            combat_points: Vec::new(),
            group_speech: [0; 4],
            player_navs: Vec::new(),
            navigator: crate::npc_navigator::Navigator::default(),
            nav: crate::npc_nav::NavInfo::default(),
            stub_class_ai: false,
            stub_jedi_ai: false,
            stub_states: false,
            stub_weapon_search: false,
            jedi_speech_debounce: [0; 4],
            limbs: Vec::new(),
            wampa_enemy_distance: 0.0,
            crt: crate::crt_rand::CrtRand::new(1),
            incoming: Vec::new(),
            avoid_dismember: crate::npc_dismember_check::AvoidDismember::No,
        }
    }
}

impl NpcLevel {
    /// Whether the Jedi's AI is stood in for by stubs ([`Self::stub_class_ai`] or
    /// [`Self::stub_jedi_ai`]).
    pub fn jedi_ai_stood_in(&self) -> bool {
        self.stub_class_ai || self.stub_jedi_ai
    }

    /// Whether the default set's states are stood in for by stubs ([`Self::stub_class_ai`]
    /// or [`Self::stub_states`]).
    pub fn states_stood_in(&self) -> bool {
        self.stub_class_ai || self.stub_states
    }

    /// A player's navigation (all zero for one never seen: a fresh entity's).
    pub fn player_nav(&self, number: u16) -> crate::npc_navigator::NavState {
        self.player_navs
            .iter()
            .find(|(known, _)| *known == number)
            .map_or_else(Default::default, |(_, nav)| *nav)
    }

    /// A player's navigation, to change it.
    pub fn player_nav_mut(&mut self, number: u16) -> &mut crate::npc_navigator::NavState {
        let at = match self
            .player_navs
            .iter()
            .position(|(known, _)| *known == number)
        {
            Some(at) => at,
            None => {
                self.player_navs.push((number, Default::default()));
                self.player_navs.len() - 1
            }
        };
        &mut self.player_navs[at].1
    }

    /// A player's `noWaypointTime`.
    pub fn player_waypoint_time(&self, number: u16) -> i32 {
        self.player_nav(number).no_waypoint_time
    }

    /// A player's `noWaypointTime` set.
    pub fn set_player_waypoint_time(&mut self, number: u16, time: i32) {
        self.player_nav_mut(number).no_waypoint_time = time;
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The rank of the NPC numbered `number` (`NPC->rank`), 0 for anything else.
    fn rank_of(&self, number: u16) -> i32 {
        self.actor_at(number)
            .map_or(0, |at| self.actors[at].definition.rank)
    }

    /// `AI_GetGroup` (`NPC_AI_Utils.c:399-500`) for the NPC at `me`.
    pub fn get_group(&mut self, me: usize) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if npc.script_flags & SCF_NO_GROUPS != 0 {
            self.actors[me].mind.tactics.group = None;
            return;
        }
        if let Some(enemy) = npc.mind.enemy
            && (self.body(enemy).is_none()
                || level_time - npc.mind.tactics.enemy_last_seen_time > 7_000)
        {
            self.actors[me].mind.tactics.group = None;
            return;
        }
        let Some(group) = self.next_empty_group(me) else {
            return;
        };
        let npc = &self.actors[me];
        let enemy = npc.mind.enemy;
        let enemy_origin = enemy
            .and_then(|number| self.body(number))
            .map(|body| body.origin);
        let (number, team) = (npc.number, npc.player_team);
        let slot = &mut self.level.groups[group];
        slot.clear();
        slot.enemy = enemy;
        slot.team = team;
        slot.commander = Some(number);
        slot.member_validate_time = level_time + 2_000;
        if let Some(origin) = enemy_origin {
            slot.last_seen_enemy_time = level_time;
            slot.last_clear_shot_time = level_time;
            slot.enemy_last_seen_pos = origin;
        }
        for index in 0..self.order.len() {
            let at = self.order[index];
            if !self.valid_member(group, at) {
                continue;
            }
            self.insert_member(group, at);
            if self.level.groups[group].len() >= MAX_GROUP_MEMBERS - 1 {
                break;
            }
        }
        if self.level.groups[group].is_empty() {
            self.actors[me].mind.tactics.group = None;
            return;
        }
        self.sort_group(group);
        self.set_closest_buddies(group);
    }

    /// `AI_GetNextEmptyGroup` (`NPC_AI_Utils.c:241-273`): a new group for the NPC, unless it
    /// is in one already or joins one after its enemy.
    fn next_empty_group(&mut self, me: usize) -> Option<usize> {
        let number = self.actors[me].number;
        if let Some(group) = self
            .level
            .groups
            .iter()
            .position(|group| group.contains(number))
        {
            self.actors[me].mind.tactics.group = Some(group);
            return None;
        }
        let enemy = self.actors[me].mind.enemy;
        for group in 0..MAX_FRAME_GROUPS {
            let slot = &self.level.groups[group];
            if !slot.is_empty()
                && slot.len() < MAX_GROUP_MEMBERS - 1
                && slot.enemy == enemy
                && self.valid_member(group, me)
            {
                self.insert_member(group, me);
                return None;
            }
        }
        let empty = self.level.groups.iter().position(AiGroup::is_empty);
        self.actors[me].mind.tactics.group = empty;
        empty
    }

    /// `AI_InsertGroupMember` (`NPC_AI_Utils.c:203-224`).
    fn insert_member(&mut self, group: usize, at: usize) {
        let (number, state, rank) = (
            self.actors[at].number,
            self.actors[at].mind.tactics.squad_state,
            self.actors[at].definition.rank,
        );
        let commander_rank = self.level.groups[group]
            .commander
            .map(|commander| self.rank_of(commander));
        let slot = &mut self.level.groups[group];
        if !slot.contains(number) {
            if slot.count < slot.slots.len() {
                slot.slots[slot.count] = GroupMember::new(number);
            } else {
                slot.slots.push(GroupMember::new(number));
            }
            slot.count += 1;
            if let Some(count) = slot.num_state.get_mut(state as usize) {
                *count += 1;
            }
        }
        if commander_rank.is_none_or(|commander| rank > commander) {
            slot.commander = Some(number);
        }
        self.actors[at].mind.tactics.group = Some(group);
    }

    /// `AI_ValidateNoEnemyGroupMember` (`NPC_AI_Utils.c:275-305`): near the commander (or
    /// the first member slot's entity, whatever it now is) and in its potentially visible
    /// set.
    fn valid_patroller(&self, group: usize, at: usize) -> bool {
        let slot = &self.level.groups[group];
        let center = match slot.commander {
            Some(commander) => self.origin_of(commander),
            None => {
                let first = slot.first_slot().number;
                if first >= crate::pmove::ENTITY_NUMBER_WORLD {
                    return false;
                }
                self.origin_of(first)
            }
        };
        let origin = self.actors[at].current_origin;
        distance_squared(center, origin) <= 147_456.0 && self.host.in_pvs(origin, center)
    }

    /// Where the entity numbered `number` is: a body's origin, else the origin of nothing.
    fn origin_of(&self, number: u16) -> [f32; 3] {
        self.body(number).map_or([0.0; 3], |body| body.origin)
    }

    /// `AI_ValidateGroupMember` (`NPC_AI_Utils.c:307-397`) of the NPC at `at` for `group`.
    fn valid_member(&self, group: usize, at: usize) -> bool {
        let level_time = self.level_time;
        let npc = &self.actors[at];
        let slot = &self.level.groups[group];
        if npc.mind.confusion_time > level_time || npc.script_flags & SCF_NO_GROUPS != 0 {
            return false;
        }
        if npc.mind.tactics.group.is_some_and(|own| own != group)
            || npc.health <= 0
            || npc.player_team != slot.team
        {
            return false;
        }
        if LONERS.contains(&npc.player.weapon())
            || LONE_CLASSES.contains(&npc.definition.client_class)
        {
            return false;
        }
        if npc.mind.enemy != slot.enemy {
            if npc.mind.enemy.is_some() {
                return false;
            }
            let enemy = slot.enemy.map_or([0.0; 3], |enemy| self.origin_of(enemy));
            if !self.host.in_pvs(npc.current_origin, enemy) {
                return false;
            }
        } else if slot.enemy.is_none() && !self.valid_patroller(group, at) {
            return false;
        }
        npc.mind.timers.done("interrogating", level_time)
    }

    /// `AI_SortGroupByPathCostToEnemy` (`NPC_AI_Utils.c:120-196`): the enemy's nearest
    /// waypoint, and each member's — the enemy's again, as the reference asks for it
    /// (`NAV_FindClosestWaypointForEnt( group->enemy, ...)`) — and its route's cost; then
    /// the reference's sort, whose shifting loop never runs: a member cheaper than a slot's
    /// takes that slot over. Every member's cost is the same, so nothing moves.
    fn sort_group(&mut self, group: usize) {
        let enemy = self.level.groups[group]
            .enemy
            .and_then(|number| self.nav_holder_of(number));
        let enemy_waypoint = enemy.map_or(WAYPOINT_NONE, |holder| {
            self.closest_waypoint_for(holder, WAYPOINT_NONE)
        });
        self.level.groups[group].enemy_waypoint = enemy_waypoint;
        let count = self.level.groups[group].count;
        let mut sort = false;
        for index in 0..count {
            let (waypoint, cost) = match enemy {
                Some(holder) if enemy_waypoint != WAYPOINT_NONE => {
                    let waypoint = self.closest_waypoint_for(holder, WAYPOINT_NONE);
                    if waypoint == WAYPOINT_NONE {
                        (waypoint, Q3_INFINITE)
                    } else {
                        sort = true;
                        (
                            waypoint,
                            self.level
                                .navigator
                                .graph
                                .path_cost(waypoint, enemy_waypoint),
                        )
                    }
                }
                _ => (WAYPOINT_NONE, Q3_INFINITE),
            };
            let member = &mut self.level.groups[group].slots[index];
            (member.waypoint, member.path_cost_to_enemy) = (waypoint, cost);
        }
        if !sort {
            return;
        }
        // `bestMembers`: a slot unoccupied is `ENTITYNUM_NONE` (its other fields stack
        // garbage in the reference, zero here).
        let mut best = [GroupMember {
            number: ENTITYNUM_NONE,
            waypoint: 0,
            path_cost_to_enemy: 0,
            closest_buddy: 0,
        }; MAX_GROUP_MEMBERS];
        for index in 0..count {
            let member = self.level.groups[group].slots[index];
            for slot in best.iter_mut().take(count) {
                if slot.number == ENTITYNUM_NONE
                    || member.path_cost_to_enemy < slot.path_cost_to_enemy
                {
                    *slot = member;
                    break;
                }
            }
        }
        self.level.groups[group].slots[..count].copy_from_slice(&best[..count]);
    }

    /// `AI_SetClosestBuddy` (`NPC_AI_Utils.c:100-118`): the distances truncated to ints,
    /// and each member its own closest (at distance 0) unless another stands on it.
    fn set_closest_buddies(&mut self, group: usize) {
        let count = self.level.groups[group].count;
        for i in 0..count {
            let mine = self.origin_of(self.level.groups[group].slots[i].number);
            let (mut best, mut buddy) = (Q3_INFINITE, ENTITYNUM_NONE);
            for j in 0..count {
                let other = self.level.groups[group].slots[j].number;
                let distance = distance_squared(mine, self.origin_of(other)) as i32;
                if distance < best {
                    best = distance;
                    buddy = other;
                }
            }
            self.level.groups[group].slots[i].closest_buddy = buddy;
        }
    }

    /// `AI_SetNewGroupCommander` (`NPC_AI_Utils.c:502-516`).
    fn set_new_commander(&mut self, group: usize) {
        let mut commander: Option<(u16, i32)> = None;
        for member in self.level.groups[group].members() {
            let rank = self.rank_of(member.number);
            if commander.is_none_or(|(_, best)| rank > best) {
                commander = Some((member.number, rank));
            }
        }
        self.level.groups[group].commander = commander.map(|(number, _)| number);
    }

    /// `AI_DeleteGroupMember` (`NPC_AI_Utils.c:518-547`): the member at `index` out of the
    /// group, the ones after it shifted down.
    fn delete_member(&mut self, group: usize, index: usize) {
        let number = self.level.groups[group].slots[index].number;
        if self.level.groups[group].commander == Some(number) {
            self.level.groups[group].commander = None;
        }
        if let Some(at) = self.actor_at(number) {
            self.actors[at].mind.tactics.group = None;
        }
        let slot = &mut self.level.groups[group];
        let count = slot.count;
        slot.slots.copy_within(index + 1..count, index);
        if (index as i32) < slot.active_member_num {
            slot.active_member_num = (slot.active_member_num - 1).max(0);
        }
        slot.count = count.saturating_sub(1);
        self.set_new_commander(group);
    }

    /// `AI_DeleteSelfFromGroup` (`NPC_AI_Utils.c:549-561`).
    pub fn delete_from_group(&mut self, me: usize) {
        let Some(group) = self.actors[me].mind.tactics.group else {
            return;
        };
        let number = self.actors[me].number;
        if let Some(index) = self.level.groups[group]
            .members()
            .iter()
            .position(|member| member.number == number)
        {
            self.delete_member(group, index);
        }
    }

    /// `AI_GroupUpdateSquadstates` (`NPC_AI_Utils.c:706-727`): the NPC's squad state, and
    /// its group's count of each (only if it is in `group`).
    pub fn update_squad_state(&mut self, group: Option<usize>, at: usize, state: i32) {
        let Some(group) = group else {
            self.actors[at].mind.tactics.squad_state = state;
            return;
        };
        let number = self.actors[at].number;
        if !self.level.groups[group].contains(number) {
            return;
        }
        let old = self.actors[at].mind.tactics.squad_state;
        let slot = &mut self.level.groups[group];
        if let Some(count) = slot.num_state.get_mut(old as usize) {
            *count -= 1;
        }
        if let Some(count) = slot.num_state.get_mut(state as usize) {
            *count += 1;
        }
        self.actors[at].mind.tactics.squad_state = state;
    }

    /// `AI_UpdateGroups` (`NPC_AI_Utils.c:935-950`): every group refreshed, the empty and
    /// merged ones cleared. The start of every frame.
    pub fn update_groups(&mut self) {
        for group in 0..MAX_FRAME_GROUPS {
            if self.level.groups[group].is_empty() || !self.refresh_group(group) {
                self.level.groups[group].clear();
            }
        }
    }

    /// `AI_RefreshGroup` (`NPC_AI_Utils.c:729-933`). Whether the group still has members.
    fn refresh_group(&mut self, group: usize) -> bool {
        if self.merge_group(group) {
            return false;
        }
        let level_time = self.level_time;
        let slot = &mut self.level.groups[group];
        slot.num_state = [0; NUM_SQUAD_STATES];
        slot.commander = None;
        let mut index = 0;
        while index < self.level.groups[group].count {
            let number = self.level.groups[group].slots[index].number;
            let at = self.actor_at(number);
            let health = at.map_or(0, |at| self.actors[at].health);
            let stale = self.level.groups[group].member_validate_time < level_time;
            let Some(at) = at.filter(|&at| health > 0 && !(stale && !self.valid_member(group, at)))
            else {
                self.delete_member(group, index);
                continue;
            };
            let (state, rank) = (
                self.actors[at].mind.tactics.squad_state,
                self.actors[at].definition.rank,
            );
            let commander_rank = self.level.groups[group]
                .commander
                .map(|commander| self.rank_of(commander));
            let slot = &mut self.level.groups[group];
            if let Some(count) = slot.num_state.get_mut(state as usize) {
                *count += 1;
            }
            if commander_rank.is_none_or(|commander| rank > commander) {
                slot.commander = Some(number);
            }
            index += 1;
        }
        if self.level.groups[group].member_validate_time < level_time {
            let delay = self.host.irand(500, 2_500);
            self.level.groups[group].member_validate_time = level_time + delay;
        }
        self.group_morale(group);
        let slot = &mut self.level.groups[group];
        if slot.morale_debounce < level_time {
            slot.morale_adjust -= slot.morale_adjust.signum();
            slot.morale_debounce = level_time + 1_000;
        }
        slot.processed = false;
        !slot.is_empty()
    }

    /// `AI_RefreshGroup`'s merge (`NPC_AI_Utils.c:735-775`): a group after the same enemy
    /// as an earlier one (no enemy is the same enemy too) moved into it when both fit in
    /// one. Returns whether the group is to be cleared.
    fn merge_group(&mut self, group: usize) -> bool {
        for earlier in 0..group {
            if self.level.groups[earlier].enemy != self.level.groups[group].enemy {
                continue;
            }
            if self.level.groups[earlier].count + self.level.groups[group].count
                >= MAX_GROUP_MEMBERS - 1
            {
                continue;
            }
            let mut delete_when_done = true;
            let mut index = 0;
            while index < self.level.groups[group].count {
                let number = self.level.groups[group].slots[index].number;
                let Some(at) = self.actor_at(number) else {
                    index += 1;
                    continue;
                };
                if self.level.groups[earlier].enemy.is_none() && !self.valid_patroller(earlier, at)
                {
                    delete_when_done = false;
                    index += 1;
                    continue;
                }
                self.delete_member(group, index);
                self.insert_member(earlier, at);
            }
            if delete_when_done {
                return true;
            }
        }
        false
    }

    /// `AI_RefreshGroup`'s morale (`NPC_AI_Utils.c:830-918`): a point for each grunt, its
    /// rank for each officer, more against a hurt enemy, less against a dangerous weapon.
    fn group_morale(&mut self, group: usize) {
        let mut morale = self.level.groups[group].morale_adjust;
        for member in self.level.groups[group].members() {
            let rank = self.rank_of(member.number);
            morale += if rank < RANK_ENSIGN { 1 } else { rank };
        }
        if let Some(enemy) = self.level.groups[group].enemy {
            let body: Option<Body> = self.body(enemy);
            let (health, weapon) = body.map_or((0, 0), |body| (body.health, body.weapon));
            morale += if health < 10 {
                10
            } else if health < 25 {
                5
            } else if health < 50 {
                2
            } else {
                0
            };
            morale += match weapon {
                3 => -5,
                4 => 3,
                6 => 2,
                8 => -1,
                10 => -2,
                11 => -10,
                12 => -5,
                13 => -3,
                14 => -10,
                1 => 10,
                17 => -8,
                _ => 0,
            };
        }
        self.level.groups[group].morale = morale;
    }

    /// `AI_GroupMemberKilled` (`NPC_AI_Utils.c:566-643`): an officer's death lowers its
    /// group's morale and its grunts' aggression and aim; the commander's makes the grunts
    /// near it or the enemy flee, and the others take cover, unless another officer holds
    /// them.
    pub fn group_member_killed(&mut self, me: usize) {
        let Some(group) = self.actors[me].mind.tactics.group else {
            return;
        };
        let rank = self.actors[me].definition.rank;
        if rank < RANK_ENSIGN {
            return;
        }
        let number = self.actors[me].number;
        self.level.groups[group].morale_adjust -= rank;
        let mut no_flee = false;
        for index in 0..self.level.groups[group].count {
            let member = self.level.groups[group].slots[index].number;
            let Some(at) = self.actor_at(member).filter(|_| member != number) else {
                continue;
            };
            if self.actors[at].definition.rank > RANK_ENSIGN {
                no_flee = true;
            } else {
                self.aggression_adjust(at, -1);
                let drop = self.host.irand(0, 10);
                self.actors[at].mind.current_aim -= drop;
            }
        }
        if self.level.groups[group].commander != Some(number) || no_flee {
            return;
        }
        self.level.groups[group].speech_debounce_time = 0;
        let enemy = self.level.groups[group]
            .enemy
            .and_then(|enemy| self.body(enemy));
        let my_origin = self.actors[me].current_origin;
        for index in 0..self.level.groups[group].count {
            let member = self.level.groups[group].slots[index].number;
            let Some(at) = self.actor_at(member).filter(|_| member != number) else {
                continue;
            };
            if self.actors[at].definition.rank < RANK_ENSIGN {
                let origin = self.actors[at].current_origin;
                let near_enemy =
                    enemy.is_some_and(|enemy| distance_squared(origin, enemy.origin) < 65_536.0);
                let flee = near_enemy
                    || distance_squared(origin, my_origin) < 65_536.0
                    || self.host.irand(0, rank) > self.actors[at].definition.rank;
                if flee {
                    self.st_start_flee(
                        at,
                        enemy.map(|enemy| enemy.number),
                        origin,
                        crate::npc_senses::AEL_DANGER + 1,
                        3_000,
                        5_000,
                    );
                } else {
                    self.mark_to_cover(at);
                }
                let drop = self.host.irand(1, 15);
                self.actors[at].mind.current_aim -= drop;
            }
            let drop = self.host.irand(1, 15);
            self.actors[at].mind.current_aim -= drop;
        }
    }

    /// `AI_GroupUpdateEnemyLastSeen` (`NPC_AI_Utils.c:684-693`).
    pub fn group_saw_enemy(&mut self, group: Option<usize>, spot: [f32; 3]) {
        if let Some(group) = group {
            self.level.groups[group].last_seen_enemy_time = self.level_time;
            self.level.groups[group].enemy_last_seen_pos = spot;
        }
    }

    /// `AI_GroupUpdateClearShotTime` (`NPC_AI_Utils.c:695-704`).
    pub fn group_clear_shot(&mut self, group: Option<usize>) {
        if let Some(group) = group {
            self.level.groups[group].last_clear_shot_time = self.level_time;
        }
    }
}
