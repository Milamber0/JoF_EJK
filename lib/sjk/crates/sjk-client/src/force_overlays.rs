//! BaseJKA actor-shell and body-overlay compatibility policy.
//!
//! The order and gates mirror `CG_Player` in OpenJK `codemp/cgame/cg_players.c`:
//! team-power shells at 9673-9723, then rage/sight/protect/absorb/Jedi Master/
//! Seeing/electrocution/shield-hit at 10857-11119. Event-maintained timers follow
//! `cg_event.c:2529-2537,2559-2580,3361-3367`.

use sjk_protocol::{EntityState, GameState, PlayerState};

const EF_DEAD: u32 = 1 << 1;
const FP_RAGE: u32 = 1 << 8;
const FP_PROTECT: u32 = 1 << 9;
const FP_ABSORB: u32 = 1 << 10;
const FP_SEE: u32 = 1 << 14;
const PW_SHIELDHIT: u32 = 1 << 7;
const GT_TEAM: i32 = 6;
const GT_SIEGE: i32 = 7;
const TEAM_FREE: u8 = 0;
const TEAM_RED: u8 = 1;
const TEAM_BLUE: u8 = 2;
const TEAM_SPECTATOR: u8 = 3;
const CS_SERVERINFO: usize = 0;
const CS_PLAYERS: usize = 1_131;
const MAX_OVERLAYS: usize = 9;

/// How rd-vanilla should interpret an overlay's entity colour.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LegacyOverlayTint {
    /// Supply `shaderRGBA` without forcing a generator.
    Shader,
    /// Apply `RF_RGB_TINT`.
    Rgb,
    /// Apply `RF_RGB_TINT | RF_FORCE_ENT_ALPHA`.
    Rgba,
}

/// One cgame custom-shader resubmission of a skinned actor.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyOverlayRequest {
    /// Registered shader name used as the actor-wide material override.
    pub shader: &'static str,
    /// `shaderRGBA` supplied to the overlay refEntity.
    pub rgba: [u8; 4],
    /// Whether the compatibility layer also forces RGB or RGBA generation.
    pub tint: LegacyOverlayTint,
    /// Whether this submission disables depth testing.
    pub no_depth: bool,
}

const EMPTY_REQUEST: LegacyOverlayRequest = LegacyOverlayRequest {
    shader: "",
    rgba: [255; 4],
    tint: LegacyOverlayTint::Shader,
    no_depth: false,
};

/// Fixed-capacity result in the exact `CG_Player` submission order.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LegacyOverlayList {
    entries: [LegacyOverlayRequest; MAX_OVERLAYS],
    len: usize,
}

impl Default for LegacyOverlayList {
    fn default() -> Self {
        Self {
            entries: [EMPTY_REQUEST; MAX_OVERLAYS],
            len: 0,
        }
    }
}

impl LegacyOverlayList {
    fn push(&mut self, request: LegacyOverlayRequest) {
        if let Some(slot) = self.entries.get_mut(self.len) {
            *slot = request;
            self.len += 1;
        }
    }

    /// Iterate without allocating.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = &LegacyOverlayRequest> {
        self.entries[..self.len].iter()
    }
}

/// Event-maintained team-power effect on one client entity.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyTeamPowerEffect {
    /// Cgame time at which this event-maintained effect expires.
    pub until: i32,
    /// 0 regen, 1 heal, 2 drain, 3 absorb-hit.
    pub kind: u8,
}

/// Per-actor values consumed by the pure overlay policy.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyActorOverlayState {
    /// Network entity/client number.
    pub number: u16,
    /// Active Force-power bit mask.
    pub force_powers_active: u32,
    /// Active powerup bit mask.
    pub powerups: u32,
    /// Whether the actor is dead.
    pub dead: bool,
    /// Whether `bolt1` requests the sight bubble.
    pub sight_bubble: bool,
    /// Whether this client currently owns the Jedi Master saber.
    pub jedi_master: bool,
    /// Cgame time through which the electric body effect is active.
    pub electrify_until: i32,
    /// Legacy team ordinal used to colour Force Seeing.
    pub team: u8,
    /// Alpha inherited from the base actor refEntity.
    pub base_alpha: u8,
}

