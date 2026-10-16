//! Nameplate maths: how large, how opaque and where a plate sits, and the
//! colours and rectangles of its bars. Pure functions, so the layout is
//! testable without a window; [`super::nameplate`] collects the players and
//! draws with these.
use sjk_ui::{Color, Rect};

/// Distance, in world units, out to which a plate keeps its full size.
const FULL_SIZE_DISTANCE: f32 = 300.0;
/// Share of the range, from the camera, over which a plate is fully opaque.
const OPAQUE_SHARE: f32 = 0.75;
/// Units above the top of the player's box at which the plate is anchored.
pub(super) const HEAD_CLEARANCE: f32 = 8.0;
/// Standing box top above the origin, used when the entity sends none.
const STANDING_TOP: f32 = 40.0;
/// Milliseconds a plate takes to fade between hidden and shown.
pub(super) const FADE_MILLIS: f32 = 120.0;
/// Share of the detail distance over which the bars fade in on approach.
const DETAIL_BLEND: f32 = 0.25;

/// Size multiplier for a plate `distance` away: full size up close, shrinking
/// linearly to `minimum` at `range`.
pub(super) fn distance_scale(distance: f32, range: f32, minimum: f32) -> f32 {
    if range <= FULL_SIZE_DISTANCE {
        return 1.0;
    }
    let t = ((distance - FULL_SIZE_DISTANCE) / (range - FULL_SIZE_DISTANCE)).clamp(0.0, 1.0);
    1.0 + (minimum.clamp(0.0, 1.0) - 1.0) * t
}

/// Opacity for a plate `distance` away: opaque for the first part of the range,
/// then fading to nothing at `range`.
pub(super) fn distance_fade(distance: f32, range: f32) -> f32 {
    let start = range * OPAQUE_SHARE;
    if distance <= start || range <= start {
        return 1.0;
    }
    (1.0 - (distance - start) / (range - start)).clamp(0.0, 1.0)
}

/// How much of the plate (the bars) shows at `distance`: none beyond `near`,
/// all of it within the inner part of `near`, blending between.
pub(super) fn detail(distance: f32, near: f32) -> f32 {
    if near <= 0.0 {
        return 0.0;
    }
    ((near - distance) / (near * DETAIL_BLEND)).clamp(0.0, 1.0)
}

/// Height of the top of an entity's box above its origin, from the packed
/// `entityState_t::solid` (`SV_LinkEntity`: bits 16-23 hold top + 32). A
/// player who sends no box is taken as standing.
pub(super) fn head_height(solid: u32) -> f32 {
    let packed = (solid >> 16) & 255;
    if solid == 0 || packed == 0 {
        STANDING_TOP
    } else {
        packed as f32 - 32.0
    }
}

/// Where a player's plate hangs: the world point it is traced from and the
/// height of the box top above it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Anchor {
    pub(super) origin: [f32; 3],
    pub(super) head: f32,
}

/// A presented body: its interpolated origin and packed `entityState_t::solid`.
pub(super) type Body = ([f32; 3], u32);

/// The anchor of a player whose own body is `own` and whose vehicle's body is
/// `vehicle`, each `None` when the world does not present it.
///
/// A pilot inside a `hideRider` vehicle carries `EF_NODRAW`, so the client never
/// presents his body (`CG_AddCEntity` skips it); his plate hangs over the
/// vehicle he sits in instead. A rider whose body is presented keeps his own.
pub(super) fn anchor(own: Option<Body>, vehicle: Option<Body>) -> Option<Anchor> {
    let (origin, solid) = own.or(vehicle)?;
    Some(Anchor {
        origin,
        head: head_height(solid),
    })
}

/// `CLASS_VEHICLE` (`teams.h`).
const CLASS_VEHICLE: u8 = 53;
/// `ET_NPC` (`entityType_t`).
const ET_NPC: u8 = 13;
/// `MAX_CLIENTS`.
const MAX_CLIENTS: u16 = 32;

/// The client sealed in a vehicle whose snapshot state is given, if his own entity
/// is not in the snapshot: stock cgame names the pilot of an `ET_NPC` /
/// `CLASS_VEHICLE` entity from its `owner` while that is below `MAX_CLIENTS`
/// (`CG_DrawCrosshair`). The viewer is not named to himself, and a pilot whose
/// entity is sent (`present`) keeps the ordinary plate.
pub(super) fn hidden_pilot(
    entity_type: u8,
    npc_class: u8,
    owner: u16,
    local: u16,
    present: impl FnOnce(u16) -> bool,
) -> Option<u16> {
    (entity_type == ET_NPC
        && npc_class == CLASS_VEHICLE
        && owner < MAX_CLIENTS
        && owner != local
        && !present(owner))
    .then_some(owner)
}

