//! The Collection page's frame, in the SJK UI's look: the map darkened behind Medals
//! and Achievements, or dark on the left and clear over the player's model behind
//! Shaders, Toys and Nameplates; the way back, the title and the row of tabs, each
//! with how many of its things the player holds; at the top right how many of all of
//! them, as a tick per thing, lit for the ones held; the tab; the keys.

use super::*;
use crate::menu::sjk::{
    Frame, TextTarget, color, fade, fade_across, key_hint, key_hint_width, text,
};
use crate::menu_widgets::TextFamily;
use crate::text::UiFont;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The page's left edge, where a tab's things start, and the right edge.
pub(super) const LEFT_X: f32 = 96.0;
pub(super) const RIGHT_X: f32 = 1_824.0;
/// Where a tab's headline stands, under the row of tabs.
pub(super) const LEAD_Y: f32 = 222.0;
/// The keys' line.
pub(super) const KEYS_Y: f32 = 1_010.0;
/// The showcase on the right of Medals and Achievements: its left edge and width.
pub(super) const SHOWCASE_X: f32 = 1_120.0;
pub(super) const SHOWCASE_WIDTH: f32 = RIGHT_X - SHOWCASE_X;
/// Where the live preview of the player's model shows over a match (frame pixels):
/// where the model stands on the menu map's stage.
pub(crate) const MODEL_AREA: [f32; 4] = [760.0, 110.0, 820.0, 840.0];
/// Under Shaders, Toys and Nameplates, the column that says what is chosen, over the
/// clear right of the screen beside the model.
pub(super) const STAGE_TEXT_X: f32 = 1_300.0;
pub(super) const STAGE_TEXT_WIDTH: f32 = RIGHT_X - STAGE_TEXT_X;

/// The tick bar at the top right: a tick's size, the gaps between ticks and groups.
const TICK: [f32; 2] = [5.0, 16.0];
const TICK_GAP: f32 = 4.0;
const GROUP_GAP: f32 = 14.0;

/// How many of a tab's things the player holds, and how many there are; `None` while
/// that is not known (no identity, no answer from the hub).
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(super) struct Count {
    pub(super) held: Option<usize>,
    pub(super) total: usize,
}

impl Count {
    /// What the tab's name says after it: "2/4", how many for a tab with no set total
    /// (the holocrons held), empty when unknown or none.
    fn label(self) -> String {
        match self.held {
            Some(held) if self.total > 0 => format!("{held}/{}", self.total),
            Some(held) if held > 0 => held.to_string(),
            _ => String::new(),
        }
    }
}

/// Each tab's count, in the row's order.
pub(super) fn counts(inputs: &Inputs<'_>) -> [Count; 7] {
    let medals = medals::held(inputs);
    let achievements = inputs
        .standings
        .iter()
        .filter(|standing| standing.unlocked.is_some())
        .count();
    let shaders = (inputs.holdings.reason().is_none()).then(|| {
        unlockables::ALL
            .iter()
            .filter(|skin| inputs.holdings.unlock(skin.id).is_some())
            .count()
    });
    [
        Count {
            held: medals,
            total: crate::medals::Medal::COUNT,
        },
        Count {
            held: Some(achievements),
            total: inputs.standings.len(),
        },
        Count {
            held: shaders,
            total: unlockables::ALL.len(),
        },
        // Illuminate is everyone's.
        Count {
            held: Some(1),
            total: 1,
        },
        Count {
            held: Some(0),
            total: 0,
        },
        // The holocrons are opened, not kept: how many, and no total.
        Count {
            held: inputs
                .snapshot
                .filter(|_| inputs.enabled)
                .and_then(|snapshot| snapshot.me.as_ref())
                .map(|me| {
                    crate::holocrons::counts_of(&me.holocron_counts)
                        .iter()
                        .sum::<u32>() as usize
                }),
            total: 0,
        },
        // The cards held, and no total until a set is out.
        Count {
            held: cards::held(inputs),
            total: 0,
        },
    ]
}

/// The Collection row's counts, for the Holocrons page, which draws the row without the
/// Collection page's inputs.
pub(crate) fn row_labels(
    enabled: bool,
    snapshot: Option<&sjk_identity::Snapshot>,
    standings: &[Standing],
) -> Vec<String> {
    let inputs = Inputs {
        enabled,
        snapshot,
        holdings: unlockables::Holdings::of(enabled, snapshot),
        setting: "",
        standings,
        illuminate: true,
        name: "",
        stock: color::HOLO,
        grid: false,
        options: "",
    };
    counts(&inputs).iter().map(|count| count.label()).collect()
}