impl LegacyActorOverlayState {
    /// Decode presentation-only fields from one network entity.
    pub fn from_entity(entity: &EntityState, game_state: &GameState) -> Self {
        Self {
            number: entity.number(),
            force_powers_active: entity.force_powers_active(),
            powerups: entity.powerups(),
            dead: entity.e_flags() & EF_DEAD != 0,
            sight_bubble: entity.bolt1(),
            jedi_master: entity.is_jedi_master(),
            electrify_until: entity.emplaced_owner(),
            team: client_team(game_state, entity.client_num()),
            base_alpha: entity.custom_rgba()[3],
        }
    }

    /// Decode the local player state used for the predicted actor.
    pub fn from_player(player: &PlayerState, game_state: &GameState, now: i32) -> Self {
        Self {
            number: player.client_num(),
            force_powers_active: player.force_powers_active(),
            powerups: u32::from(player.powerup_active(7, now)) << 7,
            dead: player.health() <= 0,
            sight_bubble: false,
            jedi_master: player.is_jedi_master(),
            electrify_until: player.electrify_time(),
            team: client_team(game_state, player.client_num()),
            base_alpha: player.custom_rgba()[3],
        }
    }
}

/// Frame-local values shared by every actor overlay decision.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct LegacyForceOverlayContext {
    /// Current cgame time.
    pub now: i32,
    /// Predicted/local client number.
    pub local_client: u16,
    /// Local predicted active Force-power mask.
    pub local_force_powers_active: u32,
    /// Local Force Seeing level.
    pub local_see_level: u8,
    /// Whether the local client is in a private duel.
    pub local_duel: bool,
    /// Local duel opponent, retained for cgame-compatible context.
    pub local_duel_index: u16,
    /// Local legacy team ordinal.
    pub local_team: u8,
    /// Current gametype ordinal.
    pub gametype: i32,
    /// Whether the local actor is rendered through a third-person view.
    pub third_person: bool,
    /// Runtime value of `cg_auraShell`.
    pub aura_shell: bool,
    /// Runtime value of `cg_spProtAbsColor`: protect and absorb together show as
    /// one cyan shell, as in single player, instead of a green and a blue one.
    pub combined_protect_absorb: bool,
}

impl LegacyForceOverlayContext {
    /// Build the cgame frame context from the current gamestate/playerstate.
    pub fn from_player(
        game_state: &GameState,
        player: &PlayerState,
        now: i32,
        third_person: bool,
        aura_shell: bool,
    ) -> Self {
        Self {
            now,
            local_client: player.client_num(),
            local_force_powers_active: player.force_powers_active(),
            local_see_level: player.force_see_level(),
            local_duel: player.duel_in_progress(),
            local_duel_index: player.duel_index(),
            local_team: client_team(game_state, player.client_num()),
            gametype: info_int(
                game_state.config_string(CS_SERVERINFO).unwrap_or_default(),
                "g_gametype",
            ),
            third_person,
            aura_shell,
            combined_protect_absorb: false,
        }
    }
}

/// Injected random draws preserving cgame's ordered presentation decisions.
pub trait LegacyOverlayRandom {
    /// Uniform draw in `[0, 1]` for cgame's electrocution skip.
    fn unit(&mut self) -> f32;
    /// One random bit used to alternate electric-body shaders.
    fn bit(&mut self) -> bool;
    /// Uniform-looking byte in `1..=255` for shield-hit brightness.
    fn byte_1_255(&mut self) -> u8;
}

