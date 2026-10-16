//! Pure readout maths for the ground HUD: which numbers it shows, in which
//! colour, the stance dot's colour, and whether the ground HUD or the normal
//! status HUD is shown this frame.
//!
//! Colours are display (sRGB) values; the renderer converts them for its target.

/// Codemp `pmtype_t::PM_DEAD` (`bg_public.h`).
const PM_DEAD: u8 = 5;
/// Codemp `pmtype_t::PM_INTERMISSION` and `PM_SPINTERMISSION`.
const PM_INTERMISSION: [u8; 2] = [7, 8];
/// Codemp `WP_SABER`.
const WEAPON_SABER: u8 = 3;
/// Stock `CG_DrawForcePower` scale (`cg_draw.c`, `maxForcePower = 100`); the
/// protocol does not carry `fd.forcePowerMax`, so the Force colour ramps
/// against it.
pub(crate) const STOCK_FORCE_MAX: i32 = 100;
/// Largest value a ground-HUD number shows; three digits always fit the layout.
pub(crate) const DISPLAY_MAX: i32 = 999;

/// Values one ground-HUD frame reads, already chosen from prediction or snapshot.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Readings {
    /// `STAT_HEALTH`; may be negative once dead.
    pub(crate) health: i32,
    /// `STAT_ARMOR` (shield).
    pub(crate) armor: i32,
    /// `STAT_MAX_HEALTH`; stock also caps shield there (`cg_draw.c` `CG_DrawArmor`).
    pub(crate) max_health: i32,
    /// `fd.forcePower`.
    pub(crate) force: i32,
    /// `fd.saberDrawAnimLevel` while the saber is the held weapon, else `None`.
    pub(crate) stance: Option<u8>,
}

impl Readings {
    /// Project raw player values; the stance only counts while the saber is held.
    pub(crate) fn new(
        health: i32,
        armor: i32,
        max_health: i32,
        force: i32,
        weapon: u8,
        draw_style: u8,
    ) -> Self {
        Self {
            health,
            armor,
            max_health,
            force,
            stance: (weapon == WEAPON_SABER).then_some(draw_style),
        }
    }
}

/// Health plus shield as one number: each part clamped to `0..=STAT_MAX_HEALTH`
/// (stock caps shield there, `cg_draw.c` `CG_DrawArmor`, and a decaying
/// overheal counts as full), so stock tops out at 200.
pub(crate) fn combined(readings: Readings) -> i32 {
    let maximum = readings.max_health.max(1);
    (readings.health.clamp(0, maximum) + readings.armor.clamp(0, maximum)).min(DISPLAY_MAX)
}

/// Fraction of the combined maximum (`2 * STAT_MAX_HEALTH`) the number shows.
pub(crate) fn combined_fraction(readings: Readings) -> f32 {
    combined(readings) as f32 / (2 * readings.max_health.max(1)) as f32
}

/// The Force number: `fd.forcePower`, never negative. A mod that grants more
/// than the stock 100 shows its real value; its colour stays at "full".
pub(crate) fn force_value(readings: Readings) -> i32 {
    readings.force.clamp(0, DISPLAY_MAX)
}

/// Fraction of the stock Force maximum the number shows, for its colour.
pub(crate) fn force_fraction(readings: Readings) -> f32 {
    force_value(readings) as f32 / STOCK_FORCE_MAX as f32
}

/// A non-negative number as up to three ASCII digits, formatted without allocating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Digits {
    bytes: [u8; 3],
    len: u8,
}

impl Digits {
    /// Format `value`, clamped to `0..=DISPLAY_MAX`, without leading zeros.
    pub(crate) fn new(value: i32) -> Self {
        let mut rest = value.clamp(0, DISPLAY_MAX);
        let len = if rest >= 100 {
            3
        } else if rest >= 10 {
            2
        } else {
            1
        };
        let mut bytes = [0; 3];
        for byte in bytes[..len].iter_mut().rev() {
            *byte = b'0' + (rest % 10) as u8;
            rest /= 10;
        }
        Self {
            bytes,
            len: len as u8,
        }
    }

    /// The digits, most significant first.
    pub(crate) fn as_bytes(&self) -> &[u8] {
        &self.bytes[..usize::from(self.len)]
    }
}

