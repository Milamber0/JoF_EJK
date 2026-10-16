//! `G_TouchTriggers` for an NPC (`g_active.c:531-603`, which `ClientThink_real` runs for
//! every client not in noclip, NPCs too): what the map's triggers read of it and do to it.
//! The triggers themselves are the host's (a server keeps the map's brushes); the rules
//! are [`crate::triggers`]' and [`crate::mover_team`]'s, with the NPC as the toucher:
//! `Touch_Multi` answers an NPC by its spawnflags and `NPC_targetname`
//! ([`crate::triggers::Activator`]), `hurt_touch` kills an NPC outright where a pit's brush
//! would begin a player's fall, `TeleportPlayer` snaps an NPC's entity as a player's, and
//! `Touch_DoorTrigger` opens no door for a vehicle.

use crate::npc_spawn::{NpcActor, es};
use crate::triggers::{Teleported, Toucher};

/// `STAT_HEALTH`; `ps.weaponTime`, `ps.forceHandExtend`, `ps.torsoAnim` (wire fields).
const STAT_HEALTH: usize = 0;
const PS_WEAPON_TIME: usize = 10;
const PS_HAND_EXTEND: usize = 80;
const PS_TORSO_ANIM: usize = 15;
/// `CLASS_VEHICLE`.
const CLASS_VEHICLE: i32 = 53;
/// `ET_NPC`.
const ET_NPC: u32 = 13;

/// NPC `npc` as `Touch_Multi` reads a client: its view, its command's buttons, its health,
/// what its hands are doing. An NPC is never a spectator.
pub fn toucher(npc: &NpcActor) -> Toucher {
    Toucher {
        view_angles: npc.player.view_angles(),
        buttons: npc.mind.command.buttons,
        health: npc.player.stats[STAT_HEALTH] as i32,
        spectating: false,
        weapon_time: npc.player.raw_field(PS_WEAPON_TIME).unwrap_or(0) as i32,
        hand_extend: npc.player.raw_field(PS_HAND_EXTEND).unwrap_or(0) as u8,
        torso_anim: npc.player.raw_field(PS_TORSO_ANIM).unwrap_or(0) as u16,
        team: npc.session_team,
    }
}

/// Whether `G_TouchTriggers` runs for NPC `npc` at all: a living client
/// (`ps.stats[STAT_HEALTH] > 0`), not in noclip (`g_active.c:3376`).
pub fn touches_triggers(npc: &NpcActor) -> bool {
    npc.player.stats[STAT_HEALTH] as i32 > 0 && !npc.mind.noclip
}

/// `Touch_DoorTrigger`'s refusal (`g_mover.c:1127-1140`): doors do not open for a vehicle
/// (`s.NPC_class`).
pub fn opens_doors(npc: &NpcActor) -> bool {
    npc.definition.entity_class != CLASS_VEHICLE
}

/// `TeleportPlayer` (`g_misc.c`) on NPC `npc`: its player state sent to `destination`
/// facing `angles` ([`crate::triggers::teleport_player`], [`crate::triggers::face`] from
/// its own command's angles), then its entity snapped from the player state
/// (`BG_PlayerStateToEntityState(..., qtrue)`, `s.eType` kept `ET_NPC`) and linked where
/// it now is. What it answers says whether the events flash and the box kills.
pub fn teleport(npc: &mut NpcActor, destination: [f32; 3], angles: [f32; 3]) -> Teleported {
    let teleported = crate::triggers::teleport_player(&mut npc.player, destination, angles, false);
    crate::triggers::face(&mut npc.player, teleported.angles, npc.mind.command.angles);
    crate::player_entity::player_entity_state(
        &npc.player,
        &mut npc.mind.shown_events,
        crate::player_entity::PlayerEntityMotion::Interpolated,
        true,
        &mut npc.state,
    );
    npc.state.set_raw_field(es::TYPE, ET_NPC);
    npc.current_origin = npc.player.origin();
    npc.relink();
    teleported
}

/// `Touch_Multi`'s pose for a USE_BUTTON trigger on NPC `npc` (`g_trigger.c:547-559`), its
/// torso doing `torso_anim` as it touched: `BOTH_BUTTON_HOLD` taken, or the hold made
/// longer; its weapon waits as long.
pub fn use_pose(npc: &mut NpcActor, torso_anim: u16) {
    use crate::pmove_anim::{SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE, SETANIM_TORSO};
    npc.movement = npc.movement.reseeded(&npc.player);
    match crate::triggers::using_pose(torso_anim) {
        Some(pose) => npc.movement.set_animation_parts(
            SETANIM_TORSO,
            pose,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        ),
        None => npc.movement.set_torso_timer(crate::triggers::USING_AGAIN),
    }
    let timer = npc.movement.state().torso_timer;
    npc.movement.set_weapon_time(timer);
    npc.movement.write_player_state(&mut npc.player);
}