/// A soft round glow of `colour` at (`x`, `y`) (frame pixels), `radius` across its
/// half, `strength` its alpha at the middle: discs stacked from the rim inwards.
pub(super) fn glow(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    centre: [f32; 2],
    radius: f32,
    colour: Color,
    strength: f32,
) {
    const LAYERS: usize = 8;
    let [x, y] = centre;
    for layer in 0..LAYERS {
        let r = radius * (1.0 - layer as f32 / LAYERS as f32);
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x - r, y - r, r * 2.0, r * 2.0),
            radius: r * frame.s,
            color: color::alpha(colour, strength / LAYERS as f32),
        });
    }
}

/// Lines of `body`, centred in the column from `x` `width` wide, from `y`, `chars` to a
/// line and at most `lines` of them; returns where the next line would stand.
#[allow(clippy::too_many_arguments)]
pub(super) fn centred_lines(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    body: &str,
    [x, y, width]: [f32; 3],
    chars: usize,
    lines: usize,
    size: f32,
    colour: Color,
) -> f32 {
    left_lines(
        canvas,
        frame,
        body,
        [x, y, width],
        (chars, lines),
        size,
        colour,
        TextAlign::Center,
    )
}

/// Lines of `body` from (`x`, `y`) in a column `width` wide, aligned `align`; `fit` is
/// the characters to a line and the most lines. Returns where the next line would
/// stand.
#[allow(clippy::too_many_arguments)]
pub(super) fn left_lines(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    body: &str,
    [x, mut y, width]: [f32; 3],
    (chars, lines): (usize, usize),
    size: f32,
    colour: Color,
    align: TextAlign,
) -> f32 {
    let step = size * 1.42;
    let parts: Vec<&str> = crate::menu::sjk::wrap(body, chars).collect();
    for (index, part) in parts.iter().take(lines).enumerate() {
        let cut = index + 1 == lines && parts.len() > lines;
        text(
            canvas,
            TextFamily::Body,
            format_args!("{part}{}", if cut { "..." } else { "" }),
            frame.rect(x, y, width, step),
            size * frame.s,
            colour,
            FontWeight::Regular,
            align,
        );
        y += step;
    }
    y
}

