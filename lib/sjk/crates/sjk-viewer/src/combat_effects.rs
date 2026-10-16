//! Legacy weapon sound and impact-effect selection kept out of the frame loop.

use glam::{Quat, Vec3};
use sjk_protocol::{EntityState, GameState};

pub(crate) fn rotation_from_direction(direction: [f32; 3]) -> Quat {
    let direction = Vec3::from_array(direction);
    let forward = if direction.length_squared() > f32::EPSILON {
        direction.normalize()
    } else {
        Vec3::Y
    };
    Quat::from_rotation_arc(Vec3::X, forward)
}

/// Borrow the built-in or current CS_EFFECTS name selected by an event.
pub(crate) fn for_event<'a>(
    event: u16,
    entity: &EntityState,
    game_state: &'a GameState,
) -> Option<&'a str> {
    if event == 68 {
        return BUILTIN_EFFECTS
            .get(usize::from(entity.event_parameter()))
            .copied()
            .flatten();
    }
    // `EV_PLAY_EFFECT_ID` and `EV_PLAY_PORTAL_EFFECT_ID` name their effect
    // through `CS_EFFECTS + eventParm` (`cg_event.c:3042-3043`). They are
    // ordinals 69 and 70 in `entityEvent_t` (`bg_public.h`); 79 and 80 are
    // `EV_ENTITY_SOUND` and `EV_PLAY_ROFF`, whose `eventParm` is a sound
    // index. Reading the effect table with a sound index both lost every
    // effect-by-id and spawned a stray effect on ordinary entity sounds.
    if matches!(event, 69 | 70) {
        const CS_EFFECTS: usize = 1_355;
        return game_state
            .config_string(CS_EFFECTS + usize::from(entity.event_parameter()))
            .and_then(|value| std::str::from_utf8(value).ok())
            .filter(|value| !value.is_empty());
    }
    sjk_client::legacy_visual_effect(event)
}

// effectTypes_t (bg_public.h:693-714), cg_event.c:2962-3038 and
// cg_main.c:1033-1041,1154-1163. Index zero deliberately has no effect.
const BUILTIN_EFFECTS: [Option<&str>; 20] = [
    None,
    Some("emplaced/dead_smoke"),
    Some("emplaced/explode"),
    Some("turret/explode"),
    Some("sparks/spark_explosion"),
    Some("tripmine/explosion"),
    Some("detpack/explosion"),
    Some("flechette/alt_blow"),
    Some("stunbaton/flesh_impact"),
    Some("demp2/altdetonate"),
    Some("turret/explode"),
    Some("sparks/spark_exp_nosnd"),
    Some("env/water_impact"),
    Some("env/acid_splash"),
    Some("env/lava_splash"),
    Some("materials/mud_large"),
    Some("materials/sand_large"),
    Some("materials/dirt_large"),
    Some("materials/snow_large"),
    Some("materials/gravel_large"),
];

/// Event 68 carries a direction; 69/70 carry Euler angles (cg_event.c:3028,3058).
pub(crate) fn event_direction(event: u16, entity: &EntityState) -> [f32; 3] {
    let angles = entity.angles();
    if matches!(event, 69 | 70) {
        let (pitch_sin, pitch_cos) = angles[0].to_radians().sin_cos();
        let (yaw_sin, yaw_cos) = angles[1].to_radians().sin_cos();
        [pitch_cos * yaw_cos, pitch_cos * yaw_sin, -pitch_sin]
    } else if matches!(event, 34 | 64 | 65) {
        // `EV_PLAYER_TELEPORT_IN/OUT` play `mp/spawn`, and `EV_BECOME_JEDIMASTER`
        // `mp/jedispawn`, along `ang = (0, 0, 1)` (`cg_event.c:2409-2430,2699-2751`):
        // straight up. The effects' emitter flies at 3,000 units a second until it hits
        // something and their beam lines are traced along this axis; sent sideways the
        // emitter crossed open maps for its full eight seconds, shedding bolts thousands
        // of units long.
        [0.0, 0.0, 1.0]
    } else if angles == [0.0; 3] {
        [0.0, 1.0, 0.0]
    } else {
        angles
    }
}
