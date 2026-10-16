//! Rarity effects: the small animated touches that tell a tier at a glance, shared by
//! everything that has one (saber shaders, holocrons, what comes later). Uncommon has
//! none; each tier up adds to the one under it:
//!
//! - Rare: the frame's glow breathes, a slow soft pulse ([`BREATH_SECONDS`]).
//! - Legendary: the breath, and a slanted sheen of light sweeping across now and then
//!   ([`SHEEN_SECONDS`]).
//! - Mythical: the sheen, a bright spark running round the border with a short tail
//!   ([`SPARK_SECONDS`]), and on the larger frames a few faint motes rising inside.
//!
//! A locked item keeps only a faint breath, so what the player holds stands out. With
//! `ui_rarityEffects 0` nothing here draws: the callers' static tier frames stay.
//!
//! Everything is drawn with the UI's own shapes into the frame's draw list (no texture,
//! no pipeline of its own), at most [`MAX_COMMANDS`] commands an item, all inside the
//! item's rectangle, and the same at the same time (no state, no randomness).

use sjk_ui::{Color, DrawCommand, DrawList, Gradient, Rect};
use std::f32::consts::{PI, TAU};
use std::sync::atomic::{AtomicBool, Ordering};

/// The setting that turns the effects on or off.
pub(crate) const CVAR: &str = "ui_rarityEffects";

/// Live `ui_rarityEffects`.
static ENABLED: AtomicBool = AtomicBool::new(true);

/// One breath, in seconds.
pub(crate) const BREATH_SECONDS: f32 = 3.0;
/// One sheen's period: it crosses in [`SHEEN_CROSS`] of it and rests the rest.
pub(crate) const SHEEN_SECONDS: f32 = 4.0;
const SHEEN_CROSS: f32 = 0.35;
/// One lap of the spark round the border.
pub(crate) const SPARK_SECONDS: f32 = 3.2;
/// A mote's life, bottom to top.
const MOTE_SECONDS: f32 = 5.0;

/// The sheen's slices (a slanted band drawn as a stair of horizontal fades).
const SHEEN_SLICES_FULL: usize = 8;
const SHEEN_SLICES_SMALL: usize = 5;
/// The spark's tail: dots behind its head.
const TAIL: usize = 4;
/// Motes inside a full frame.
const MOTES: usize = 4;

/// The most commands one item draws (the tests hold every size and tier to it).
#[cfg(test)]
const MAX_COMMANDS: usize = 2 // breath
    + 2 + 2 * SHEEN_SLICES_FULL // sheen: clip, slices, unclip
    + 2 + TAIL // spark: halo, head, tail
    + MOTES;

/// Register `ui_rarityEffects` (default 1, archived, live).
pub(crate) fn register(cvars: &mut sjk_shell::CvarRegistry) -> Result<(), sjk_shell::CvarError> {
    cvars.register(sjk_shell::CvarDefinition::new(
        CVAR,
        1_i64,
        sjk_shell::CvarFlags::ARCHIVE,
        "Animate rarity: Rare frames breathe, Legendary ones catch a sheen, Mythical ones \
         a running spark and rising motes; 0 keeps the tier frames still",
    ))?;
    let on = |value: &sjk_shell::CvarValue| !matches!(value, sjk_shell::CvarValue::Integer(0));
    ENABLED.store(
        on(&cvars.get(CVAR).expect("registered").value),
        Ordering::Relaxed,
    );
    cvars.on_change(CVAR, move |change| {
        ENABLED.store(on(&change.current), Ordering::Relaxed)
    })
}

/// Whether the effects draw (`ui_rarityEffects`).
pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
}

/// How much of the show a frame gets, by how large it is.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Size {
    /// A card or a tile: everything its tier has.
    Full,
    /// A swatch's frame or a pill: no motes, a shorter sheen.
    Small,
    /// A line on the HUD: the breath alone, quiet.
    Tiny,
}

