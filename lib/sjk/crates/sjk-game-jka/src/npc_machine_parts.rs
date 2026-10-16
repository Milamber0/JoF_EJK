//! What the walking machines' AI shares (NPC plan step 8, `NPC_AI_Mark1.c`,
//! `NPC_AI_Mark2.c`, `NPC_AI_GalakMech.c`, `NPC_AI_Atst.c`): the parts a blow struck on their
//! own models (`G_GetHitLocFromSurfName`'s machine branches, `g_combat.c:3739-3822`, and
//! `gPainHitLoc` with `locationDamage`, `g_combat.c:5417-5456`), their bolts read whole
//! (`G2API_GetBoltMatrix` with `BG_GiveMeVectorFromMatrix`), the bolts they fire from
//! (`CreateMissile`, `g_missile.c:297-327`) and the sounds they make (`G_SoundAtLoc`); the
//! effects they play there are the droids' (`G_PlayEffectID`,
//! [`NpcWorld::play_effect_at`]).
//!
//! What a machine keeps beyond `gNPC_t` is [`Machine`], on its creature record: the damage
//! each part took, what its pain read, and the mech's laser, shield and speech. All of it is
//! the NPC's own and meets a protocol number only at the host. What the flying machines
//! share with them (the idle, the self-inflicted blows, the missiles' launch through
//! [`NpcHost::launch_missile`]) is [`crate::npc_machine`]'s.

use crate::npc_spawn::NpcHost;
use crate::npc_world::NpcWorld;
use crate::weapon_fire::Missile;

/// `hitLocation_t` (`g_local.h:139-165`) as the machines' pains read it.
pub const HL_NONE: i32 = 0;
pub const HL_CHEST: i32 = 11;
pub const HL_ARM_RT: i32 = 12;
pub const HL_ARM_LT: i32 = 13;
pub const HL_GENERIC1: i32 = 17;
pub const HL_GENERIC2: i32 = 18;
/// `HL_MAX`: the parts `locationDamage` counts.
pub const HL_MAX: usize = 23;
/// `class_t`s with parts of their own.
pub const CLASS_ATST: i32 = 1;
pub const CLASS_MARK1: i32 = 23;
pub const CLASS_MARK2: i32 = 24;
pub const CLASS_GALAKMECH: i32 = 25;
/// `CLASS_VEHICLE`: no part of a vehicle counts its damage here.
const CLASS_VEHICLE: i32 = 53;
/// `Q3_INFINITE`: a part that cannot be hurt more.
const Q3_INFINITE: i32 = 16_777_216;
/// `TURN_OFF` (`G2SURFACEFLAG_NODESCENDANTS`), `TURN_ON`.
pub const TURN_OFF: u32 = 0x100;
pub const TURN_ON: u32 = 0;
/// `SVF_USE_CURRENT_ORIGIN`-free missiles: `MASK_SHOT | CONTENTS_LIGHTSABER`.
pub const MISSILE_CLIP: u32 = 0x1 | 0x100 | 0x200 | 0x1000 | 0x4_0000;
/// `DAMAGE_HALF_ABSORB`: `G_MissileImpact`'s flags for a bowcaster's bolt striking home
/// (`g_missile.c:668-691`); any other of the machines' bolts strikes with none (their own
/// `dflags`, `DAMAGE_DEATH_KNOCKBACK`, is never read).
pub const IMPACT_HALF_ABSORB: u32 = 0x400;
/// The humanoid bolts `G_GetHitLocFromSurfName` adds before it looks at the class
/// (`g_combat.c:3729-3737`), on a machine posed on Kyle's model.
const HUMANOID_HIT_BOLTS: [&str; 6] = [
    "*l_hand",
    "*r_hand",
    "*hips_l_knee",
    "*hips_r_knee",
    "*l_leg_foot",
    "*r_leg_foot",
];

