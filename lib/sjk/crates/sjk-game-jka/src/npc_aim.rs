//! Where an NPC looks and whom it can shoot (`codemp/game/NPC_utils.c`, `NPC_combat.c`,
//! `NPC_reactions.c`): turning to face a place or a body (`NPC_FacePosition`,
//! `NPC_FaceEntity`, `NPC_FaceEnemy`), what its shot would hit (`NPC_ShotEntity`, from the
//! muzzle to the target's chest), its aim getting better or worse (`NPC_AimAdjust`), a
//! glance at someone (`NPC_TempLookTarget`), and the enemy the class AI keeps or finds
//! (`NPC_CheckEnemyExt`: `NPC_FindEnemy`, `NPC_PickEnemyExt`, `NPC_FindNearestEnemy`,
//! `NPC_TargetVisible`).
//!
//! `NPC_FindNearestEnemy` looks at the bodies in a box round the NPC — the players and the
//! NPCs. Anything else with health the reference would also take (a breakable, a mine) is
//! not offered by the host yet.
//!
//! Held to `tools/game-oracle/npcst.c` (`game-npcst.txt`).

use crate::npc_senses::{Body, Spot, distance_squared, in_fov, spot};
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::player_angle_math::{angle_mod, vector_angles};
use sjk_protocol::UserCommand;

/// `VALID_ATTACK_CONE` (`NPC_utils.c:33`).
const VALID_ATTACK_CONE: f64 = 2.0;
/// `CLASS_ATST`, `CLASS_GALAKMECH`, `CLASS_RANCOR`, `CLASS_WAMPA`.
const CLASS_ATST: i32 = 1;
const CLASS_GALAKMECH: i32 = 25;
const CLASS_RANCOR: i32 = 54;
const CLASS_WAMPA: i32 = 55;
/// `WP_BLASTER`, `WP_THERMAL`.
const WP_BLASTER: i32 = 5;
const WP_THERMAL: i32 = 12;
/// `MASK_SHOT`.
pub(crate) const MASK_SHOT: u32 = 0x1 | 0x100 | 0x200 | 0x1000;
/// `EF2_HELD_BY_MONSTER`; `ps.eFlags2`.
const EF2_HELD_BY_MONSTER: u32 = 1;
const PS_EFLAGS2: usize = 103;

