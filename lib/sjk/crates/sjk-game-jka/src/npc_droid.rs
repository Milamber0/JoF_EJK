//! The droids' AI (`codemp/game/NPC_AI_Droid.c`): R2-D2, R5-D2, the gonk, the mouse and the
//! protocol droid (`NPC_BSDroid_Default`, `619-643`: wandering on, spinning when hurt,
//! backing up), their parts (`R2D2_PartsMove`, `46-68`), their turning (`R2D2_TurnAnims`,
//! `87-117`) and the pain of every droid without its own (`NPC_Droid_Pain`, `295-456`: an
//! astromech's head popped off low on health, a spin; the interrogator pushed off by a
//! DEMP2 shot). Also the part-turning every droid's AI shares: `NPC_SetBoneAngles`
//! (`NPC_utils.c:928-1011`), the angles on the entity for the clients to turn its bones by.

use crate::npc_spawn::{NpcHost, es};
use crate::npc_world::NpcWorld;
use crate::pmove_anim::{SETANIM_BOTH, SETANIM_FLAG_HOLD, SETANIM_FLAG_OVERRIDE};
use sjk_protocol::UserCommand;

/// `LSTATE_NONE`, `LSTATE_BACKINGUP`, `LSTATE_SPINNING`, `LSTATE_PAIN`, `LSTATE_DROP`.
const LSTATE_NONE: i32 = 0;
const LSTATE_BACKINGUP: i32 = 1;
const LSTATE_SPINNING: i32 = 2;
const LSTATE_PAIN: i32 = 3;
const LSTATE_DROP: i32 = 4;
/// `class_t`s: `CLASS_GONK`, `CLASS_INTERROGATOR`, `CLASS_MOUSE`, `CLASS_R2D2`, `CLASS_R5D2`.
const CLASS_GONK: i32 = 11;
const CLASS_INTERROGATOR: i32 = 16;
const CLASS_MOUSE: i32 = 29;
const CLASS_R2D2: i32 = 34;
const CLASS_R5D2: i32 = 35;
// `MOD_DEMP2`, `MOD_DEMP2_ALT`.
use crate::means_of_death::{MOD_DEMP2, MOD_DEMP2_ALT};
/// `BOTH_STAND2`, `BOTH_PAIN1`, `BOTH_PAIN2`, `BOTH_TURN_LEFT1`, `BOTH_TURN_RIGHT1`,
/// `BOTH_RUN1`.
const BOTH_STAND2: u16 = 917;
const BOTH_PAIN1: u16 = 95;
const BOTH_PAIN2: u16 = 96;
const BOTH_TURN_LEFT1: u16 = 1_126;
const BOTH_TURN_RIGHT1: u16 = 1_127;
const BOTH_RUN1: u16 = 1_111;
/// `SCF_LOOK_FOR_ENEMIES`; `BUTTON_WALKING`.
const SCF_LOOK_FOR_ENEMIES: u32 = 0x800;
const BUTTON_WALKING: u16 = 16;
/// `TURN_OFF` (`G2SURFACEFLAG_NODESCENDANTS`).
const TURN_OFF: u32 = 0x100;
/// `EV_PLAY_EFFECT_ID`, `EV_ENTITY_SOUND`.
const EV_PLAY_EFFECT_ID: u32 = 69;
/// `ps.electrifyTime`, `ps.m_iVehicleNum`; `s.m_iVehicleNum`.
const PS_ELECTRIFY_TIME: usize = 73;
const PS_VEHICLE: usize = 84;
const ES_VEHICLE: usize = 93;
/// `s.boneIndex1..4`, `s.boneAngles1..4`, `s.boneOrient` (the entity's wire fields).
const ES_BONE_INDEX: [usize; 4] = [87, 107, 108, 109];
const ES_BONE_ANGLES: [[usize; 3]; 4] = [
    [110, 45, 67],
    [111, 112, 113],
    [114, 115, 116],
    [117, 118, 119],
];
const ES_BONE_ORIENT: usize = 90;
/// `(NEGATIVE_Z) | (NEGATIVE_Y << 3) | (POSITIVE_X << 6)`: `BONE_ANGLES_POSTMULT`'s axes.
const BONE_ORIENT: u32 = 0x75;