/// Produce actor custom-shader submissions in codemp order.
pub fn legacy_force_overlays(
    actor: LegacyActorOverlayState,
    team_power: LegacyTeamPowerEffect,
    context: LegacyForceOverlayContext,
    random: &mut impl LegacyOverlayRandom,
) -> LegacyOverlayList {
    let mut output = LegacyOverlayList::default();
    let mut legs_alpha = actor.base_alpha;
    if team_power.until > context.now && team_power.kind != 3 {
        let rgb = match team_power.kind {
            1 => [0, 255, 0],
            0 => [0, 0, 255],
            _ => [255, 0, 0],
        };
        output.push(request(
            "powerups/ysalimarishell",
            [
                rgb[0],
                rgb[1],
                rgb[2],
                ((team_power.until - context.now) / 8) as u8,
            ],
            LegacyOverlayTint::Rgba,
            false,
        ));
    }
    let local = actor.number == context.local_client;
    if actor.force_powers_active & FP_RAGE != 0 && (context.third_person || !local) {
        legs_alpha = 255;
        output.push(request(
            electric_shader(random.bit()),
            [255, 0, 0, 255],
            LegacyOverlayTint::Rgb,
            false,
        ));
    }
    if !context.local_duel && actor.sight_bubble && !actor.dead && !local {
        output.push(request(
            "gfx/misc/sightbubble",
            [50, 50, 255, 255],
            LegacyOverlayTint::Shader,
            false,
        ));
    }
    // JoF EJK `CG_Player` with `cg_spprotabscolor 1` (cg_players.c, "absorb + protect is
    // represented by cyan"): the two shells merge into one cyan protect shell.
    let protecting = actor.force_powers_active & FP_PROTECT != 0;
    let team_absorb = team_power.until > context.now && team_power.kind == 3;
    let combined = context.combined_protect_absorb
        && protecting
        && (actor.force_powers_active & FP_ABSORB != 0 || team_absorb);
    if protecting && !combined {
        output.push(request(
            "gfx/misc/forceprotect",
            [0, 128, 0, 254],
            LegacyOverlayTint::Shader,
            false,
        ));
    }
    // JoF EJK `cg_alwaysShowAbsorb`: every player holding Absorb wears the shell, not only the
    // local one. The local bit is the predicted one (`CG_Player`, `cg_players.c:10953`).
    let absorbing = if local {
        context.local_force_powers_active & FP_ABSORB != 0
    } else {
        actor.force_powers_active & FP_ABSORB != 0
    };
    if (absorbing && !(context.combined_protect_absorb && protecting)) || team_absorb {
        legs_alpha = 254;
        output.push(request(
            "gfx/misc/personalshield",
            [0, 0, 255, 254],
            LegacyOverlayTint::Shader,
            false,
        ));
    }
    if combined {
        output.push(request(
            "gfx/misc/forceprotect",
            [0, 255, 255, 254],
            LegacyOverlayTint::Shader,
            false,
        ));
    }
    if actor.jedi_master && !local {
        output.push(request(
            "powerups/forceshell",
            [100, 100, 255, legs_alpha],
            LegacyOverlayTint::Shader,
            true,
        ));
    }
    if context.local_force_powers_active & FP_SEE != 0 && !local && context.aura_shell {
        let rgb = seeing_color(actor.team, context);
        // `cgs.media.sightShell` (`cg_main.c:1215`), not the Jedi Master shell.
        output.push(request(
            "powerups/sightshell",
            [rgb[0], rgb[1], rgb[2], legs_alpha],
            LegacyOverlayTint::Shader,
            context.local_see_level >= 2,
        ));
    }
    let remaining = actor.electrify_until - context.now;
    if remaining > 0 && random.unit() > 0.4 {
        legs_alpha = 255;
        let brightness = if remaining < 500 {
            ((remaining as f32 / 500.0) * 255.0).floor() as u8
        } else {
            255
        };
        output.push(request(
            electric_shader(random.bit()),
            [brightness, brightness, brightness, 255],
            LegacyOverlayTint::Rgb,
            false,
        ));
    }
    if actor.powerups & PW_SHIELDHIT != 0 {
        let grey = random.byte_1_255();
        output.push(request(
            "gfx/misc/personalshield",
            [grey, grey, grey, legs_alpha],
            LegacyOverlayTint::Shader,
            false,
        ));
    }
    output
}

fn request(
    shader: &'static str,
    rgba: [u8; 4],
    tint: LegacyOverlayTint,
    no_depth: bool,
) -> LegacyOverlayRequest {
    LegacyOverlayRequest {
        shader,
        rgba,
        tint,
        no_depth,
    }
}