/// `SHORT2ANGLE`: a double.
fn short_to_angle(value: i32) -> f64 {
    f64::from(value) * (360.0 / 65_536.0)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `CalcEntitySpot(NPC, SPOT_WEAPON)` (`NPC_utils.c:147-157`, `CalcMuzzlePoint`): the
    /// muzzle along its view, or along its shot's own angles where they are set and differ
    /// from it (`shootAngles`, [`crate::npc_states::StatesMind::shoot_angles`]).
    pub fn weapon_spot(&self, me: usize) -> [f32; 3] {
        let npc = &self.actors[me];
        let shoot = npc.mind.states.shoot_angles;
        if shoot != [0.0; 3] && shoot != npc.player.view_angles() {
            let mut aimed = npc.player.clone();
            aimed.set_view_angles(shoot);
            return crate::weapon_fire::muzzle_point(&aimed, &npc.state).0;
        }
        crate::weapon_fire::muzzle_point(&npc.player, &npc.state).0
    }

    /// `NPC_FacePosition` (`NPC_utils.c:1506-1564`): the desired angles toward `position`
    /// from the NPC's eyes (a monster's upper body, the mech's gun), turned to at once
    /// (`NPC_UpdateAngles`). Whether it now faces it within two degrees.
    pub fn face_position(
        &mut self,
        me: usize,
        position: [f32; 3],
        do_pitch: bool,
        command: &mut UserCommand,
    ) -> bool {
        let body = self.npc(me);
        let muzzle = match body.class {
            CLASS_RANCOR | CLASS_WAMPA => {
                let mut muzzle = spot(&body, Spot::Origin);
                muzzle[2] += body.maxs[2] * 0.75;
                muzzle
            }
            CLASS_GALAKMECH => self.weapon_spot(me),
            _ => spot(&body, Spot::HeadLean),
        };
        let angles = vector_angles(crate::npc_senses::subtract(position, muzzle));
        let enemy_class = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            .map(|enemy| enemy.class);
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        npc.desired_yaw = angle_mod(angles[1]);
        npc.mind.desired_pitch = angle_mod(angles[0]);
        if enemy_class == Some(CLASS_ATST) {
            let wobble = self.host.rng().flrand(-5.0, 5.0);
            let sway = f64::from(level_time as f32 * 0.004).sin() * 7.0;
            let npc = &mut self.actors[me];
            npc.desired_yaw = (f64::from(npc.desired_yaw) + (f64::from(wobble) + sway)) as f32;
            let pitch = self.host.rng().flrand(-2.0, 2.0);
            self.actors[me].mind.desired_pitch += pitch;
        }
        self.update_angles(me, true, true, command);
        let npc = &self.actors[me];
        let delta = npc.player.delta_angles();
        let yaw_now = short_to_angle(command.angles[1] + delta[1]);
        let yaw_delta = angle_mod((f64::from(npc.desired_yaw) - yaw_now) as f32);
        let mut facing = f64::from(yaw_delta).abs() <= VALID_ATTACK_CONE;
        if do_pitch {
            let pitch_now = short_to_angle(command.angles[0] + delta[0]) as f32;
            if f64::from(npc.mind.desired_pitch - pitch_now).abs() > VALID_ATTACK_CONE {
                facing = false;
            }
        }
        facing
    }

    /// `NPC_FaceEnemy` (`NPC_utils.c:1586-1597`): its enemy's leaning head faced.
    pub fn face_enemy(&mut self, me: usize, do_pitch: bool, command: &mut UserCommand) -> bool {
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
        else {
            return false;
        };
        self.face_position(me, spot(&enemy, Spot::HeadLean), do_pitch, command)
    }

    /// `NPC_ShotEntity` (`NPC_combat.c:2118-2172`): what a shot from the NPC's muzzle (a
    /// thermal's from above its head) at `target`'s chest would hit — a blaster's bolt a
    /// four-unit box — and where.
    pub fn shot_entity(&mut self, me: usize, target: &Body) -> (u16, [f32; 3]) {
        let body = self.npc(me);
        let number = body.number;
        let muzzle = if body.weapon == WP_THERMAL {
            let head = spot(&body, Spot::Head);
            let (forward, _) = crate::pmove::flight::flight_axes([0.0, body.view_angles[1], 0.0]);
            let forward = forward.to_array();
            let mut end: [f32; 3] = std::array::from_fn(|axis| head[axis] + 8.0 * forward[axis]);
            end[2] += 24.0;
            self.trace_bodies(head, [0.0; 3], [0.0; 3], end, number, MASK_SHOT)
                .end_position
        } else {
            self.weapon_spot(me)
        };
        let chest = spot(target, Spot::Chest);
        let (mins, maxs) = if body.weapon == WP_BLASTER {
            ([-2.0; 3], [2.0; 3])
        } else {
            ([0.0; 3], [0.0; 3])
        };
        let trace = self.trace_bodies(muzzle, mins, maxs, chest, number, MASK_SHOT);
        (trace.entity_number, trace.end_position)
    }

    /// `NPC_AimAdjust` (`NPC_combat.c:3058-3089`): the aim bettered or worsened by `change`
    /// once its debounce runs out (between the stats' best and -30), and the debounce set
    /// again (the first time only set).
    pub fn aim_adjust(&mut self, me: usize, change: i32) {
        let level_time = self.level_time;
        let debounce = 500 + (3 - self.host.skill()) * 100;
        if !self.actors[me].mind.timers.exists("aimDebounce") {
            let time = self.host.irand(debounce, debounce + 1_000);
            self.actors[me]
                .mind
                .timers
                .set("aimDebounce", level_time, time);
            return;
        }
        if !self.actors[me].mind.timers.done("aimDebounce", level_time) {
            return;
        }
        let npc = &mut self.actors[me];
        npc.mind.current_aim += change;
        let best = npc.definition.stats.aim;
        if npc.mind.current_aim > best {
            npc.mind.current_aim = best;
        } else if npc.mind.current_aim < -30 {
            npc.mind.current_aim = -30;
        }
        let time = self.host.irand(debounce, debounce + 1_000);
        self.actors[me]
            .mind
            .timers
            .set("aimDebounce", level_time, time);
    }

    /// `NPC_TempLookTarget` (`NPC_reactions.c:677-705`): a glance at `number` for a while,
    /// unless the NPC is looking at someone else already.
    pub fn temp_look_target(&mut self, me: usize, number: u16, least: i32, most: i32) {
        if self.actors[me].player.raw_field(PS_EFLAGS2).unwrap_or(0) & EF2_HELD_BY_MONSTER != 0 {
            return;
        }
        let (least, most) = (
            if least == 0 { 1_000 } else { least },
            if most == 0 { 1_000 } else { most },
        );
        if !self.check_look_target(me) {
            let time = self.level_time + self.host.irand(least, most);
            crate::npc_enemy::set_look(&mut self.actors[me], number, time);
        }
    }

    /// `NPC_ClearLOS4` (`NPC_senses.c:811-819`): a clear line from the NPC's leaning head
    /// to the body's origin or head.
    pub fn clear_los4(&mut self, me: usize, target: &Body) -> bool {
        let eyes = spot(&self.npc(me), Spot::HeadLean);
        self.clear_los_to(eyes, target)
    }

    /// `NPC_CheckEnemyExt(qfalse)` (`NPC_utils.c:1484-1500`, `NPC_FindEnemy`): the enemy
    /// kept while it is still one, else the nearest valid one it sees taken. Whether the
    /// NPC has one.
    pub fn check_enemy_ext(&mut self, me: usize) -> bool {
        if self.actors[me].mind.confusion_time > self.level_time {
            return false;
        }
        if let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|enemy| self.body(enemy))
            && self.valid_for(me, &enemy)
        {
            return true;
        }
        let Some(enemy) = self.nearest_enemy(me) else {
            return false;
        };
        self.set_enemy(me, enemy);
        true
    }

    /// `NPC_FindNearestEnemy` (`NPC_utils.c:1283-1330`): the nearest valid enemy in a box
    /// of its sight's reach round it that is in range, in view and in sight.
    fn nearest_enemy(&mut self, me: usize) -> Option<u16> {
        let origin = self.actors[me].current_origin;
        let sight = self.sight(me);
        let reach = sight.visrange;
        let me_body = self.npc(me);
        let mut best: Option<(u16, f32)> = None;
        let players = self.host.players().len();
        for index in 0..players + self.order.len() {
            let body = if index < players {
                self.host.players()[index]
            } else {
                self.npc(self.order[index - players])
            };
            let inside = (0..3).all(|axis| {
                body.origin[axis] + body.mins[axis] - 1.0 <= origin[axis] + reach
                    && body.origin[axis] + body.maxs[axis] + 1.0 >= origin[axis] - reach
            });
            if !inside || body.number == me_body.number || !self.valid_for(me, &body) {
                continue;
            }
            // `NPC_TargetVisible`.
            if distance_squared(body.origin, origin) > sight.visrange * sight.visrange
                || !in_fov(&body, &me_body, sight.hfov, sight.vfov)
                || !self.clear_los4(me, &body)
            {
                continue;
            }
            let distance = distance_squared(origin, body.origin);
            if best.is_none_or(|(_, nearest)| distance < nearest) {
                best = Some((body.number, distance));
            }
        }
        best.map(|(number, _)| number)
    }
}
