//! An NPC's weapon (`codemp/game/NPC_combat.c`): the timing a weapon is given when it is
//! taken up (`ChangeWeapon`, `:594-858`), the think that fires it (`WeaponThink`,
//! `:1052-1076`; `ShootThink`, `:941-1045`; `NPC_ApplyWeaponFireDelay`, `:894-935`;
//! `NPC_AttackDebounceForWeapon`, `:1258-1297`), and the fire events its move raises,
//! which fire the weapon as a player's do (`ClientEvents`' `EV_FIRE_WEAPON` and
//! `EV_ALT_FIRE`, `g_active.c:1013-1025` → `FireWeapon`, which is the host's:
//! [`NpcHost::fire_weapon`]).
//!
//! What calls `WeaponThink` is a behaviour's: `NPC_BSDefault` with `SCF_FIRE_WEAPON` (a
//! script's), and the class AI of the NPC plan's later steps (`NPC_BSST_Attack` and the
//! rest), which choose when to fire.
//!
//! Held to `tools/game-oracle/npccombat.c` (`game-npccombat.txt`).

use crate::npc_spawn::{NpcActor, NpcHost};
use crate::npc_world::NpcWorld;
use sjk_protocol::UserCommand;

/// `weapon_t`s the timing names.
const WP_NONE: u8 = 0;
const WP_STUN_BATON: u8 = 1;
const WP_SABER: u8 = 3;
const WP_BRYAR_PISTOL: u8 = 4;
const WP_BLASTER: u8 = 5;
const WP_DISRUPTOR: u8 = 6;
const WP_BOWCASTER: u8 = 7;
const WP_REPEATER: u8 = 8;
const WP_DEMP2: u8 = 9;
const WP_FLECHETTE: u8 = 10;
const WP_ROCKET_LAUNCHER: u8 = 11;
const WP_THERMAL: u8 = 12;
const WP_EMPLACED_GUN: u8 = 17;
/// `NPCAI_BURST_WEAPON`.
const NPCAI_BURST_WEAPON: u32 = 0x2;
/// `SCF_ALT_FIRE`.
const SCF_ALT_FIRE: u32 = 0x40;
/// `CLASS_REELO`.
const CLASS_REELO: i32 = 38;
/// `BUTTON_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
/// `weaponstate`s: `WEAPON_READY`, `WEAPON_RAISING`, `WEAPON_DROPPING`, `WEAPON_FIRING`,
/// `WEAPON_IDLE`.
const WEAPON_READY: u32 = 0;
const WEAPON_RAISING: u32 = 1;
const WEAPON_DROPPING: u32 = 2;
const WEAPON_FIRING: u32 = 3;
const WEAPON_IDLE: u32 = 6;
/// `ps.weaponTime`, `ps.weaponstate`.
const PS_WEAPON_TIME: usize = 10;
const PS_WEAPON_STATE: usize = 33;
/// `AMMO_MAX`, and `ammoData[].max` (`bg_weapons.c:378-430`).
pub const AMMO_MAXIMA: [i32; 10] = [0, 100, 300, 300, 300, 25, 800, 10, 10, 10];
use crate::weapon_fire::{EV_ALT_FIRE, EV_FIRE_WEAPON};

/// A value by `g_npcspskill`: easy, medium, hard (anything above medium is hard).
fn by_skill(skill: i32, easy: i32, medium: i32, hard: i32) -> i32 {
    match skill {
        0 => easy,
        1 => medium,
        _ => hard,
    }
}

