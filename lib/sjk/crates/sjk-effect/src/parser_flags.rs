//! Flag-list parsing for Raven EFX primitive templates.

use crate::parser::Cursor;
use crate::{PrimitiveFlags, SpawnFlags};

pub(crate) fn primitive(cursor: &mut Cursor<'_>) -> PrimitiveFlags {
    let mut flags = PrimitiveFlags::default();
    while let Some(token) = cursor.peek() {
        if matches!(token.to_ascii_lowercase().as_str(), "impactfx" | "deathfx")
            && cursor.peek_after() == Some("[")
        {
            break;
        }
        let recognized = match token.to_ascii_lowercase().as_str() {
            "usealpha" => set(&mut flags.use_alpha),
            "usephysics" => {
                flags.apply_physics = true;
                set(&mut flags.physics_flag_authored)
            }
            "expensivephysics" => {
                flags.expensive_physics = true;
                set(&mut flags.expensive_physics_flag_authored)
            }
            "impactkills" | "impactkill" => set(&mut flags.kill_on_impact),
            "impactfx" => set(&mut flags.impact_runs_effect),
            "deathfx" | "death" => set(&mut flags.death_runs_effect),
            "setshadertime" => set(&mut flags.set_shader_time),
            "usemodel" => set(&mut flags.use_model),
            "usebbox" => set(&mut flags.use_bounding_box),
            "ghoul2collision" | "ghoul2trace" => {
                flags.apply_physics = true;
                flags.expensive_physics = true;
                set(&mut flags.ghoul2_collision)
            }
            "ghoul2decals" => set(&mut flags.ghoul2_decals),
            "emitfx" => set(&mut flags.emit_effect),
            "depthhack" => set(&mut flags.depth_hack),
            "relative" => set(&mut flags.relative),
            "lessattenuation" => true,
            _ => false,
        };
        if !recognized {
            break;
        }
        cursor.next();
    }
    flags
}

pub(crate) fn spawn(cursor: &mut Cursor<'_>) -> SpawnFlags {
    let mut flags = SpawnFlags::default();
    while let Some(token) = cursor.peek() {
        let recognized = match token.to_ascii_lowercase().as_str() {
            "absolutevel" => set(&mut flags.absolute_velocity),
            "absoluteaccel" => set(&mut flags.absolute_acceleration),
            "orgonsphere" => set(&mut flags.origin_on_sphere),
            "orgoncylinder" => set(&mut flags.origin_on_cylinder),
            "axisfromsphere" => set(&mut flags.axis_from_sphere),
            "randrotaroundfwd" => set(&mut flags.random_rotation_around_forward),
            "evendistribution" => set(&mut flags.even_distribution),
            "rgbcomponentinterpolation" => set(&mut flags.rgb_component_interpolation),
            "org2fromtrace" => set(&mut flags.origin2_from_trace),
            "traceimpactfx" => set(&mut flags.trace_impact_effect),
            "org2isoffset" => set(&mut flags.origin2_is_offset),
            "cheaporgcalc" => set(&mut flags.cheap_origin),
            "cheaporg2calc" => set(&mut flags.cheap_origin2),
            "affectedbywind" => set(&mut flags.affected_by_wind),
            "lessattenuation" => set(&mut flags.less_attenuation),
            _ => false,
        };
        if !recognized {
            break;
        }
        cursor.next();
    }
    flags
}

fn set(value: &mut bool) -> bool {
    *value = true;
    true
}
