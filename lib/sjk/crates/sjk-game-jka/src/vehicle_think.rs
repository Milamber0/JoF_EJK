//! A vehicle NPC's think and move: `NPC_Think`'s vehicle branch (`codemp/game/NPC.c:
//! 1839-1856`: no behaviour of its own, its stored command cleared and handed to the
//! vehicle; nothing at all while someone rides it) and `ClientThink_real`'s
//! (`g_active.c:2372-2380`, `3012-3043`: the vehicle's gravity, the pilot's command it moves
//! by, and `Pmove` with the vehicle and the game's vehicle functions,
//! [`crate::vehicle_update::VehicleThink`]).

use crate::npc_spawn::{NpcActor, NpcHost, es};
use crate::pmove::MoveContext;
use crate::pmove::vehicle_impact::{ImpactBody, ImpactClass};
use crate::vehicle_rider::Rider;
use crate::vehicle_update::{VehicleRequest, VehicleThink};
use sjk_protocol::UserCommand;

/// `BUTTON_ATTACK`, `BUTTON_ALT_ATTACK`.
const BUTTON_ATTACK: u16 = 1;
const BUTTON_ALT_ATTACK: u16 = 128;

/// `NPC_Think`'s vehicle branch, after its next think is set: `true` where the think
/// ends here — a vehicle with someone aboard does not think on its own.
pub fn before_think(npc: &mut NpcActor) -> bool {
    let Some(vehicle) = npc.vehicle.as_deref_mut() else {
        return false;
    };
    if npc.player.vehicle_entity_num() != 0 {
        return true;
    }
    npc.mind.move_dir = [0.0; 3];
    let command = &mut npc.mind.command;
    command.forward_move = 0;
    command.right_move = 0;
    command.up_move = 0;
    command.buttons = 0;
    vehicle.ucmd = *command;
    false
}

/// `ClientThink_real`'s gravity for a vehicle (`g_active.c:2372-2396`), where the NPC does
/// not keep a gravity of its own: its definition's, or 1 in space (`trigger_space`);
/// `None` for the level's.
pub fn gravity(npc: &NpcActor) -> Option<i32> {
    let vehicle = npc.vehicle.as_deref()?;
    if vehicle.info.gravity != 0 {
        Some(vehicle.info.gravity)
    } else {
        vehicle.in_space().then_some(1)
    }
}

/// `Pmove` for a vehicle NPC: its vehicle and `moveDir` in the move, the game's vehicle
/// functions called where `PmoveSingle` calls them, and what they changed back on the
/// NPC — its entity's angles, its stored command, and what the move could not do in its
/// middle. A piloted vehicle moves by its own command (`m_ucmd`, `g_active.c:3012-3033`):
/// no strafing, crouching or jumping, the pilot's attack buttons.
#[allow(clippy::too_many_arguments)]
pub fn move_vehicle<'r>(
    npc: &mut NpcActor,
    host: &mut impl NpcHost,
    command: UserCommand,
    context: &MoveContext,
    bodies: &[crate::entity_clip::BoxObstacle],
    level_time: i32,
    rider: Option<&mut Rider<'r>>,
    passengers: &mut [Rider<'r>],
    impact_bodies: &[ImpactBody],
) -> Vec<VehicleRequest> {
    let mut command = command;
    let NpcActor {
        movement,
        player,
        mind,
        vehicle,
        health,
        spawnflags,
        number,
        state,
        current_origin,
        mins,
        maxs,
        clip_mask,
        contents,
        ..
    } = npc;
    let Some(own) = vehicle.as_deref_mut() else {
        return Vec::new();
    };
    let pilot = own.pilot;
    let in_space = own.in_space();
    let rider_origin = rider
        .as_deref()
        .filter(|rider| Some(rider.number) == pilot)
        .map(|rider| rider.movement.origin);
    if pilot.is_some() {
        if level_time - own.ucmd.server_time > 2_000 {
            // "Previous owner disconnected, maybe".
            own.ucmd.server_time = level_time;
            movement.state_mut().command_time = level_time - 100;
        }
        let pilot_buttons = rider
            .as_deref()
            .filter(|rider| Some(rider.number) == pilot)
            .map_or(0, |rider| rider.command.buttons);
        command = own.ucmd;
        command.right_move = 0;
        command.up_move = 0;
        command.buttons = pilot_buttons & (BUTTON_ATTACK | BUTTON_ALT_ATTACK);
    }
    // "ATST crushes anything underneath it", where it stood after its last move (the
    // world, which has no health, never).
    const ENTITYNUM_WORLD: u16 = 1_022;
    let under = player.ground_entity_num();
    let crush = (own.kind() == crate::vehicle_fields::kind::WALKER && under < ENTITYNUM_WORLD)
        .then_some(VehicleRequest::Crush { under });
    // The pilot and the passengers the vehicle owns, which its move goes through.
    let mut owned =
        [crate::vehicle_rider::ENTITYNUM_NONE; 1 + crate::vehicle_fields::MAX_PASSENGERS as usize];
    let mut count = 0;
    for number in pilot
        .into_iter()
        .chain(own.passengers.iter().flatten().copied())
    {
        if count < owned.len() {
            owned[count] = number;
            count += 1;
        }
    }
    let riders: &[u16] = &owned[..count];
    movement.set_vehicle(vehicle.take());
    movement.set_move_dir(mind.move_dir);
    let lengths = movement.shared_animation_lengths();
    let driver_offset = host.vehicle_driver_offset(*number);
    let mut rng = *host.rng();
    let mut think = VehicleThink {
        level_time,
        rng: &mut rng,
        number: *number,
        player,
        entity: state,
        parent_command: &mut mind.command,
        parent_health: *health,
        parent_spawnflags: *spawnflags,
        parent_yaw: mind.current_angles[1],
        parent_origin: *current_origin,
        parent_mins: *mins,
        parent_maxs: *maxs,
        parent_clip_mask: *clip_mask,
        parent_contents: *contents,
        lengths,
        rider,
        passengers,
        last_pilot_origin: None,
        driver_offset,
        entity_angles: None,
        gravity: host.gravity(),
        in_space,
        fighter_alt_control: host.fighter_alt_control(),
        impact_bodies,
        rider_origin,
        requests: crush.into_iter().collect(),
    };
    host.move_vehicle(
        movement, command, context, *number, bodies, riders, &mut think,
    );
    let VehicleThink {
        entity_angles,
        requests,
        ..
    } = think;
    *host.rng() = rng;
    *vehicle = movement.take_vehicle();
    mind.move_dir = movement.move_dir();
    if let Some(angles) = entity_angles {
        for axis in 0..3 {
            state.set_raw_field(es::ANGLES[axis], angles[axis].to_bits());
        }
    }
    requests
}