/// `ChangeWeapon(ent, weapon)` (`NPC_combat.c:594-858`): the weapon in hand, the shot and
/// burst reset, and the weapon's timing — whether it fires in bursts, how many shots a
/// burst has and the pause after one, by weapon, alternate fire and `g_npcspskill`. An
/// emplaced gun's own chair is never set for an NPC here (`ent->parent`).
pub fn change_weapon(npc: &mut NpcActor, weapon: u8, skill: i32) {
    npc.player
        .set_raw_field(crate::npc_begin::ps::WEAPON, u32::from(weapon));
    npc.mind.command.weapon = weapon;
    npc.mind.attack_hold = 0;
    let alternate = npc.script_flags & SCF_ALT_FIRE != 0;
    let fight = &mut npc.mind.fight;
    fight.shot_time = 0;
    fight.burst_count = 0;
    fight.current_ammo = ammo_in_hand(&npc.player, weapon);
    // (a burst's shots: least, mean, most; or none), and the spacing where it is set.
    let (burst, spacing): (Option<(i32, i32, i32)>, Option<i32>) = match weapon {
        WP_BRYAR_PISTOL | WP_DEMP2 | WP_STUN_BATON => (None, Some(1_000)),
        WP_SABER => (None, Some(0)),
        // With the alternate fire, a spacing only for the three skills.
        WP_DISRUPTOR if alternate => (
            None,
            (0..=2)
                .contains(&skill)
                .then(|| by_skill(skill, 2_500, 2_000, 1_500)),
        ),
        WP_DISRUPTOR => (None, Some(1_000)),
        WP_BOWCASTER => (None, Some(by_skill(skill, 1_000, 750, 500))),
        WP_REPEATER if alternate => (None, Some(2_000)),
        WP_REPEATER => (Some((3, 6, 10)), Some(by_skill(skill, 1_500, 1_000, 500))),
        WP_FLECHETTE => (None, Some(if alternate { 2_000 } else { 1_000 })),
        WP_ROCKET_LAUNCHER => (None, Some(by_skill(skill, 2_500, 2_000, 1_500))),
        WP_THERMAL => (None, Some(by_skill(skill, 3_000, 2_500, 2_000))),
        WP_BLASTER if alternate => (Some((3, 3, 3)), Some(by_skill(skill, 1_500, 1_000, 500))),
        WP_BLASTER => (None, Some(by_skill(skill, 1_000, 750, 500))),
        WP_EMPLACED_GUN if npc.definition.client_class == CLASS_REELO => (None, Some(1_000)),
        // "3 shots, really"; two on easy.
        WP_EMPLACED_GUN if skill == 0 => (Some((1, 2, 1)), Some(1_200)),
        WP_EMPLACED_GUN => (Some((2, 2, 2)), Some(by_skill(skill, 1_200, 1_000, 800))),
        _ => (None, None),
    };
    match burst {
        Some((least, mean, most)) => {
            npc.ai_flags |= NPCAI_BURST_WEAPON;
            (fight.burst_min, fight.burst_mean, fight.burst_max) = (least, mean, most);
        }
        None => npc.ai_flags &= !NPCAI_BURST_WEAPON,
    }
    if let Some(spacing) = spacing {
        fight.burst_spacing = spacing;
    }
}

/// `client->ps.ammo[weaponData[weapon].ammoIndex]`.
fn ammo_in_hand(player: &sjk_protocol::PlayerState, weapon: u8) -> i32 {
    crate::weapon_data::legacy_weapon_data(weapon)
        .map_or(0, |data| player.ammo[data.ammo_index] as i32)
}

