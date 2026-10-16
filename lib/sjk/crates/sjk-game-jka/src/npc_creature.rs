//! What the monsters' and the droids' AI share (NPC plan steps 7 and 8): the bolts their
//! attacks read (`G2API_AddBolt`, `renderInfo`'s bolts, `G_GetBoltPosition`,
//! `NPC_GetEntsNearBolt`), and what their blows do to whoever they reach — a player or an
//! NPC alike, as the reference's `gentity_t` makes them one: `G_Damage`, `G_Throw`,
//! `G_Knockdown`, `G_Sound`, `G_AddEvent`, `NPC_SetAnim` and the victim's `eFlags2`.
//!
//! An NPC's bolts are its server-side Ghoul2 instance's: the list `G2API_AddBolt` fills
//! ([`CreatureBolts`], in the order the reference adds them — the saber user's hand bolts
//! and `lower_lumbar` as its model is set up, then a class's own, `Rancor_SetBolts` and
//! `Wampa_SetBolts`), and `renderInfo`'s indices into it ([`RenderBolts`], zero until set:
//! the zeroed client's). Where a bolt is is the host's ([`NpcHost::npc_bolt`]): the model
//! posed at the Ghoul2 clock. A player reached is the host's too, through
//! [`NpcHost::player_force`] ([`crate::npc_force_update::PlayerForce`]): its state, its
//! knockdown, `G_Damage` on it.

use crate::damage::{Attacker, DamageRequest};
use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;

/// `ps.eFlags2` (the player state's wire field), `EF2_HELD_BY_MONSTER`.
pub(crate) const PS_EFLAGS2: usize = 103;
pub(crate) const EF2_HELD_BY_MONSTER: u32 = 1 << 0;
/// `EF2_USE_ALT_ANIM`, `EF2_GENERIC_NPC_FLAG` (`bg_public.h:682-687`).
pub(crate) const EF2_USE_ALT_ANIM: u32 = 1 << 1;
pub(crate) const EF2_GENERIC_NPC_FLAG: u32 = 1 << 3;
/// `CHAN_AUTO`, `CHAN_WEAPON`, `CHAN_VOICE` (`soundChannel_t`).
pub(crate) const CHAN_AUTO: u32 = 0;
pub(crate) const CHAN_WEAPON: u32 = 2;
pub(crate) const CHAN_VOICE: u32 = 3;
/// `MOD_MELEE`.
pub(crate) const MOD_MELEE: u32 = 2;
/// `DAMAGE_NO_ARMOR`, `DAMAGE_NO_KNOCKBACK`, `DAMAGE_NO_PROTECTION`, `DAMAGE_NO_HIT_LOC`
/// (`g_local.h:1165-1181`).
pub(crate) const DAMAGE_NO_ARMOR: u32 = 0x2;
pub(crate) const DAMAGE_NO_KNOCKBACK: u32 = 0x4;
pub(crate) const DAMAGE_NO_PROTECTION: u32 = 0x8;
pub(crate) const DAMAGE_NO_HIT_LOC: u32 = 0x2000;
/// The bolts `SetupGameGhoul2Model` adds a saber user (`g_client.c:1897-1906`), in order.
const SABER_USER_BOLTS: [&str; 5] = [
    "*r_hand",
    "*l_hand",
    "*chestg",
    "*r_hand_cap_r_arm",
    "*l_hand_cap_l_arm",
];

/// An NPC's server-side instance's bolt list (`CGhoul2Info::mBltlist`), in the order
/// `G2API_AddBolt` added them. Never more than a dozen names.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CreatureBolts(Vec<&'static str>);

impl CreatureBolts {
    /// `G2API_AddBolt`: the bolt's index — the one it already has, or the next — or -1
    /// where the model has no such bolt (`has`).
    pub fn add(&mut self, has: bool, name: &'static str) -> i32 {
        if !has {
            return -1;
        }
        let at = self
            .0
            .iter()
            .position(|known| known.eq_ignore_ascii_case(name))
            .unwrap_or_else(|| {
                self.0.push(name);
                self.0.len() - 1
            });
        at as i32
    }

    /// The bolt at `index`, `None` for an index the list does not hold (-1, or one never
    /// added), which `G2API_GetBoltMatrix` answers with the model's origin.
    pub fn name(&self, index: i32) -> Option<&'static str> {
        usize::try_from(index)
            .ok()
            .and_then(|index| self.0.get(index))
            .copied()
    }
}