/// `AngleNormalize360` (`q_math.c:511-513`): through sixteen bits, in single precision
/// (`angle * (65536 / 360.0f)` is a float product: npcunseen.c's probe turns its head by a
/// yaw whose double product falls a sixteenth-bit short).
pub(crate) fn angle_normalize360(angle: f32) -> f32 {
    crate::player_angle_math::angle_mod(angle)
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `NPC_BSDroid_Default` (`NPC_AI_Droid.c:619-643`).
    pub fn bs_droid_default(&mut self, me: usize, command: &mut UserCommand) {
        match self.actors[me].mind.fight.local_state {
            LSTATE_SPINNING => self.droid_spin(me, command),
            LSTATE_PAIN => {
                // `Droid_Pain`: "He's done jumping around".
                if self.actors[me]
                    .mind
                    .timers
                    .done("droidpain", self.level_time)
                {
                    self.actors[me].mind.fight.local_state = LSTATE_NONE;
                }
            }
            LSTATE_DROP => {
                self.update_angles(me, true, true, command);
                command.up_move = (self.host.rng().flrand(-1.0, 1.0) * 64.0) as i8;
            }
            _ if self.actors[me].script_flags & SCF_LOOK_FOR_ENEMIES != 0 => {
                self.droid_patrol(me, command)
            }
            _ => self.droid_run(me, command),
        }
    }

    /// `R2D2_PartsMove` (`46-68`): the front eye's lens turned now and then.
    fn r2d2_parts_move(&mut self, me: usize) {
        let level_time = self.level_time;
        if !self.actors[me].mind.timers.done("eyeDelay", level_time) {
            return;
        }
        let mut eye = self.actors[me].mind.creature.parts[0];
        eye[1] = angle_normalize360(eye[1]);
        eye[0] += self.host.irand(-20, 20) as f32;
        eye[1] = self.host.irand(-20, 20) as f32;
        eye[2] = self.host.irand(-20, 20) as f32;
        self.actors[me].mind.creature.parts[0] = eye;
        self.npc_set_bone_angles(me, b"f_eye", eye);
        let delay = self.host.irand(100, 1_000);
        self.actors[me]
            .mind
            .timers
            .set("eyeDelay", level_time, delay);
    }

    /// `R2D2_TurnAnims` (`87-117`): an astromech turning more than 20 degrees plays its
    /// turn; anything else runs.
    fn r2d2_turn_anims(&mut self, me: usize) {
        let npc = &self.actors[me];
        let delta = crate::npc_senses::angle_delta(npc.mind.current_angles[1], npc.desired_yaw);
        let class = npc.definition.client_class;
        let legs = npc.player.leg_animation();
        if delta.abs() > 20.0 && (class == CLASS_R2D2 || class == CLASS_R5D2) {
            let turn = if delta < 0.0 {
                BOTH_TURN_LEFT1
            } else {
                BOTH_TURN_RIGHT1
            };
            if legs != turn {
                self.set_animation(
                    me,
                    SETANIM_BOTH,
                    turn,
                    SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
                );
            }
        } else {
            self.set_animation(
                me,
                SETANIM_BOTH,
                BOTH_RUN1,
                SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
            );
        }
    }

    /// `Droid_Patrol` (`124-190`): its parts and turns, to its goal walking — the mouse
    /// weaving — and each droid's chatter now and then.
    fn droid_patrol(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        let eye = &mut self.actors[me].mind.creature.parts[0];
        eye[1] = angle_normalize360(eye[1]);
        let class = self.actors[me].definition.client_class;
        if class != CLASS_GONK {
            if class != CLASS_R5D2 {
                // "he doesn't have an eye"
                self.r2d2_parts_move(me);
            }
            self.r2d2_turn_anims(me);
        }
        if self.update_goal(me, command).is_some() {
            command.buttons |= BUTTON_WALKING;
            self.move_to_goal(me, true, command);
            let talk = |world: &mut Self, first: i32, last: i32, format: fn(i32) -> String| {
                if world.actors[me].mind.timers.done("patrolNoise", level_time) {
                    let which = world.host.irand(first, last);
                    world.sound_on_entity(me, format(which).as_bytes());
                    let delay = world.host.irand(2_000, 4_000);
                    world.actors[me]
                        .mind
                        .timers
                        .set("patrolNoise", level_time, delay);
                }
            };
            match class {
                CLASS_MOUSE => {
                    // "Weaves side to side a little": `sin(level.time*.5) * 25` in double.
                    let npc = &mut self.actors[me];
                    npc.desired_yaw = (f64::from(npc.desired_yaw)
                        + (f64::from(level_time) * 0.5).sin() * 25.0)
                        as f32;
                    talk(self, 1, 3, |which| {
                        format!("sound/chars/mouse/misc/mousego{which}.wav")
                    });
                }
                CLASS_R2D2 => talk(self, 1, 3, |which| {
                    format!("sound/chars/r2d2/misc/r2d2talk0{which}.wav")
                }),
                CLASS_R5D2 => talk(self, 1, 4, |which| {
                    format!("sound/chars/r5d2/misc/r5talk{which}.wav")
                }),
                _ => {}
            }
            if class == CLASS_GONK {
                talk(self, 1, 2, |which| {
                    format!("sound/chars/gonk/misc/gonktalk{which}.wav")
                });
            }
        }
        self.update_angles(me, true, true, command);
    }

    /// `Droid_Run` (`197-222`): backing up once when told, else on at half speed toward its
    /// goal, weaving a little.
    fn droid_run(&mut self, me: usize, command: &mut UserCommand) {
        self.r2d2_parts_move(me);
        if self.actors[me].mind.fight.local_state == LSTATE_BACKINGUP {
            command.forward_move = -127;
            self.actors[me].desired_yaw += 5.0;
            // "So he doesn't constantly backup."
            self.actors[me].mind.fight.local_state = LSTATE_NONE;
        } else {
            command.forward_move = 64;
            if self.update_goal(me, command).is_some() && self.move_to_goal(me, false, command) {
                let npc = &mut self.actors[me];
                npc.desired_yaw = (f64::from(npc.desired_yaw)
                    + (f64::from(self.level_time) * 0.5).sin() * 5.0)
                    as f32;
            }
        }
        self.update_angles(me, true, true, command);
    }

    /// `Droid_Spin` (`229-288`): an astromech without its head spins and sparks and roams
    /// about; any other spins until its roam is over.
    fn droid_spin(&mut self, me: usize, command: &mut UserCommand) {
        let level_time = self.level_time;
        self.r2d2_turn_anims(me);
        let class = self.actors[me].definition.client_class;
        let number = self.actors[me].number;
        if (class == CLASS_R5D2 || class == CLASS_R2D2)
            && self.host.surface_status(number, "head") > 0
        {
            let origin = self.actors[me].current_origin;
            let timers = &self.actors[me].mind.timers;
            if timers.done("smoke", level_time) && !timers.done("droidsmoketotal", level_time) {
                self.actors[me].mind.timers.set("smoke", level_time, 100);
                self.play_effect_at(b"volumetric/droid_smoke", origin, [0.0, 0.0, 1.0]);
            }
            if self.actors[me].mind.timers.done("droidspark", level_time) {
                let delay = self.host.irand(100, 500);
                self.actors[me]
                    .mind
                    .timers
                    .set("droidspark", level_time, delay);
                self.play_effect_at(b"sparks/spark", origin, [0.0, 0.0, 1.0]);
            }
            command.forward_move = self.host.irand(-64, 64) as i8;
            if self.actors[me].mind.timers.done("roam", level_time) {
                let delay = self.host.irand(250, 1_000);
                self.actors[me].mind.timers.set("roam", level_time, delay);
                // "Go in random directions"
                self.actors[me].desired_yaw = self.host.irand(0, 360) as f32;
            }
        } else if self.actors[me].mind.timers.done("roam", level_time) {
            self.actors[me].mind.fight.local_state = LSTATE_NONE;
        } else {
            // "Spin around"
            let npc = &mut self.actors[me];
            npc.desired_yaw = angle_normalize360(npc.desired_yaw + 40.0);
        }
        self.update_angles(me, true, true, command);
    }

    /// `NPC_Droid_Pain` (`NPC_AI_Droid.c:295-456`), with `gPainMOD` (`means`), then `NPC_Pain`.
    pub(crate) fn droid_pain(&mut self, me: usize, attacker: Option<u16>, damage: i32, means: u32) {
        let angles = self.actors[me].mind.tactics.last_path_angles;
        for (index, value) in es::ANGLES.into_iter().zip(angles) {
            self.actors[me].state.set_raw_field(index, value.to_bits());
        }
        let demp2 = means == MOD_DEMP2 || means == MOD_DEMP2_ALT;
        match self.actors[me].definition.client_class {
            CLASS_R5D2 => self.astromech_pain(
                me,
                damage,
                demp2,
                b"chunks/r5d2head_veh",
                b"chunks/r5d2head",
            ),
            CLASS_R2D2 => self.astromech_pain(
                me,
                damage,
                demp2,
                b"chunks/r2d2head_veh",
                b"chunks/r2d2head",
            ),
            CLASS_MOUSE => {
                let level_time = self.level_time;
                let npc = &mut self.actors[me];
                if demp2 {
                    npc.mind.fight.local_state = LSTATE_SPINNING;
                    npc.player
                        .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 3_000) as u32);
                } else {
                    npc.mind.fight.local_state = LSTATE_BACKINGUP;
                }
                npc.script_flags &= !SCF_LOOK_FOR_ENEMIES;
            }
            CLASS_INTERROGATOR if demp2 => {
                if let Some(other) = attacker.and_then(|number| self.body(number)) {
                    let npc = &mut self.actors[me];
                    let mut direction =
                        crate::npc_senses::subtract(npc.current_origin, other.origin);
                    crate::player_angle_math::normalize(&mut direction);
                    let velocity = npc.player.velocity();
                    let mut velocity: [f32; 3] =
                        std::array::from_fn(|axis| velocity[axis] + 550.0 * direction[axis]);
                    velocity[2] -= 127.0;
                    npc.player.set_velocity(velocity);
                }
            }
            _ => {}
        }
        self.npc_pain(me, attacker, damage, means);
    }

    /// `NPC_Droid_Pain` for R2-D2 and R5-D2 (`304-365`, `382-443`): struck hard enough (by
    /// its pain chance, or any DEMP2 shot), its head pops off below 30 health — a spin with
    /// sparks and smoke — or it spins in pain a while.
    fn astromech_pain(
        &mut self,
        me: usize,
        damage: i32,
        demp2: bool,
        vehicle_chunks: &[u8],
        chunks: &[u8],
    ) {
        let level_time = self.level_time;
        let chance = self.pain_chance(me, damage);
        if !(demp2 || self.host.rng().flrand(0.0, 1.0) < chance) {
            return;
        }
        let npc = &self.actors[me];
        let on_vehicle = npc.state.raw_field(ES_VEHICLE).unwrap_or(0) != 0;
        if !on_vehicle && (npc.health < 30 || demp2) {
            // "Doesn't have to ALWAYSDIE"
            let number = npc.number;
            if npc.spawnflags & 2 == 0
                && npc.mind.fight.local_state != LSTATE_SPINNING
                && self.host.surface_status(number, "head") == 0
            {
                self.npc_surface_off(me, "head");
                let origin = self.actors[me].current_origin;
                if self.actors[me].player.raw_field(PS_VEHICLE).unwrap_or(0) != 0 {
                    let up =
                        crate::pmove::flight::angles_to_axis(self.actors[me].mind.current_angles)
                            [2];
                    self.play_effect_at(vehicle_chunks, origin, up);
                } else {
                    self.play_effect_at(b"small_chunks", origin, [0.0; 3]);
                    self.play_effect_at(chunks, origin, [0.0; 3]);
                }
                let npc = &mut self.actors[me];
                npc.player
                    .set_raw_field(PS_ELECTRIFY_TIME, (level_time + 3_000) as u32);
                npc.mind.timers.set("droidsmoketotal", level_time, 5_000);
                npc.mind.timers.set("droidspark", level_time, 100);
                npc.mind.fight.local_state = LSTATE_SPINNING;
            }
            return;
        }
        // "Just give him normal pain for a little while": on two legs or on three.
        let animation = if self.actors[me].player.leg_animation() == BOTH_STAND2 {
            BOTH_PAIN1
        } else {
            BOTH_PAIN2
        };
        self.set_animation(
            me,
            SETANIM_BOTH,
            animation,
            SETANIM_FLAG_OVERRIDE | SETANIM_FLAG_HOLD,
        );
        self.actors[me].mind.fight.local_state = LSTATE_SPINNING;
        let roam = self.host.irand(1_000, 2_000);
        self.actors[me].mind.timers.set("roam", level_time, roam);
    }

    /// `NPC_SetSurfaceOnOff(NPC, name, TURN_OFF)` (`NPC_utils.c:1018-1056`): the surface's bit
    /// in `s.surfacesOff`, and its instance's surface hidden.
    fn npc_surface_off(&mut self, me: usize, name: &str) {
        let number = self.actors[me].number;
        crate::npc_begin::set_surface(&mut self.actors[me], name, false, self.host);
        self.host.set_npc_surface(number, name, TURN_OFF);
    }

    /// `NPC_SetBoneAngles(NPC, bone, angles)` (`NPC_utils.c:928-1011`): the bone's slot among
    /// the entity's four (its own, or the first free one), its angles there, and the axes
    /// they turn it by. (Its server-side instance's bone is not turned here.)
    pub(crate) fn npc_set_bone_angles(&mut self, me: usize, bone: &[u8], angles: [f32; 3]) {
        let (number, level_time) = (self.actors[me].number, self.level_time);
        set_bone_angles(
            &mut self.actors[me].state,
            &mut *self.host,
            number,
            bone,
            angles,
            level_time,
        );
    }

    /// `G_SoundOnEnt(NPC, CHAN_AUTO, name)` (`g_utils.c:1409-1418`): an entity sound naming
    /// the NPC.
    pub(crate) fn sound_on_entity(&mut self, me: usize, name: &[u8]) {
        let sound = self.host.sound_index(name);
        let npc = &self.actors[me];
        let mut event = crate::knockdown::entity_sound(
            npc.current_origin,
            npc.number,
            crate::npc_creature::CHAN_AUTO,
        );
        event.parameter = u32::from(sound);
        self.host.raise(event);
    }

    /// `G_PlayEffectID(G_EffectIndex(name), origin, angles)` (`g_utils.c:1271-1288`): no
    /// angles play along +y.
    pub(crate) fn play_effect_at(&mut self, name: &[u8], origin: [f32; 3], angles: [f32; 3]) {
        let effect = self.host.effect_index(name);
        let angles = if angles == [0.0; 3] {
            [0.0, 1.0, 0.0]
        } else {
            angles
        };
        let mut event = crate::event_entity::EventEntity {
            event: EV_PLAY_EFFECT_ID,
            parameter: u32::from(effect),
            origin,
            client: None,
            broadcast: false,
            extra: [(0, 0); 12],
        };
        for axis in 0..3 {
            event.extra[axis] = (es::ORIGIN[axis], origin[axis].to_bits());
            event.extra[3 + axis] = (es::ANGLES[axis], angles[axis].to_bits());
        }
        self.host.raise(event);
    }
}

