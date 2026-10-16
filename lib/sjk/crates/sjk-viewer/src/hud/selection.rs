//! Hero selectors: readable names, selected accent, no tinted backplate.
use super::HudOverlay;
use sjk_client::force_wheel;
use sjk_client::selection::{Selection, SelectionView};
use sjk_protocol::PlayerState;
use sjk_ui::{
    Color, DrawCommand, DrawList, FontWeight, Rect, TextAlign, TextId, TextOverflow, Theme,
};

/// Retail's `SP_INGAME_*2` names in `forcePowers_t` order, then JoF EJK's wheel
/// entries and the JA+ flamethrower.
const POWERS: [&str; 22] = [
    "Heal",
    "Jump",
    "Speed",
    "Push",
    "Pull",
    "Mind Trick",
    "Grip",
    "Lightning",
    "Dark Rage",
    "Protect",
    "Absorb",
    "Team Heal",
    "Team Energize",
    "Drain",
    "Sense",
    "Saber Offense",
    "Saber Defense",
    "Saber Throw",
    "Stasis",
    "Repulse",
    "Dash",
    "Flamethrower",
];
/// Name of the flamethrower in [`POWERS`].
const FLAMETHROWER_NAME: u32 = 21;
/// `FP_LIGHTNING`.
const LIGHTNING: u8 = 7;
const ITEMS: [&str; 12] = [
    "",
    "Seeker Drone",
    "Portable Shield",
    "Bacta",
    "Large Bacta",
    "Electrobinoculars",
    "Sentry Gun",
    "Jetpack",
    "Health Dispenser",
    "Ammo Dispenser",
    "E-Web",
    "Cloaking Device",
];

impl HudOverlay {
    /// Project the local selector without allocating or owning input state;
    /// `ja_plus` is a JA+ server, whose merc mode shows Lightning as a flamethrower.
    pub(crate) fn update_selection(
        &mut self,
        selection: &Selection,
        player: &PlayerState,
        time: i32,
        ja_plus: bool,
    ) {
        self.selector = selection.view(player, time);
        self.flamethrower.observe(player, ja_plus);
        self.flamethrower_shown = self
            .flamethrower
            .shows_flamethrower(LIGHTNING, player, ja_plus);
    }

    /// Draw the Force selector as the retail icon bar (classic and game HUD
    /// styles) rather than the radial HUD's list.
    pub(crate) fn set_force_wheel_bar(&mut self, bar: bool) {
        self.force_wheel_bar = bar;
    }
}

/// The name of wheel entry `slot`; `flamethrower` names Lightning the flamethrower.
pub(super) fn name_id(slot: u8, flamethrower: bool) -> TextId {
    if flamethrower && slot == LIGHTNING {
        TextId(1000 + FLAMETHROWER_NAME)
    } else {
        TextId(1000 + u32::from(slot))
    }
}

/// Resolve static selector names (cg_draw.c:92-113).
pub(super) fn text(id: TextId) -> &'static str {
    match id.0 {
        1200 => "FORCE",
        1201 => "INVENTORY",
        1000..=1021 => POWERS[(id.0 - 1000) as usize],
        1100..=1111 => ITEMS[(id.0 - 1100) as usize],
        _ => "",
    }
}

/// Emit a bounded two-column selector into the existing retained HUD draw list.
pub(super) fn emit(
    draw: &mut DrawList,
    view: Option<SelectionView>,
    flamethrower: bool,
    theme: Theme,
    viewport: [f32; 2],
    user_scale: f32,
) {
    let Some(view) = view else { return };
    let scale = crate::ui_scale::height_scale(viewport[1]) * user_scale.clamp(0.25, 2.0);
    let x = 60.0 * scale;
    let y = viewport[1] - 180.0 * (viewport[1] / 1080.0) - 350.0 * scale;
    let base = if view.inventory { 1100 } else { 1000 };
    let fade = |mut color: Color| {
        color.a *= view.alpha;
        color
    };
    label(
        draw,
        Rect::new(x, y, 430.0 * scale, 24.0 * scale),
        TextId(1200 + u32::from(view.inventory)),
        20.0 * scale,
        fade(theme.muted),
    );
    let name = |tag: u8| {
        if view.inventory {
            TextId(base + u32::from(tag))
        } else {
            name_id(tag, flamethrower)
        }
    };
    label(
        draw,
        Rect::new(x, y + 25.0 * scale, 440.0 * scale, 42.0 * scale),
        name(view.selected),
        44.0 * scale,
        fade(theme.foreground),
    );
    // Force walks JoF EJK's wheel order (stock order plus JoF's entries).
    let (wheel, wheel_count) = force_wheel::build(view.available);
    let items: [u8; 18] = std::array::from_fn(|tag| tag as u8);
    let tags = if view.inventory {
        &items[..]
    } else {
        &wheel[..wheel_count]
    };
    let mut row = 0;
    for &tag in tags {
        if view.available & (1 << tag) == 0 {
            continue;
        }
        let column = row / 7;
        let rect = Rect::new(
            x + column as f32 * 270.0 * scale,
            y + (82.0 + (row % 7) as f32 * 38.0) * scale,
            258.0 * scale,
            32.0 * scale,
        );
        let selected = tag == view.selected;
        if selected {
            let _ = draw.push(DrawCommand::SolidRect {
                rect: Rect::new(
                    rect.x - 10.0 * scale,
                    rect.y + 3.0 * scale,
                    3.0 * scale,
                    20.0 * scale,
                ),
                color: fade(theme.accent),
            });
        }
        label(
            draw,
            rect,
            name(tag),
            26.0 * scale,
            fade(if selected { theme.accent } else { theme.muted }),
        );
        row += 1;
    }
}

fn label(draw: &mut DrawList, rect: Rect, text: TextId, size: f32, color: Color) {
    let _ = draw.push(DrawCommand::Text {
        rect,
        text,
        size,
        color,
        align: TextAlign::Start,
        overflow: TextOverflow::Ellipsis,
        weight: FontWeight::Semibold,
        letter_spacing: 0.0,
    });
}