/// `renderInfo`'s bolt indices a class's AI reads (`headBolt`, `handRBolt`, ...): zero —
/// the instance's first bolt — until a class sets them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderBolts {
    pub head: i32,
    pub hand_r: i32,
    pub hand_l: i32,
    pub torso: i32,
    pub crotch: i32,
    pub foot_r: i32,
    pub foot_l: i32,
}

/// What a monster or droid keeps on its entity beyond `gNPC_t`: `ent->activator` (the
/// victim a rancor holds), its instance's bolts and `renderInfo`'s indices into them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Creature {
    /// Native sand-creature hunting and breach state, independent of MP class IDs.
    pub sand: crate::npc_sand_creature::SandCreature,
    /// `ent->activator`.
    pub activator: Option<u16>,
    /// The instance's bolts.
    pub bolts: CreatureBolts,
    /// `renderInfo`'s.
    pub render: RenderBolts,
    /// `pos1`, `pos2`, `pos3`: the angles a droid's parts are turned to (the interrogator's
    /// syringe, scalpel and claw, R2-D2's eye).
    pub parts: [[f32; 3]; 3],
    /// What a flying machine keeps ([`crate::npc_machine::MachineMemory`]).
    pub machine: crate::npc_machine::MachineMemory,
    /// A walking machine's parts, laser and shield ([`crate::npc_machine_parts::Machine`]).
    pub walker: crate::npc_machine_parts::Machine,
}

/// `SetupGameGhoul2Model`'s bolts for an NPC (`g_client.c:1897-1937`): a saber user's hands,
/// chest and wrists, then `lower_lumbar` — which `noLumbar` records the want of.
pub fn setup_bolts(npc: &mut NpcActor, host: &mut impl NpcHost) {
    let model = npc.definition.player_model.clone();
    let bolts = &mut npc.mind.creature.bolts;
    if npc.player.weapon() == 3 {
        for name in SABER_USER_BOLTS {
            bolts.add(host.model_has_bolt(&model, name), name);
        }
    }
    bolts.add(host.model_has_bolt(&model, "lower_lumbar"), "lower_lumbar");
}

/// `WP_SaberPositionUpdate`'s `renderInfo` bolts for a humanoid (`w_saber.c:7361-7376`), which
/// it adds the first time it meets the NPC's instance: its eyes, hands, thoracic, pelvis, feet
/// and `Motion`, after the bolts the instance has (Boba Fett's flamethrower reads the hands).
/// Nothing for a creature (`localAnimIndex > 1`), whose class bolts stay.
pub fn add_render_bolts(npc: &mut NpcActor, host: &mut impl NpcHost) {
    if !npc.humanoid {
        return;
    }
    let model = npc.definition.player_model.clone();
    let creature = &mut npc.mind.creature;
    let mut add = |name: &'static str| creature.bolts.add(host.model_has_bolt(&model, name), name);
    let head = add("*head_eyes");
    let hand_r = add("*r_hand");
    let hand_l = add("*l_hand");
    let torso = add("thoracic");
    let crotch = add("pelvis");
    let foot_r = add("*r_leg_foot");
    let foot_l = add("*l_leg_foot");
    add("Motion");
    creature.render = RenderBolts {
        head,
        hand_r,
        hand_l,
        torso,
        crotch,
        foot_r,
        foot_l,
    };
}

/// `Rancor_SetBolts` (`NPC_AI_Rancor.c:37-47`) and `Wampa_SetBolts` (`NPC_AI_Wampa.c:38-58`),
/// which `NPC_SetMiscDefaultData` gives the rancor's class and the NPC called `wampa`.
pub fn set_class_bolts(npc: &mut NpcActor, host: &mut impl NpcHost, rancor: bool, wampa: bool) {
    let model = npc.definition.player_model.clone();
    let creature = &mut npc.mind.creature;
    let mut add = |name: &'static str| creature.bolts.add(host.model_has_bolt(&model, name), name);
    if wampa {
        let head = add("*head_eyes");
        let torso = add("lower_spine");
        let crotch = add("rear_bone");
        let hand_l = add("*l_hand");
        let hand_r = add("*r_hand");
        let foot_l = add("*l_leg_foot");
        let foot_r = add("*r_leg_foot");
        creature.render = RenderBolts {
            head,
            hand_r,
            hand_l,
            torso,
            crotch,
            foot_r,
            foot_l,
        };
    }
    if rancor {
        let hand_r = add("*r_hand");
        let hand_l = add("*l_hand");
        let head = add("*head_eyes");
        let torso = add("jaw_bone");
        creature.render = RenderBolts {
            hand_r,
            hand_l,
            head,
            torso,
            ..creature.render
        };
    }
}

