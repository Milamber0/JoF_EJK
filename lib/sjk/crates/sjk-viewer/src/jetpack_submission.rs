//! Bolt a player's jetpack on, burn its jets and sound its start and stop
//! (`CG_Player`, `codemp/cgame/cg_players.c`, JoF EJK 11843-11935); see `jetpack.rs`.

use super::*;

/// What the actor submission knows about the body this frame.
pub(super) struct Body {
    pub(super) mesh: usize,
    pub(super) number: u16,
    pub(super) transform: sjk_runtime::Transform,
    pub(super) e_flags: u32,
    /// `CG_IsMindTricked` for the viewer: the pack draws no flames and makes no sound.
    pub(super) tricked: bool,
    /// The body is hidden by a mind trick, or fading with this alpha.
    pub(super) hidden: bool,
    pub(super) fade: Option<u8>,
    pub(super) draw_actor: bool,
    pub(super) muted: bool,
}

/// Submit one actor's pack, flames and cues.
pub(super) fn submit(sinks: &mut Sinks<'_>, body: Body, time: i32, now: Instant) {
    let jetpack = sjk_client::LegacyJetpack::from_flags(body.e_flags);
    let meshes = sinks.actor_meshes;
    let actor = &meshes[body.mesh];
    let chest = actor.jetpack.attachment(jetpack.worn, || {
        actor.animator.requested.and_then(|_| {
            saber::attachment_from_matrices(&actor.preview, actor.animator.matrices(), "*chestg")
        })
    });
    let cue =
        sinks
            .jetpack_sounds
            .advance(body.number, jetpack.active, !body.tricked && !body.muted);
    let origin = Vec3::from_array(body.transform.translation);
    if let Some(cue) = cue
        && let Some(audio) = sinks.game_audio.as_mut()
    {
        // `CHAN_LOCAL` at the player's origin.
        audio.play_on_channel(
            cue.path(),
            1.0,
            origin.to_array(),
            sjk_audio::SourceId(u32::from(body.number)),
            sjk_audio::ChannelId(1),
        );
    }
    let Some(chest) = chest else {
        return;
    };
    let rotation = weapon_view::actor_world_rotation(body.transform.rotation);
    let (pack_origin, pack_rotation) = saber::world_attachment(origin, rotation, chest);
    if let Some(mesh) = sinks.jetpack.mesh
        && !body.hidden
    {
        let mut instance =
            ActorInstance::new(pack_origin.to_array(), pack_rotation.to_array(), [1.0; 3]);
        instance.view_flags = sinks.entity_view_flags | u32::from(!body.draw_actor);
        match body.fade {
            // A bolt-on of the body's Ghoul2 instance fades with it.
            Some(alpha) if sinks.overrides.len() < sinks.overrides.capacity() => {
                sinks.overrides.push(entity_materials::OverrideInstance {
                    mesh: entity_materials::OverrideMesh::Object(mesh),
                    material: None,
                    instance: instance.with_forced_alpha(alpha),
                    no_depth: false,
                    forced_alpha: true,
                });
            }
            Some(_) => {}
            None => sinks.object_groups[mesh].push(instance),
        }
    }
    // Re-played on the reference 8 ms cadence, not per rendered frame (`effect_cadence.rs`).
    if !jetpack.active || body.tricked || !sinks.effect_aux.continuous.due(now) {
        return;
    }
    let plays = if jetpack.flaming { 2 } else { 1 };
    for (jet, flame) in sinks
        .jetpack
        .flames(pack_origin, pack_rotation)
        .into_iter()
        .enumerate()
    {
        let Some((position, direction)) = flame else {
            continue;
        };
        for play in 0..plays {
            effect_runtime::spawn_effect(
                sinks.particles,
                sinks.effect_aux,
                sinks.effects,
                sinks.vfs,
                crate::jetpack::EFFECT,
                position,
                now,
                u32::from(body.number) ^ (time as u32).rotate_left(9) ^ (jet as u32 * 2 + play),
                0,
                sinks.game_audio,
                combat_effects::rotation_from_direction(direction.to_array()),
            );
        }
    }
}