/// What a vehicle's move owns that the move's write-back does not carry: a speeder's or a
/// fighter's turbo (`EF_JETPACK_ACTIVE`, `SpeederNPC.c:190-201`, `FighterNPC.c:430-447`) and
/// a fighter's strafe (`hackingTime`).
pub fn write_flags(npc: &mut NpcActor) {
    const PS_EFLAGS: usize = 17;
    const PS_HACKING_TIME: usize = 91;
    if npc.vehicle.is_none() {
        return;
    }
    npc.player
        .set_raw_field(PS_HACKING_TIME, npc.movement.state().hacking_time as u32);
    let bit = crate::vehicle_move::EF_JETPACK_ACTIVE;
    let flags = npc.player.raw_field(PS_EFLAGS).unwrap_or(0);
    let flags = if npc.movement.state().entity_flags & bit != 0 {
        flags | bit
    } else {
        flags & !bit
    };
    npc.player.set_raw_field(PS_EFLAGS, flags);
}

/// What the move of the vehicle at `me` may bump into (`PM_VehicleImpact` reads it): the
/// players, the other NPCs and the host's own entities, into `out` (cleared first). Only a
/// speeder's or a fighter's knock reads it; for anything else it stays empty.
pub fn gather_impact_bodies(
    actors: &[NpcActor],
    me: usize,
    host: &mut impl NpcHost,
    out: &mut Vec<ImpactBody>,
) {
    out.clear();
    let judged = actors[me].vehicle.as_deref().is_some_and(|vehicle| {
        matches!(
            vehicle.kind(),
            crate::vehicle_fields::kind::SPEEDER | crate::vehicle_fields::kind::FIGHTER
        )
    });
    if !judged {
        return;
    }
    const ENTITYNUM_NONE: u16 = 1_023;
    for player in host.players() {
        let class = ImpactClass::Player;
        out.push(ImpactBody {
            number: player.number,
            class,
            origin: player.origin,
            speed: 0.0,
            owner: ENTITYNUM_NONE,
            takes_damage: player.health > 0 && !player.spectating,
        });
    }
    for (at, npc) in actors.iter().enumerate() {
        if at == me {
            continue;
        }
        let class = match npc.vehicle.as_deref() {
            Some(vehicle) => {
                let mut state = crate::pmove::MovementState::from_player_state(&npc.player);
                state.speed = npc.player.speed();
                let fighter = crate::vehicle_fighter::FighterSteering {
                    server: true,
                    suspended: npc.spawnflags & 2 != 0,
                    ..Default::default()
                };
                let landed =
                    crate::vehicle_fighter::is_landed(vehicle, &state) || fighter.suspended;
                ImpactClass::Vehicle {
                    kind: vehicle.kind(),
                    landed_or_suspended: landed,
                    mass: vehicle.info.mass,
                    orientation: vehicle.orientation,
                    velocity: npc.player.velocity(),
                }
            }
            None => ImpactClass::Npc,
        };
        out.push(ImpactBody {
            number: npc.number,
            class,
            origin: npc.current_origin,
            speed: npc.player.speed(),
            owner: ENTITYNUM_NONE,
            takes_damage: npc.takes_damage,
        });
    }
    host.impact_bodies(out);
}

/// A vehicle its own move killed (a wreck, a crash, the wear of torn surfaces): the
/// reference's `G_Damage` ran inside that move, whose `PM_Weapon` then found it dead and took
/// its weapon (`bg_pmove.c:6676-6680`), and whose box `ClientThink_real` then linked
/// (`g_active.c:3322`) over whatever the death set. `alive` and `moved_box` are how the move
/// left it.
pub fn killed_by_its_move(npc: &mut NpcActor, alive: bool, moved_box: ([f32; 3], [f32; 3])) {
    if !alive || npc.health > 0 || npc.vehicle.is_none() {
        return;
    }
    npc.player.set_raw_field(crate::npc_begin::ps::WEAPON, 0);
    npc.movement.state_mut().weapon = 0;
    npc.movement.set_box_bounds(moved_box);
}
