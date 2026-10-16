//! An NPC's pain (`codemp/game/NPC_reactions.c`): which pain function it has
//! (`NPC_PainFunc`, `NPC_spawn.c:116-199`, and `NPC_SetMiscDefaultData`'s wampa and
//! rancor), a stormtrooper's (`NPC_ST_Pain`, `NPC_AI_Stormtrooper.c:280-296`) and everyone
//! else's (`NPC_Pain`, `:378-543`): an ally's patience with the player's stray shots, the
//! flinch (`NPC_ChoosePainAnimation`, `:221-372`, with `NPC_GetPainChance`, `:171-213`),
//! the pain sound (`NPC_SetPainEvent`, `:151-165`), the attacker taken for an enemy
//! (`NPC_CheckAttacker`, `:62-149`) and the `paintarget` fired.
//!
//! The pain functions of the classes with their own — Jedi (a saber carrier), seekers,
//! remotes, mine monsters, howlers, droids, probes, sentries, Mark I and II, the AT-ST,
//! Galak's mech, the rancor and the wampa — are their AI's (the NPC plan's steps 6–8):
//! [`NpcHost::stub`] names them. Nothing runs scripts (`G_ActivateBehavior`'s
//! `BSET_PAIN`, `BSET_FLEE`, `BSET_FFIRE`), and no NPC is gripped by the Force yet.
//!
//! Held to `tools/game-oracle/npccombat.c` (`game-npccombat.txt`).

use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_LEGS};

/// The pain function an NPC was given (`ent->pain`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PainFunc {
    /// `NPC_Pain`.
    #[default]
    Plain,
    /// `NPC_ST_Pain`: stormtroopers and swamptroopers.
    Trooper,
    /// `NPC_ATST_Pain`: a walker vehicle's (`G_ATSTCheckPain`'s groan, then `NPC_Pain`).
    Atst,
    /// A class's own, a later step's: its name.
    Stub(&'static str),
}

/// `class_t`s the pain reads.
const CLASS_ATST: i32 = 1;
const CLASS_DESANN: i32 = 6;
const CLASS_GALAKMECH: i32 = 25;
const CLASS_PROTOCOL: i32 = 33;
const CLASS_STORMTROOPER: i32 = 44;
const CLASS_SWAMPTROOPER: i32 = 46;
const CLASS_RANCOR: i32 = 54;
/// `WP_SABER`, `WP_THERMAL`.
const WP_SABER: i32 = 3;
const WP_THERMAL: i32 = 12;
/// `MOD_MELEE`, `MOD_CRUSH`.
const MOD_MELEE: u32 = 2;
const MOD_CRUSH: u32 = 36;
/// `RANK_CAPTAIN`.
const RANK_CAPTAIN: i32 = 7;
/// `LSTATE_UNDERFIRE`.
const LSTATE_UNDERFIRE: i32 = 1;
/// `NPCTEAM_PLAYER`.
const NPCTEAM_PLAYER: i32 = 2;
/// `NPCAI_DIE_ON_IMPACT`.
const NPCAI_DIE_ON_IMPACT: u32 = 0x10_0000;
/// `PM_DEAD`.
const PM_DEAD: u8 = 5;
/// `BS_DEFAULT`.
const BS_DEFAULT: i32 = 0;
/// `FL_NOTARGET`.
const FL_NOTARGET: u32 = 0x20;
/// `EV_PAIN`, `EV_PUSHED1`, `EV_CHOKE1`, `EV_FFWARN`, `EV_FFTURN`.
const EV_PAIN: u32 = 89;
const EV_PUSHED1: i32 = 125;
const EV_CHOKE1: i32 = 128;
const EV_CHOKE3: i32 = 130;
const EV_FFWARN: i32 = 131;
const EV_FFTURN: i32 = 132;
/// `BOTH_PAIN1`, `BOTH_PAIN2`, `BOTH_PAIN3`, `BOTH_PAIN18`.
const BOTH_PAIN1: u16 = 95;
const BOTH_PAIN2: u16 = 96;
const BOTH_PAIN3: u16 = 97;
const BOTH_PAIN18: u16 = 112;
/// `FORCE_LEVEL_1`; `LS_READY`.
const FORCE_LEVEL_1: u32 = 1;
const LS_READY: u32 = 1;
/// `ps.legsAnim`, `ps.torsoAnim`, `ps.legsTimer`, `ps.weaponTime`,
/// `ps.fd.saberAnimLevel`, `ps.saberMove`.
const PS_LEGS_ANIM: usize = 13;
const PS_TORSO_ANIM: usize = 15;
const PS_LEGS_TIMER: usize = 21;
const PS_WEAPON_TIME: usize = 10;
const PS_SABER_ANIM_LEVEL: usize = 23;
const PS_SABER_MOVE: usize = 34;
/// `STAT_MAX_HEALTH`.
const STAT_MAX_HEALTH: usize = 8;
/// `SCF_*` the ally turning on the player clears and sets.
const SCF_CROUCHED: u32 = 0x1;
const SCF_WALKING: u32 = 0x2;
const SCF_CHASE_ENEMIES: u32 = 0x400;
const SCF_DONT_FIRE: u32 = 0x4000;
const SCF_NO_COMBAT_TALK: u32 = 0x200;
const SCF_NO_MIND_TRICK: u32 = 0x8_0000;
const SCF_FORCED_MARCH: u32 = 0x1_0000;
/// `SVF_ICARUS_FREEZE`.
const SVF_ICARUS_FREEZE: u32 = 0x8000;