/// One item's frame to animate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Item {
    /// Its tier's rank: 0 Uncommon, 1 Rare, 2 Legendary, 3 Mythical (the holocron
    /// tiers' order, [`crate::holocrons::TIERS`]).
    pub(crate) rank: usize,
    /// Its tier's colour.
    pub(crate) colour: Color,
    /// Whether the player holds it: a locked one only breathes, faintly.
    pub(crate) owned: bool,
    pub(crate) size: Size,
}

impl Item {
    /// A saber shader's (or any unlockable's) tier.
    pub(crate) fn of(tier: crate::unlockables::Rarity, owned: bool, size: Size) -> Self {
        Self {
            rank: tier.effect_rank(),
            colour: tier.colour(),
            owned,
            size,
        }
    }

    /// A holocron tier's.
    pub(crate) fn holocron(tier: &crate::holocrons::Tier, size: Size) -> Self {
        Self {
            rank: tier.index,
            colour: tier.colour,
            owned: true,
            size,
        }
    }
}

/// Draw `item`'s effects over its frame `rect` (pixels, corners rounded by `radius`)
/// at `seconds`; `px` is pixels per frame unit (the menu's scale). Draws nothing for
/// Uncommon or with the effects off.
pub(crate) fn draw(
    list: &mut DrawList,
    item: Item,
    rect: Rect,
    radius: f32,
    px: f32,
    seconds: f32,
) {
    if !enabled() {
        return;
    }
    draw_always(list, item, rect, radius, px, seconds);
}

/// [`draw`] whatever the setting says (tests, and the setting's own check).
fn draw_always(list: &mut DrawList, item: Item, rect: Rect, radius: f32, px: f32, seconds: f32) {
    if item.rank == 0 || rect.width <= 2.0 || rect.height <= 2.0 {
        return;
    }
    let px = px.max(0.25);
    breath(list, item, rect, radius, px, seconds);
    if !item.owned || item.size == Size::Tiny {
        return;
    }
    if item.rank >= 2 {
        sheen(list, item, rect, px, seconds);
    }
    if item.rank >= 3 {
        spark(list, item, rect, radius, px, seconds);
        if item.size == Size::Full {
            motes(list, item, rect, px, seconds);
        }
    }
}

/// `colour` at `alpha`.
fn with_alpha(colour: Color, alpha: f32) -> Color {
    Color::new(colour.r, colour.g, colour.b, alpha.clamp(0.0, 1.0))
}

/// `colour` brought `amount` of the way to white.
fn lit(colour: Color, amount: f32) -> Color {
    Color::new(
        colour.r + (1.0 - colour.r) * amount,
        colour.g + (1.0 - colour.g) * amount,
        colour.b + (1.0 - colour.b) * amount,
        colour.a,
    )
}

/// `rect` shrunk by `by` on every side.
fn inset(rect: Rect, by: f32) -> Rect {
    let by = by.min(rect.width * 0.5).min(rect.height * 0.5);
    Rect::new(
        rect.x + by,
        rect.y + by,
        rect.width - 2.0 * by,
        rect.height - 2.0 * by,
    )
}

/// Rare and up: the frame's glow breathing, inside its edge.
fn breath(list: &mut DrawList, item: Item, rect: Rect, radius: f32, px: f32, seconds: f32) {
    let pulse = 0.5 - 0.5 * (seconds / BREATH_SECONDS * TAU).cos();
    // Higher tiers breathe a little deeper; a locked item barely.
    let depth = match item.size {
        Size::Tiny => 0.5,
        _ => 0.7 + 0.15 * item.rank as f32,
    } * if item.owned { 1.0 } else { 0.35 };
    let width = match item.size {
        Size::Full => 4.0,
        Size::Small => 2.6,
        Size::Tiny => 1.6,
    } * px;
    let _ = list.push(DrawCommand::Border {
        rect: inset(rect, width * 0.5),
        radius: (radius - width * 0.5).max(0.0),
        width,
        color: with_alpha(item.colour, (0.06 + 0.16 * pulse) * depth),
    });
    let _ = list.push(DrawCommand::Border {
        rect,
        radius,
        width: 1.2 * px,
        color: with_alpha(lit(item.colour, 0.25), (0.25 + 0.5 * pulse) * depth),
    });
}