/// What a machine keeps on its entity and `gNPC_t` beyond what every NPC does.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Machine {
    /// `locationDamage[HL_MAX]`: the damage each part took from blows that struck it.
    pub location_damage: [i32; HL_MAX],
    /// `gPainHitLoc` as the last blow that took health left it: the part a blade struck
    /// this frame, -1 for none.
    pub pain_hit_location: i32,
    /// `lockCount`: the mech's laser — 1 charging, 2 firing.
    pub lock_count: i32,
    /// `NPCInfo->coverTarg`: the entity the laser's loop sound moves with.
    pub cover_target: Option<u16>,
    /// `delay`: when the mech may cry out in pain again.
    pub delay: i32,
    /// `NPCInfo->movementSpeech`: the mech's taunts as its enemy weakens (0 to 3).
    pub movement_speech: i32,
    /// `NPCInfo->investigateDebounceTime`: when the mech's shield comes back.
    pub investigate_debounce_time: i32,
    /// `client->hiddenDir`, `hiddenDist`: the mech's last lob, which others read to find it.
    pub hidden_dir: [f32; 3],
    pub hidden_dist: f32,
    /// `NPCInfo->blockedDebounceTime`: the mech's smack dealt (read only by code compiled out).
    pub blocked_debounce_time: i32,
    /// `alt_fire`: written by the mech's AI; nothing in multiplayer reads it for an NPC.
    pub alt_fire: bool,
}

/// `G_GetHitLocFromSurfName` for the machines (`g_combat.c:3739-3822`): the part a surface
/// is, `HL_NONE` for any other of theirs; `None` for a class with no parts of its own.
pub fn surface_part(class: i32, surface: &str) -> Option<i32> {
    let named = |pairs: &[(&str, i32)]| {
        pairs
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(surface))
            .map_or(HL_NONE, |(_, part)| *part)
    };
    Some(match class {
        CLASS_ATST => named(&[
            ("head_light_blaster_cann", HL_ARM_LT),
            ("head_concussion_charger", HL_ARM_RT),
        ]),
        CLASS_MARK1 => named(&[
            ("l_arm", HL_ARM_LT),
            ("r_arm", HL_ARM_RT),
            ("torso_front", HL_CHEST),
            ("torso_tube1", HL_GENERIC1),
            ("torso_tube2", HL_GENERIC1 + 1),
            ("torso_tube3", HL_GENERIC1 + 2),
            ("torso_tube4", HL_GENERIC1 + 3),
            ("torso_tube5", HL_GENERIC1 + 4),
            ("torso_tube6", HL_GENERIC1 + 5),
        ]),
        CLASS_MARK2 => named(&[
            ("torso_canister1", HL_GENERIC1),
            ("torso_canister2", HL_GENERIC1 + 1),
            ("torso_canister3", HL_GENERIC1 + 2),
        ]),
        CLASS_GALAKMECH => {
            let named = named(&[
                ("torso_antenna", HL_GENERIC1),
                ("torso_antenna_base", HL_GENERIC1),
                ("torso_shield", HL_GENERIC2),
            ]);
            if named == HL_NONE { HL_CHEST } else { named }
        }
        _ => return None,
    })
}

/// A machine's part as `G_LocationBasedDamageModifier` places a blow by it
/// ([`surface_part`]): its arms and chest are the body's, anything else no part at all.
pub fn surface_location(class: i32, surface: &str) -> Option<crate::damage::HitLocation> {
    use crate::damage::HitLocation;
    surface_part(class, surface).map(|part| match part {
        HL_ARM_LT => HitLocation::ArmLeft,
        HL_ARM_RT => HitLocation::ArmRight,
        HL_CHEST => HitLocation::Chest,
        _ => HitLocation::None,
    })
}

/// `G2API_GetBoltMatrix` for a bolt the instance does not have (`G2_API.cpp:1876-1880`):
/// the world matrix times the Ghoul2 identity (its 90° turn), not swapped.
pub fn unbolted_matrix(angles: [f32; 3], origin: [f32; 3]) -> [[f32; 4]; 3] {
    let [forward, left, up] = crate::pmove::flight::angles_to_axis(angles);
    // `Multiply_3x4Matrix(world, identity)`: the world's second column negated first,
    // then its first.
    std::array::from_fn(|row| [left[row], -forward[row], up[row], origin[row]])
}

/// `BG_GiveMeVectorFromMatrix(matrix, ORIGIN)` and `(NEGATIVE_Y)`: where a bolt is, and
/// the way it points.
pub fn origin_and_back(matrix: [[f32; 4]; 3]) -> ([f32; 3], [f32; 3]) {
    (
        std::array::from_fn(|row| matrix[row][3]),
        std::array::from_fn(|row| -matrix[row][1]),
    )
}

