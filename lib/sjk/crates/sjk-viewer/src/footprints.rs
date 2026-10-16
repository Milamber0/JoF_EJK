//! What a step leaves on the ground (`_PlayerFootStep`, `codemp/cgame/cg_players.c:2889-3086`):
//! under the foot, a 14x14x2 box traced 32 units down finds the ground (none in water,
//! where the reference returns first); its material picks the step's sound bank
//! ([`sjk_client::animation_events::footsteps`]) and, on mud, dirt, sand and snow, a
//! puff of dust (`materials/*`) and a footprint (`footstep_*`, radius 6, turned with
//! the legs) that lasts ten seconds (`CG_ImpactMark`, a lasting cgame mark). Gravel
//! kicks up dust and leaves no print.
//!
//! Retail ties the dust to `cg_footsteps 2` and the prints to `3`, its default. SJK
//! saved its own `cg_footsteps` as an on/off `1`, which retail reads as sounds alone, so
//! the dust and prints have their own switch, [`PRINTS_CVAR`] (on), and
//! `cg_footsteps 0` still silences and hides everything. Two SJK additions have theirs:
//! [`GUESS_CVAR`], ground the map left untagged taking its material from its texture's
//! name, and [`JUMP_CVAR`], the jump and forward-landing steps
//! ([`sjk_client::animation_events::footsteps::JUMP_LAND_STEPS`]).

use crate::console::ViewerConsole;
use crate::decal_marks::DecalRequest;
use crate::decal_store::DecalStore;
use glam::Vec3;
use sjk_bsp::{Aabb, Bsp, TraceScratch};
use sjk_client::animation_events::footsteps;
use std::sync::Arc;

/// The dust and the prints of a step on mud, dirt, sand and snow (retail `cg_footsteps 3`).
pub(crate) const PRINTS_CVAR: &str = "cg_footprints";
/// SJK's steps at a jump's takeoff and a forward landing.
pub(crate) const JUMP_CVAR: &str = "cg_jumpFootsteps";
/// SJK's guess of an untagged ground's material from its texture name.
pub(crate) const GUESS_CVAR: &str = "cg_footstepGuess";

/// `FOOTSTEP_DISTANCE` (`cg_players.c:2888`).
const DISTANCE: f32 = 32.0;
/// `MASK_PLAYERSOLID` (`bg_public.h`): solid, player clip, body, terrain.
const MASK_PLAYERSOLID: u32 = 0x0000_0001 | 0x0000_0010 | 0x0000_0100 | 0x0000_1000;
/// `MASK_WATER`: water, lava, slime.
const MASK_WATER: u32 = 0x0000_0004 | 0x0000_0002 | 0x0002_0000;
/// The radius `CG_PlayerFootsteps` gives the print.
const PRINT_RADIUS: f32 = 6.0;

/// The footstep switches, read once a frame.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Settings {
    /// `cg_footsteps`: steps at all.
    pub(crate) steps: bool,
    /// Dust and prints.
    pub(crate) prints: bool,
    pub(crate) jump: bool,
    pub(crate) guess: bool,
}

impl Settings {
    pub(crate) fn read(console: Option<&ViewerConsole>) -> Self {
        let on = |name| {
            console
                .and_then(|console| console.bool_cvar(name))
                .unwrap_or(true)
        };
        let steps = on("cg_footsteps");
        Self {
            steps,
            prints: steps && on(PRINTS_CVAR),
            jump: on(JUMP_CVAR),
            guess: on(GUESS_CVAR),
        }
    }
}

/// The ground under a foot: its sound family, where the trace met it and its normal.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Ground {
    pub(crate) family: usize,
    pub(crate) point: Vec3,
    pub(crate) normal: Vec3,
}

/// `_PlayerFootStep`'s two traces from `start` (the foot, 15 units up): nothing in
/// water, nothing more than 32 units above the ground.
pub(crate) fn ground(
    bsp: &Bsp,
    scratch: &mut TraceScratch,
    start: Vec3,
    guess: bool,
) -> Option<Ground> {
    let bounds = Aabb::new([-7.0, -7.0, 0.0], [7.0, 7.0, 2.0]).ok()?;
    let end = (start - Vec3::Z * DISTANCE).to_array();
    if bsp
        .trace_box_with(scratch, start.to_array(), end, bounds, MASK_WATER)
        .fraction
        < 1.0
    {
        return None;
    }
    let trace = bsp.trace_box_with(scratch, start.to_array(), end, bounds, MASK_PLAYERSOLID);
    if trace.fraction >= 1.0 {
        return None;
    }
    let family = if trace.surface_flags & 31 == 0 {
        // Ground the map gives no material: stone, or guessed from its shader.
        trace
            .shader
            .filter(|_| guess)
            .and_then(|index| bsp.shaders().get(index))
            .and_then(|shader| footsteps::material_from_name(&shader.name_lossy()))
            .unwrap_or(0)
    } else {
        footsteps::material(trace.surface_flags)
    };
    Some(Ground {
        family,
        point: Vec3::from_array(trace.end_position),
        normal: Vec3::from_array(trace.plane.map_or([0.0, 0.0, 1.0], |plane| plane.normal)),
    })
}

/// The print a step leaves on `ground`, if its material takes one: the lasting mark
/// `CG_ImpactMark` makes, turned `yaw` degrees (the legs').
pub(crate) fn print(ground: &Ground, right: bool, heavy: bool, yaw: f32) -> Option<DecalRequest> {
    let (_, mark) = footsteps::trace_of(ground.family)?;
    // `CG_ImpactMark` takes no plane-less trace.
    (mark && ground.normal != Vec3::ZERO).then(|| DecalRequest {
        origin: ground.point,
        direction: ground.normal,
        orientation: yaw,
        color: [1.0; 4],
        radius: PRINT_RADIUS,
        shader: Arc::from(footsteps::print_shader(right, heavy)),
    })
}

/// Queue the print of a step, if any.
pub(crate) fn leave_print(
    decals: &mut DecalStore,
    ground: &Ground,
    right: bool,
    heavy: bool,
    yaw: f32,
) {
    if let Some(request) = print(ground, right, heavy, yaw) {
        decals.request_mark(request);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ground(family: usize) -> Ground {
        Ground {
            family,
            point: Vec3::new(1.0, 2.0, 3.0),
            normal: Vec3::Z,
        }
    }

    #[test]
    fn sand_takes_a_print_turned_with_the_legs_stone_and_gravel_none() {
        let sand = print(&ground(3), true, false, 90.0).expect("a sand print");
        assert_eq!(&*sand.shader, "footstep_r");
        assert_eq!(sand.radius, 6.0);
        assert_eq!(sand.orientation, 90.0);
        assert_eq!(sand.origin, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(
            &*print(&ground(4), false, true, 0.0).unwrap().shader,
            "footstep_heavy_l"
        );
        assert!(print(&ground(0), true, false, 0.0).is_none());
        assert!(print(&ground(8), true, false, 0.0).is_none());
    }

    #[test]
    fn every_switch_is_on_without_a_console() {
        assert_eq!(
            Settings::read(None),
            Settings {
                steps: true,
                prints: true,
                jump: true,
                guess: true
            }
        );
    }
}
