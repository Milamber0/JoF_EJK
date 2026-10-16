//! Team heal and team replenish (`ForceTeamHeal`, `ForceTeamForceReplenish`,
//! `w_force.c:1170-1372`): every teammate within 256 units (384 at level 2, 512 at 3), in
//! the user's PVS and open to the power, that is hurt (heal) or short of Force
//! (replenish), gains 50, 33 or 25 — by how many there are — at most every two seconds.
//! One `EV_TEAM_POWER` names them all (`trickedentindex` .. `4`) for their clients'
//! effects. Heal costs only when it healed someone; replenish costs once anyone is found.
//!
//! Only a team game has teammates.

use crate::event_entity::EventEntity;
use crate::force_powers::{FORCE_POWER_NEEDED, FP_TEAM_HEAL, Forcer};

const EV_TEAM_POWER: u32 = 41;
const STAT_HEALTH: usize = 0;
const STAT_MAX_HEALTH: usize = 8;
const PS_FORCE_POWER: usize = 18;
/// `trickedentindex` .. `4`: sixteen clients each.
const ES_TRICKED: [usize; 4] = [58, 74, 92, 94];

impl Forcer<'_, '_> {
    /// `ForceTeamHeal` (`power` is `FP_TEAM_HEAL`) or `ForceTeamForceReplenish`.
    pub(crate) fn team_power(&mut self, power: usize) {
        let level_time = self.frame.level_time;
        if *self.frame.health <= 0
            || !self.usable(power)
            || self.force.debounce[power] >= level_time
        {
            return;
        }
        let heal = power == FP_TEAM_HEAL;
        let level = self.force.levels[power];
        let mut radius = 256.0_f32;
        if level == 2 {
            radius *= 1.5;
        }
        if level == 3 {
            radius *= 2.0;
        }
        let origin = self.origin();
        let (gametype, team, client) = (self.frame.gametype, self.frame.team, self.frame.client);
        // The teammates reached, in client order (a press, not every frame).
        let mut reached = Vec::new();
        for number in 0..self.frame.others.slots() {
            if number == client {
                continue;
            }
            let Some(player) = self.frame.others.player(number) else {
                continue;
            };
            let wants = if heal {
                let health = player.state.stats[STAT_HEALTH] as i32;
                health < player.state.stats[STAT_MAX_HEALTH] as i32 && health > 0
            } else {
                (player.state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32) < 100
            };
            let (their_origin, their_team) = (player.state.origin(), player.team);
            if !crate::force_dark::same_team(gametype, team, their_team)
                || !wants
                || !self.usable_on(number, power)
                || !self.frame.others.in_pvs(origin, their_origin)
            {
                continue;
            }
            let apart: [f32; 3] = std::array::from_fn(|axis| origin[axis] - their_origin[axis]);
            if apart.iter().map(|axis| axis * axis).sum::<f32>().sqrt() <= radius {
                reached.push(number);
            }
        }
        if reached.is_empty() {
            return;
        }
        let amount = match reached.len() {
            1 => 50,
            2 => 33,
            _ => 25,
        };
        self.force.debounce[power] = level_time + 2_000;
        let cost = FORCE_POWER_NEEDED[usize::from(level)][power];
        if !heal {
            self.drain(power, cost);
        }
        let mut event = EventEntity {
            event: EV_TEAM_POWER,
            parameter: if heal { 1 } else { 2 },
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        let mut any = false;
        for number in reached {
            let Some(player) = self.frame.others.player(number) else {
                continue;
            };
            if heal {
                if player.state.stats[STAT_HEALTH] as i32 <= 0 || *player.health <= 0 {
                    continue;
                }
                let health = (player.state.stats[STAT_HEALTH] as i32 + amount)
                    .min(player.state.stats[STAT_MAX_HEALTH] as i32);
                player.state.stats[STAT_HEALTH] = health as u32;
                *player.health = health;
            } else {
                let pool = (player.state.raw_field(PS_FORCE_POWER).unwrap_or(0) as i32 + amount)
                    .min(player.force.max);
                player.state.set_raw_field(PS_FORCE_POWER, pool as u32);
            }
            if !any && heal {
                // Heal pays once it has healed someone.
                self.drain(power, cost);
            }
            any = true;
            // `WP_AddToClientBitflags`.
            let slot = usize::from(number / 16);
            let Some(&field) = ES_TRICKED.get(slot) else {
                continue;
            };
            event.extra[slot] = (field, event.extra[slot].1 | 1 << (number % 16));
        }
        if any {
            self.raise(event);
        }
    }
}
