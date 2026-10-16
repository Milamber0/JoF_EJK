//! How the default set's states aim and shoot: the firing angles with the aim's wobble
//! (`NPC_UpdateFiringAngles`, `NPC_AimWiggle`, `NPC_utils.c:541-752`), the shot's own
//! angles (`NPC_UpdateShootAngles`, `:761-829`), whether a shot would reach
//! (`CanShoot`, `NPC_CheckVisibility`'s `CHECK_SHOOT`, `NPC_combat.c:1095-1190`), the
//! stand-and-shoot check (`NPC_CheckCanAttack`, `NPC_CheckAttack`, `NPC_CheckDefend`,
//! `:2194-2431`); and the two states that fight standing — the advance on a capture goal
//! (`NPC_BSAdvanceFight`, `NPC_behavior.c:49-201`) and the emplaced gunner
//! (`NPC_BSEmplaced`, `:1671-1769`).

use crate::npc_aim::MASK_SHOT;
use crate::npc_senses::{
    Body, CHECK_360, CHECK_FOV, CHECK_SHOOT, Spot, Visibility, spot, subtract,
};
use crate::npc_spawn::{NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::player_angle_math::vector_angles;
use sjk_protocol::UserCommand;

/// `ps.weaponTime`.
pub(crate) const PS_WEAPON_TIME: usize = 10;
/// `SCF_DONT_FIRE`, `SCF_FIRE_WEAPON`.
const SCF_DONT_FIRE: u32 = 0x100;
const SCF_FIRE_WEAPON: u32 = 0x4_0000;
/// `FL_NOTARGET`.
const FL_NOTARGET: u32 = 0x20;
/// `WP_SABER`.
const WP_SABER: i32 = 3;

/// `AngleVectors`' forward.
pub(crate) fn forward(angles: [f32; 3]) -> [f32; 3] {
    crate::pmove::flight::flight_axes(angles).0.to_array()
}

/// `VectorMA(start, scale, direction)`.
pub(crate) fn along(start: [f32; 3], scale: f32, direction: [f32; 3]) -> [f32; 3] {
    std::array::from_fn(|axis| start[axis] + scale * direction[axis])
}

/// `VectorLength`.
pub(crate) fn length(vector: [f32; 3]) -> f32 {
    crate::npc_states_search::length_squared(vector).sqrt()
}

/// `VectorNormalize`, returning the length.
fn normalize(vector: &mut [f32; 3]) -> f32 {
    crate::saber_clash::normalize(vector)
}

/// The decay of a turn toward `target` from `from` by `decay` a think
/// (`NPC_UpdateFiringAngles`, `NPC_UpdateShootAngles`): the error left.
fn decayed(from: f32, target: f32, decay: f32) -> f32 {
    let error = crate::npc_senses::angle_delta(from, target);
    if error == 0.0 {
        return error;
    }
    if error < 0.0 {
        let error = error + decay;
        if error > 0.0 { 0.0 } else { error }
    } else {
        let error = error - decay;
        if error < 0.0 { 0.0 } else { error }
    }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_AimWiggle` (`NPC_utils.c:541-555`): `point` moved by the aim's offset, drawn
    /// afresh within the enemy's box (between its head and its middle) whenever the aim's
    /// debounce has run out.
    pub(crate) fn aim_wiggle(&mut self, me: usize, enemy: &Body, point: [f32; 3]) -> [f32; 3] {
        if self.actors[me].mind.states.aim_error_debounce_time < self.level_time {
            let x = self.host.rng().flrand(enemy.mins[0], enemy.maxs[0]);
            let y = self.host.rng().flrand(enemy.mins[1], enemy.maxs[1]);
            let states = &mut self.actors[me].mind.states;
            states.aim_offset[0] = (0.3 * f64::from(x)) as f32;
            states.aim_offset[1] = (0.3 * f64::from(y)) as f32;
            if enemy.maxs[2] > 0.0 {
                let z = self.host.rng().flrand(0.0, -1.0);
                self.actors[me].mind.states.aim_offset[2] = enemy.maxs[2] * z;
            }
        }
        let offset = self.actors[me].mind.states.aim_offset;
        std::array::from_fn(|axis| point[axis] + offset[axis])
    }

    /// `NPC_UpdateFiringAngles` (`NPC_utils.c:562-752`): the command turned toward the
    /// desired angles by seven degrees a think, plus the aim's error — drawn afresh
    /// (each half the time) every quarter to two seconds. Whether it faces them exactly.
    pub(crate) fn update_firing_angles(
        &mut self,
        me: usize,
        do_pitch: bool,
        do_yaw: bool,
        command: &mut UserCommand,
    ) -> bool {
        let level_time = self.level_time;
        let npc = &mut self.actors[me];
        let (mut target_pitch, mut target_yaw) = (0.0, 0.0);
        if level_time < npc.mind.aim_time {
            if do_pitch {
                target_pitch = npc.mind.locked_desired_pitch;
            }
            if do_yaw {
                target_yaw = npc.mind.locked_desired_yaw;
            }
        } else {
            if do_pitch {
                target_pitch = npc.mind.desired_pitch;
                npc.mind.locked_desired_pitch = npc.mind.desired_pitch;
            }
            if do_yaw {
                target_yaw = npc.desired_yaw;
                npc.mind.locked_desired_yaw = npc.desired_yaw;
            }
        }
        if npc.mind.states.aim_error_debounce_time < level_time {
            let aim = (6 - npc.definition.stats.aim) as f32;
            if self.host.irand(0, 1) != 0 {
                let error = self.host.rng().flrand(-1.0, 1.0);
                self.actors[me].mind.states.last_aim_error_yaw = aim * error;
            }
            if self.host.irand(0, 1) != 0 {
                let error = self.host.rng().flrand(-1.0, 1.0);
                self.actors[me].mind.states.last_aim_error_pitch = aim * error;
            }
            let debounce = self.host.irand(250, 2_000);
            self.actors[me].mind.states.aim_error_debounce_time = level_time + debounce;
        }
        let npc = &self.actors[me];
        let (view, delta) = (npc.player.view_angles(), npc.player.delta_angles());
        // `60.0 + 80.0`, times `50.0f / 1000.0f`.
        let decay = 140.0_f32 * (50.0_f32 / 1_000.0);
        let mut exact = true;
        if do_yaw {
            if crate::npc_senses::angle_delta(view[1], target_yaw) != 0.0 {
                exact = false;
            }
            let diff = decayed(view[1], target_yaw, decay);
            let error = npc.mind.states.last_aim_error_yaw;
            command.angles[1] =
                crate::npc_think::angle_to_short(target_yaw + diff + error).wrapping_sub(delta[1]);
        }
        if do_pitch {
            if crate::npc_senses::angle_delta(view[0], target_pitch) != 0.0 {
                exact = false;
            }
            let diff = decayed(view[0], target_pitch, decay);
            let error = npc.mind.states.last_aim_error_pitch;
            command.angles[0] = crate::npc_think::angle_to_short(target_pitch + diff + error)
                .wrapping_sub(delta[0]);
        }
        command.angles[2] = crate::npc_think::angle_to_short(view[2]).wrapping_sub(delta[2]);
        exact
    }

    /// `NPC_UpdateShootAngles` (`NPC_utils.c:761-829`): the shot's angles turned toward
    /// `angles` by `(60 + 80 * aim) / 10` degrees a think.
    pub(crate) fn update_shoot_angles(
        &mut self,
        me: usize,
        angles: [f32; 3],
        do_pitch: bool,
        do_yaw: bool,
    ) {
        let aim = self.actors[me].definition.stats.aim;
        let decay = ((60.0 + 80.0 * f64::from(aim)) as f32) * (100.0_f32 / 1_000.0);
        let shoot = &mut self.actors[me].mind.states.shoot_angles;
        let (target_pitch, target_yaw) = (
            if do_pitch { angles[0] } else { 0.0 },
            if do_yaw { angles[1] } else { 0.0 },
        );
        if do_yaw {
            shoot[1] = target_yaw + decayed(shoot[1], target_yaw, decay);
        }
        if do_pitch {
            shoot[0] = target_pitch + decayed(shoot[0], target_pitch, decay);
        }
    }

    /// `NPC_CheckAttack` (`NPC_combat.c:2194-2208`): aggressive enough (its aggression
    /// times `scale` against up to four) and its next shot due.
    pub(crate) fn check_attack(&mut self, me: usize, scale: f32) -> bool {
        let scale = if scale == 0.0 { 1.0 } else { scale };
        let aggression = self.actors[me].definition.stats.aggression as f32;
        if aggression * scale < self.host.rng().flrand(0.0, 4.0) {
            return false;
        }
        self.actors[me].mind.fight.shot_time <= self.level_time
    }

    /// `NPC_CheckDefend` (`NPC_combat.c:2216-2225`): evasive enough (its evasion against
    /// up to four times `scale`).
    pub(crate) fn check_defend(&mut self, me: usize, scale: f32) -> bool {
        let scale = if scale == 0.0 { 1.0 } else { scale };
        let evasion = self.actors[me].definition.stats.evasion as f32;
        evasion > self.host.rng().flrand(0.0, 1.0) * 4.0 * scale
    }

    /// Whether client `number` holds its attack down this frame (`client->buttons &
    /// BUTTON_ATTACK`).
    pub(crate) fn enemy_attacking(&self, number: u16) -> bool {
        let buttons = match self.actor_at(number) {
            Some(at) => self.actors[at].mind.command.buttons,
            None => self.host.client_buttons(number),
        };
        buttons & crate::npc_states::BUTTON_ATTACK != 0
    }

    /// `g_entities[number].health` as a shot's checks read it: a client's, a breakable's,
    /// zero for the world and anything else.
    fn health_of(&self, number: u16) -> i32 {
        self.body(number).map_or_else(
            || self.host.damageable_health(number).unwrap_or(0),
            |body| body.health,
        )
    }

    /// A `MASK_SHOT` line from `start` to `end` past `pass`, and past a pane of glass it
    /// meets that is not `target` (`ShotThroughGlass`, `NPC_combat.c:1095-1109`).
    fn shot_line(
        &mut self,
        start: [f32; 3],
        end: [f32; 3],
        pass: u16,
        target: u16,
    ) -> crate::pmove::MovementTrace {
        let trace = self.trace_bodies(start, [0.0; 3], [0.0; 3], end, pass, MASK_SHOT);
        if trace.entity_number != target
            && trace.entity_number < crate::npc_spawn::ENTITYNUM_WORLD
            && self.host.glass(trace.entity_number)
        {
            return self.trace_bodies(
                trace.end_position,
                [0.0; 3],
                [0.0; 3],
                end,
                trace.entity_number,
                MASK_SHOT,
            );
        }
        trace
    }

    /// `CanShoot` (`NPC_combat.c:1119-1190`): a shot from the NPC's muzzle would reach
    /// `target` — its middle, else its head, else near enough by chance, else through a
    /// dead body or someone not of its team.
    pub(crate) fn can_shoot(&mut self, me: usize, target: &Body) -> bool {
        let muzzle = self.weapon_spot(me);
        let number = self.actors[me].number;
        let middle = spot(target, Spot::Origin);
        let first = self.trace_bodies(muzzle, [0.0; 3], [0.0; 3], middle, number, MASK_SHOT);
        let mut struck = first.entity_number;
        if first.start_solid
            && let Some(toucher) = self.actors[me].mind.touched_by
        {
            struck = toucher;
        }
        if first.entity_number != target.number
            && first.entity_number < crate::npc_spawn::ENTITYNUM_WORLD
            && self.host.glass(first.entity_number)
        {
            struck = self
                .trace_bodies(
                    first.end_position,
                    [0.0; 3],
                    [0.0; 3],
                    middle,
                    first.entity_number,
                    MASK_SHOT,
                )
                .entity_number;
        }
        if struck == target.number {
            return true;
        }
        let head = spot(target, Spot::Head);
        let trace = self.trace_bodies(muzzle, [0.0; 3], [0.0; 3], head, number, MASK_SHOT);
        if trace.entity_number == target.number {
            return true;
        }
        if length(subtract(head, trace.end_position)) < self.host.rng().flrand(0.0, 1.0) * 32.0 {
            return true;
        }
        let Some(hit) = self.body(trace.entity_number) else {
            return false;
        };
        hit.health <= 0 || hit.player_team != self.actors[me].player_team
    }

    /// `NPC_CheckVisibility` with its shot check (`CHECK_SHOOT`, `NPC_senses.c:278-350`).
    pub(crate) fn visibility_with_shot(
        &mut self,
        me: usize,
        target: &Body,
        flags: u32,
    ) -> Visibility {
        let seen = self.visibility(me, target, flags);
        if seen == Visibility::Fov && flags & CHECK_SHOOT != 0 && self.can_shoot(me, target) {
            return Visibility::Shoot;
        }
        seen
    }

    /// `NPC_CheckCanAttack` (`NPC_combat.c:2229-2431`): the NPC turned toward its enemy's
    /// wobbling head and, the enemy in reach, in view and no teammate in the way, fires
    /// when aggressive enough — or ducks an enemy firing at it. Whether it attacks.
    pub(crate) fn check_can_attack(
        &mut self,
        me: usize,
        scale: f32,
        _stationary: bool,
        command: &mut UserCommand,
    ) -> bool {
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
        else {
            return false;
        };
        if enemy.flags & FL_NOTARGET != 0 {
            return false;
        }
        let mut scale = if scale == 0.0 { 1.0 } else { scale };
        let max_aim_off = 128.0 - 16.0 * self.actors[me].definition.stats.aim as f32;
        let enemy_org = self.aim_wiggle(me, &enemy, spot(&enemy, Spot::Head));
        let muzzle = self.weapon_spot(me);
        let mut delta = subtract(enemy_org, muzzle);
        let angles = vector_angles(delta);
        let distance = normalize(&mut delta);
        self.actors[me].desired_yaw = angles[1];
        self.update_firing_angles(me, false, true, command);
        if self.enemy_too_far(me, &enemy, distance * distance, true) {
            return false;
        }
        if self.actors[me]
            .player
            .raw_field(PS_WEAPON_TIME)
            .unwrap_or(0) as i32
            > 0
        {
            self.actors[me].mind.desired_pitch = angles[0];
            self.update_firing_angles(me, true, false, command);
            return false;
        }
        if self.actors[me].script_flags & SCF_DONT_FIRE != 0 {
            return false;
        }
        let mut attack = false;
        if self.visibility(me, &enemy, CHECK_360 | CHECK_FOV) >= Visibility::Fov {
            attack = true;
            let number = self.actors[me].number;
            if enemy.enemy == Some(number)
                && self.enemy_attacking(enemy.number)
                && self.check_defend(me, 1.0)
            {
                attack = false;
                command.up_move = -127;
            }
            let (mut hitspot, mut dead_on, mut struck) =
                (muzzle, false, crate::npc_spawn::ENTITYNUM_NONE);
            if attack {
                let aim = along(
                    muzzle,
                    distance,
                    forward(self.actors[me].player.view_angles()),
                );
                let trace = self.shot_line(muzzle, aim, number, enemy.number);
                struck = trace.entity_number;
                hitspot = trace.end_position;
                let hit = self.body(struck);
                let enemy_team = self.actors[me].enemy_team;
                if struck == enemy.number
                    || hit.is_some_and(|hit| enemy_team != 0 && enemy_team == hit.player_team)
                {
                    dead_on = true;
                } else {
                    scale *= 0.5;
                    let team = self.actors[me].player_team;
                    if team != 0
                        && hit.is_some_and(|hit| hit.player_team != 0 && hit.player_team == team)
                    {
                        attack = false;
                    }
                }
            }
            if attack {
                let pitch = vector_angles(subtract(hitspot, muzzle))[0];
                self.actors[me].mind.desired_pitch = pitch;
                self.update_firing_angles(me, true, false, command);
                let easy = self.health_of(struck) <= 30
                    || (struck < crate::npc_spawn::ENTITYNUM_WORLD && self.host.glass(struck));
                if !dead_on && !easy {
                    // "try a suppressing fire": too far off the enemy's wobbling head?
                    let aimed = along(
                        muzzle,
                        distance,
                        forward(self.actors[me].player.view_angles()),
                    );
                    let mut off = length(subtract(aimed, enemy_org));
                    if off > self.host.rng().flrand(0.0, 1.0) * max_aim_off {
                        scale *= 0.75;
                        off = length(subtract(aimed, enemy_org));
                        if off > self.host.rng().flrand(0.0, 1.0) * max_aim_off {
                            attack = false;
                        }
                    }
                    scale *= (max_aim_off - off + 1.0) / max_aim_off;
                }
            }
        } else {
            self.actors[me].mind.desired_pitch = angles[0];
            self.update_firing_angles(me, true, false, command);
        }
        if attack {
            if self.check_attack(me, scale) {
                self.weapon_think(me, command);
            } else {
                attack = false;
            }
        }
        attack
    }

    /// `NPC_BSAdvanceFight` (`NPC_behavior.c:49-201`): toward the capture goal (a
    /// navigation goal kept for 100 s), shooting its enemy on the way when the shot's own
    /// angles bear on it.
    pub fn bs_advance_fight(&mut self, me: usize, command: &mut UserCommand) {
        if let Some(capture) = self.actors[me].mind.states.capture_goal
            && let Some(origin) = self.entity_origin(me, capture)
        {
            self.set_move_goal(me, origin, 16, true, -1, None);
            self.actors[me].mind.tactics.goal_time = self.level_time + 100_000;
        }
        self.check_enemy(me, true, false, true);
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
        else {
            let view = self.actors[me].player.view_angles();
            self.update_shoot_angles(me, view, true, true);
            return;
        };
        let number = self.actors[me].number;
        // `r.absmin`: as the enemy was last linked (an NPC links before its origin follows
        // its move).
        let absmin = match self.actor_at(enemy.number) {
            Some(at) => self.actors[at].link.0,
            None => std::array::from_fn(|axis| enemy.origin[axis] + enemy.mins[axis] - 1.0),
        };
        let enemy_org = along(absmin, 0.5, enemy.maxs);
        let muzzle = self.weapon_spot(me);
        let mut delta = subtract(enemy_org, muzzle);
        let angles = vector_angles(delta);
        let distance = normalize(&mut delta);
        let mut attack = !self.enemy_too_far(me, &enemy, distance * distance, true);
        let mut scale = 1.0_f32;
        if attack {
            self.update_shoot_angles(me, angles, false, true);
            if self.visibility(me, &enemy, CHECK_FOV) == Visibility::Fov {
                let head = spot(&enemy, Spot::Head);
                let enemy_team = self.actors[me].enemy_team;
                let ally = |hit: Option<Body>| {
                    hit.is_some_and(|hit| enemy_team != 0 && enemy_team == hit.player_team)
                };
                let mut trace =
                    self.trace_bodies(muzzle, [0.0; 3], [0.0; 3], enemy_org, number, MASK_SHOT);
                if trace.entity_number != enemy.number && !ally(self.body(trace.entity_number)) {
                    scale *= 0.75;
                    trace = self.trace_bodies(muzzle, [0.0; 3], [0.0; 3], head, number, MASK_SHOT);
                }
                let hitspot = trace.end_position;
                let hit = self.body(trace.entity_number);
                let dead_on = trace.entity_number == enemy.number || ally(hit);
                if !dead_on {
                    scale *= 0.5;
                    let team = self.actors[me].player_team;
                    if team != 0
                        && hit.is_some_and(|hit| hit.player_team != 0 && hit.player_team == team)
                    {
                        attack = false;
                    }
                }
                if attack {
                    let aim = vector_angles(subtract(hitspot, muzzle));
                    self.actors[me].mind.desired_pitch = aim[0];
                    self.update_shoot_angles(me, aim, true, false);
                    if !dead_on {
                        let shot = along(
                            muzzle,
                            distance,
                            forward(self.actors[me].mind.states.shoot_angles),
                        );
                        let mut off = length(subtract(shot, enemy_org));
                        if off > self.host.rng().flrand(0.0, 1.0) * 64.0 {
                            scale *= 0.75;
                            off = length(subtract(shot, head));
                            if off > self.host.rng().flrand(0.0, 1.0) * 64.0 {
                                attack = false;
                            }
                        }
                        scale *= (64.0 - off + 1.0) / 64.0;
                    }
                }
            }
        }
        if attack && self.check_attack(me, scale) {
            self.weapon_think(me, command);
        }
        // No forward or side move: the capture goal reached (the script's task, none here).
    }

    /// `NPC_BSEmplaced` (`NPC_behavior.c:1671-1769`): hurt, it only turns; with no enemy
    /// it looks about now and then; with one in sight it faces it and, with a clear shot
    /// (or something breakable in the way), fires — never between two saber duellists.
    pub fn bs_emplaced(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        if self.actors[me].mind.fight.pain_debounce_time > level_time {
            self.update_angles(me, true, true, command);
            return;
        }
        let scripted_fire = self.actors[me].script_flags & SCF_FIRE_WEAPON != 0;
        if scripted_fire {
            self.weapon_think(me, command);
        }
        if !self.check_enemy_ext(me) {
            if self.host.irand(0, 30) == 0 {
                let yaw =
                    f32::from_bits(self.actors[me].state.raw_field(es::ANGLES[1]).unwrap_or(0));
                let turn = self.host.irand(-90, 90);
                self.actors[me].desired_yaw = yaw + turn as f32;
            }
            if self.host.irand(0, 30) == 0 {
                let pitch = self.host.irand(-20, 20);
                self.actors[me].mind.desired_pitch = pitch as f32;
            }
            self.update_angles(me, true, true, command);
            return;
        }
        let Some(enemy) = self.actors[me]
            .mind
            .enemy
            .and_then(|number| self.body(number))
        else {
            return;
        };
        let (mut seen, mut shoot) = (false, false);
        if self.clear_los4(me, &enemy) {
            seen = true;
            let (hit, _) = self.shot_entity(me, &enemy);
            if hit == enemy.number || self.takes_damage(hit) {
                shoot = true;
                self.aim_adjust(me, 2);
                self.actors[me].mind.tactics.enemy_last_seen_location = enemy.origin;
            }
        }
        if seen {
            self.face_enemy(me, true, command);
        } else {
            self.update_angles(me, true, true, command);
        }
        if self.actors[me].script_flags & SCF_DONT_FIRE != 0 {
            shoot = false;
        }
        if let Some(theirs) = enemy.enemy.and_then(|number| self.body(number))
            && enemy.weapon == WP_SABER
            && theirs.weapon == WP_SABER
        {
            shoot = false;
        }
        if shoot && !scripted_fire {
            self.weapon_think(me, command);
        }
    }
}