/// Bar fills until the HUD in use names its own: the retail HUD's red health,
/// green shield (armour) and light blue Force.
pub(super) const HEALTH_COLOR: Color = Color::new(1.0, 0.25, 0.2, 1.0);
pub(super) const SHIELD_COLOR: Color = Color::new(0.3, 0.9, 0.35, 1.0);
pub(super) const FORCE_COLOR: Color = Color::new(0.36, 0.7, 1.0, 1.0);
/// The grey of an empty shield's broken bar.
pub(super) const EMPTY_SHIELD: Color = Color::new(0.55, 0.57, 0.6, 0.7);
/// The "?" over a bar the estimate cannot fill.
pub(super) const UNKNOWN_MARK: Color = Color::new(1.0, 0.85, 0.15, 1.0);

/// A deeper, more saturated `color`: the second layer of a bar over its maximum
/// (overheal, overshield). Each channel moves away from the mean and the whole
/// darkens, so it reads as the same colour, deeper, even for a green that is
/// already near full.
pub(super) fn saturated(color: Color) -> Color {
    let mean = (color.r + color.g + color.b) / 3.0;
    let push = |channel: f32| ((mean + (channel - mean) * 1.8) * 0.72).clamp(0.0, 1.0);
    Color::new(push(color.r), push(color.g), push(color.b), color.a)
}

/// Where a bar's second layer (over the maximum) is drawn in `bar`: an inner band,
/// so the full bar shows round it. The nameplates and the HUD's meters share it.
pub(super) fn overflow_band(bar: Rect) -> Rect {
    let inset = bar.height * 0.22;
    Rect::new(bar.x, bar.y + inset, bar.width, bar.height - inset * 2.0)
}

/// The pieces of an empty shield's broken bar, as `(start, end)` shares of its
/// width: dashes with gaps between, a crack's look.
pub(super) fn broken_dashes() -> impl Iterator<Item = (f32, f32)> {
    const DASHES: usize = 7;
    const SOLID: f32 = 0.7;
    (0..DASHES).map(|index| {
        let start = index as f32 / DASHES as f32;
        (start, start + SOLID / DASHES as f32)
    })
}

/// `color` mixed `share` of the way to white, at opacity `alpha`: a bar's outline.
pub(super) fn lighter(color: Color, share: f32, alpha: f32) -> Color {
    let mix = |channel: f32| channel + (1.0 - channel) * share.clamp(0.0, 1.0);
    Color::new(mix(color.r), mix(color.g), mix(color.b), alpha)
}

/// Which bars a plate holds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Rows {
    pub(super) health: bool,
    pub(super) shield: bool,
    pub(super) force: bool,
}

impl Rows {
    pub(super) fn any(self) -> bool {
        self.health || self.shield || self.force
    }
}

/// The plate under a name: its frame and the bars inside (shield on top, then
/// health, then Force), in physical pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Stack {
    pub(super) frame: Rect,
    pub(super) health: Option<Rect>,
    pub(super) shield: Option<Rect>,
    pub(super) force: Option<Rect>,
}

impl Stack {
    /// Bar heights and spacing in units of `u` pixels (the scaled pixel unit).
    const HEALTH: f32 = 7.0;
    const SHIELD: f32 = 5.0;
    const FORCE: f32 = 4.0;
    const GAP: f32 = 2.5;
    const PAD: f32 = 4.0;
    /// Plate width in units of `u`.
    pub(super) const WIDTH: f32 = 112.0;

    /// Total height of a stack holding `rows`, in pixels.
    pub(super) fn height(rows: Rows, u: f32) -> f32 {
        let bars = [
            (rows.shield, Self::SHIELD),
            (rows.health, Self::HEALTH),
            (rows.force, Self::FORCE),
        ];
        let heights: f32 = bars.iter().filter(|b| b.0).map(|b| b.1).sum();
        let count = bars.iter().filter(|b| b.0).count();
        if count == 0 {
            return 0.0;
        }
        (Self::PAD * 2.0 + heights + Self::GAP * (count - 1) as f32) * u
    }

