//! Whether a blow takes a limb off (`G_CheckForDismemberment`, `g_combat.c:4149-4281`): on a
//! killing saber blow (`player_die`, `g_combat.c:2788-2792`), on a corpse cut again
//! (`G_Damage`, `g_combat.c:5502-5510`) and on a lost saber lock's finishing blow
//! (`g_active.c:3091-3095`) — for a player or an NPC. `g_dismember` (the host's
//! [`crate::npc_spawn::NpcHost::dismember_setting`], 0 on a retail server) is the chance in a
//! hundred; 0 cuts nothing. The part is the one the blade struck on the model this frame
//! (`G_GetHitLocFromSurfName`), else where the point lies in the box (`G_GetHitLocation`),
//! else the quarter of the body (`G_GetHitQuad`); the cut is made at that part's bone on the
//! victim's posed model (`G_GetDismemberBolt`), by [`crate::npc_dismember`]'s `G_Dismember`.

use crate::damage::{HitLocation, hit_location};
use crate::event_entity::EventEntity;
use crate::npc_dismember::part;
use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;

/// `CLASS_PROTOCOL`: the one non-humanoid that loses limbs.
const CLASS_PROTOCOL: i32 = 33;
/// The wire fields a cut hand's sparks set (`otherEntityNum`, `otherEntityNum2`, `weapon`,
/// `legsAnim`, `origin`, `angles`), as a blade's hit sets them ([`crate::saber_damage`]).
const ES_OTHER_ENTITY_NUM: usize = 59;
const ES_OTHER_ENTITY_NUM2: usize = 39;
const ES_WEAPON: usize = 14;
const ES_LEGS_ANIM: usize = 16;
const ES_ORIGIN: [usize; 3] = [11, 12, 13];
const ES_ANGLES: [usize; 3] = [25, 9, 24];
/// `ENTITYNUM_NONE`.
const ENTITY_NUMBER_NONE: u32 = 1_023;

/// `gGAvoidDismember`: a lost saber lock's finishing blow cuts nothing as it kills
/// ([`Self::Always`]); after it, the loser's sword hand comes off whatever the chance
/// ([`Self::RightHand`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AvoidDismember {
    /// 0: the chance and the part decide.
    #[default]
    No,
    /// 1: no limb at all.
    Always,
    /// 2: the right hand, whatever the chance and the damage.
    RightHand,
}

/// One `G_CheckForDismemberment(ent, enemy, point, damage, deathAnim, postDeath)` call.
#[derive(Clone, Copy, Debug)]
pub struct DismemberCheck {
    /// `ent`: the client (a player or an NPC) cut.
    pub victim: u16,
    /// `enemy`: who cut it (the limb flies along its blade).
    pub enemy: u16,
    /// `point`: where the blow landed (`pos1`).
    pub point: [f32; 3],
    /// The blow's damage.
    pub damage: i32,
    /// `deathAnim`, `postDeath`: passed on to `G_Dismember`.
    pub death_anim: u16,
    pub post_death: bool,
    /// `gGAvoidDismember` at the call.
    pub avoid: AvoidDismember,
}

/// What `G_CheckForDismemberment` reads of its victim.
struct Victim {
    /// `localAnimIndex <= 1` (the humanoid skeleton) or `CLASS_PROTOCOL`.
    cuttable: bool,
    npc: bool,
    /// `ps.origin`, `ps.velocity`, `ps.viewangles`, `ps.viewheight`.
    origin: [f32; 3],
    velocity: [f32; 3],
    view: [f32; 3],
    view_height: i32,
    /// `r.currentAngles`' yaw (a player's is never set: 0) and `r.absmin`, `r.absmax`.
    current_yaw: f32,
    bounds: ([f32; 3], [f32; 3]),
}

/// `G_GetHitQuad(self, hitloc)` (`g_combat.c:3603-3674`) for a client: the arm, the head
/// or the leg the point is by, from the eyes (`ps.origin` raised by `viewheight`) facing
/// the view's yaw.
pub fn hit_quad(origin: [f32; 3], view_height: i32, view_yaw: f32, point: [f32; 3]) -> i32 {
    let eye = [origin[0], origin[1], origin[2] + view_height as f32];
    let mut difference = [point[0] - eye[0], point[1] - eye[1], 0.0];
    crate::saber_clash::normalize(&mut difference);
    let (_, right) = crate::pmove::flight::flight_axes([0.0, view_yaw, 0.0]);
    let right = right.to_array();
    let right_dot = right[0] * difference[0] + right[1] * difference[1] + right[2] * difference[2];
    let z_difference = point[2] - eye[2];
    let (right_arm, left_arm) = if z_difference > 0.0 {
        (0.3, -0.3)
    } else {
        (0.1, -0.1)
    };
    if z_difference > -20.0 {
        if right_dot > right_arm {
            part::RARM
        } else if right_dot < left_arm {
            part::LARM
        } else {
            part::HEAD
        }
    } else if right_dot >= 0.0 {
        part::RLEG
    } else {
        part::LLEG
    }
}