/// `NPC_PainFunc` and `NPC_SetMiscDefaultData`'s own (`NPC_spawn.c:116-199`, `256-271`):
/// the pain an NPC is given as it begins — a saber carrier's the Jedi's, a trooper's its
/// own, a droid's or a monster's its class's, anyone else's `NPC_Pain`; the wampa (by
/// name) and the rancor their own whatever they carry.
pub fn pain_func(npc: &NpcActor) -> PainFunc {
    let class = npc.definition.client_class;
    if npc.npc_type.eq_ignore_ascii_case(b"wampa") {
        return PainFunc::Stub("NPC_Wampa_Pain");
    }
    if class == CLASS_RANCOR {
        return PainFunc::Stub("NPC_Rancor_Pain");
    }
    // A walker vehicle groans as an AT-ST (`NPC_SetMiscDefaultData`, `NPC_spawn.c:243-253`).
    if npc
        .vehicle
        .as_deref()
        .is_some_and(|vehicle| vehicle.kind() == crate::vehicle_fields::kind::WALKER)
    {
        return PainFunc::Atst;
    }
    if i32::from(npc.player.weapon()) == WP_SABER {
        return PainFunc::Stub("NPC_Jedi_Pain");
    }
    match class {
        CLASS_STORMTROOPER | CLASS_SWAMPTROOPER => PainFunc::Trooper,
        41 => PainFunc::Stub("NPC_Seeker_Pain"),
        39 => PainFunc::Stub("NPC_Remote_Pain"),
        26 => PainFunc::Stub("NPC_MineMonster_Pain"),
        13 => PainFunc::Stub("NPC_Howler_Pain"),
        // CLASS_GONK, CLASS_R2D2, CLASS_R5D2, CLASS_MOUSE, CLASS_PROTOCOL,
        // CLASS_INTERROGATOR.
        11 | 34 | 35 | 29 | CLASS_PROTOCOL | 16 => PainFunc::Stub("NPC_Droid_Pain"),
        32 => PainFunc::Stub("NPC_Probe_Pain"),
        42 => PainFunc::Stub("NPC_Sentry_Pain"),
        23 => PainFunc::Stub("NPC_Mark1_Pain"),
        24 => PainFunc::Stub("NPC_Mark2_Pain"),
        CLASS_ATST => PainFunc::Atst,
        CLASS_GALAKMECH => PainFunc::Stub("NPC_GM_Pain"),
        _ => PainFunc::Plain,
    }
}

/// An animation by name, for the sets the pain asks after.
pub(crate) fn named(animation: u16, names: &[&str]) -> bool {
    crate::legacy_animation_name(usize::from(animation)).is_some_and(|name| names.contains(&name))
}

/// `PM_InCartwheel` (`bg_panimate.c:1014-1027`).
fn in_cartwheel(animation: u16) -> bool {
    named(
        animation,
        &[
            "BOTH_ARIAL_LEFT",
            "BOTH_ARIAL_RIGHT",
            "BOTH_ARIAL_F1",
            "BOTH_CARTWHEEL_LEFT",
            "BOTH_CARTWHEEL_RIGHT",
        ],
    )
}