/// `NPC_AttackDebounceForWeapon` (`NPC_combat.c:1258-1297`): how long the weapon is held up
/// after a shot — nothing for a saber, else the burst's spacing.
fn attack_debounce(npc: &NpcActor) -> i32 {
    if npc.player.weapon() == WP_SABER {
        0
    } else {
        npc.mind.fight.burst_spacing
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `WeaponThink(qtrue)` (`NPC_combat.c:1052-1076`) into the think's `command`: nothing
    /// while the weapon is being raised or put away; else the rounds topped up when low
    /// (nobody runs out, `Add_Ammo`), the weapon asked for, and `ShootThink`.
    pub fn weapon_think(&mut self, me: usize, command: &mut UserCommand) {
        let npc = &mut self.actors[me];
        let weapon = npc.player.weapon();
        let state = npc.player.raw_field(PS_WEAPON_STATE).unwrap_or(0);
        if state == WEAPON_RAISING || state == WEAPON_DROPPING {
            command.weapon = weapon;
            command.buttons &= !BUTTON_ATTACK;
            return;
        }
        if ammo_in_hand(&npc.player, weapon) < 10 {
            add_ammo(&mut npc.player, weapon, 100);
        }
        command.weapon = weapon;
        self.shoot_think(me, command);
    }

    /// `ShootThink` (`NPC_combat.c:941-1045`): the attack pressed when the weapon is ready
    /// and the shot's time has come; the fire delay; the next shot's time — at once within
    /// a burst, after the burst's spacing at its end — and how long the weapon is held up.
    fn shoot_think(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        command.buttons &= !BUTTON_ATTACK;
        let npc = &mut self.actors[me];
        let weapon = npc.player.weapon();
        if weapon == WP_NONE {
            return;
        }
        let state = npc.player.raw_field(PS_WEAPON_STATE).unwrap_or(0);
        if state != WEAPON_READY && state != WEAPON_FIRING && state != WEAPON_IDLE {
            return;
        }
        if level_time < npc.mind.fight.shot_time {
            return;
        }
        command.buttons |= BUTTON_ATTACK;
        npc.mind.fight.current_ammo = ammo_in_hand(&npc.player, weapon);
        apply_fire_delay(npc, level_time);
        let fight = &mut npc.mind.fight;
        let delay = if npc.ai_flags & NPCAI_BURST_WEAPON != 0 {
            let delay = if fight.burst_count == 0 {
                fight.burst_count = self.host.irand(fight.burst_min, fight.burst_max);
                0
            } else {
                fight.burst_count -= 1;
                if fight.burst_count == 0 {
                    fight.burst_spacing
                } else {
                    0
                }
            };
            // An emplaced gun's own chair's timing (`ent->parent`) is never set here.
            if delay == 0 && weapon == WP_EMPLACED_GUN {
                by_skill(self.host.skill(), 350, 300, 200)
            } else {
                delay
            }
        } else {
            fight.burst_spacing
        };
        let npc = &mut self.actors[me];
        npc.mind.fight.shot_time = level_time + delay;
        npc.mind.attack_debounce_time = level_time + attack_debounce(npc);
    }

    /// `ClientEvents`' fire events (`g_active.c:929-1025`) for the NPC at `me`: each of
    /// the move's `EV_FIRE_WEAPON` and `EV_ALT_FIRE` fires the weapon (`FireWeapon`, the
    /// host's) from the entity as the move converted it — of the move's last two events
    /// only (`MAX_PS_EVENTS`: an earlier one went out in a temp entity of its own, unfired).
    pub(crate) fn fire_events(&mut self, me: usize) {
        const MAX_PS_EVENTS: usize = 2;
        let raised = self.actors[me].movement.command_events().count();
        // A move raises a fire event at most for each of its slices; no allocation.
        let mut fires = [None; 16];
        for (slot, alternate) in fires.iter_mut().zip(
            self.actors[me]
                .movement
                .command_events()
                .skip(raised.saturating_sub(MAX_PS_EVENTS))
                .filter(|event| matches!(event.event, EV_FIRE_WEAPON | EV_ALT_FIRE))
                .map(|event| event.event == EV_ALT_FIRE),
        ) {
            *slot = Some(alternate);
        }
        for alternate in fires.into_iter().flatten() {
            let npc = &mut self.actors[me];
            self.host
                .fire_weapon(npc.number, &mut npc.player, &npc.state, alternate);
        }
    }
}

/// `NPC_ApplyWeaponFireDelay` (`NPC_combat.c:894-935`): unless it just fired (a burst's
/// next shot), the weapon's wind-up — a thermal's throw 700 ms, a baton's swing 300 ms,
/// anything else none.
fn apply_fire_delay(npc: &mut NpcActor, level_time: i32) {
    if npc.mind.attack_debounce_time > level_time {
        return;
    }
    let time = match npc.player.weapon() {
        // "NPCs delay...": every NPC's `clientNum` is its own number, never 0.
        WP_THERMAL => 700,
        WP_STUN_BATON => 300,
        _ => 0,
    };
    npc.player.set_raw_field(PS_WEAPON_TIME, time);
}

/// `Add_Ammo(NPC, ps.weapon, count)` (`g_items.c:2146-2163`) as `WeaponThink` calls it: with
/// the weapon's number where an ammunition kind belongs, so the rounds go to the slot of
/// the kind numbered as the weapon is, up to that kind's maximum. The reference reads past
/// its table of maxima for weapons numbered from `AMMO_MAX` on; nothing is added for those.
fn add_ammo(player: &mut sjk_protocol::PlayerState, weapon: u8, count: i32) {
    let kind = usize::from(weapon);
    let Some(&maximum) = AMMO_MAXIMA.get(kind) else {
        return;
    };
    let held = player.ammo[kind] as i32;
    if held < maximum {
        player.ammo[kind] = (held + count).min(maximum) as u32;
    }
}