/// Where in its period a frame's sheen is at time 0: each frame keeps its own moment
/// (from where it stands), so a page of cards does not flash all at once.
fn sheen_offset(rect: Rect) -> f32 {
    scatter((rect.x * 0.37 + rect.y * 1.91) as u32)
}

/// Legendary and up: a slanted band of light crossing the frame left to right, then a
/// rest.
fn sheen(list: &mut DrawList, item: Item, rect: Rect, px: f32, seconds: f32) {
    let phase = (seconds / SHEEN_SECONDS + sheen_offset(rect)).rem_euclid(1.0);
    if phase >= SHEEN_CROSS {
        return;
    }
    let travel = phase / SHEEN_CROSS;
    let slices = if item.size == Size::Full {
        SHEEN_SLICES_FULL
    } else {
        SHEEN_SLICES_SMALL
    };
    let band = (rect.width * 0.22).clamp(10.0 * px, 90.0 * px);
    // The band leans: its top is this far right of its bottom (a slight lean, so the
    // slices' steps stay a few pixels).
    let slant = rect.height * 0.22;
    let span = rect.width + band + slant;
    let left = rect.x - band - slant + span * travel;
    // Faded in and out at the ends of its crossing.
    let strength = (travel * PI).sin() * if item.rank >= 3 { 0.28 } else { 0.22 };
    let bright = with_alpha(lit(item.colour, 0.7), strength);
    let clear = with_alpha(lit(item.colour, 0.7), 0.0);
    let _ = list.push(DrawCommand::PushClip(rect));
    let height = rect.height / slices as f32;
    for slice in 0..slices {
        let y = rect.y + height * slice as f32;
        // Slice 0 is the top, furthest right.
        let lean = slant * (1.0 - (slice as f32 + 0.5) / slices as f32);
        let x = left + lean;
        let half = band * 0.5;
        for (start, end, from) in [(clear, bright, x), (bright, clear, x + half)] {
            let x0 = from.max(rect.x);
            let x1 = (from + half).min(rect.right());
            if x1 - x0 < 0.5 {
                continue;
            }
            let _ = list.push(DrawCommand::GradientRect {
                rect: Rect::new(
                    x0,
                    y,
                    x1 - x0,
                    height + 0.5_f32.min(rect.bottom() - y - height),
                ),
                radius: 0.0,
                gradient: Gradient {
                    start,
                    end,
                    vertical: false,
                },
            });
        }
    }
    let _ = list.push(DrawCommand::PopClip);
}

/// The point `along` (0..1) the rounded rectangle `rect`'s outline, clockwise from the
/// top edge's left end.
fn outline_point(rect: Rect, radius: f32, along: f32) -> [f32; 2] {
    let r = radius.min(rect.width * 0.5).min(rect.height * 0.5).max(0.0);
    let straight_x = rect.width - 2.0 * r;
    let straight_y = rect.height - 2.0 * r;
    let corner = 0.5 * PI * r;
    let total = 2.0 * (straight_x + straight_y) + 4.0 * corner;
    let mut d = along.rem_euclid(1.0) * total;
    // The four sides and corners, clockwise from the top edge.
    let corners = [
        [rect.right() - r, rect.y + r, -0.5 * PI],
        [rect.right() - r, rect.bottom() - r, 0.0],
        [rect.x + r, rect.bottom() - r, 0.5 * PI],
        [rect.x + r, rect.y + r, PI],
    ];
    let sides = [
        ([rect.x + r, rect.y], [1.0, 0.0], straight_x),
        ([rect.right(), rect.y + r], [0.0, 1.0], straight_y),
        ([rect.right() - r, rect.bottom()], [-1.0, 0.0], straight_x),
        ([rect.x, rect.bottom() - r], [0.0, -1.0], straight_y),
    ];
    for (side, &(start, step, length)) in sides.iter().enumerate() {
        if d <= length {
            return [start[0] + step[0] * d, start[1] + step[1] * d];
        }
        d -= length;
        if d <= corner {
            let [cx, cy, from] = corners[side];
            let angle = from + if r > 0.0 { d / r } else { 0.0 };
            return [cx + r * angle.cos(), cy + r * angle.sin()];
        }
        d -= corner;
    }
    [rect.x + r, rect.y]
}