/// `CreateMissile(org, dir, vel, life, owner, qfalse)` (`g_missile.c:297-327`): a linear
/// missile of `owner`'s from `origin` (snapped) along `direction` at `velocity`, freed
/// unfired after `life`; its weapon, damage, box and bounce the caller's.
pub fn create_missile(
    owner: u16,
    origin: [f32; 3],
    direction: [f32; 3],
    velocity: f32,
    life: i32,
    level_time: i32,
) -> Missile {
    let origin = crate::weapon_fire::snap_vector(origin);
    let mut missile = crate::weapon_fire::create_missile_by(
        owner, origin, direction, velocity, level_time, false,
    );
    missile.free_at = level_time + life;
    missile
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `gPainHitLoc` and `locationDamage` for a blow that took `take` from the NPC at `me`
    /// (`g_combat.c:5417-5456`): the part of the surface a blade struck its model at this
    /// frame, and that part's damage counted. A class without parts of its own reads -1:
    /// only the machines' pains ask ([`surface_part`]); `g_armBreakage` is off.
    pub(crate) fn note_pain_part(&mut self, me: usize, take: i32) {
        let level_time = self.level_time;
        let npc = &self.actors[me];
        let class = npc.definition.client_class;
        let part = self.host.npc_struck_part(npc, level_time);
        if part.is_some() && npc.humanoid {
            // `G_GetHitLocFromSurfName` adds a humanoid's bolts first, whatever its class.
            let model = npc.definition.player_model.clone();
            for name in HUMANOID_HIT_BOLTS {
                let has = self.host.model_has_bolt(&model, name);
                self.actors[me].mind.creature.bolts.add(has, name);
            }
        }
        let machine = &mut self.actors[me].mind.creature.walker;
        machine.pain_hit_location = part.unwrap_or(-1);
        if let Some(part) = part
            .and_then(|part| usize::try_from(part).ok())
            .filter(|part| *part < HL_MAX)
            && machine.location_damage[part] < Q3_INFINITE
            && class != CLASS_VEHICLE
        {
            machine.location_damage[part] += take;
        }
    }

    /// `G2API_GetBoltMatrix(NPC->ghoul2, 0, bolt, &matrix, r.currentAngles,
    /// r.currentOrigin, level.time, NULL, modelScale)`: the NPC at `me`'s bolt `index`
    /// (its instance's) whole ([`NpcHost::npc_bolt_matrix`]).
    pub(crate) fn bolt_matrix(&mut self, me: usize, index: i32) -> [[f32; 4]; 3] {
        let npc = &self.actors[me];
        let name = npc.mind.creature.bolts.name(index);
        let (angles, origin) = (npc.mind.current_angles, npc.current_origin);
        let level_time = self.level_time;
        self.host
            .npc_bolt_matrix(&self.actors[me], index, name, angles, origin, level_time)
    }

    /// `G2API_AddBolt(NPC->ghoul2, 0, name)` on the NPC at `me`'s instance: the bolt's
    /// index, -1 where its model has none.
    pub(crate) fn add_bolt(&mut self, me: usize, name: &'static str) -> i32 {
        let model = self.actors[me].definition.player_model.clone();
        let has = self.host.model_has_bolt(&model, name);
        self.actors[me].mind.creature.bolts.add(has, name)
    }

    /// `G_SoundAtLoc(origin, channel, G_SoundIndex(name))` (`g_utils.c:1386-1392`).
    pub(crate) fn sound_at_location(&mut self, origin: [f32; 3], channel: u32, name: &[u8]) {
        let sound = self.host.sound_index(name);
        self.host
            .raise(crate::weapon_fire::sound_event(origin, channel, sound));
    }

    /// `NPC_SetSurfaceOnOff(NPC, name, flags)` (`NPC_utils.c:1018-1056`): the entity's bits
    /// for the clients and its instance's surface.
    pub(crate) fn machine_surface(&mut self, me: usize, name: &str, flags: u32) {
        let number = self.actors[me].number;
        let known =
            crate::npc_begin::set_surface(&mut self.actors[me], name, flags == TURN_ON, self.host);
        if known {
            self.host.set_npc_surface(number, name, flags);
        }
    }

    /// `G2API_GetSurfaceRenderStatus(NPC->ghoul2, 0, name)` on the NPC at `me`'s instance.
    pub(crate) fn machine_surface_status(&mut self, me: usize, name: &str) -> i32 {
        let number = self.actors[me].number;
        self.host.surface_status(number, name)
    }

    /// `CreateMissile` from the NPC at `me`, handed to the host ([`NpcHost::launch_missile`]).
    pub(crate) fn launch(&mut self, me: usize, missile: Missile) {
        let number = self.actors[me].number;
        self.host.launch_missile(number, missile);
    }
}
