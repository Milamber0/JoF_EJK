//! `CG_PlayerSprites` / `CG_PlayerFloatSprite` (`codemp/cgame/cg_players.c`): the
//! connection icon or talk balloon over a player's head, as a frame billboard in the
//! effect pool that `pickups::simple::append_frame` clears on the next frame. It
//! uses the pool's billboard reserve ([`crate::particle_types::FRAME_BILLBOARD_RESERVE`]),
//! so effects filling the pool do not hide it.
use super::*;
use crate::particle_types::PrimitiveShape;

/// `ET_PLAYER`: the local player is drawn from its own (predicted) player state.
const ET_PLAYER: u8 = 1;

/// `RT_SPRITE`: a camera-facing frame billboard. Its image is upright, v=0 at the
/// top of the quad (`effect_submission::billboard_uv_transform`).
const SHAPE: PrimitiveShape = PrimitiveShape::FrameBillboard;

/// Float the stock sprite over one submitted actor, without growing the pool.
#[allow(clippy::too_many_arguments)]
pub(super) fn submit(
    sinks: &mut Sinks<'_>,
    kind: EntityKind,
    origin: [f32; 3],
    snapshot: &Snapshot,
    state: Option<&sjk_protocol::EntityState>,
    local: bool,
    draw_actor: bool,
    now: Instant,
) {
    // The local first-person model is `RF_THIRD_PERSON`, and so is its sprite; no
    // player is drawn during intermission (`CG_Player`), sprites included.
    if !draw_actor
        || kind != EntityKind::Actor
        || snapshot.player.movement_type() == sjk_client::PM_INTERMISSION
    {
        return;
    }
    let (flags, entity_type, mind_tricked) = if local {
        let flags = sinks
            .predicted_local_state
            .map_or(snapshot.player.entity_flags(), |state| state.entity_flags);
        (flags, ET_PLAYER, false)
    } else if let Some(state) = state {
        (
            state.e_flags(),
            state.entity_type(),
            state.client_bitflag(snapshot.player.client_num()),
        )
    } else {
        return;
    };
    let Some(sprite) = sjk_client::legacy_player_sprite(flags, entity_type, mind_tricked) else {
        return;
    };
    if !crate::particle_types::frame_billboard_fits(sinks.particles.len()) {
        return;
    }
    let constant = |value| {
        crate::effect_envelope::Envelope::from_values(
            value,
            value,
            0.0,
            sjk_effect::CurveFlags::default(),
        )
    };
    let origin = Vec3::from_array(origin) + Vec3::Z * sjk_client::LEGACY_PLAYER_SPRITE_HEIGHT;
    sinks.particles.push(Particle {
        motion: crate::particle_motion::Motion::new(origin, Vec3::ZERO, Vec3::ZERO, 0.0, 0.0, now),
        spawned_at: now,
        delay: Duration::ZERO,
        lifetime: Duration::from_secs(60),
        size: constant(sjk_client::LEGACY_PLAYER_SPRITE_RADIUS),
        start_length: 1.0,
        end_length: 1.0,
        streak: None,
        trace_streak: false,
        normal: None,
        alpha: constant(1.0),
        use_alpha: true,
        set_shader_time: false,
        rgb: [constant(1.0); 3],
        seed: 0,
        shader: sinks.effects.shader(sprite.shader()),
        physics: crate::particle_physics::State::new(
            sinks.effects.code_primitive_definition(),
            0,
            0,
            0.0,
            sjk_effect::PrimitiveFlags::default(),
        ),
        shape: SHAPE,
    });
}
