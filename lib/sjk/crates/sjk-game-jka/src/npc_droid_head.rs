//! A droid's head turned toward what it looks at (`G_G2NPCAngles`, `w_saber.c:746-893`, with
//! `G_CheckLookTarget`, `:656-741`): what `WP_SaberPositionUpdate` does in place of the
//! humanoid spine for the probe and the astromechs, R2-D2 and R5-D2, while a client may see
//! them. The head's angles go on the entity for the clients to turn its `cranium` bone by
//! (`NPC_SetBoneAngles`, [`NpcWorld::npc_set_bone_angles`]). The AT-ST also pitches its body
//! (`thoracic`), and its head turns against its legs' trailing yaw — a local the reference
//! never sets (`trailingLegsAngles`, `CG_ATSTLegsYaw` commented out): a host's
//! [`NpcHost::atst_head_yaw`] answers for it.

use crate::npc_spawn::{ENTITYNUM_WORLD, NpcHost};
use crate::npc_world::NpcWorld;
use crate::player_angle_math::vector_angles;

/// `class_t`s: `CLASS_ATST`, `CLASS_PROBE`, `CLASS_R2D2`, `CLASS_R5D2`.
const CLASS_ATST: i32 = 1;
const CLASS_PROBE: i32 = 32;
const CLASS_R2D2: i32 = 34;
const CLASS_R5D2: i32 = 35;

/// Whether an NPC of `class` turns its head as a droid (the AT-ST, the probe, R2-D2, R5-D2).
pub(crate) fn turns_head(class: i32) -> bool {
    matches!(class, CLASS_ATST | CLASS_PROBE | CLASS_R2D2 | CLASS_R5D2)
}

/// `AngleNormalize180` (`q_math.c`): through `AngleNormalize360`'s sixteen bits.
fn angle_normalize180(angle: f32) -> f32 {
    let angle = crate::npc_droid::angle_normalize360(angle);
    if angle > 180.0 { angle - 360.0 } else { angle }
}

impl<H: NpcHost> NpcWorld<'_, H> {
    /// `G_G2NPCAngles` for the NPC at `me`, a probe or an astromech: its head turned — eased
    /// over a second after it stops looking — toward its look target, else along its view.
    /// `seen`: some client has it in its potentially visible set (`w_saber.c:911-931`).
    pub(crate) fn droid_head_angles(&mut self, me: usize, seen: bool) {
        let class = self.actors[me].definition.client_class;
        if !seen || !turns_head(class) {
            return;
        }
        let level_time = self.level_time;
        let view = self.actors[me].player.view_angles();
        let mut look = [view[0] * 0.5, view[1], view[2]];
        if class == CLASS_ATST {
            // "body pitch" (`w_saber.c:794-798`).
            self.npc_set_bone_angles(me, b"thoracic", [look[0], 0.0, look[2]]);
        }
        if let Some(target) = self.head_look_target(me) {
            look = target;
            self.actors[me].mind.looking_debounce_time = level_time + 1_000;
        }
        look[0] = 0.0;
        look[2] = 0.0;
        let mind = &mut self.actors[me].mind;
        if mind.looking_debounce_time > level_time {
            look[1] = angle_normalize180(look[1]);
            let old = mind.last_head_angles;
            if old != look {
                look[1] = old[1] + (look[1] - old[1]) * 0.4;
            }
        }
        mind.last_head_angles = look;
        if class == CLASS_ATST {
            // `w_saber.c:870-875`.
            look = [0.0, view[1], 0.0];
            let npc = &self.actors[me];
            look[1] = self.host.atst_head_yaw(npc, look[1]);
        } else {
            look[1] -= view[1];
        }
        self.npc_set_bone_angles(me, b"cranium", look);
    }

    /// `G_CheckLookTarget` (`w_saber.c:656-741`): toward the entity it looks at — a client's
    /// eyes, else where the entity stands — from its own eyes, less the angles its eyes
    /// face, each normalized (its `eyeAngles` too, which it keeps). `None` for no target, one
    /// standing at the world's origin, or one that is not there.
    fn head_look_target(&mut self, me: usize) -> Option<[f32; 3]> {
        let target = self.actors[me].mind.look_target;
        if target >= ENTITYNUM_WORLD {
            return None;
        }
        let at = match self.body(target) {
            Some(body) if body.npc => body.eye_point,
            // A player's eyes: its origin raised by its view height (`UpdateClientRenderinfo`).
            Some(body) => [
                body.origin[0],
                body.origin[1],
                body.origin[2] + body.view_height as f32,
            ],
            None => {
                let (origin, _, _) = self.host.entity_box(target)?;
                if origin == [0.0; 3] {
                    return None;
                }
                origin
            }
        };
        let mind = &mut self.actors[me].mind;
        let eyes = mind.eye_point;
        let mut look = vector_angles(std::array::from_fn(|axis| at[axis] - eyes[axis]));
        for axis in 0..3 {
            look[axis] = angle_normalize180(look[axis]);
            mind.eye_angles[axis] = angle_normalize180(mind.eye_angles[axis]);
        }
        // `AnglesSubtract`.
        Some(std::array::from_fn(|axis| {
            crate::player_angle_math::angle_subtract(look[axis], mind.eye_angles[axis])
        }))
    }
}