    /// Lay a stack out centred on `centre_x` with its bottom edge on
    /// `bottom_y`, kept on screen.
    pub(super) fn new(
        centre_x: f32,
        bottom_y: f32,
        rows: Rows,
        u: f32,
        viewport: [f32; 2],
    ) -> Self {
        let width = Self::WIDTH * u;
        let height = Self::height(rows, u);
        let x = (centre_x - width * 0.5).clamp(0.0, (viewport[0] - width).max(0.0));
        let y = bottom_y - height;
        let inner_x = x + Self::PAD * u;
        let inner_width = width - Self::PAD * 2.0 * u;
        let mut cursor = y + Self::PAD * u;
        let mut bar = |wanted: bool, h: f32| {
            wanted.then(|| {
                let rect = Rect::new(inner_x, cursor, inner_width, h * u);
                cursor += (h + Self::GAP) * u;
                rect
            })
        };
        let shield = bar(rows.shield, Self::SHIELD);
        let health = bar(rows.health, Self::HEALTH);
        let force = bar(rows.force, Self::FORCE);
        Self {
            frame: Rect::new(x, y, width, height),
            health,
            shield,
            force,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plates_shrink_with_distance_to_the_minimum() {
        assert_eq!(distance_scale(100.0, 3000.0, 0.5), 1.0);
        assert_eq!(distance_scale(300.0, 3000.0, 0.5), 1.0);
        assert!((distance_scale(1650.0, 3000.0, 0.5) - 0.75).abs() < 1e-5);
        assert!((distance_scale(3000.0, 3000.0, 0.5) - 0.5).abs() < 1e-5);
        assert!((distance_scale(9000.0, 3000.0, 0.5) - 0.5).abs() < 1e-5);
        assert_eq!(distance_scale(1000.0, 200.0, 0.5), 1.0);
    }

    #[test]
    fn plates_fade_out_over_the_last_quarter_of_the_range() {
        assert_eq!(distance_fade(0.0, 4000.0), 1.0);
        assert_eq!(distance_fade(3000.0, 4000.0), 1.0);
        assert!((distance_fade(3500.0, 4000.0) - 0.5).abs() < 1e-5);
        assert_eq!(distance_fade(4000.0, 4000.0), 0.0);
        assert_eq!(distance_fade(9000.0, 4000.0), 0.0);
    }

    #[test]
    fn bars_fade_in_between_far_and_close() {
        assert_eq!(detail(1500.0, 1000.0), 0.0);
        assert_eq!(detail(1000.0, 1000.0), 0.0);
        assert!((detail(875.0, 1000.0) - 0.5).abs() < 1e-5);
        assert_eq!(detail(750.0, 1000.0), 1.0);
        assert_eq!(detail(10.0, 1000.0), 1.0);
        assert_eq!(detail(10.0, 0.0), 0.0);
    }

    #[test]
    fn head_height_unpacks_the_encoded_box() {
        // Standing player: x 15, mins z -24 (24), maxs z 40 -> 72.
        let standing = (72 << 16) | (24 << 8) | 15;
        assert_eq!(head_height(standing), 40.0);
        // Crouching: maxs z 16 -> 48.
        let crouching = (48 << 16) | (24 << 8) | 15;
        assert_eq!(head_height(crouching), 16.0);
        assert_eq!(head_height(0), 40.0);
    }

    #[test]
    fn a_hidden_pilot_is_anchored_over_his_vehicle() {
        let ship = ([100.0, 50.0, 400.0], 96 << 16);
        let body = ([1.0, 2.0, 3.0], 72 << 16);
        // Presented body wins, even inside a presented vehicle.
        let own = anchor(Some(body), Some(ship)).unwrap();
        assert_eq!((own.origin, own.head), (body.0, 40.0));
        // No body (EF_NODRAW): the vehicle's origin and box top.
        let hidden = anchor(None, Some(ship)).unwrap();
        assert_eq!((hidden.origin, hidden.head), (ship.0, 64.0));
        // A vehicle that sends no box falls back to the standing height.
        let bare = anchor(None, Some((ship.0, 0))).unwrap();
        assert_eq!(bare.head, 40.0);
        // Nothing presented, nothing to anchor to.
        assert_eq!(anchor(None, None), None);
    }

    #[test]
    fn a_vehicles_owner_below_max_clients_is_its_hidden_pilot() {
        let never = |_| false;
        assert_eq!(hidden_pilot(13, 53, 5, 0, never), Some(5));
        // Not a vehicle, an empty one (ENTITYNUM_NONE), an NPC pilot, or myself.
        assert_eq!(hidden_pilot(13, 20, 5, 0, never), None);
        assert_eq!(hidden_pilot(1, 53, 5, 0, never), None);
        assert_eq!(hidden_pilot(13, 53, 1023, 0, never), None);
        assert_eq!(hidden_pilot(13, 53, 40, 0, never), None);
        assert_eq!(hidden_pilot(13, 53, 5, 5, never), None);
        // A pilot whose own entity is in the snapshot is drawn from it.
        assert_eq!(hidden_pilot(13, 53, 5, 0, |pilot| pilot == 5), None);
    }

    #[test]
    fn the_overflow_layer_is_the_same_hue_stronger() {
        for color in [HEALTH_COLOR, SHIELD_COLOR] {
            let deep = saturated(color);
            let spread = |c: Color| c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b);
            let strongest = |c: Color| {
                [c.r, c.g, c.b]
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.total_cmp(b.1))
                    .map(|(index, _)| index)
            };
            assert!(spread(deep) >= spread(color) - 0.2, "{deep:?}");
            assert_eq!(strongest(deep), strongest(color), "same hue");
            let sum = |c: Color| c.r + c.g + c.b;
            assert!(sum(deep) < sum(color), "darker");
        }
    }