/// `BG_CrouchAnim` (`bg_panimate.c:68-88`).
pub(crate) fn crouching(animation: u16) -> bool {
    named(
        animation,
        &[
            "BOTH_SIT1",
            "BOTH_SIT2",
            "BOTH_SIT3",
            "BOTH_CROUCH1",
            "BOTH_CROUCH1IDLE",
            "BOTH_CROUCH1WALK",
            "BOTH_CROUCH1WALKBACK",
            "BOTH_CROUCH2TOSTAND1",
            "BOTH_CROUCH3",
            "BOTH_KNEES1",
            "BOTH_CROUCHATTACKBACK1",
            "BOTH_ROLL_STAB",
        ],
    )
}

/// `PM_RollingAnim` (`bg_pmove.c:4633-4645`).
pub(crate) fn rolling(animation: u16) -> bool {
    named(
        animation,
        &["BOTH_ROLL_F", "BOTH_ROLL_B", "BOTH_ROLL_L", "BOTH_ROLL_R"],
    )
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `ent->pain(npc, attacker, take)` as `G_Damage` calls it, with `gPainMOD` and
    /// `gPainPoint`; `gPainHitLoc` is -1, no Ghoul2 surface having been struck.
    pub(crate) fn pain(
        &mut self,
        me: usize,
        attacker: Option<u16>,
        damage: i32,
        means: u32,
        _point: [f32; 3],
    ) {
        let pain = self.actors[me].mind.fight.pain;
        match pain {
            PainFunc::Plain => self.npc_pain(me, attacker, damage, means),
            PainFunc::Trooper => {
                let level_time = self.level_time;
                let npc = &mut self.actors[me];
                npc.mind.fight.local_state = LSTATE_UNDERFIRE;
                npc.mind.timers.set("duck", level_time, -1);
                npc.mind.timers.set("hideTime", level_time, -1);
                npc.mind.timers.set("stand", level_time, 2_000);
                self.npc_pain(me, attacker, damage, means);
                if damage == 0 && self.actors[me].health > 0 {
                    let event = self.host.irand(EV_PUSHED1, EV_PUSHED1 + 2);
                    self.add_voice(me, event, 2_000);
                }
            }
            PainFunc::Atst => {
                // `G_ATSTCheckPain` (`NPC_AI_Atst.c:88-135`): one of two groans, by `rand()`.
                let groan: &[u8] = if self.host.crt_rand() & 1 != 0 {
                    b"sound/chars/atst/atst_damaged1"
                } else {
                    b"sound/chars/atst/atst_damaged2"
                };
                // `G_SoundOnEnt(self, CHAN_LESS_ATTEN, ...)`: an `EV_ENTITY_SOUND` naming it.
                // `CHAN_LESS_ATTEN` (`q_shared.h:865`).
                const CHAN_LESS_ATTEN: u32 = 10;
                let sound = self.host.sound_index(groan);
                let npc = &self.actors[me];
                let mut event =
                    crate::knockdown::entity_sound(npc.current_origin, npc.number, CHAN_LESS_ATTEN);
                event.parameter = u32::from(sound);
                self.host.raise(event);
                self.npc_pain(me, attacker, damage, means);
            }
            // `NPC_Jedi_Pain` ([`crate::npc_jedi_patrol`]), but for a replay whose driver stood
            // the class pains in.
            PainFunc::Stub("NPC_Jedi_Pain") if !self.level.jedi_ai_stood_in() => {
                self.jedi_pain(me, attacker, damage, means)
            }
            // The monsters' and droids' own ([`crate::npc_creature`]).
            PainFunc::Stub(name) if self.creature_pain(me, name, attacker, damage, means) => {}
            PainFunc::Stub(name) => self.host.stub(self.actors[me].number, name),
        }
    }

    /// `NPC_Pain` (`NPC_reactions.c:378-543`).
    pub(crate) fn npc_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        let Some(other_number) = attacker else {
            // The world: no team, no weapon, nobody to be angry at (`VALIDENT` fails).
            self.flinch_unless_ignoring(me, None, damage, means, -1);
            return;
        };
        let npc = &self.actors[me];
        if npc.player.movement_type() == PM_DEAD || other_number == npc.number {
            return;
        }
        let other = self.body(other_number);
        let mut voice = -1;
        let teammate =
            other.is_some_and(|other| npc.player_team != 0 && other.player_team == npc.player_team);
        if let Some(other) = other.filter(|_| teammate)
            && npc.mind.enemy != Some(other_number)
            && other.enemy != Some(npc.number)
        {
            if npc.mind.enemy.is_some() || other.enemy.is_some() {
                // An accident in a fight: a flinch, maybe a word of warning.
                if damage != -1 {
                    let warn = self.host.irand(0, 1) != 0;
                    self.choose_pain_animation(
                        me,
                        Some(other_number),
                        damage,
                        means,
                        if warn { EV_FFWARN } else { -1 },
                    );
                }
                return;
            }
            if other_number == 0 {
                if npc.mind.charmed_time != 0 {
                    return;
                }
                if npc.mind.ffire_count < 3 + (2 - self.host.skill()) * 2 {
                    if damage != -1 {
                        let warn = self.host.irand(0, 1) != 0;
                        self.choose_pain_animation(
                            me,
                            Some(other_number),
                            damage,
                            means,
                            if warn { EV_FFWARN } else { -1 },
                        );
                    }
                    return;
                }
                // Enough: the ally turns on the player (no friendly-fire script).
                voice = EV_FFTURN;
                self.turn_on(me, other_number);
            }
        }
        self.flinch_unless_ignoring(me, Some(other_number), damage, means, voice);
        // `paintarget` (`G_UseTargets2(self, other, paintarget)`): fired by the roster's caller.
        if let Some(target) = self.actors[me]
            .pain_target
            .clone()
            .filter(|target| !target.is_empty())
        {
            self.fired.push(target);
        }
    }

    /// `NPC_Pain`'s end (`NPC_reactions.c:506-530`): unless it ignores pain (a script's
    /// `ignorePain`, never set here), no confusion, the flinch, and the attacker taken on.
    fn flinch_unless_ignoring(
        &mut self,
        me: usize,
        other: Option<u16>,
        damage: i32,
        means: u32,
        voice: i32,
    ) {
        self.actors[me].mind.confusion_time = 0;
        if damage != -1 {
            self.choose_pain_animation(me, other, damage, means, voice);
        }
        if let Some(other) = other
            && self.actors[me].mind.enemy != Some(other)
        {
            self.check_attacker(me, other, means);
        }
    }

    /// An ally turning on the player who kept shooting it (`NPC_reactions.c:481-500`).
    fn turn_on(&mut self, me: usize, player: u16) {
        let npc = &mut self.actors[me];
        npc.mind.blocked_speech_until = 0;
        npc.behavior_state = BS_DEFAULT;
        npc.mind.temp_behavior = BS_DEFAULT;
        npc.default_behavior = BS_DEFAULT;
        npc.server_flags &= !SVF_ICARUS_FREEZE;
        self.host.clear_player_notarget(player);
        self.set_enemy(me, player);
        let npc = &mut self.actors[me];
        npc.script_flags &=
            !(SCF_DONT_FIRE | SCF_CROUCHED | SCF_WALKING | SCF_NO_COMBAT_TALK | SCF_FORCED_MARCH);
        npc.script_flags |= SCF_CHASE_ENEMIES | SCF_NO_MIND_TRICK;
    }

    /// `NPC_ChoosePainAnimation` (`NPC_reactions.c:221-372`): unless it hurts already (a
    /// punch always gets through) or is throwing a detonator, the chance of a flinch — by
    /// the attacker's saber, a punch against rank, a protocol droid's nerves, or
    /// `NPC_GetPainChance` — and, drawn, the pain animation (none out of a spin, a special
    /// attack, a knockdown, a roll or a flip), the saber's next attack a quick one, the
    /// voice or the pain event, and no more pain or weapon until the animation is over.
    fn choose_pain_animation(
        &mut self,
        me: usize,
        other: Option<u16>,
        damage: i32,
        means: u32,
        voice: i32,
    ) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        if level_time < npc.mind.fight.pain_debounce_time && means != MOD_MELEE {
            return;
        }
        let weapon = i32::from(npc.player.weapon());
        if weapon == WP_THERMAL && (npc.player.raw_field(PS_WEAPON_TIME).unwrap_or(0) as i32) > 0 {
            return;
        }
        let class = npc.definition.client_class;
        let other_body = other.and_then(|number| self.body(number));
        let chance = if class == CLASS_GALAKMECH {
            // Its antenna struck (`HL_GENERIC1`) always hurts.
            if npc.mind.creature.walker.pain_hit_location == crate::npc_machine_parts::HL_GENERIC1 {
                1.0
            } else if npc.health > 200 && damage < 100 {
                0.05
            } else {
                (200.0 - npc.health as f32) / 100.0 + damage as f32 / 50.0
            }
        } else if npc.player_team == NPCTEAM_PLAYER && other == Some(0) {
            1.1
        } else {
            // The world is an attacker too (`G_Damage`'s default), with no weapon.
            let mut chance =
                if other_body.is_some_and(|body| body.weapon == WP_SABER) || means == MOD_CRUSH {
                    1.0
                } else if means == MOD_MELEE {
                    1.0 - ((RANK_CAPTAIN - npc.definition.rank) as f32 / RANK_CAPTAIN as f32)
                } else if class == CLASS_PROTOCOL {
                    1.0
                } else {
                    self.pain_chance(me, damage)
                };
            if class == CLASS_DESANN {
                chance *= 0.5;
            }
            chance
        };
        if self.host.rng().flrand(0.0, 1.0) >= chance {
            return;
        }
        let animation = if self.actors[me].force.grip_being_gripped < level_time as f32 {
            let Some(animation) = self.flinch(me, class, means, weapon) else {
                return;
            };
            if voice != -1 {
                let debounce = self.host.irand(2_000, 4_000);
                self.add_voice(me, voice, debounce);
            } else {
                self.pain_event(me);
            }
            animation
        } else {
            // Gripped (or drained): it chokes instead (`NPC_reactions.c:352-355`).
            let event = self.host.irand(EV_CHOKE1, EV_CHOKE3);
            self.add_voice(me, event, 0);
            // `pain_anim` stays -1, and the reference reads its length from before the
            // animation table (undefined); the oracle's build reads 0.
            None
        };
        // How long the animation runs: its frames on the NPC's own skeleton, at the
        // humanoid's frame time (`bgHumanoidAnimations[pain_anim].frameLerp`).
        let length = animation.map_or(0, |animation| self.animation_length(me, animation));
        let npc = &mut self.actors[me];
        npc.mind.fight.pain_debounce_time = level_time + length;
        npc.player.set_raw_field(PS_WEAPON_TIME, 0);
    }

    /// The flinch of an NPC nobody grips (`NPC_reactions.c:293-343`): `None` when a strong
    /// attack, roll, knockdown, flip or spin cannot be interrupted (the pain ends there);
    /// else the pain animation played, if any, the next attack a quick one.
    fn flinch(&mut self, me: usize, class: i32, means: u32, weapon: i32) -> Option<Option<u16>> {
        let npc = &self.actors[me];
        let legs = npc.player.raw_field(PS_LEGS_ANIM).unwrap_or(0) as u16;
        let torso = npc.player.raw_field(PS_TORSO_ANIM).unwrap_or(0) as u16;
        let legs_timer = npc.player.raw_field(PS_LEGS_TIMER).unwrap_or(0) as i32;
        let flipping = crate::pmove_roll_anim::flipping(legs) && !in_cartwheel(legs);
        if crate::pmove_roll_anim::spinning_saber(legs)
            || crate::saber_rules::special_attack(torso)
            || crate::pmove_hand_extend::in_knockdown(legs, legs_timer)
            || rolling(legs)
            || flipping
        {
            return None;
        }
        let animation = if class == CLASS_GALAKMECH {
            Some(BOTH_PAIN1)
        } else if means == MOD_MELEE || weapon == WP_SABER {
            self.pick_animation(me, BOTH_PAIN2, BOTH_PAIN3)
        } else {
            None
        };
        let animation = animation.or_else(|| self.pick_animation(me, BOTH_PAIN1, BOTH_PAIN18));
        let npc = &mut self.actors[me];
        npc.player.set_raw_field(PS_SABER_ANIM_LEVEL, FORCE_LEVEL_1);
        npc.player.set_raw_field(PS_SABER_MOVE, LS_READY);
        let parts = if crouching(legs) || in_cartwheel(legs) {
            SETANIM_LEGS
        } else {
            SETANIM_BOTH
        };
        if let Some(animation) = animation {
            self.set_animation(
                me,
                parts,
                animation,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
        Some(animation)
    }

    /// `NPC_GetPainChance` (`NPC_reactions.c:171-213`): certain for an NPC with no enemy
    /// (surprised) or a blow above half its health; else more the more it is hurt and the
    /// harder the blow, less on harder skills.
    pub(crate) fn pain_chance(&self, me: usize, damage: i32) -> f32 {
        let npc = &self.actors[me];
        if npc.mind.enemy.is_none() {
            return 1.0;
        }
        let max = npc.player.stats[STAT_MAX_HEALTH] as i32;
        if damage as f32 > max as f32 / 2.0 {
            return 1.0;
        }
        let chance =
            (max - npc.health) as f32 / (max as f32 * 2.0) + damage as f32 / (max as f32 / 2.0);
        match self.host.skill() {
            0 => chance,
            1 => chance * 0.5,
            _ => chance * 0.1,
        }
    }

    /// `NPC_SetPainEvent` (`NPC_reactions.c:151-165`): `EV_PAIN` with the health left, in
    /// hundredths of the most — unless it is to die on impact.
    fn pain_event(&mut self, me: usize) {
        let npc = &self.actors[me];
        if npc.ai_flags & NPCAI_DIE_ON_IMPACT != 0 {
            return;
        }
        let max = npc.player.stats[STAT_MAX_HEALTH] as i32;
        // `floor((float)health / max * 100.0f)`.
        let parameter = (f64::from(npc.health as f32 / max as f32 * 100.0)).floor() as i32;
        self.add_event(me, EV_PAIN, parameter as u32);
    }

    /// `G_AddEvent` on an NPC: its external event, and when (`eventTime`).
    pub(crate) fn add_event(&mut self, me: usize, event: u32, parameter: u32) {
        let npc = &mut self.actors[me];
        crate::player_entity::add_event(&mut npc.player, event, parameter);
        npc.mind.event_time = self.level_time;
    }

    /// `BG_PickAnim(localAnimIndex, first, last)` (`bg_panimate.c:2912-2930`): drawn until
    /// one the NPC's skeleton has, `None` after a thousand draws.
    pub(crate) fn pick_animation(&mut self, me: usize, first: u16, last: u16) -> Option<u16> {
        for _ in 0..1_000 {
            let animation = self.host.irand(i32::from(first), i32::from(last)) as u16;
            let has = self.actors[me]
                .movement
                .animation_lengths()
                .and_then(|lengths| lengths.timing(animation))
                .is_some_and(|timing| timing.frame_count > 0);
            if has {
                return Some(animation);
            }
        }
        None
    }

    /// An animation's frames on the NPC's skeleton times the humanoid's frame time for it
    /// (`numFrames * fabs((float)frameLerp)`, truncated).
    fn animation_length(&mut self, me: usize, animation: u16) -> i32 {
        let frames = self.actors[me]
            .movement
            .animation_lengths()
            .and_then(|lengths| lengths.timing(animation))
            .map_or(0, |timing| i32::from(timing.frame_count));
        let lerp = match self.host.humanoid_animations() {
            Some(humanoid) => humanoid
                .timing(animation)
                .map_or(0, |timing| timing.frame_lerp_ms),
            None => self.actors[me]
                .movement
                .animation_lengths()
                .and_then(|lengths| lengths.timing(animation))
                .map_or(0, |timing| timing.frame_lerp_ms),
        };
        (frames as f32 * (lerp as f32).abs()) as i32
    }

    /// `NPC_CheckAttacker` (`NPC_reactions.c:62-149`): the one who hurt the NPC, unless it
    /// wants no enemies (`FL_NOTARGET`), taken for its enemy — at once with none or a dead
    /// one, a Jedi's at once for a saber's blow; the player, instead, maybe made to take
    /// the NPC for its own by chance (more likely on harder skills).
    fn check_attacker(&mut self, me: usize, other: u16, means: u32) {
        let Some(body) = self.body(other) else { return };
        if other == self.actors[me].number || body.flags & FL_NOTARGET != 0 {
            return;
        }
        let Some(enemy) = self.actors[me].mind.enemy else {
            self.set_enemy(me, other);
            return;
        };
        if self.body(enemy).is_none_or(|enemy| enemy.health <= 0) {
            self.clear_enemy(me);
            self.set_enemy(me, other);
            return;
        }
        if other == enemy {
            return;
        }
        if i32::from(self.actors[me].player.weapon()) == WP_SABER
            && means == crate::means_of_death::MOD_SABER
        {
            self.clear_enemy(me);
            self.set_enemy(me, other);
            return;
        }
        if other == 0 {
            let luck = match self.host.skill() {
                0 => 0.9,
                1 => 0.5,
                _ => 0.0,
            };
            if self.host.rng().flrand(0.0, 1.0) > luck {
                self.host.set_player_enemy(0, Some(self.actors[me].number));
            }
        }
    }
}