/// Linear interpolation through sorted `(position, colour)` stops.
fn ramp(stops: &[(f32, [f32; 3])], t: f32) -> [f32; 3] {
    let t = t.clamp(0.0, 1.0);
    let mut previous = stops[0];
    for &stop in stops {
        if t <= stop.0 {
            let width = (stop.0 - previous.0).max(f32::EPSILON);
            let k = ((t - previous.0) / width).clamp(0.0, 1.0);
            return std::array::from_fn(|i| previous.1[i] + (stop.1[i] - previous.1[i]) * k);
        }
        previous = stop;
    }
    previous.1
}

/// Health colour (526k's ramp): green while healthy, through amber and
/// orange, to red at 0. The combined number uses it over [`combined_fraction`].
pub(crate) fn health_color(fraction: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 5] = [
        (0.0, [1.00, 0.16, 0.14]),
        (0.25, [1.00, 0.46, 0.12]),
        (0.5, [1.00, 0.80, 0.22]),
        (0.8, [0.36, 0.95, 0.46]),
        (1.0, [0.30, 0.95, 0.50]),
    ];
    ramp(&STOPS, fraction)
}

/// Force colour (526k's ramp): azure when full, violet at half, a hot magenta
/// near empty. The Force number uses it over [`force_fraction`].
pub(crate) fn force_color(fraction: f32) -> [f32; 3] {
    const STOPS: [(f32, [f32; 3]); 4] = [
        (0.0, [1.00, 0.22, 0.42]),
        (0.3, [0.74, 0.36, 1.00]),
        (0.7, [0.46, 0.56, 1.00]),
        (1.0, [0.36, 0.70, 1.00]),
    ];
    ramp(&STOPS, fraction)
}

/// Dot colour of codemp `saber_styles_t`. The three stock stances follow the
/// HUD color scheme (fast blue, medium yellow, strong red); the variants take a
/// neighbour of their parent's colour so the dot still reads at a glance.
pub(crate) fn stance_color(style: u8) -> [f32; 3] {
    match style {
        1 => [0.25, 0.55, 1.00], // SS_FAST
        2 => [1.00, 0.86, 0.20], // SS_MEDIUM
        3 => [1.00, 0.20, 0.16], // SS_STRONG
        4 => [1.00, 0.52, 0.12], // SS_DESANN: a heavy variant, strong's orange neighbour
        5 => [0.30, 0.92, 1.00], // SS_TAVION: a quick variant, fast's cyan neighbour
        6 => [0.78, 0.40, 1.00], // SS_DUAL: violet, its own family
        7 => [0.30, 0.95, 0.50], // SS_STAFF: green, its own family
        _ => [0.92, 0.92, 0.92], // unknown styles stay neutral
    }
}

/// Which status presentation a frame uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Presentation {
    /// Normal HUD status widgets; no ground HUD.
    Normal,
    /// Ground HUD drawn; the normal HUD's health, shield, Force and stance hide.
    Ground,
}

/// Everything [`select`] needs, gathered once per frame.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct Conditions {
    /// `cg_groundHud`.
    pub(crate) enabled: bool,
    /// Third-person view of the local player (not detached, not a backdrop).
    pub(crate) third_person: bool,
    /// Normal HUD status would be drawn (`cg_draw2D`, `cg_drawHud`, `cg_drawStatus`, no menu).
    pub(crate) status_visible: bool,
    /// A live or demo session is presenting a world.
    pub(crate) in_game: bool,
    /// An intermission view is up.
    pub(crate) intermission: bool,
    /// The player state's `pm_type`.
    pub(crate) movement_type: u8,
    /// Spectator team or `PM_SPECTATOR`.
    pub(crate) spectator: bool,
    /// `PMF_FOLLOW`: the player state is someone else's.
    pub(crate) following: bool,
    /// Current `STAT_HEALTH`.
    pub(crate) health: i32,
}

/// Ground HUD only for an alive, in-game local player seen in third person.
/// Following someone hides it: the view then belongs to the followed player,
/// and the normal HUD already shows their values.
pub(crate) fn select(conditions: Conditions) -> Presentation {
    let alive = conditions.health > 0 && conditions.movement_type != PM_DEAD;
    let playing = conditions.in_game
        && !conditions.intermission
        && !PM_INTERMISSION.contains(&conditions.movement_type)
        && !conditions.spectator
        && !conditions.following;
    if conditions.enabled
        && conditions.third_person
        && conditions.status_visible
        && playing
        && alive
    {
        Presentation::Ground
    } else {
        Presentation::Normal
    }
}