/// `NPC_SetBoneAngles(ent, bone, angles)` (`NPC_utils.c:928-1011`) on entity `number`'s
/// `state`: the bone's slot among the entity's four (its own, or the first free one), its
/// angles there and the axes they turn it by; and the bone turned on its server-side
/// instance ([`NpcHost::set_npc_bone_angles`]).
pub(crate) fn set_bone_angles(
    state: &mut sjk_protocol::EntityState,
    host: &mut impl NpcHost,
    number: u16,
    bone: &[u8],
    angles: [f32; 3],
    level_time: i32,
) {
    let index = u32::from(host.bone_index(bone));
    if !write_bone_angles(state, index, angles, BONE_ORIENT) {
        host.print("WARNING: NPC has no free bone indexes\n");
        return;
    }
    host.set_npc_bone_angles(number, &String::from_utf8_lossy(bone), angles, level_time);
}

/// The wire half of `NPC_SetBoneAngles` and `G2Tur_SetBoneAngles` (`g_turret_G2.c:46-127`):
/// bone `index`'s slot among the entity's four (its own, or the first free one), its
/// `angles` there and the axes (`orient`) they turn it by. False where every slot holds
/// another bone.
pub(crate) fn write_bone_angles(
    state: &mut sjk_protocol::EntityState,
    index: u32,
    angles: [f32; 3],
    orient: u32,
) -> bool {
    let slots = ES_BONE_INDEX.map(|field| state.raw_field(field).unwrap_or(0));
    let found = slots.iter().position(|slot| *slot != 0 && *slot == index);
    let Some(slot) = found.or_else(|| slots.iter().position(|slot| *slot == 0)) else {
        return false;
    };
    state.set_raw_field(ES_BONE_INDEX[slot], index);
    for (field, value) in ES_BONE_ANGLES[slot].into_iter().zip(angles) {
        state.set_raw_field(field, value.to_bits());
    }
    state.set_raw_field(ES_BONE_ORIENT, orient);
    true
}