impl Panel {
    /// Draw the page with what `inputs` says.
    pub(crate) fn append_sjk(
        &mut self,
        inputs: &Inputs<'_>,
        target: TextTarget<'_>,
        viewport: [f32; 2],
    ) {
        let body = match &target {
            TextTarget::Families(fonts, _) => fonts.body.1,
            TextTarget::Inter(_, font) => *font,
        };
        // Inter, where the families are not loaded, is wider than the display family.
        self.widen = if matches!(target, TextTarget::Inter(..)) {
            1.3
        } else {
            1.0
        };
        self.build(inputs, body, viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the page out; `_body` is the body family, kept for measured layouts.
    pub(super) fn build(&mut self, inputs: &Inputs<'_>, _body: &UiFont, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        let seconds = self.epoch.elapsed().as_secs_f32();
        #[cfg(test)]
        let seconds = self.shot_seconds.unwrap_or(seconds);
        self.ui.begin_transparent(viewport);
        let kit = matches!(self.tab, Tab::Shaders | Tab::Toys | Tab::Nameplates);
        if kit && self.backstage != Backstage::None {
            stage_scrims(&mut self.ui, viewport, &frame);
        } else {
            crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        }
        if let Backstage::Preview { ready: true } = self.backstage
            && kit
        {
            let [x, y, width, height] = MODEL_AREA;
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(x, y, width, height),
                texture: crate::ui_renderer::PREVIEW_TEXTURE,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
        let counts = counts(inputs);
        let labels: Vec<String> = counts.iter().map(|count| count.label()).collect();
        let back = if self.hub {
            crate::profile_hub::back_words(self.back)
        } else {
            "Back"
        };
        crate::profile_hub::collection_header(
            &mut self.ui,
            &frame,
            back,
            BACK_TOKEN,
            self.tab,
            (&labels, self.widen),
        );
        self.tally(&frame, &counts);
        // A window narrower than 16:9 shows the model nearer the words beside it: they
        // keep a dark ground of their own.
        if kit && self.backstage != Backstage::None && viewport[0] / viewport[1] < 1.7 {
            // A soft dark pad: rounded layers stacked from the rim inwards.
            const LAYERS: usize = 6;
            for layer in 0..LAYERS {
                let inset = layer as f32 * 14.0;
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(
                        STAGE_TEXT_X - 90.0 + inset,
                        590.0 + inset,
                        RIGHT_X - STAGE_TEXT_X + 150.0 - inset * 2.0,
                        360.0 - inset * 2.0,
                    ),
                    radius: (80.0 - inset) * frame.s,
                    color: color::alpha(color::SPACE, 0.14),
                });
            }
        }
        match self.tab {
            Tab::Medals => self.medals(&frame, inputs),
            Tab::Achievements => self.achievements(&frame, inputs),
            Tab::Shaders => self.shaders(&frame, inputs, seconds),
            Tab::Toys => self.toys(&frame, inputs),
            Tab::Cards => self.cards(&frame, inputs, seconds),
            _ => self.nameplates(&frame, inputs),
        }
        self.keys(&frame, inputs);
        self.hover_card(&frame, inputs);
        self.ui.finish(self.focus_token());
    }

    /// How many of all the things the player holds, at the top right: the words, and a
    /// tick for each thing, lit for the ones held, a group to a tab.
    fn tally(&mut self, frame: &Frame, counts: &[Count; 7]) {
        let s = frame.s;
        // Only the things with a set total: the holocrons are opened, not collected.
        let total: usize = counts.iter().map(|count| count.total).sum();
        let held: usize = counts
            .iter()
            .filter(|count| count.total > 0)
            .filter_map(|count| count.held)
            .sum();
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{held} of {total} collected"),
            frame.rect(RIGHT_X - 500.0, 58.0, 500.0, 30.0),
            22.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::End,
        );
        let groups: Vec<Count> = counts.iter().copied().filter(|c| c.total > 0).collect();
        let width: f32 = groups
            .iter()
            .map(|count| count.total as f32 * (TICK[0] + TICK_GAP) - TICK_GAP)
            .sum::<f32>()
            + GROUP_GAP * groups.len().saturating_sub(1) as f32;
        let mut x = RIGHT_X - width;
        for count in groups {
            let held = count.held.unwrap_or(0);
            for index in 0..count.total {
                let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                    rect: frame.rect(x, 96.0, TICK[0], TICK[1]),
                    radius: 1.0 * s,
                    color: if index < held {
                        color::GOLD_BRIGHT
                    } else {
                        color::alpha(color::HOLO, 0.24)
                    },
                });
                x += TICK[0] + TICK_GAP;
            }
            x += GROUP_GAP - TICK_GAP;
        }
    }

    /// A tab's headline at the top left and the line under it.
    pub(super) fn lead(&mut self, frame: &Frame, headline: &str, line: &str) {
        let s = frame.s;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{headline}"),
            frame.rect(LEFT_X, LEAD_Y, 900.0, 40.0),
            30.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        if !line.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(LEFT_X, LEAD_Y + 42.0, 940.0, 24.0),
                16.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// The keys of the tab on show, right-aligned at the bottom.
    fn keys(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        let mut keys: Vec<(&[&str], &str)> = Vec::with_capacity(6);
        match self.tab {
            Tab::Medals => keys.push((&["Left", "Right"], "choose")),
            Tab::Cards if self.cards_shown > 1 => keys.push((&["Left", "Right"], "choose")),
            Tab::Achievements => keys.push((&["Arrows"], "choose")),
            Tab::Shaders => {
                if self.grid {
                    keys.push((&["Arrows"], "choose"));
                    keys.push((&["V"], "list"));
                } else {
                    keys.push((&["Up", "Down"], "choose"));
                    keys.push((&["V"], "grid"));
                }
                keys.push((&["/"], "search"));
                let worn = self.shader_rows.get(self.shader).and_then(|row| row.wear);
                match worn {
                    Some("") if self.shader > 0 => keys.push((&["Enter"], "unequip")),
                    Some(_) => keys.push((&["Enter"], "equip")),
                    None => {}
                }
            }
            Tab::Toys => keys.push((
                &["Enter"],
                if inputs.illuminate {
                    "put out"
                } else {
                    "light"
                },
            )),
            _ => {}
        }
        keys.push((&["Ctrl", "Tab"], self.tab.next(true).label()));
        keys.push((&["Esc"], "back"));
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (keys.len() - 1) as f32;
        let [right, y] = frame.point(RIGHT_X, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

/// Dark behind the things on the left, fading out before the model; light fades at the
/// top behind the title and at the bottom behind the showcase's words and the keys.
fn stage_scrims(canvas: &mut crate::menu_widgets::MenuCanvas, viewport: [f32; 2], frame: &Frame) {
    let [width, height] = viewport;
    let space = |alpha| color::alpha(color::SPACE, alpha);
    let x = |frame_x: f32| frame.point(frame_x, 0.0)[0];
    let y = |frame_y: f32| frame.point(0.0, frame_y)[1];
    fade_across(
        canvas,
        sjk_ui::Rect::new(0.0, 0.0, x(780.0), height),
        space(0.94),
        space(0.86),
    );
    fade_across(
        canvas,
        sjk_ui::Rect::new(x(780.0), 0.0, x(1_060.0) - x(780.0), height),
        space(0.86),
        space(0.0),
    );
    fade(
        canvas,
        sjk_ui::Rect::new(0.0, 0.0, width, y(250.0)),
        space(0.7),
        space(0.0),
    );
    fade(
        canvas,
        sjk_ui::Rect::new(0.0, y(560.0), width, height - y(560.0)),
        space(0.0),
        space(0.86),
    );
}