/// The part a hit location cuts off (`g_combat.c:4217-4253`); `None`: the quarter decides.
fn part_of(location: HitLocation) -> Option<i32> {
    Some(match location {
        HitLocation::FootRight | HitLocation::LegRight => part::RLEG,
        HitLocation::FootLeft | HitLocation::LegLeft => part::LLEG,
        HitLocation::Waist => part::WAIST,
        HitLocation::ArmRight => part::RARM,
        HitLocation::HandRight => part::RHAND,
        HitLocation::ArmLeft | HitLocation::HandLeft => part::LARM,
        HitLocation::Head => part::HEAD,
        _ => return None,
    })
}

/// `G_GetDismemberBolt`'s bone for a part (`g_combat.c:3179-3209`): a creature's waist is
/// its pelvis; anything unknown the right shin.
pub fn dismember_bone(limb: i32, humanoid: bool) -> &'static str {
    match limb {
        part::HEAD => "cranium",
        part::WAIST if humanoid => "thoracic",
        part::WAIST => "pelvis",
        part::LARM => "lradius",
        part::RARM => "rradius",
        part::RHAND => "rhand",
        part::LLEG => "ltibia",
        _ => "rtibia",
    }
}

/// `G_GetDismemberBolt`'s `properOrigin` (`g_combat.c:3214-3252`): `ps.origin` led along the
/// velocity by 0.08 of its summed components, "so it's more like what the client is seeing".
pub fn dismember_origin(origin: [f32; 3], velocity: [f32; 3]) -> [f32; 3] {
    let mut direction = velocity;
    crate::saber_clash::normalize(&mut direction);
    let mut speed = 0.0_f32;
    for axis in velocity {
        speed += axis.abs();
    }
    speed *= 0.08;
    std::array::from_fn(|axis| origin[axis] + direction[axis] * speed)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// Whether a blow of `means` by `attacker` may cut: a saber's, or the melee of a siege
    /// class with heavy melee (`G_HeavyMelee`).
    pub(crate) fn cuts_limbs(&self, means: u32, attacker: u16) -> bool {
        means == crate::means_of_death::MOD_SABER
            || (means == crate::means_of_death::MOD_MELEE && self.host.heavy_melee(attacker))
    }

    /// `G_CheckForDismemberment` ([`DismemberCheck`]).
    pub(crate) fn check_for_dismemberment(&mut self, check: DismemberCheck) {
        // (The reference asks whether the victim can lose limbs first; nothing is drawn or
        // changed before either answer.)
        let dismember = self.host.dismember_setting();
        if dismember == 0 || check.avoid == AvoidDismember::Always {
            return;
        }
        let Some(victim) = self.dismember_victim(check.victim) else {
            return;
        };
        if !victim.cuttable {
            return;
        }
        if check.avoid != AvoidDismember::RightHand
            && (self.host.irand(0, 100) > dismember || check.damage < 5)
        {
            return;
        }
        let location = if check.avoid == AvoidDismember::RightHand {
            HitLocation::HandRight
        } else {
            // A blade's strike on the model this frame names the part (`d_saberGhoul2Collision`),
            // else the box does.
            let level_time = self.level_time;
            let struck = match self.actor_at(check.victim) {
                Some(at) => {
                    let NpcWorld { actors, host, .. } = &mut *self;
                    host.npc_surface_location(&actors[at], 0, check.point, level_time)
                }
                None => self
                    .host
                    .player_surface_location(check.victim, check.point, level_time),
            };
            struck.unwrap_or_else(|| hit_location(victim.current_yaw, victim.bounds, check.point))
        };
        let limb = part_of(location).unwrap_or_else(|| {
            hit_quad(
                victim.origin,
                victim.view_height,
                victim.view[1],
                check.point,
            )
        });
        let at = self.dismember_bolt(check.victim, &victim, limb);
        self.dismember(
            check.victim,
            check.enemy,
            at,
            limb,
            90.0,
            0.0,
            check.death_anim,
            check.post_death,
        );
    }

    /// What the check reads of client `number`: an NPC's own record, or the player's.
    fn dismember_victim(&mut self, number: u16) -> Option<Victim> {
        if let Some(at) = self.actor_at(number) {
            let npc = &self.actors[at];
            return Some(Victim {
                cuttable: npc.humanoid || npc.definition.client_class == CLASS_PROTOCOL,
                npc: true,
                origin: npc.player.origin(),
                velocity: npc.player.velocity(),
                view: npc.player.view_angles(),
                view_height: npc.player.view_height(),
                current_yaw: npc.mind.current_angles[1],
                bounds: npc.link,
            });
        }
        let body = *self
            .host
            .players()
            .iter()
            .find(|body| body.number == number)?;
        let (origin, velocity) =
            self.with_client(number, |state, _| (state.origin(), state.velocity()))?;
        let bounds = (
            std::array::from_fn(|axis| body.origin[axis] + body.mins[axis] - 1.0),
            std::array::from_fn(|axis| body.origin[axis] + body.maxs[axis] + 1.0),
        );
        Some(Victim {
            cuttable: true,
            npc: false,
            origin,
            velocity,
            view: body.view_angles,
            view_height: body.view_height,
            current_yaw: 0.0,
            bounds,
        })
    }

    /// `G_GetDismemberBolt(self, boltPoint, limbType)` (`g_combat.c:3171-3294`): the part's
    /// bone on the victim's posed model, the model at its velocity-led origin facing its
    /// view's yaw; a cut hand throws sparks along the first hilt's blade (where it holds
    /// none, along the placing the failed read leaves).
    fn dismember_bolt(&mut self, number: u16, victim: &Victim, limb: i32) -> [f32; 3] {
        let origin = dismember_origin(victim.origin, victim.velocity);
        let angles = [0.0, victim.view[1], 0.0];
        let humanoid = !victim.npc
            || self
                .actor_at(number)
                .is_none_or(|at| self.actors[at].humanoid);
        let bone = dismember_bone(limb, humanoid);
        let matrix = self
            .host
            .client_bolt_matrix(number, bone, angles, origin)
            .unwrap_or_else(|| crate::npc_machine_parts::unbolted_matrix(angles, origin));
        let at = [matrix[0][3], matrix[1][3], matrix[2][3]];
        // The first hilt is read whatever the part; a read of an instance the client lacks
        // leaves the model's own placing.
        let hilt = self.host.client_hilt_direction(number, angles, origin);
        if limb == part::RHAND {
            let axis = hilt.unwrap_or_else(|| {
                let placed = crate::npc_machine_parts::unbolted_matrix(angles, origin);
                [-placed[0][1], -placed[1][1], -placed[2][1]]
            });
            let mut sparks = EventEntity {
                event: crate::saber_damage::EV_SABER_HIT,
                parameter: 16,
                origin: at,
                client: None,
                broadcast: false,
                extra: [(0, 0); 12],
            };
            let axis = if axis == [0.0; 3] {
                [0.0, 1.0, 0.0]
            } else {
                axis
            };
            let fields = [
                (ES_OTHER_ENTITY_NUM, u32::from(number)),
                (ES_OTHER_ENTITY_NUM2, ENTITY_NUMBER_NONE),
                (ES_WEAPON, 0),
                (ES_LEGS_ANIM, 0),
                (ES_ORIGIN[0], at[0].to_bits()),
                (ES_ORIGIN[1], at[1].to_bits()),
                (ES_ORIGIN[2], at[2].to_bits()),
                (ES_ANGLES[0], axis[0].to_bits()),
                (ES_ANGLES[1], axis[1].to_bits()),
                (ES_ANGLES[2], axis[2].to_bits()),
            ];
            sparks.extra[..fields.len()].copy_from_slice(&fields);
            self.host.raise(sparks);
        }
        at
    }
}

impl crate::npc_roster::NpcRoster {
    /// `G_CheckForDismemberment` for a player the host's game killed or finished
    /// (`player_die`, a lost lock): its limb, if one comes off, is the level's
    /// ([`crate::npc_dismember`]).
    pub fn check_for_dismemberment(
        &mut self,
        check: DismemberCheck,
        level_time: i32,
        host: &mut impl NpcHost,
    ) {
        let mut fired = crate::npc_roster::Fired::new();
        self.world(level_time, host, &mut fired)
            .check_for_dismemberment(check);
    }
}