    #[test]
    fn the_overflow_band_is_inside_the_bar_and_centred() {
        let bar = Rect::new(10.0, 20.0, 100.0, 10.0);
        let band = overflow_band(bar);
        assert!(band.y > bar.y && band.bottom() < bar.bottom());
        assert!((band.y + band.height * 0.5 - 25.0).abs() < 1e-4);
        assert_eq!((band.x, band.width), (bar.x, bar.width));
    }

    #[test]
    fn a_broken_bar_has_gaps_and_stays_inside() {
        let dashes: Vec<_> = broken_dashes().collect();
        assert!(dashes.len() > 3);
        for pair in dashes.windows(2) {
            assert!(pair[0].1 < pair[1].0, "a gap between dashes");
        }
        assert!(
            dashes
                .iter()
                .all(|(start, end)| 0.0 <= *start && *end <= 1.0)
        );
    }

    #[test]
    fn outlines_are_a_lighter_shade() {
        let outline = lighter(Color::new(0.2, 0.4, 1.0, 1.0), 0.5, 0.75);
        assert!((outline.r - 0.6).abs() < 1e-5 && (outline.g - 0.7).abs() < 1e-5);
        assert_eq!((outline.b, outline.a), (1.0, 0.75));
    }

    #[test]
    fn stacks_hold_only_the_wanted_bars_inside_their_frame() {
        let viewport = [1920.0, 1080.0];
        let all = Rows {
            health: true,
            shield: true,
            force: true,
        };
        let stack = Stack::new(960.0, 500.0, all, 2.0, viewport);
        assert!((stack.frame.x + stack.frame.width * 0.5 - 960.0).abs() < 1e-3);
        assert!((stack.frame.y + stack.frame.height - 500.0).abs() < 1e-3);
        let (hp, shield, force) = (
            stack.health.unwrap(),
            stack.shield.unwrap(),
            stack.force.unwrap(),
        );
        // Shield on top, then health, then Force.
        assert!(shield.y > stack.frame.y && hp.y > shield.y + shield.height && force.y > hp.y);
        assert!(force.y + force.height < stack.frame.y + stack.frame.height);

        let only_force = Rows {
            force: true,
            ..Rows::default()
        };
        let slim = Stack::new(960.0, 500.0, only_force, 2.0, viewport);
        assert!(slim.health.is_none() && slim.shield.is_none() && slim.force.is_some());
        assert!(slim.frame.height < stack.frame.height);
        assert_eq!(Stack::height(Rows::default(), 2.0), 0.0);
    }

    #[test]
    fn stacks_stay_on_screen() {
        let viewport = [1920.0, 1080.0];
        let rows = Rows {
            health: true,
            ..Rows::default()
        };
        assert_eq!(Stack::new(5.0, 500.0, rows, 2.0, viewport).frame.x, 0.0);
        let right = Stack::new(1915.0, 500.0, rows, 2.0, viewport).frame;
        assert!((right.x + right.width - viewport[0]).abs() < 1e-3);
    }
}