/// A round dot of `size` (diameter) at `centre`.
fn dot(list: &mut DrawList, centre: [f32; 2], size: f32, colour: Color) {
    let _ = list.push(DrawCommand::RoundedRect {
        rect: Rect::new(centre[0] - size * 0.5, centre[1] - size * 0.5, size, size),
        radius: size * 0.5,
        color: colour,
    });
}

/// Mythical: a spark running clockwise round the border, a soft halo and a short tail.
fn spark(list: &mut DrawList, item: Item, rect: Rect, radius: f32, px: f32, seconds: f32) {
    let (head, halo) = match item.size {
        Size::Full => (5.0 * px, 13.0 * px),
        _ => (3.0 * px, 7.0 * px),
    };
    // The path runs just inside the edge, so the halo stays inside the frame.
    let path = inset(rect, halo * 0.5);
    let path_radius = (radius - halo * 0.5).max(0.0);
    let perimeter = 2.0 * (path.width + path.height);
    let along = seconds / SPARK_SECONDS;
    let colour = lit(item.colour, 0.55);
    // The tail: dots behind the head, smaller and fainter, a fixed length in pixels.
    let gap = (5.0 * px) / perimeter.max(1.0);
    for step in (1..=TAIL).rev() {
        let fade = 1.0 - step as f32 / (TAIL as f32 + 1.0);
        dot(
            list,
            outline_point(path, path_radius, along - gap * step as f32),
            head * (0.45 + 0.5 * fade),
            with_alpha(colour, 0.65 * fade),
        );
    }
    let at = outline_point(path, path_radius, along);
    dot(list, at, halo, with_alpha(item.colour, 0.28));
    dot(list, at, head, with_alpha(lit(item.colour, 0.85), 0.95));
}

/// A number in [0, 1) from `n` (a fixed scatter, the same every frame).
fn scatter(n: u32) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9) ^ 0x85EB_CA6B;
    x ^= x >> 15;
    x = x.wrapping_mul(0x2C1B_3C6D);
    x ^= x >> 12;
    (x & 0xFFFF) as f32 / 65_536.0
}

