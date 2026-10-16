//! The Collection's Cards tab (`docs/cards.md`, "In the client"): the cards opening
//! holocrons brought and the player's credits. On the left the copies the player holds,
//! one card per kind (card, rarity, finish, edition) with how many, the best first; on
//! the right the one chosen up close, big enough to read, in its finish's light, with
//! what it is. A revealed card shows its face from the SJK packs
//! ([`crate::cards::faces`]); a sealed one its back until it is revealed. When the packs
//! have faces of a sealed copy's rarity, Reveal (or R) asks the hub to reveal them, and a
//! line under it says what came of it. The cards come from the player's own hub profile
//! ([`crate::cards::stacks`]).

use super::view::{LEAD_Y, LEFT_X, SHOWCASE_WIDTH, SHOWCASE_X, centred_lines};
use super::*;
use crate::cards::{self, Stack, look};
use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::TextFamily;
use sjk_ui::{DrawCommand, FontWeight, TextAlign};

/// The cards on the left: their width, the room each takes across and down, how many to
/// a line and the lines shown.
const CARD_WIDTH: f32 = 128.0;
const STEP_X: f32 = 150.0;
const STEP_Y: f32 = 236.0;
const COLUMNS: usize = 6;
const LINES: usize = 2;
/// The most kinds the tab shows; more are counted under them.
pub(super) const SHOWN_MAX: usize = COLUMNS * LINES;
/// The first line's top.
const GRID_TOP: f32 = 330.0;
/// The card up close: wide enough that a face's lore reads at 1080p.
const BIG_WIDTH: f32 = 470.0;
const BIG_TOP: f32 = 150.0;
/// Reveal, under the grid.
const REVEAL_Y: f32 = 860.0;
const REVEAL_WIDTH: f32 = 330.0;

/// How many cards the player holds, once the hub answered with the identity on.
pub(super) fn held(inputs: &Inputs<'_>) -> Option<usize> {
    me(inputs).map(|me| cards::total(&cards::stacks(&me.cards)) as usize)
}

/// The own profile, once known with the identity on.
fn me<'a>(inputs: &Inputs<'a>) -> Option<&'a sjk_identity::Profile> {
    inputs
        .snapshot
        .filter(|_| inputs.enabled)
        .and_then(|snapshot| snapshot.me.as_ref())
}

/// What the tab says at its top.
fn headline(inputs: &Inputs<'_>, stacks: Option<&[Stack]>) -> (String, &'static str) {
    let sealed = stacks.is_some_and(|stacks| stacks.iter().any(Stack::sealed));
    match (inputs.enabled, stacks) {
        (false, _) => (
            "Identity is off".to_owned(),
            "Your cards are kept on the SJK hub: switch the identity on in Settings, Network, to see them.",
        ),
        (true, None) => (
            "Contacting the hub...".to_owned(),
            "Your cards show once the SJK hub has answered.",
        ),
        (true, Some([])) => (
            "No cards yet".to_owned(),
            "Open a holocron on the Holocrons tab: each holds a card and credits.",
        ),
        (true, Some(stacks)) => {
            let total = cards::total(stacks);
            (
                if total == 1 {
                    "1 card".to_owned()
                } else {
                    format!("{total} cards")
                },
                if sealed {
                    "A sealed card shows its face once revealed: its card is picked then."
                } else {
                    "Every card you hold, from the holocrons you opened."
                },
            )
        }
    }
}