/// `BG_AttachToRancor`'s placing of a victim (`bg_g2_utils.c:36-98`) from the rancor's
/// bolt — its jaw (`in_mouth`: `EF2_GENERIC_NPC_FLAG`) or its right hand — as
/// `G2API_GetBoltMatrix` gives it: where the victim is, and its view (the bolt's forward
/// axis, rolled by its up axis' pitch).
pub fn attach_to_rancor(bolt: [[f32; 4]; 3], in_mouth: bool) -> ([f32; 3], [f32; 3]) {
    let column = |index: usize, sign: f32| -> [f32; 3] {
        std::array::from_fn(|row| sign * bolt[row][index])
    };
    let origin = column(3, 1.0);
    // `BG_GiveMeVectorFromMatrix`: `POSITIVE_Z`/`NEGATIVE_X` in the mouth, `NEGATIVE_Y`/
    // `POSITIVE_Z` in the hand.
    let (forward, up) = if in_mouth {
        (column(2, 1.0), column(0, -1.0))
    } else {
        (column(1, -1.0), column(2, 1.0))
    };
    let mut angles = crate::player_angle_math::vector_angles(forward);
    let up_angles = crate::player_angle_math::vector_angles(up);
    angles[2] = -up_angles[0];
    (origin, angles)
}

/// The bolt `BG_AttachToRancor` reads: the jaw in the mouth, the right hand otherwise.
pub fn rancor_attach_bolt(in_mouth: bool) -> &'static str {
    if in_mouth { "jaw_bone" } else { "*r_hand" }
}

