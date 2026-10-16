//! The rope of a JA+ grapple hook, as EternalJK's `CG_Missile` draws it for JA+
//! servers (`codemp/cgame/cg_ents.c:2771-2795`).
//!
//! JA+'s hook is an `ET_MISSILE` with `WP_STUN_BATON` whose `otherEntityNum` is the
//! hooked player. Instead of a missile model, cgame draws `CG_TestLine(rHandPos, pos,
//! 1, 6, 1)` each frame: a one-unit-wide `RT_LINE` with the `white` shader from the
//! player's right hand to where the hook is at presentation time. Colour 6 is not a
//! saber colour, so `CGDEBUG_SaberColor` passes it through as RGB (6, 0, 0): a nearly
//! black rope. No rope is drawn while the local player duels. The hook model of the
//! JA+ client plugin (`cp_hookModel`) is not drawn, as EternalJK does not draw it.
//!
//! Hooks are gathered once per frame into a fixed table, so each actor only scans
//! the few hooks rather than every snapshot entity; the rope is a frame billboard
//! streak, which the next frame's `pickups::simple::append_frame` clears.
use super::*;
use crate::particle_types::PrimitiveShape;

/// `ET_MISSILE`.
const ET_MISSILE: u8 = 3;
/// `WP_STUN_BATON`: JA+ fires its hook as this weapon's missile.
const WP_STUN_BATON: u8 = 1;
/// More hooks than a full server of players can fire at once.
const MAX_HOOKS: usize = 64;
/// `RT_LINE` half-width: `CG_TestLine`'s radius 1 halved (`cg_effects.c:200`).
const HALF_WIDTH: f32 = 0.5;
/// `CG_TestLine` colour 6 through `CGDEBUG_SaberColor` (`cg_effects.c:156-219`).
const COLOR: [f32; 3] = [6.0 / 255.0, 0.0, 0.0];
/// `cgs.media.whiteShader`.
const SHADER: &str = "white";

/// This frame's JA+ hooks: the hooked client and the hook's presented position.
#[derive(Clone, Copy)]
pub(super) struct Hooks {
    entries: [(u16, [f32; 3]); MAX_HOOKS],
    len: usize,
}

impl Hooks {
    pub(super) const EMPTY: Self = Self {
        entries: [(0, [0.0; 3]); MAX_HOOKS],
        len: 0,
    };

    /// Gather the hooks of a JA+ server's `snapshot` at `presentation_time`; none on
    /// other servers or while the local player duels ("Don't show grapple in duels").
    pub(super) fn collect(
        snapshot: Option<&Snapshot>,
        game_state: Option<&GameState>,
        presentation_time: i32,
    ) -> Self {
        let mut hooks = Self::EMPTY;
        let ja_plus = game_state
            .and_then(|game| game.config_string(0))
            .and_then(|info| sjk_client::LegacyClientInfo::new(info).text("gamename"))
            .is_some_and(sjk_protocol::is_ja_plus_game_name);
        let Some(snapshot) =
            snapshot.filter(|snapshot| ja_plus && !snapshot.player.duel_in_progress())
        else {
            return hooks;
        };
        for state in snapshot
            .entities
            .iter()
            .filter(|state| state.entity_type() == ET_MISSILE && state.weapon() == WP_STUN_BATON)
        {
            if hooks.len == MAX_HOOKS {
                break;
            }
            let position = sjk_client::legacy_evaluate_trajectory(
                state.trajectory_base(),
                state.trajectory_delta(),
                state.trajectory_type(),
                state.trajectory_time(),
                state.trajectory_duration(),
                presentation_time,
            );
            hooks.entries[hooks.len] = (state.other_entity_num(), position);
            hooks.len += 1;
        }
        hooks
    }

    fn of(&self, client: u16) -> impl Iterator<Item = [f32; 3]> + '_ {
        self.entries[..self.len]
            .iter()
            .filter(move |(hooked, _)| *hooked == client)
            .map(|(_, position)| *position)
    }
}

/// Draw the ropes of `client`'s hooks from its right `hand`.
pub(super) fn submit(sinks: &mut Sinks<'_>, client: u16, hand: Vec3, now: Instant) {
    let constant = |value| {
        crate::effect_envelope::Envelope::from_values(
            value,
            value,
            0.0,
            sjk_effect::CurveFlags::default(),
        )
    };
    let hooks = sinks.hooks;
    for hook in hooks.of(client) {
        if !crate::particle_types::frame_billboard_fits(sinks.particles.len()) {
            return;
        }
        sinks.particles.push(Particle {
            motion: crate::particle_motion::Motion::new(
                hand,
                Vec3::ZERO,
                Vec3::ZERO,
                0.0,
                0.0,
                now,
            ),
            spawned_at: now,
            delay: Duration::ZERO,
            lifetime: Duration::from_secs(60),
            size: constant(HALF_WIDTH),
            start_length: 1.0,
            end_length: 1.0,
            streak: Some(Vec3::from_array(hook) - hand),
            trace_streak: false,
            normal: None,
            alpha: constant(1.0),
            use_alpha: true,
            set_shader_time: false,
            rgb: COLOR.map(constant),
            seed: 0,
            shader: sinks.effects.shader(SHADER),
            physics: crate::particle_physics::State::new(
                sinks.effects.code_primitive_definition(),
                0,
                0,
                0.0,
                sjk_effect::PrimitiveFlags::default(),
            ),
            shape: PrimitiveShape::FrameBillboard,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_player_gets_only_its_own_hooks() {
        let mut hooks = Hooks::EMPTY;
        for (index, client) in [3_u16, 7, 3].into_iter().enumerate() {
            hooks.entries[index] = (client, [index as f32; 3]);
        }
        hooks.len = 3;
        assert_eq!(hooks.of(3).collect::<Vec<_>>(), [[0.0; 3], [2.0; 3]]);
        assert_eq!(hooks.of(7).collect::<Vec<_>>(), [[1.0; 3]]);
        assert_eq!(hooks.of(0).count(), 0);
    }

    #[test]
    fn non_ja_plus_servers_and_missing_snapshots_have_no_hooks() {
        assert_eq!(Hooks::collect(None, None, 0).len, 0);
    }
}