impl Panel {
    /// The Cards tab.
    pub(super) fn cards(&mut self, frame: &Frame, inputs: &Inputs<'_>, seconds: f32) {
        let (stacks, credits) = self.holdings(inputs);
        let (headline, line) = headline(inputs, stacks.as_deref());
        self.lead(frame, &headline, line);
        if let Some(credits) = credits {
            self.balance(frame, credits);
        }
        let stacks = stacks.unwrap_or_default();
        let faces = cards::faces::current();
        let can_reveal = inputs
            .snapshot
            .and_then(|snapshot| snapshot.holocrons)
            .is_some_and(|progress| progress.can_open);
        self.revealable = if can_reveal {
            cards::revealable(&stacks, &faces)
        } else {
            0
        };
        self.cards_shown = stacks.len().min(SHOWN_MAX);
        self.card = self.card.min(self.cards_shown.saturating_sub(1));
        if stacks.is_empty() {
            // A dark outline where the first card will lie.
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect: frame.rect(LEFT_X, GRID_TOP, CARD_WIDTH, CARD_WIDTH * look::ASPECT),
                radius: CARD_WIDTH * 0.07 * frame.s,
                width: 1.5 * frame.s,
                color: color::alpha(color::HOLO, 0.22),
            });
            return;
        }
        for (index, stack) in stacks.iter().take(SHOWN_MAX).enumerate() {
            self.small_card(frame, stack, &faces, index, seconds);
        }
        self.reveal_button(frame);
        if stacks.len() > SHOWN_MAX {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("And {} more kinds", stacks.len() - SHOWN_MAX),
                frame.rect(LEFT_X, GRID_TOP + STEP_Y * LINES as f32, 600.0, 24.0),
                15.0 * frame.s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if let Some(chosen) = stacks.get(self.card) {
            self.card_up_close(frame, chosen, &faces, seconds);
        }
    }

    /// The player's cards and credits, once known (a world shot's own in its place).
    pub(super) fn holdings(&self, inputs: &Inputs<'_>) -> (Option<Vec<Stack>>, Option<u64>) {
        #[cfg(test)]
        if let Some((list, credits)) = &self.preview_cards {
            return (Some(cards::stacks(list)), Some(*credits));
        }
        let me = me(inputs);
        (
            me.map(|me| cards::stacks(&me.cards)),
            me.map(|me| me.credits),
        )
    }

    /// The player's credits, at the right of the tab's headline.
    fn balance(&mut self, frame: &Frame, credits: u64) {
        let s = frame.s;
        let words = cards::credits_text(credits);
        let size = 30.0;
        let width = crate::text::display_width(&words, size) * crate::text::style::current().scale;
        let right = SHOWCASE_X - 40.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{words}"),
            frame.rect(right - width - 8.0, LEAD_Y, width + 8.0, 40.0),
            size * s,
            color::GOLD_BRIGHT,
            FontWeight::Semibold,
            TextAlign::End,
        );
        let mark = 24.0;
        let [x, y] = frame.point(
            right - width - 8.0 - 10.0 - cards::mark::width(mark),
            LEAD_Y + 8.0,
        );
        cards::mark::draw(self.ui.draw_list_mut(), [x, y], mark * s, color::GOLD);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("credits"),
            frame.rect(right - 200.0, LEAD_Y + 42.0, 200.0, 24.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::End,
        );
    }

    /// How card `key` (0 the card up close, 1 and up the grid's) at `[x, y, width]` (frame
    /// pixels) tilts towards the pointer at `seconds`: eased in while the pointer rests on
    /// it, eased back out when it leaves; one card tilts at a time.
    fn card_tilt(
        &mut self,
        frame: &Frame,
        [x, y, width]: [f32; 3],
        key: usize,
        seconds: f32,
    ) -> Option<look::Tilt> {
        let height = width * look::ASPECT;
        let rect = frame.rect(x, y, width, height);
        let over = self.pointer.filter(|[px, py]| {
            *px >= rect.x && *px <= rect.right() && *py >= rect.y && *py <= rect.bottom()
        });
        let tilt = &mut self.tilt;
        if let Some([px, py]) = over {
            if tilt.key != Some(key) {
                *tilt = CardTilt {
                    key: Some(key),
                    at: seconds,
                    ..CardTilt::default()
                };
            }
            tilt.target = [
                (px - rect.x - rect.width * 0.5) / (rect.width * 0.5),
                (py - rect.y - rect.height * 0.5) / (rect.height * 0.5),
            ];
            tilt.over = true;
        } else if tilt.key == Some(key) {
            tilt.over = false;
        } else {
            return None;
        }
        // Eased towards the pointer, and in or out, about a tenth of a second.
        let step = 1.0 - (-(seconds - tilt.at).max(0.0) * 12.0).exp();
        tilt.at = seconds;
        for axis in 0..2 {
            tilt.value[axis] += (tilt.target[axis] - tilt.value[axis]) * step;
        }
        let goal = if tilt.over { 1.0 } else { 0.0 };
        tilt.strength += (goal - tilt.strength) * step;
        if !tilt.over && tilt.strength < 0.01 {
            *tilt = CardTilt::default();
            return None;
        }
        Some(look::Tilt {
            x: tilt.value[0] * tilt.strength,
            y: tilt.value[1] * tilt.strength,
        })
    }

    /// Reveal under the grid, when sealed copies can be revealed, and under it what the
    /// last reveal asked here brought.
    fn reveal_button(&mut self, frame: &Frame) {
        let outcome = self.reveal_asked.and_then(|asked| {
            crate::player_identity::reveal_outcome().filter(|outcome| outcome.serial > asked)
        });
        if self.revealable > 0 {
            let waiting = self.reveal_asked.is_some() && outcome.is_none();
            let label = if waiting {
                "Revealing...".to_owned()
            } else if self.revealable == 1 {
                "Reveal 1 sealed card".to_owned()
            } else {
                format!("Reveal {} sealed cards", self.revealable)
            };
            crate::menu::sjk::kit::button(
                &mut self.ui,
                frame,
                [LEFT_X, REVEAL_Y, REVEAL_WIDTH, 44.0],
                &label,
                true,
                !waiting,
                false,
                REVEAL_TOKEN,
            );
        }
        let Some(outcome) = outcome else {
            return;
        };
        let words = match (outcome.revealed.len(), outcome.message.is_empty()) {
            (_, false) => outcome.message.clone(),
            (0, true) => "Nothing to reveal yet: no card of their rarity is out.".to_owned(),
            (1, true) => "1 card revealed.".to_owned(),
            (count, true) => format!("{count} cards revealed."),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{words}"),
            frame.rect(LEFT_X, REVEAL_Y + 54.0, 900.0, 24.0),
            16.0 * frame.s,
            if outcome.message.is_empty() {
                color::GOLD_BRIGHT
            } else {
                color::EMBER
            },
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// Kind `index` on the left: its card, how many, and its finish.
    fn small_card(
        &mut self,
        frame: &Frame,
        stack: &Stack,
        faces: &cards::faces::Faces,
        index: usize,
        seconds: f32,
    ) {
        let s = frame.s;
        let x = LEFT_X + (index % COLUMNS) as f32 * STEP_X;
        let y = GRID_TOP + (index / COLUMNS) as f32 * STEP_Y;
        let chosen = index == self.card;
        let token = CARD_BASE + index as u16;
        let height = CARD_WIDTH * look::ASPECT;
        if chosen {
            let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                rect: frame.rect(x - 5.0, y - 5.0, CARD_WIDTH + 10.0, height + 10.0),
                radius: (CARD_WIDTH * 0.07 + 5.0) * s,
                width: 2.0 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        let tilt = self.card_tilt(frame, [x, y, CARD_WIDTH], index + 1, seconds);
        look::draw(
            &mut self.ui,
            frame,
            [x, y, CARD_WIDTH],
            look::Card {
                rarity: stack.rarity,
                finish: stack.finish,
                edition: "",
                face: cards::shown(faces, stack.card.as_deref()),
                tilt,
            },
            seconds,
            1.0,
            1.0,
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("x{}", stack.count),
            frame.rect(x, y + height + 6.0, CARD_WIDTH * 0.5, 24.0),
            19.0 * s,
            if chosen {
                color::GOLD_BRIGHT
            } else {
                color::TEXT
            },
            FontWeight::Semibold,
            TextAlign::Start,
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", stack.finish.name),
            frame.rect(
                x + CARD_WIDTH * 0.4,
                y + height + 8.0,
                CARD_WIDTH * 0.6,
                20.0,
            ),
            14.0 * s,
            if stack.finish.index == 2 {
                color::GOLD
            } else {
                color::MUTED
            },
            FontWeight::Regular,
            TextAlign::End,
        );
        self.ui.hit_region(
            token,
            frame.rect(x - 5.0, y - 5.0, CARD_WIDTH + 10.0, height + 36.0),
        );
    }

    /// The kind chosen, up close on the right.
    fn card_up_close(
        &mut self,
        frame: &Frame,
        stack: &Stack,
        faces: &cards::faces::Faces,
        seconds: f32,
    ) {
        let s = frame.s;
        let middle = SHOWCASE_X + SHOWCASE_WIDTH * 0.5;
        let shown = cards::shown(faces, stack.card.as_deref());
        let tilt = self.card_tilt(
            frame,
            [middle - BIG_WIDTH * 0.5, BIG_TOP, BIG_WIDTH],
            0,
            seconds,
        );
        look::draw(
            &mut self.ui,
            frame,
            [middle - BIG_WIDTH * 0.5, BIG_TOP, BIG_WIDTH],
            look::Card {
                rarity: stack.rarity,
                finish: stack.finish,
                edition: &stack.edition,
                face: shown,
                tilt,
            },
            seconds,
            1.0,
            1.0,
        );
        let mut y = BIG_TOP + BIG_WIDTH * look::ASPECT + 18.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", stack.name_in(faces)),
            frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 40.0),
            32.0 * s,
            if stack.finish.index == 2 {
                color::GOLD_BRIGHT
            } else {
                stack.rarity.colour
            },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        y += 46.0;
        let first = crate::medals::date_text(stack.first);
        let held = match (stack.count, first.is_empty()) {
            (1, true) => "1 held".to_owned(),
            (1, false) => format!("1 held, found {first}"),
            (count, true) => format!("{count} held"),
            (count, false) => format!("{count} held, the first found {first}"),
        };
        let edition = match (stack.sealed(), stack.edition.is_empty()) {
            (true, true) => held,
            (true, false) => format!("{}. {held}", stack.edition),
            (false, true) => format!("{}, {}. {held}", stack.rarity.name, stack.finish.name),
            (false, false) => format!(
                "{}, {}. {}. {held}",
                stack.rarity.name, stack.finish.name, stack.edition
            ),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{edition}"),
            frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 26.0),
            18.0 * s,
            color::TEXT,
            FontWeight::Regular,
            TextAlign::Center,
        );
        y += 34.0;
        if let Some(shown) = shown {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{}, {}: card {} of {}",
                    shown.face.kind.name(),
                    shown.face.set_name,
                    shown.face.number,
                    shown.of
                ),
                frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 24.0),
                16.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Center,
            );
            return;
        }
        let y = centred_lines(
            &mut self.ui,
            frame,
            stack.finish.about,
            [SHOWCASE_X, y, SHOWCASE_WIDTH],
            70,
            2,
            16.0,
            color::MUTED,
        );
        if stack.sealed() {
            centred_lines(
                &mut self.ui,
                frame,
                "Sealed: its card is picked when it is revealed.",
                [SHOWCASE_X, y + 4.0, SHOWCASE_WIDTH],
                70,
                2,
                15.0,
                color::QUIET,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::inputs;
    use super::*;
    use crate::unlockables::Holdings;

    /// Every kind of sealed card: each rarity in each finish, the counts rising.
    pub(crate) fn every_kind() -> Vec<sjk_identity::CardStack> {
        let mut list = Vec::new();
        for finish in &cards::FINISHES {
            for rarity in &cards::RARITIES {
                list.push(sjk_identity::CardStack {
                    card: None,
                    rarity: rarity.id.to_owned(),
                    finish: finish.id.to_owned(),
                    edition: "alpha".to_owned(),
                    count: (list.len() as u32 % 4) + 1,
                    first: 1_791_641_100,
                    last: 1_791_641_100,
                });
            }
        }
        list
    }

    /// Every state fits the canvas: no identity, waiting, none, one kind and more kinds
    /// than the tab shows; the arrows walk the kinds shown.
    #[test]
    fn the_tab_shows_the_kinds_and_fits() {
        let fonts = crate::text::load_modern(1.0, None).expect("Inter");
        let shown = inputs(Holdings::Known(&[]), "");
        let mut many = every_kind();
        many.push(sjk_identity::CardStack {
            edition: "beta".to_owned(),
            ..many[0].clone()
        });
        assert!(many.len() > SHOWN_MAX);
        let states = [
            None,
            Some((Vec::new(), 0)),
            Some((many[..1].to_vec(), 35)),
            Some((many.clone(), 123_456)),
        ];
        for state in &states {
            let mut panel = Panel::new();
            panel.open(Tab::Cards, true, true, ReturnTarget::MainMenu);
            panel.preview_cards = state.clone();
            for viewport in [[1920.0, 1080.0], [3840.0, 2160.0], [1440.0, 1080.0]] {
                panel.build(&shown, &fonts.font, viewport);
                assert!(!panel.overflowed(), "{viewport:?}");
            }
        }
        let mut panel = Panel::new();
        panel.open(Tab::Cards, true, true, ReturnTarget::MainMenu);
        panel.preview_cards = states[3].clone();
        panel.build(&shown, &fonts.font, [1920.0, 1080.0]);
        assert_eq!(panel.cards_shown, SHOWN_MAX);
        let _ = panel.key(KeyCode::ArrowRight, false, true);
        let _ = panel.key(KeyCode::ArrowRight, false, true);
        assert_eq!(panel.card, 2);
        for _ in 0..40 {
            let _ = panel.key(KeyCode::ArrowRight, false, true);
        }
        assert_eq!(
            panel.card,
            SHOWN_MAX - 1,
            "the arrows stop at the last shown"
        );
        assert!(
            panel.ui.rect_for(CARD_BASE).is_some(),
            "a card answers the pointer"
        );
    }
}
