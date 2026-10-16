//! Pure crosshair-name policy from `codemp/cgame/cg_draw.c:6080-6360`.

/// Trace/presentation facts needed to decide whether a name is visible.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct CrosshairCandidate {
    /// Candidate client number from the trace.
    pub client_num: u16,
    /// Local predicted client number.
    pub local_client: u16,
    /// Local legacy team ordinal.
    pub local_team: u8,
    /// Candidate legacy team ordinal.
    pub target_team: u8,
    /// Current presentation server time.
    pub now: i32,
    /// Server time at which this target was last acquired.
    pub acquired_at: i32,
    /// Whether the local player is a spectator.
    pub spectator: bool,
    /// Whether the presentation is in intermission.
    pub intermission: bool,
    /// Whether the scoreboard obscures HUD identification.
    pub scoreboard: bool,
    /// Whether the trace ended in fog.
    pub in_fog: bool,
    /// Whether the candidate carries `PW_CLOAKED`.
    pub cloaked: bool,
    /// Whether the candidate's bitfields hide it from this local client.
    pub mind_tricked: bool,
    /// Whether the candidate carries `EF_DEAD`.
    pub dead: bool,
}

/// Visible name target and codemp's one-second linear fade alpha.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CrosshairName {
    /// Client whose name should be resolved from `CS_PLAYERS`.
    pub client_num: u16,
    /// Whether team-game presentation should use the friendly colour.
    pub teammate: bool,
    /// One-second fade alpha.
    pub alpha: f32,
}

/// Apply crosshair exclusions and `CG_FadeColor(..., 1000)`.
///
/// NPCs are traced like players but never named: `CG_DrawCrosshairNames`
/// returns once `crosshairClientNum >= MAX_CLIENTS` (`cg_draw.c:6331`).
pub fn crosshair_name(candidate: CrosshairCandidate) -> Option<CrosshairName> {
    if candidate.client_num >= 32
        || candidate.client_num == candidate.local_client
        || candidate.spectator
        || candidate.intermission
        || candidate.scoreboard
        || candidate.in_fog
        || candidate.cloaked
        || candidate.mind_tricked
        || candidate.dead
    {
        return None;
    }
    let age = candidate.now.wrapping_sub(candidate.acquired_at);
    if !(0..NAME_FADE_MS).contains(&age) {
        return None;
    }
    // `CG_FadeColor` (`cg_drawtools.c:395-420`): fully opaque until the last
    // `FADE_TIME` (200 ms, `cg_local.h:40`), then linear to zero.
    let remaining = NAME_FADE_MS - age;
    let alpha = if remaining < FADE_TIME_MS {
        remaining as f32 / FADE_TIME_MS as f32
    } else {
        1.0
    };
    Some(CrosshairName {
        client_num: candidate.client_num,
        teammate: candidate.local_team >= 1 && candidate.local_team == candidate.target_team,
        alpha,
    })
}

/// `CG_DrawCrosshairNames` passes 1000 ms to `CG_FadeColor` (`cg_draw.c:6297`).
const NAME_FADE_MS: i32 = 1_000;
/// `FADE_TIME` from `cg_local.h:40`.
const FADE_TIME_MS: i32 = 200;