fn electric_shader(second: bool) -> &'static str {
    if second {
        "gfx/misc/electric"
    } else {
        "gfx/misc/fullbodyelectric2"
    }
}

fn seeing_color(team: u8, context: LegacyForceOverlayContext) -> [u8; 3] {
    if context.gametype == GT_SIEGE {
        if matches!(team, TEAM_FREE | TEAM_SPECTATOR) {
            [255, 255, 0]
        } else if team != context.local_team {
            [255, 50, 50]
        } else {
            [50, 255, 50]
        }
    } else if context.gametype >= GT_TEAM {
        match team {
            TEAM_RED => [255, 50, 50],
            TEAM_BLUE => [75, 75, 255],
            _ => [255, 255, 0],
        }
    } else {
        [255, 255, 0]
    }
}

fn client_team(game_state: &GameState, client: u16) -> u8 {
    game_state
        .config_string(CS_PLAYERS + usize::from(client))
        .map_or(0, |info| info_int(info, "t") as u8)
}

fn info_int(info: &[u8], key: &str) -> i32 {
    let mut fields = info
        .split(|byte| *byte == b'\\')
        .filter(|field| !field.is_empty());
    while let (Some(candidate), Some(value)) = (fields.next(), fields.next()) {
        if candidate.eq_ignore_ascii_case(key.as_bytes()) {
            return crate::team_info::legacy_atoi(value);
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    struct NoRandom;

    impl LegacyOverlayRandom for NoRandom {
        fn unit(&mut self) -> f32 {
            0.0
        }
        fn bit(&mut self) -> bool {
            false
        }
        fn byte_1_255(&mut self) -> u8 {
            1
        }
    }

    fn shells(active: u32, local: bool, combined: bool) -> Vec<(&'static str, [u8; 4])> {
        let actor = LegacyActorOverlayState {
            number: 1,
            force_powers_active: active,
            base_alpha: 255,
            ..LegacyActorOverlayState::default()
        };
        let context = LegacyForceOverlayContext {
            local_client: if local { 1 } else { 0 },
            local_force_powers_active: if local { active } else { 0 },
            combined_protect_absorb: combined,
            ..LegacyForceOverlayContext::default()
        };
        legacy_force_overlays(
            actor,
            LegacyTeamPowerEffect::default(),
            context,
            &mut NoRandom,
        )
        .iter()
        .map(|request| (request.shader, request.rgba))
        .collect()
    }

    #[test]
    fn protect_and_absorb_together_are_one_cyan_shell_when_combined() {
        let both = FP_PROTECT | FP_ABSORB;
        assert_eq!(
            shells(both, true, true),
            [("gfx/misc/forceprotect", [0, 255, 255, 254])]
        );
        // Another player's absorb counts through the entity's own bit.
        assert_eq!(
            shells(both, false, true),
            [("gfx/misc/forceprotect", [0, 255, 255, 254])]
        );
    }

    #[test]
    fn without_the_combo_protect_is_green_and_the_local_absorb_blue() {
        let both = FP_PROTECT | FP_ABSORB;
        assert_eq!(
            shells(both, true, false),
            [
                ("gfx/misc/forceprotect", [0, 128, 0, 254]),
                ("gfx/misc/personalshield", [0, 0, 255, 254]),
            ]
        );
        // Another player's absorb is drawn too (EJK `cg_alwaysShowAbsorb`).
        assert_eq!(
            shells(both, false, false),
            [
                ("gfx/misc/forceprotect", [0, 128, 0, 254]),
                ("gfx/misc/personalshield", [0, 0, 255, 254]),
            ]
        );
    }

    #[test]
    fn a_single_power_keeps_its_own_shell_with_the_combo_on() {
        assert_eq!(
            shells(FP_PROTECT, true, true),
            [("gfx/misc/forceprotect", [0, 128, 0, 254])]
        );
        assert_eq!(
            shells(FP_ABSORB, true, true),
            [("gfx/misc/personalshield", [0, 0, 255, 254])]
        );
        assert_eq!(
            shells(FP_ABSORB, false, true),
            [("gfx/misc/personalshield", [0, 0, 255, 254])]
        );
    }
}