/// Mythical, full frames: a few faint motes rising inside, each on its own clock.
fn motes(list: &mut DrawList, item: Item, rect: Rect, px: f32, seconds: f32) {
    let size = 3.0 * px;
    let margin = 6.0 * px + size;
    let inner = inset(rect, margin);
    if inner.width <= 0.0 || inner.height <= 0.0 {
        return;
    }
    let colour = lit(item.colour, 0.4);
    for mote in 0..MOTES as u32 {
        let life = (seconds / MOTE_SECONDS + scatter(mote * 2 + 1)).rem_euclid(1.0);
        let sway = (seconds * 1.3 + scatter(mote * 2 + 7) * TAU).sin() * 4.0 * px;
        let x =
            (inner.x + inner.width * scatter(mote * 2 + 2) + sway).clamp(inner.x, inner.right());
        let y = inner.bottom() - inner.height * life;
        let grown = size * (0.6 + 0.4 * scatter(mote * 2 + 3));
        dot(
            list,
            [x, y],
            grown,
            with_alpha(colour, 0.38 * (life * PI).sin()),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unlockables::Rarity;

    const RECT: Rect = Rect::new(100.0, 200.0, 160.0, 190.0);

    fn commands(item: Item, rect: Rect, px: f32, seconds: f32) -> Vec<DrawCommand> {
        let mut list = DrawList::new(256);
        draw_always(&mut list, item, rect, 12.0, px, seconds);
        list.commands().to_vec()
    }

    fn every_item() -> Vec<Item> {
        let mut items = Vec::new();
        for tier in [
            Rarity::Common,
            Rarity::Uncommon,
            Rarity::Rare,
            Rarity::Legendary,
            Rarity::Mythical,
        ] {
            for owned in [true, false] {
                for size in [Size::Full, Size::Small, Size::Tiny] {
                    items.push(Item::of(tier, owned, size));
                }
            }
        }
        items
    }

    /// Every rectangle a command draws, and the clip it pushes.
    fn rects(commands: &[DrawCommand]) -> Vec<Rect> {
        commands
            .iter()
            .filter_map(|command| match *command {
                DrawCommand::SolidRect { rect, .. }
                | DrawCommand::RoundedRect { rect, .. }
                | DrawCommand::GradientRect { rect, .. }
                | DrawCommand::Border { rect, .. }
                | DrawCommand::PushClip(rect)
                | DrawCommand::PushRoundedClip { rect, .. } => Some(rect),
                _ => None,
            })
            .collect()
    }

    /// A moment `share` of the way into `rect`'s sheen period.
    fn sheen_at(rect: Rect, share: f32) -> f32 {
        (share - sheen_offset(rect)).rem_euclid(1.0) * SHEEN_SECONDS
    }

    #[test]
    fn uncommon_draws_nothing_and_each_tier_up_draws_more() {
        let at = sheen_at(RECT, 0.15); // inside the sheen's crossing
        let count = |tier| commands(Item::of(tier, true, Size::Full), RECT, 1.0, at).len();
        assert_eq!(count(Rarity::Common), 0);
        assert_eq!(count(Rarity::Uncommon), 0);
        assert!(count(Rarity::Rare) > 0);
        assert!(count(Rarity::Legendary) > count(Rarity::Rare));
        assert!(count(Rarity::Mythical) > count(Rarity::Legendary));
    }

    #[test]
    fn every_item_stays_within_its_budget_and_its_frame() {
        for item in every_item() {
            for px in [0.5, 1.0, 2.0] {
                let rect = Rect::new(RECT.x, RECT.y, RECT.width * px, RECT.height * px);
                for step in 0..64 {
                    let seconds = step as f32 * 0.37;
                    let drawn = commands(item, rect, px, seconds);
                    assert!(drawn.len() <= MAX_COMMANDS, "{item:?}: {}", drawn.len());
                    for inside in rects(&drawn) {
                        assert!(
                            inside.x >= rect.x - 0.01
                                && inside.y >= rect.y - 0.01
                                && inside.right() <= rect.right() + 0.01
                                && inside.bottom() <= rect.bottom() + 0.01,
                            "{item:?} at {seconds}: {inside:?} outside {rect:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn small_frames_stay_inside_too() {
        let rect = Rect::new(10.0, 10.0, 60.0, 14.0);
        for item in every_item() {
            for step in 0..40 {
                for inside in rects(&commands(item, rect, 1.0, step as f32 * 0.11)) {
                    assert!(
                        inside.x >= rect.x - 0.01
                            && inside.y >= rect.y - 0.01
                            && inside.right() <= rect.right() + 0.01
                            && inside.bottom() <= rect.bottom() + 0.01,
                        "{item:?}: {inside:?} outside {rect:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_same_time_draws_the_same_and_time_moves_it() {
        let item = Item::of(Rarity::Mythical, true, Size::Full);
        assert_eq!(
            commands(item, RECT, 1.0, 2.5),
            commands(item, RECT, 1.0, 2.5)
        );
        assert_ne!(
            commands(item, RECT, 1.0, 2.5),
            commands(item, RECT, 1.0, 2.6)
        );
    }

    #[test]
    fn a_page_of_mythical_cards_stays_light() {
        // Fifteen owned Mythical cards as the Collection's grid lays them out: their
        // sheens cross at their own moments, so a frame never draws them all at once.
        let item = Item::of(Rarity::Mythical, true, Size::Full);
        let most = (0..120)
            .map(|step| {
                let seconds = step as f32 * 0.05;
                (0..15)
                    .map(|card| {
                        let rect = Rect::new(
                            84.0 + (card % 5) as f32 * 174.0,
                            324.0 + (card / 5) as f32 * 210.0,
                            164.0,
                            196.0,
                        );
                        commands(item, rect, 1.0, seconds).len()
                    })
                    .sum::<usize>()
            })
            .max()
            .unwrap_or(0);
        assert!(most <= 300, "{most} commands at once");
    }

    #[test]
    fn a_locked_item_only_breathes() {
        for tier in [Rarity::Rare, Rarity::Legendary, Rarity::Mythical] {
            let drawn = commands(Item::of(tier, false, Size::Full), RECT, 1.0, 0.4);
            assert_eq!(drawn.len(), 2, "{tier:?}");
            assert!(
                drawn
                    .iter()
                    .all(|c| matches!(c, DrawCommand::Border { .. }))
            );
        }
    }

    #[test]
    fn the_hud_line_only_breathes() {
        let drawn = commands(Item::of(Rarity::Mythical, true, Size::Tiny), RECT, 1.0, 0.4);
        assert_eq!(drawn.len(), 2);
    }

    #[test]
    fn the_sheen_crosses_then_rests() {
        let item = Item::of(Rarity::Legendary, true, Size::Full);
        let resting = commands(item, RECT, 1.0, sheen_at(RECT, 0.8)).len();
        let crossing = commands(item, RECT, 1.0, sheen_at(RECT, 0.15)).len();
        assert_eq!(resting, 2);
        assert!(crossing > resting);
    }

    #[test]
    fn the_outline_runs_round_the_frame() {
        let start = outline_point(RECT, 12.0, 0.0);
        assert_eq!(start, [RECT.x + 12.0, RECT.y]);
        // A quarter of the way it is on the right side, half way on the bottom.
        let half = outline_point(RECT, 12.0, 0.5);
        assert!((half[1] - RECT.bottom()).abs() < 1.0, "{half:?}");
        for step in 0..100 {
            let [x, y] = outline_point(RECT, 12.0, step as f32 / 100.0);
            assert!(x >= RECT.x - 0.01 && x <= RECT.right() + 0.01);
            assert!(y >= RECT.y - 0.01 && y <= RECT.bottom() + 0.01);
        }
    }

    #[test]
    fn off_draws_nothing() {
        let mut cvars = sjk_shell::CvarRegistry::new();
        register(&mut cvars).unwrap();
        assert!(enabled());
        let item = Item::of(Rarity::Mythical, true, Size::Full);
        cvars.set_text(CVAR, "0").unwrap();
        let mut list = DrawList::new(64);
        draw(&mut list, item, RECT, 12.0, 1.0, 0.4);
        let off = list.len();
        cvars.set_text(CVAR, "1").unwrap();
        list.clear();
        draw(&mut list, item, RECT, 12.0, 1.0, 0.4);
        assert_eq!(off, 0);
        assert!(!list.is_empty());
    }

    #[test]
    fn holocron_tiers_match_the_shader_tiers() {
        for (index, tier) in crate::holocrons::TIERS.iter().enumerate() {
            let item = Item::holocron(tier, Size::Full);
            assert_eq!(item.rank, index);
        }
        let mythical = Item::of(Rarity::Mythical, true, Size::Full);
        assert_eq!(mythical.colour, crate::holocrons::TIERS[3].colour);
    }
}