/// A client a creature's blow reaches, as the blow reads it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Reached {
    pub number: u16,
    /// `r.currentOrigin`.
    pub origin: [f32; 3],
    pub health: i32,
    /// `client->NPC_class` (`CLASS_NONE`, 0, for a player).
    pub class: i32,
    /// `ps.eFlags2`, `ps.groundEntityNum`.
    pub flags2: u32,
    pub ground: u16,
    /// `BG_KnockDownable(&ps)`.
    pub knockdownable: bool,
    /// Whether it has a pain function (an NPC; a player has none).
    pub has_pain: bool,
    /// `localAnimIndex == 0`.
    pub humanoid: bool,
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// The monsters' and droids' own behaviours as `NPC_RunBehavior` reaches them
    /// ([`crate::npc_behavior::Behavior::Stub`] by the reference's name): run, and `true`;
    /// `false` for a name that is none of theirs, or while a replay's driver stands the
    /// later classes' AI in (`NpcLevel::jedi_ai_stood_in`).
    pub(crate) fn run_creature(
        &mut self,
        me: usize,
        name: &str,
        command: &mut sjk_protocol::UserCommand,
    ) -> bool {
        if self.level.jedi_ai_stood_in() {
            return false;
        }
        match name {
            "NPC_BSRancor_Default" => self.bs_rancor_default(me, command),
            "NPC_BSWampa_Default" => self.bs_wampa_default(me, command),
            "NPC_BSHowler_Default" => self.bs_howler_default(me, command),
            "NPC_BSMineMonster_Default" => self.bs_mine_monster_default(me, command),
            "NPC_BSInterrogator_Default" => self.bs_interrogator_default(me, command),
            "NPC_BSDroid_Default" => self.bs_droid_default(me, command),
            "NPC_BSSentry_Default" => self.bs_sentry_default(me, command),
            "NPC_BSImperialProbe_Default" => self.bs_imperial_probe_default(me, command),
            "NPC_BSSeeker_Default" => self.bs_seeker_default(me, command),
            "NPC_BSRemote_Default" => self.bs_remote_default(me, command),
            "NPC_BSMark1_Default" => self.bs_mark1_default(me, command),
            "NPC_BSMark2_Default" => self.bs_mark2_default(me, command),
            "NPC_BSGM_Default" => self.bs_gm_default(me, command),
            "NPC_BSATST_Default" => self.bs_atst_default(me, command),
            _ => return false,
        }
        true
    }

    /// The monsters' and droids' own pains (`NPC_PainFunc`'s, by the reference's name):
    /// run, and `true`; `false` as for [`Self::run_creature`].
    pub(crate) fn creature_pain(
        &mut self,
        me: usize,
        name: &str,
        attacker: Option<u16>,
        damage: i32,
        means: u32,
    ) -> bool {
        if self.level.jedi_ai_stood_in() {
            return false;
        }
        match name {
            "NPC_Rancor_Pain" => self.rancor_pain(me, attacker, damage),
            "NPC_Wampa_Pain" => self.wampa_pain(me, attacker, damage),
            "NPC_Howler_Pain" => self.howler_pain(me, attacker, damage),
            "NPC_MineMonster_Pain" => self.mine_monster_pain(me, attacker, damage),
            "NPC_Droid_Pain" => self.droid_pain(me, attacker, damage, means),
            "NPC_Sentry_Pain" => self.sentry_pain(me, attacker, damage, means),
            "NPC_Probe_Pain" => self.probe_pain(me, attacker, damage, means),
            "NPC_Seeker_Pain" => self.seeker_pain(me, attacker, damage, means),
            "NPC_Remote_Pain" => self.remote_pain(me, attacker, damage, means),
            "NPC_Mark1_Pain" => self.mark1_pain(me, attacker, damage, means),
            "NPC_Mark2_Pain" => self.mark2_pain(me, attacker, damage, means),
            "NPC_GM_Pain" => self.gm_pain(me, attacker, damage, means),
            _ => return false,
        }
        true
    }

    /// `G_GetBoltPosition(NPC, bolt, pos, 0)`: where bolt `index` of the NPC at `me` is.
    pub(crate) fn bolt_position(&mut self, me: usize, index: i32) -> [f32; 3] {
        let name = self.actors[me].mind.creature.bolts.name(index);
        let level_time = self.level_time;
        self.host
            .npc_bolt(&self.actors[me], index, name, level_time)
    }

    /// `NPC_GetEntsNearBolt` (`NPC_utils.c:1777-1798`): where bolt `index` is, and every
    /// client (player or NPC) whose linked box meets the cube `radius` about it, in entity
    /// number order — `trap->EntitiesInBox`, but for the entities that are no client, which
    /// every caller skips. Appended to `out`.
    pub(crate) fn ents_near_bolt(
        &mut self,
        me: usize,
        radius: f32,
        index: i32,
        out: &mut Vec<u16>,
    ) -> [f32; 3] {
        let at = self.bolt_position(me, index);
        let mins = at.map(|axis| axis - radius);
        let maxs = at.map(|axis| axis + radius);
        let meets = |absmin: [f32; 3], absmax: [f32; 3]| {
            (0..3).all(|axis| absmin[axis] <= maxs[axis] && absmax[axis] >= mins[axis])
        };
        out.clear();
        for player in self.host.players() {
            let absmin = std::array::from_fn(|axis| player.origin[axis] + player.mins[axis] - 1.0);
            let absmax = std::array::from_fn(|axis| player.origin[axis] + player.maxs[axis] + 1.0);
            if meets(absmin, absmax) {
                out.push(player.number);
            }
        }
        for &npc in self.order {
            let npc = &self.actors[npc];
            if npc.begun() && meets(npc.link.0, npc.link.1) {
                out.push(npc.number);
            }
        }
        at
    }

    /// Client `number` as a blow reads it: an NPC's own record, or the player the host
    /// hands out; `None` for neither.
    pub(crate) fn reached(&mut self, number: u16) -> Option<Reached> {
        if let Some(at) = self.actor_at(number) {
            let npc = &self.actors[at];
            return Some(Reached {
                number,
                origin: npc.current_origin,
                health: npc.health,
                class: npc.definition.client_class,
                flags2: npc.player.raw_field(PS_EFLAGS2).unwrap_or(0),
                ground: npc.player.ground_entity_num(),
                knockdownable: crate::knockdown::knockdownable(&npc.player),
                has_pain: true,
                humanoid: npc.humanoid,
            });
        }
        let body = *self
            .host
            .players()
            .iter()
            .find(|body| body.number == number)?;
        let players = self.host.player_force()?;
        players.force_begin();
        let reached = players.force_player(number).map(|other| Reached {
            number,
            origin: body.origin,
            health: *other.health,
            class: 0,
            flags2: other.state.raw_field(PS_EFLAGS2).unwrap_or(0),
            ground: other.state.ground_entity_num(),
            knockdownable: crate::knockdown::knockdownable(other.state),
            has_pain: false,
            humanoid: true,
        });
        players.force_end();
        reached
    }

    /// Runs `change` on client `number`'s state and knockdown memory: an NPC's own, or the
    /// player's the host hands out (within a use of the players' reach, so that the host
    /// restarts a changed player's movement: [`crate::npc_force_update::PlayerForce`]).
    /// `None` where it is neither.
    pub(crate) fn with_client<T>(
        &mut self,
        number: u16,
        change: impl FnOnce(&mut sjk_protocol::PlayerState, &mut crate::knockdown::Knockdown) -> T,
    ) -> Option<T> {
        if let Some(at) = self.actor_at(number) {
            let npc = &mut self.actors[at];
            return Some(change(&mut npc.player, &mut npc.mind.knockdown));
        }
        let players = self.host.player_force()?;
        players.force_begin();
        let changed = players
            .force_player(number)
            .map(|other| change(other.state, other.knockdown));
        players.force_end();
        changed
    }

    /// `G_Damage(target, NPC, NPC, dir, point, damage, dflags, mod)` by the NPC at `me`: an
    /// NPC's through its own damage rules (its pain, its death), a player's through the
    /// host; the NPC's hit counter credited.
    pub(crate) fn creature_damage(
        &mut self,
        me: usize,
        target: u16,
        direction: Option<[f32; 3]>,
        point: Option<[f32; 3]>,
        damage: i32,
        flags: u32,
        means: u32,
    ) {
        let npc = &self.actors[me];
        let attacker = Attacker {
            npc: true,
            client: npc.number,
            max_health: npc.player.stats[crate::npc_begin::STAT_MAX_HEALTH] as i32,
            team: npc.session_team,
            saber_knockback: [0.0; 4],
        };
        let request = DamageRequest {
            level_time: self.level_time,
            attacker: Some(attacker),
            direction,
            point,
            damage,
            flags,
            means,
        };
        let (hits, armor) = if self.actor_at(target).is_some() {
            let damaged = self.force_blow(target, request);
            (damaged.attacker_hits, damaged.attackee_armor)
        } else {
            match self.host.player_force() {
                Some(players) => players.force_damage(target, request),
                None => (0, None),
            }
        };
        if hits != 0 {
            let persistent = &mut self.actors[me].player.persistent;
            persistent[1] = (persistent[1] as i32 + hits) as u32;
            persistent[7] = armor.unwrap_or(0);
        }
    }

    /// `G_Throw(target, dir, push)` on a client (`g_combat.c`, [`crate::saber_splash::throw`]).
    pub(crate) fn creature_throw(&mut self, target: u16, direction: [f32; 3], push: f32) {
        self.with_client(target, |state, _| {
            crate::saber_splash::throw(state, direction, push)
        });
    }

    /// `G_Knockdown(target)` on a client (`g_combat.c:4383-4392`).
    pub(crate) fn creature_knockdown(&mut self, target: u16) {
        let level_time = self.level_time;
        self.with_client(target, |state, memory| {
            crate::knockdown::knock_down(state, memory, level_time)
        });
    }

    /// `G_Sound(ent, channel, sound)`: a sound where client or NPC `number` stands.
    pub(crate) fn creature_sound(&mut self, number: u16, channel: u32, name: &[u8]) {
        let sound = self.host.sound_index(name);
        let Some(origin) = self.body(number).map(|body| body.origin) else {
            return;
        };
        self.host
            .raise(crate::weapon_fire::sound_event(origin, channel, sound));
    }

    /// `NPC_SetAnim(ent, parts, anim, flags)` on client `number`: an NPC with its own
    /// skeleton's lengths, a player with the humanoid's (and its sabers' speed taken as
    /// plain).
    pub(crate) fn client_animation(&mut self, number: u16, parts: u8, animation: u16, flags: u8) {
        if let Some(at) = self.actor_at(number) {
            self.set_animation(at, parts, animation, flags);
            return;
        }
        let Some(lengths) = self.host.humanoid_animations() else {
            return;
        };
        self.with_client(number, |state, _| {
            crate::pmove_anim::animate(state, parts, animation, flags, &*lengths, [1.0; 2])
        });
    }

    /// `G_AddEvent(ent, event, parm)` on client `number`.
    pub(crate) fn client_event(&mut self, number: u16, event: u32, parameter: u32) {
        if let Some(at) = self.actor_at(number) {
            self.add_event(at, event, parameter);
            return;
        }
        self.with_client(number, |state, _| {
            crate::player_entity::add_event(state, event, parameter)
        });
    }

    /// `AngleVectors(angles)`'s forward.
    pub(crate) fn forward_of(angles: [f32; 3]) -> [f32; 3] {
        crate::pmove::flight::flight_axes(angles).0.to_array()
    }
}
