//! The Staff page's drawing, in the SJK UI's look: the search pill in the top bar,
//! the players found down the left, the chosen player's picture and name (with Take
//! picture down) over their medals with Give and Take back and their unlockables with
//! Unlock and Relock in the middle, and their
//! achievements with Clear on the right. Under the players, the holocrons: a tier to
//! choose, Give holocron (with the note) and the chosen player's recent holocrons with
//! Remove. Verify sits beside Take picture down; the right column's Keys and merge tab
//! lists the player's keys with Unlink and merges another player into them, its Logs
//! tab asks the player for their game's logs and lists the requests with Open, and its
//! Crashes tab lists every player's crash reports by kind, then one kind's with Open.

use super::*;
use crate::achievements;
use crate::medals::Medal;
use crate::menu::sjk::{
    Frame, SearchPill, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar, wrap,
};
use crate::menu_widgets::TextFamily;
use crate::text::UiFont;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The columns (frame pixels).
const LEFT_X: f32 = 96.0;
const LEFT_WIDTH: f32 = 560.0;
const MIDDLE_X: f32 = 720.0;
const MIDDLE_WIDTH: f32 = 520.0;
const RIGHT_X: f32 = 1_300.0;
const RIGHT_WIDTH: f32 = 524.0;
const TOP: f32 = 170.0;
/// The players' rows.
const PLAYERS_TOP: f32 = 236.0;
const PLAYER_ROW: f32 = 46.0;
/// The medals' rows and the achievements' rows.
const SECTION_TOP: f32 = 330.0;
const MEDAL_ROW: f32 = 64.0;
/// An unlockable's cell, two to a row: compact, so the whole catalogue (fourteen blade
/// skins since 10/10/2026) fits above the keys.
const UNLOCK_ROW: f32 = 36.0;
const UNLOCK_GAP: f32 = 14.0;
const ACHIEVEMENT_ROW: f32 = 34.0;
const ACHIEVEMENTS_SHOWN: usize = 16;
/// The Logs and Crashes views' rows.
const LOG_ROW: f32 = 36.0;
const CRASH_GROUP_ROW: f32 = 52.0;
/// The holocrons' section under the players: its heading, the tier chips, and its rows.
const HOLOCRONS_TOP: f32 = 770.0;
const TIER_CHIP: [f32; 2] = [104.0, 42.0];
const HOLOCRON_ROW: f32 = 34.0;
/// The Keys and merge view: the linked keys' rows, then the merge section.
const LINKED_ROW: f32 = 36.0;
const MERGE_TOP: f32 = 640.0;
/// The keys' line.
const KEYS_Y: f32 = 1_010.0;

/// `text` cut to `chars` characters with an ellipsis.
fn cut(text: &str, chars: usize) -> String {
    if text.chars().count() <= chars {
        return text.to_owned();
    }
    let kept: String = text.chars().take(chars.saturating_sub(3)).collect();
    format!("{kept}...")
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
        self.build(inputs, body, viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the page out; `_body` is the body family, kept for measured layouts.
    pub(super) fn build(&mut self, inputs: &Inputs<'_>, _body: &UiFont, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        self.order.clear();
        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        let caret = (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let query = if self.focus == SEARCH_TOKEN && caret {
            format!("{}|", self.query)
        } else {
            self.query.clone()
        };
        top_bar(
            &mut self.ui,
            &frame,
            "Back",
            BACK_TOKEN,
            "Staff",
            Some(SearchPill {
                query: &query,
                active: self.focus == SEARCH_TOKEN,
                prompt: "Find a player by name or key id",
                found: (!inputs.staff.players.is_empty()).then_some(inputs.staff.players.len()),
                token: SEARCH_TOKEN,
            }),
        );
        self.order.push(SEARCH_TOKEN);
        self.shown.me = inputs.me.map(|me| me.key_id.clone());
        self.players(&frame, inputs);
        let target = self.target(inputs).cloned();
        self.shown.target = target.as_ref().map(|profile| profile.key_id.clone());
        self.shown.medals = [0; Medal::COUNT];
        self.shown.picture = target
            .as_ref()
            .is_some_and(|profile| !profile.avatar.is_empty());
        self.shown.unlocks = vec![false; crate::unlockables::ALL.len()];
        self.shown.holocron_rows = 0;
        self.shown.verified = target.as_ref().is_some_and(|profile| profile.verified);
        self.shown.linked.clear();
        self.shown.log_rows.clear();
        self.shown.crash_groups.clear();
        self.shown.crash_rows.clear();
        if let Some(profile) = &target {
            self.shown
                .linked
                .extend(profile.linked_keys().take(LINKED_SHOWN).map(str::to_owned));
        }
        self.shown.merge_problem = Self::merge_problem(
            &merge_key(&self.merge_from),
            target.as_ref(),
            &inputs.staff.players,
        );
        // A merge confirmed for another player than the one now chosen is let go.
        if self
            .pending_merge
            .as_ref()
            .is_some_and(|merge| self.shown.target.as_ref() != Some(&merge.kept))
        {
            self.pending_merge = None;
        }
        if let Some(profile) = &target {
            for medal in &profile.medals {
                if let Some(known) = Medal::from_id(&medal.id) {
                    self.shown.medals[known.index()] = medal.count.max(1);
                }
            }
            for (held, unlockable) in self.shown.unlocks.iter_mut().zip(&crate::unlockables::ALL) {
                *held = profile
                    .unlocks
                    .iter()
                    .any(|unlock| unlock.id == unlockable.id);
            }
            let mine = self.shown.me.as_deref() == Some(profile.key_id.as_str());
            self.header(&frame, profile, mine);
            self.medals(&frame, profile);
            self.unlockables(&frame, profile);
            self.holocrons(&frame, profile);
            self.right_tabs(&frame);
            match self.right {
                RightView::Achievements => self.achievements(&frame, profile, mine),
                RightView::Keys => self.keys_and_merge(&frame, profile, &inputs.staff.players),
                RightView::Logs => self.logs(&frame, profile, inputs.staff),
                RightView::Crashes => self.crashes(&frame, inputs.staff),
            }
        }
        self.status(&frame, inputs.staff);
        self.keys(&frame);
        if !self.order.contains(&self.focus) {
            self.focus = SEARCH_TOKEN;
        }
        self.ui.finish(self.focus);
    }

    /// The players found, with Me and Recent over them.
    fn players(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let s = frame.s;
        for (index, (label, token)) in [("Me", ME_TOKEN), ("Seen lately", RECENT_TOKEN)]
            .into_iter()
            .enumerate()
        {
            let width = if index == 0 { 90.0 } else { 160.0 };
            let x = LEFT_X + index as f32 * 104.0;
            kit::button(
                &mut self.ui,
                frame,
                [x, TOP - 2.0, width, 42.0],
                label,
                false,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
        }
        self.shown.players.clear();
        let selected = self.shown_selection(inputs);
        for (index, player) in inputs.staff.players.iter().take(PLAYERS_SHOWN).enumerate() {
            let token = PLAYER_BASE + index as u16;
            let y = PLAYERS_TOP + index as f32 * PLAYER_ROW;
            let row = [LEFT_X, y, LEFT_WIDTH, PLAYER_ROW - 4.0];
            if self.focus == token || selected.as_deref() == Some(player.key_id.as_str()) {
                kit::band(&mut self.ui, frame, row);
            }
            let name = if player.name.is_empty() {
                "(no name yet)".to_owned()
            } else {
                cut(&player.name, 34)
            };
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{name}"),
                frame.rect(LEFT_X + 18.0, y + 2.0, LEFT_WIDTH - 150.0, 26.0),
                21.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{}", player.key_id),
                frame.rect(LEFT_X + 18.0, y + 26.0, 220.0, 18.0),
                13.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let tags = match (player.staff, player.verified) {
                (true, true) => "Staff, verified",
                (true, false) => "Staff",
                (false, true) => "Verified",
                (false, false) => "",
            };
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{tags}"),
                frame.rect(LEFT_X + LEFT_WIDTH - 150.0, y + 10.0, 136.0, 24.0),
                16.0 * s,
                color::GOLD_BRIGHT,
                FontWeight::Regular,
                TextAlign::End,
            );
            self.ui
                .hit_region(token, frame.rect(row[0], row[1], row[2], row[3]));
            self.order.push(token);
            self.shown.players.push(player.key_id.clone());
        }
        if inputs.staff.players.len() > PLAYERS_SHOWN {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "and {} more: refine the search",
                    inputs.staff.players.len() - PLAYERS_SHOWN
                ),
                frame.rect(
                    LEFT_X + 18.0,
                    PLAYERS_TOP + PLAYERS_SHOWN as f32 * PLAYER_ROW,
                    LEFT_WIDTH,
                    22.0,
                ),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// The key of the chosen player, as the list marks it.
    fn shown_selection(&self, inputs: &Inputs<'_>) -> Option<String> {
        self.selected
            .clone()
            .or_else(|| inputs.me.map(|me| me.key_id.clone()))
    }

    /// The chosen player's picture, name and facts over the middle and right columns,
    /// and Take picture down at the right.
    fn header(&mut self, frame: &Frame, profile: &Profile, mine: bool) {
        let s = frame.s;
        let name = if profile.name.is_empty() {
            "(no name yet)".to_owned()
        } else {
            cut(&profile.name, 40)
        };
        let radius = 32.0;
        crate::profile_card::avatar(
            &mut self.ui,
            frame.point(MIDDLE_X + radius, TOP + 26.0),
            radius * s,
            &crate::profile_card::Avatar {
                key_id: &profile.key_id,
                version: &profile.avatar,
                name: &profile.name,
                verified: profile.verified,
                preview: false,
                lit: false,
            },
        );
        let name_x = MIDDLE_X + radius * 2.0 + 18.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{name}"),
            frame.rect(name_x, TOP - 6.0, 600.0, 48.0),
            38.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        // Verify shows what pressing it does: Unverify while the player is verified.
        kit::button(
            &mut self.ui,
            frame,
            [1_824.0 - 240.0 - 12.0 - 150.0, TOP, 150.0, 42.0],
            if profile.verified {
                "Unverify"
            } else {
                "Verify"
            },
            !profile.verified,
            true,
            self.focus == VERIFY_TOKEN,
            VERIFY_TOKEN,
        );
        self.order.push(VERIFY_TOKEN);
        kit::button(
            &mut self.ui,
            frame,
            [1_824.0 - 240.0, TOP, 240.0, 42.0],
            "Take picture down",
            false,
            self.shown.picture,
            self.focus == PICTURE_DOWN_TOKEN,
            PICTURE_DOWN_TOKEN,
        );
        self.order.push(PICTURE_DOWN_TOKEN);
        let mut facts = vec![format!("Key id {}", profile.key_id)];
        if profile.verified {
            facts.push("verified".to_owned());
        }
        if profile.staff {
            facts.push("staff".to_owned());
        }
        match profile.linked_keys().count() {
            0 => {}
            1 => facts.push("1 linked key".to_owned()),
            count => facts.push(format!("{count} linked keys")),
        }
        let since = crate::medals::date_text(profile.created);
        if !since.is_empty() {
            facts.push(format!("member since {since}"));
        }
        if mine {
            facts.push("this is you".to_owned());
        }
        if !profile.avatar.is_empty() {
            facts.push("has a picture".to_owned());
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{}", facts.join(", ")),
            frame.rect(name_x, TOP + 46.0, 760.0, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// Every medal of the catalogue with what the player holds, Give and Take back,
    /// then the note sent with the next one.
    fn medals(&mut self, frame: &Frame, profile: &Profile) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            MIDDLE_X,
            SECTION_TOP - 20.0,
            MIDDLE_WIDTH,
            "Medals",
        );
        for medal in Medal::ALL {
            let y = SECTION_TOP + medal.index() as f32 * MEDAL_ROW;
            let held = self.shown.medals[medal.index()];
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(MIDDLE_X, y + 4.0, 54.0, 54.0),
                texture: medal.icon(),
                color: Color::new(1.0, 1.0, 1.0, if held > 0 { 1.0 } else { 0.35 }),
            });
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", medal.label(held)),
                frame.rect(MIDDLE_X + 68.0, y + 4.0, 220.0, 28.0),
                21.0 * s,
                if held > 0 {
                    color::GOLD_BRIGHT
                } else {
                    color::MUTED
                },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let given = profile
                .medals
                .iter()
                .find(|entry| entry.id == medal.id())
                .map(|entry| crate::medals::date_text(entry.awarded))
                .filter(|date| !date.is_empty());
            let line = match (held, given) {
                (0, _) => "Not held".to_owned(),
                (_, Some(date)) => format!("Given {date}"),
                (_, None) => "Held".to_owned(),
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(MIDDLE_X + 68.0, y + 32.0, 200.0, 22.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let can_give = held == 0 || medal.repeatable();
            let give = GIVE_BASE + medal.index() as u16;
            let take = TAKE_BASE + medal.index() as u16;
            let right = MIDDLE_X + MIDDLE_WIDTH;
            kit::button(
                &mut self.ui,
                frame,
                [right - 236.0, y + 10.0, 96.0, 40.0],
                if held > 0 && medal.repeatable() {
                    "Give +1"
                } else {
                    "Give"
                },
                true,
                can_give,
                self.focus == give,
                give,
            );
            kit::button(
                &mut self.ui,
                frame,
                [right - 128.0, y + 10.0, 128.0, 40.0],
                "Take back",
                false,
                held > 0,
                self.focus == take,
                take,
            );
            if can_give {
                self.order.push(give);
            }
            if held > 0 {
                self.order.push(take);
            }
        }
        let y = SECTION_TOP + Medal::COUNT as f32 * MEDAL_ROW + 14.0;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "Note with the next medal, unlock or holocron (optional, everyone reads it)"
            ),
            frame.rect(MIDDLE_X, y, MIDDLE_WIDTH, 22.0),
            15.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let field = [MIDDLE_X, y + 28.0, MIDDLE_WIDTH, 44.0];
        let focused = self.focus == NOTE_TOKEN;
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let shown = if self.note.chars().count() > 56 {
            let tail: String = self
                .note
                .chars()
                .skip(self.note.chars().count() - 53)
                .collect();
            format!("...{tail}")
        } else {
            self.note.clone()
        };
        kit::field(
            &mut self.ui,
            frame,
            field,
            format_args!("{shown}{}", if caret { "|" } else { "" }),
            focused,
            false,
        );
        self.ui.hit_region(
            NOTE_TOKEN,
            frame.rect(field[0], field[1], field[2], field[3]),
        );
        self.order.push(NOTE_TOKEN);
    }

    /// Every unlockable of the catalogue, held or not, with Unlock and Relock, under the
    /// note's field.
    fn unlockables(&mut self, frame: &Frame, profile: &Profile) {
        let s = frame.s;
        let top = SECTION_TOP + Medal::COUNT as f32 * MEDAL_ROW + 128.0;
        kit::heading(
            &mut self.ui,
            frame,
            MIDDLE_X,
            top,
            MIDDLE_WIDTH,
            "Unlockables",
        );
        let cell = (MIDDLE_WIDTH - UNLOCK_GAP) * 0.5;
        let rows = ((KEYS_Y - 16.0 - (top + 18.0)) / UNLOCK_ROW).floor() as usize;
        let per_page = (rows * 2).max(2);
        self.shown.unlock_per_page = per_page;
        let pages = crate::unlockables::ALL.len().div_ceil(per_page);
        self.unlock_page = self.unlock_page.min(pages - 1);
        if pages > 1 {
            let page = self.unlock_page;
            let buttons = [
                (
                    UNLOCK_PREV_TOKEN,
                    "<",
                    MIDDLE_X + MIDDLE_WIDTH - 148.0,
                    page > 0,
                ),
                (
                    UNLOCK_NEXT_TOKEN,
                    ">",
                    MIDDLE_X + MIDDLE_WIDTH - 40.0,
                    page + 1 < pages,
                ),
            ];
            for (token, label, x, enabled) in buttons {
                kit::button(
                    &mut self.ui,
                    frame,
                    [x, top - 4.0, 40.0, 26.0],
                    label,
                    false,
                    enabled,
                    self.focus == token,
                    token,
                );
                if enabled {
                    self.order.push(token);
                }
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{} / {pages}", page + 1),
                frame.rect(MIDDLE_X + MIDDLE_WIDTH - 104.0, top, 60.0, 20.0),
                13.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Center,
            );
        }
        let first = self.unlock_page * per_page;
        let last = (first + per_page).min(crate::unlockables::ALL.len());
        for (index, unlockable) in crate::unlockables::ALL
            .iter()
            .enumerate()
            .take(last)
            .skip(first)
        {
            let slot = index - first;
            let x = MIDDLE_X + (slot % 2) as f32 * (cell + UNLOCK_GAP);
            let y = top + 18.0 + (slot / 2) as f32 * UNLOCK_ROW;
            let held = self.shown.unlocks.get(index).copied().unwrap_or(false);
            // Held: gold, with the day it was given; not held: muted.
            let granted = profile
                .unlocks
                .iter()
                .find(|unlock| unlock.id == unlockable.id)
                .map(|unlock| crate::medals::date_text(unlock.granted))
                .filter(|date| !date.is_empty());
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", unlockable.name),
                frame.rect(x, y + 1.0, cell - 96.0, 20.0),
                16.0 * s,
                if held {
                    color::GOLD_BRIGHT
                } else {
                    color::MUTED
                },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let line = match (held, granted) {
                (false, _) => "Not held".to_owned(),
                (true, Some(date)) => format!("Since {date}"),
                (true, None) => "Held".to_owned(),
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{line}"),
                frame.rect(x, y + 20.0, cell - 96.0, 15.0),
                12.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            // One button: Unlock when not held, Relock when held.
            let token = if held {
                RELOCK_BASE + index as u16
            } else {
                UNLOCK_BASE + index as u16
            };
            kit::button(
                &mut self.ui,
                frame,
                [x + cell - 88.0, y + 3.0, 88.0, 30.0],
                if held { "Relock" } else { "Unlock" },
                !held,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
        }
    }

    /// What the chosen player holds of holocrons, the tier Give holocron gives and the
    /// recent ones with Remove, under the players.
    fn holocrons(&mut self, frame: &Frame, profile: &Profile) {
        use crate::holocrons::{self, TIERS};
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            LEFT_X,
            HOLOCRONS_TOP - 36.0,
            LEFT_WIDTH,
            "Holocrons",
        );
        let counts = holocrons::counts_of(&profile.holocron_counts);
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "Holds {} uncommon, {} rare, {} legendary, {} mythical",
                counts[0], counts[1], counts[2], counts[3]
            ),
            frame.rect(LEFT_X, HOLOCRONS_TOP - 30.0 + 22.0, LEFT_WIDTH, 20.0),
            14.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        // The tier chips: its gem and name, the chosen tier ringed in its colour.
        let chips_y = HOLOCRONS_TOP + 16.0;
        for (index, tier) in TIERS.iter().enumerate() {
            let token = HOLOCRON_TIER_BASE + index as u16;
            let x = LEFT_X + index as f32 * (TIER_CHIP[0] + 8.0);
            kit::button(
                &mut self.ui,
                frame,
                [x, chips_y, TIER_CHIP[0], TIER_CHIP[1]],
                "",
                false,
                true,
                self.focus == token,
                token,
            );
            let chosen = self.holocron_tier == index;
            if chosen {
                let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
                    rect: frame.rect(x, chips_y, TIER_CHIP[0], TIER_CHIP[1]),
                    radius: TIER_CHIP[1] * 0.5 * s,
                    width: 2.0 * s,
                    color: tier.colour,
                });
            }
            let [gem_x, gem_y] = frame.point(x + 24.0, chips_y + TIER_CHIP[1] * 0.5);
            holocrons::gem::draw(
                self.ui.draw_list_mut(),
                [gem_x, gem_y],
                9.0 * s,
                tier.colour,
                if chosen { 1.0 } else { 0.7 },
                holocrons::gem::MARK_ROWS,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", tier.label),
                frame.rect(x + 38.0, chips_y + 6.0, TIER_CHIP[0] - 42.0, 30.0),
                16.0 * s,
                if chosen { color::TEXT } else { color::MUTED },
                FontWeight::Semibold,
                TextAlign::Start,
            );
            self.order.push(token);
        }
        kit::button(
            &mut self.ui,
            frame,
            [
                LEFT_X + 4.0 * (TIER_CHIP[0] + 8.0) + 4.0,
                chips_y,
                LEFT_WIDTH - 4.0 * (TIER_CHIP[0] + 8.0) - 4.0,
                TIER_CHIP[1],
            ],
            "Give",
            true,
            true,
            self.focus == HOLOCRON_GIVE_TOKEN,
            HOLOCRON_GIVE_TOKEN,
        );
        self.order.push(HOLOCRON_GIVE_TOKEN);
        // The recent ones, newest first.
        let entries = holocrons::entries(&profile.holocrons);
        let rows_y = chips_y + TIER_CHIP[1] + 14.0;
        if entries.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("No holocrons yet."),
                frame.rect(LEFT_X, rows_y, LEFT_WIDTH, 24.0),
                16.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        for (row, entry) in entries.iter().take(HOLOCRONS_SHOWN).enumerate() {
            let y = rows_y + row as f32 * HOLOCRON_ROW;
            let token = HOLOCRON_REMOVE_BASE + row as u16;
            let [gem_x, gem_y] = frame.point(LEFT_X + 14.0, y + HOLOCRON_ROW * 0.5 - 2.0);
            holocrons::gem::draw(
                self.ui.draw_list_mut(),
                [gem_x, gem_y],
                10.0 * s,
                entry.tier.colour,
                1.0,
                holocrons::gem::MARK_ROWS,
            );
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", entry.tier.name),
                frame.rect(LEFT_X + 34.0, y, 200.0, 28.0),
                18.0 * s,
                entry.tier.colour,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let kind = if entry.gift { "gift" } else { "play" };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "#{} {} {kind}",
                    entry.id,
                    holocrons::when_text(entry.dropped)
                ),
                frame.rect(LEFT_X + 236.0, y + 2.0, 210.0, 24.0),
                13.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            );
            kit::button(
                &mut self.ui,
                frame,
                [LEFT_X + LEFT_WIDTH - 100.0, y - 1.0, 100.0, 30.0],
                "Remove",
                false,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
            self.shown.holocrons[row] = entry.id;
            self.shown.holocron_rows = row + 1;
        }
        if entries.len() > HOLOCRONS_SHOWN {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("and {} more", entries.len() - HOLOCRONS_SHOWN),
                frame.rect(
                    LEFT_X,
                    rows_y + HOLOCRONS_SHOWN as f32 * HOLOCRON_ROW,
                    LEFT_WIDTH,
                    20.0,
                ),
                13.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
    }

    /// The player's achievements with a count, each with Clear, and Clear all.
    fn achievements(&mut self, frame: &Frame, profile: &Profile, mine: bool) {
        let s = frame.s;
        let confirming = self.confirming(CLEAR_ALL_TOKEN);
        kit::button(
            &mut self.ui,
            frame,
            [
                RIGHT_X + RIGHT_WIDTH - 140.0,
                SECTION_TOP + 4.0,
                140.0,
                34.0,
            ],
            if confirming {
                "Press again"
            } else {
                "Clear all"
            },
            false,
            !profile.achievements.is_empty(),
            self.focus == CLEAR_ALL_TOKEN,
            CLEAR_ALL_TOKEN,
        );
        if !profile.achievements.is_empty() {
            self.order.push(CLEAR_ALL_TOKEN);
        }
        let mut y = SECTION_TOP + 54.0;
        let listed: Vec<(usize, &sjk_identity::Achievement)> = achievements::ALL
            .iter()
            .enumerate()
            .filter_map(|(index, kind)| {
                profile
                    .achievements
                    .iter()
                    .find(|held| held.id == kind.id && held.progress > 0)
                    .map(|held| (index, held))
            })
            .collect();
        if listed.is_empty() {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("No achievements at the hub yet."),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 24.0),
                16.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 34.0;
        }
        for (index, held) in listed.iter().take(ACHIEVEMENTS_SHOWN) {
            let kind = &achievements::ALL[*index];
            let token = CLEAR_BASE + *index as u16;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", kind.name),
                frame.rect(RIGHT_X, y, 230.0, 28.0),
                19.0 * s,
                if held.unlocked > 0 {
                    color::GOLD_BRIGHT
                } else {
                    color::TEXT
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            let status = if held.unlocked > 0 {
                crate::medals::date_text(held.unlocked)
            } else {
                format!(
                    "{} / {}",
                    kind.amount(held.progress),
                    kind.amount(kind.goal)
                )
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{status}"),
                frame.rect(RIGHT_X + 236.0, y + 2.0, 170.0, 24.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            );
            kit::button(
                &mut self.ui,
                frame,
                [RIGHT_X + RIGHT_WIDTH - 96.0, y - 2.0, 96.0, 30.0],
                "Clear",
                false,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
            y += ACHIEVEMENT_ROW;
        }
        if listed.len() > ACHIEVEMENTS_SHOWN {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("and {} more", listed.len() - ACHIEVEMENTS_SHOWN),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 22.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 26.0;
        }
        let note = if mine {
            "Clearing your own also resets your game's count, so the next kill, duel or map unlocks it again."
        } else {
            "Their game sends its own counts again: only what the hub counts stays cleared."
        };
        for part in wrap(note, 72) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(RIGHT_X, y + 6.0, RIGHT_WIDTH, 22.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 22.0;
        }
    }

    /// The right column's four tabs, the one shown filled gold.
    fn right_tabs(&mut self, frame: &Frame) {
        let mut x = RIGHT_X;
        for (label, width, view, token) in [
            (
                "Achievements",
                156.0,
                RightView::Achievements,
                ACHIEVEMENTS_TAB_TOKEN,
            ),
            ("Keys and merge", 176.0, RightView::Keys, KEYS_TAB_TOKEN),
            ("Logs", 72.0, RightView::Logs, LOGS_TAB_TOKEN),
            ("Crashes", 96.0, RightView::Crashes, CRASHES_TAB_TOKEN),
        ] {
            kit::button(
                &mut self.ui,
                frame,
                [x, SECTION_TOP - 40.0, width, 38.0],
                label,
                self.right == view,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
            x += width + 8.0;
        }
    }

    /// One line of quiet text in the right column at `y`.
    fn right_line(&mut self, frame: &Frame, y: f32, line: &str, colour: Color) {
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(RIGHT_X, y, RIGHT_WIDTH, 22.0),
            14.0 * frame.s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// Request logs for the chosen player (with the note), then the log requests,
    /// newest first, each sent one with Open.
    fn logs(&mut self, frame: &Frame, profile: &Profile, staff: &StaffState) {
        let s = frame.s;
        let name = if profile.name.is_empty() {
            "this player".to_owned()
        } else {
            cut(&crate::medals::plain_note(&profile.name), 24)
        };
        let mut y = SECTION_TOP + 6.0;
        let open = staff
            .log_requests
            .iter()
            .any(|row| row.key_id == profile.key_id && row.state == "open");
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X, y, 200.0, 38.0],
            "Request logs",
            true,
            !open,
            self.focus == REQUEST_LOGS_TOKEN,
            REQUEST_LOGS_TOKEN,
        );
        if !open {
            self.order.push(REQUEST_LOGS_TOKEN);
        }
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X + RIGHT_WIDTH - 120.0, y, 120.0, 38.0],
            "Refresh",
            false,
            true,
            self.focus == LOGS_REFRESH_TOKEN,
            LOGS_REFRESH_TOKEN,
        );
        self.order.push(LOGS_REFRESH_TOKEN);
        y += 50.0;
        let explain = if open {
            format!("{name} has a request open; their game asks them when they are in the menu.")
        } else {
            format!(
                "Asks {name} for their game's logs, with the note under the medals. Their \
                 game asks them first; nothing comes without their yes."
            )
        };
        for part in wrap(&explain, 72).take(2) {
            self.right_line(frame, y, part, color::QUIET);
            y += 22.0;
        }
        y += 18.0;
        kit::heading(&mut self.ui, frame, RIGHT_X, y, RIGHT_WIDTH, "Log requests");
        y += 26.0;
        if staff.log_requests.is_empty() {
            self.right_line(
                frame,
                y,
                "None yet, or not read: Refresh reads them.",
                color::MUTED,
            );
        }
        for (row, request) in staff.log_requests.iter().take(LOG_ROWS_SHOWN).enumerate() {
            let token = LOG_OPEN_BASE + row as u16;
            let row_y = y + row as f32 * LOG_ROW;
            let sent = request.state == "sent";
            self.shown.log_rows.push((request.id, sent));
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", cut(&crate::medals::plain_note(&request.name), 18)),
                frame.rect(RIGHT_X, row_y + 2.0, 190.0, 26.0),
                18.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let mut state = match request.state.as_str() {
                "sent" => format!(
                    "sent {}, {} KB",
                    crate::medals::date_text(request.answered),
                    request.size.div_ceil(1024)
                ),
                "open" => format!("asked {}", crate::medals::date_text(request.created)),
                other => format!("{other} {}", crate::medals::date_text(request.created)),
            };
            if !request.staff_name.is_empty() {
                state = format!(
                    "{state}, by {}",
                    cut(&crate::medals::plain_note(&request.staff_name), 10)
                );
            }
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{state}"),
                frame.rect(RIGHT_X + 196.0, row_y + 4.0, 230.0, 24.0),
                14.0 * s,
                if sent {
                    color::GOLD_BRIGHT
                } else {
                    color::QUIET
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            if sent {
                kit::button(
                    &mut self.ui,
                    frame,
                    [RIGHT_X + RIGHT_WIDTH - 84.0, row_y - 1.0, 84.0, 30.0],
                    "Open",
                    false,
                    true,
                    self.focus == token,
                    token,
                );
                self.order.push(token);
            }
        }
    }

    /// Every player's crash reports: the kinds (newest first) to choose, or the chosen
    /// kind's reports with Open, which saves one as a text file and opens it.
    fn crashes(&mut self, frame: &Frame, staff: &StaffState) {
        let s = frame.s;
        let mut y = SECTION_TOP + 6.0;
        let group = self
            .crash_group
            .clone()
            .filter(|signature| *signature == staff.crashes_of);
        let (token, label) = if self.crash_group.is_some() {
            (CRASHES_BACK_TOKEN, "All kinds")
        } else {
            (CRASHES_REFRESH_TOKEN, "Refresh")
        };
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X + RIGHT_WIDTH - 140.0, y, 140.0, 38.0],
            label,
            false,
            true,
            self.focus == token,
            token,
        );
        self.order.push(token);
        let title = if self.crash_group.is_some() {
            "Reports of this kind"
        } else {
            "Kinds of crash, newest first"
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{title}"),
            frame.rect(RIGHT_X, y + 4.0, RIGHT_WIDTH - 150.0, 30.0),
            20.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        y += 54.0;
        match group {
            Some(_) => {
                if staff.crashes.is_empty() {
                    self.right_line(frame, y, "No report of this kind any more.", color::MUTED);
                }
                for (row, crash) in staff.crashes.iter().take(CRASH_ROWS_SHOWN).enumerate() {
                    let token = CRASH_OPEN_BASE + row as u16;
                    let row_y = y + row as f32 * LOG_ROW;
                    self.shown.crash_rows.push(crash.id);
                    text(
                        &mut self.ui,
                        TextFamily::Display,
                        format_args!("{}", cut(&crate::medals::plain_note(&crash.name), 16)),
                        frame.rect(RIGHT_X, row_y + 2.0, 170.0, 26.0),
                        18.0 * s,
                        color::TEXT,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!(
                            "{}, {}",
                            crate::medals::date_text(crash.created),
                            cut(&crash.build, 14)
                        ),
                        frame.rect(RIGHT_X + 176.0, row_y + 4.0, 250.0, 24.0),
                        14.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    kit::button(
                        &mut self.ui,
                        frame,
                        [RIGHT_X + RIGHT_WIDTH - 84.0, row_y - 1.0, 84.0, 30.0],
                        "Open",
                        false,
                        true,
                        self.focus == token,
                        token,
                    );
                    self.order.push(token);
                }
            }
            None if self.crash_group.is_some() => {
                self.right_line(frame, y, "Reading...", color::MUTED);
            }
            None => {
                if staff.crash_groups.is_empty() {
                    self.right_line(
                        frame,
                        y,
                        "No crash reports, or not read: Refresh reads them.",
                        color::MUTED,
                    );
                }
                for (row, kind) in staff
                    .crash_groups
                    .iter()
                    .take(CRASH_GROUPS_SHOWN)
                    .enumerate()
                {
                    let token = CRASH_GROUP_BASE + row as u16;
                    let row_y = y + row as f32 * CRASH_GROUP_ROW;
                    self.shown.crash_groups.push(kind.signature.clone());
                    let place = kind.location.rsplit(['/', '\\']).next().unwrap_or_default();
                    kit::button(
                        &mut self.ui,
                        frame,
                        [RIGHT_X, row_y - 2.0, RIGHT_WIDTH, CRASH_GROUP_ROW - 6.0],
                        "",
                        false,
                        true,
                        self.focus == token,
                        token,
                    );
                    self.order.push(token);
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!(
                            "{}{}{}, {} report{} from {} player{}, last {}",
                            kind.kind,
                            if place.is_empty() { "" } else { " in " },
                            cut(place, 24),
                            kind.count,
                            if kind.count == 1 { "" } else { "s" },
                            kind.keys,
                            if kind.keys == 1 { "" } else { "s" },
                            crate::medals::date_text(kind.last)
                        ),
                        frame.rect(RIGHT_X + 12.0, row_y + 1.0, RIGHT_WIDTH - 24.0, 22.0),
                        15.0 * s,
                        color::TEXT,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("{}", cut(&kind.message, 62)),
                        frame.rect(RIGHT_X + 12.0, row_y + 22.0, RIGHT_WIDTH - 24.0, 20.0),
                        13.0 * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
            }
        }
    }

    /// The chosen player's keys, each linked one with Unlink, then the merge: the key
    /// of the player to merge away and Merge, or the confirmation saying who is kept
    /// and who goes.
    fn keys_and_merge(&mut self, frame: &Frame, profile: &Profile, players: &[Profile]) {
        let s = frame.s;
        let mut y = SECTION_TOP + 10.0;
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("Main key {}", profile.key_id),
            frame.rect(RIGHT_X, y, RIGHT_WIDTH, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        y += 34.0;
        let linked: Vec<&str> = profile.linked_keys().collect();
        if linked.is_empty() {
            for part in wrap(
                "No linked keys. The keys of a player merged into this one are linked here.",
                72,
            ) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(RIGHT_X, y, RIGHT_WIDTH, 22.0),
                    14.0 * s,
                    color::QUIET,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 22.0;
            }
        }
        for (row, key) in linked.iter().take(LINKED_SHOWN).enumerate() {
            let token = UNLINK_BASE + row as u16;
            let row_y = y + row as f32 * LINKED_ROW;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("Linked {key}"),
                frame.rect(RIGHT_X, row_y + 2.0, RIGHT_WIDTH - 150.0, 26.0),
                18.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            let label = if self.confirming(token) {
                "Press again"
            } else {
                "Unlink"
            };
            kit::button(
                &mut self.ui,
                frame,
                [RIGHT_X + RIGHT_WIDTH - 140.0, row_y - 1.0, 140.0, 30.0],
                label,
                false,
                true,
                self.focus == token,
                token,
            );
            self.order.push(token);
        }
        if linked.len() > LINKED_SHOWN {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("and {} more", linked.len() - LINKED_SHOWN),
                frame.rect(
                    RIGHT_X,
                    y + LINKED_SHOWN as f32 * LINKED_ROW,
                    RIGHT_WIDTH,
                    20.0,
                ),
                13.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        if !linked.is_empty() {
            let rows = linked.len().min(LINKED_SHOWN) as f32 * LINKED_ROW;
            let more = if linked.len() > LINKED_SHOWN {
                22.0
            } else {
                0.0
            };
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("An unlinked key starts afresh; nothing moves back."),
                frame.rect(RIGHT_X, y + rows + more + 4.0, RIGHT_WIDTH, 20.0),
                13.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        kit::heading(
            &mut self.ui,
            frame,
            RIGHT_X,
            MERGE_TOP,
            RIGHT_WIDTH,
            "Merge another key into this player",
        );
        match self.pending_merge.clone() {
            Some(merge) => self.merge_confirmation(frame, profile, &merge, players),
            None => self.merge_entry(frame),
        }
    }

    /// The merge field with Pick in list, what is wrong with it, and Merge.
    fn merge_entry(&mut self, frame: &Frame) {
        let s = frame.s;
        let mut y = MERGE_TOP + 30.0;
        for part in wrap(
            "For a player who reset their key: this player is kept, the other one's medals, unlocks, achievements, holocrons and names move here and their keys become linked keys.",
            72,
        ) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 22.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 22.0;
        }
        let y = MERGE_TOP + 104.0;
        let pick_width = 150.0;
        let field = [RIGHT_X, y, RIGHT_WIDTH - pick_width - 10.0, 44.0];
        let focused = self.focus == MERGE_FIELD_TOKEN;
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        kit::field(
            &mut self.ui,
            frame,
            field,
            format_args!(
                "{}{}",
                cut(&self.merge_from, 28),
                if caret { "|" } else { "" }
            ),
            focused,
            false,
        );
        self.ui.hit_region(
            MERGE_FIELD_TOKEN,
            frame.rect(field[0], field[1], field[2], field[3]),
        );
        self.order.push(MERGE_FIELD_TOKEN);
        kit::button(
            &mut self.ui,
            frame,
            [
                RIGHT_X + RIGHT_WIDTH - pick_width,
                y + 1.0,
                pick_width,
                42.0,
            ],
            if self.picking {
                "Picking..."
            } else {
                "Pick in list"
            },
            self.picking,
            true,
            self.focus == MERGE_PICK_TOKEN,
            MERGE_PICK_TOKEN,
        );
        self.order.push(MERGE_PICK_TOKEN);
        let (line, colour) = if self.picking {
            (
                "Choose the player to merge away in the list (Esc lets go)",
                color::GOLD_BRIGHT,
            )
        } else {
            match self.shown.merge_problem {
                None => (
                    "Ready: Merge asks once more before anything moves",
                    color::MUTED,
                ),
                Some(MergeProblem::Empty) => (MergeProblem::Empty.words(), color::QUIET),
                Some(problem) => (problem.words(), color::EMBER),
            }
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(RIGHT_X, y + 54.0, RIGHT_WIDTH, 22.0),
            14.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let ready = self.shown.merge_problem.is_none();
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X, y + 88.0, 180.0, 42.0],
            "Merge...",
            ready,
            ready,
            self.focus == MERGE_TOKEN,
            MERGE_TOKEN,
        );
        if ready {
            self.order.push(MERGE_TOKEN);
        }
    }

    /// The merge's confirmation: who is kept, who goes, that it cannot be undone, and
    /// Merge for good or Cancel.
    fn merge_confirmation(
        &mut self,
        frame: &Frame,
        kept: &Profile,
        merge: &PendingMerge,
        players: &[Profile],
    ) {
        let s = frame.s;
        let card = [RIGHT_X - 12.0, MERGE_TOP + 26.0, RIGHT_WIDTH + 24.0, 316.0];
        kit::card(&mut self.ui, frame, card);
        let named = |name: &str| {
            if name.is_empty() {
                "(no name yet)".to_owned()
            } else {
                cut(name, 30)
            }
        };
        let gone = players
            .iter()
            .find(|player| player.has_key(&merge.from))
            .map_or_else(|| "a player not in the list".to_owned(), |p| named(&p.name));
        let mut y = MERGE_TOP + 42.0;
        for (label, who, key, colour) in [
            (
                "Kept",
                named(&kept.name),
                merge.kept.as_str(),
                color::GOLD_BRIGHT,
            ),
            ("Merged away", gone, merge.from.as_str(), color::EMBER),
        ] {
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{label}: {who}"),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 28.0),
                20.0 * s,
                colour,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("Key id {key}"),
                frame.rect(RIGHT_X, y + 28.0, RIGHT_WIDTH, 20.0),
                14.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 58.0;
        }
        for part in wrap(
            "Everything of the merged-away player moves to the kept one, and their keys become its linked keys. This cannot be undone: Unlink only detaches a key, nothing moves back.",
            68,
        ) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 22.0),
                14.0 * s,
                color::TEXT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 22.0;
        }
        let buttons_y = MERGE_TOP + 26.0 + 316.0 - 58.0;
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X, buttons_y, 200.0, 42.0],
            "Merge for good",
            true,
            true,
            self.focus == MERGE_CONFIRM_TOKEN,
            MERGE_CONFIRM_TOKEN,
        );
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X + 212.0, buttons_y, 130.0, 42.0],
            "Cancel",
            false,
            true,
            self.focus == MERGE_CANCEL_TOKEN,
            MERGE_CANCEL_TOKEN,
        );
        self.order.push(MERGE_CONFIRM_TOKEN);
        self.order.push(MERGE_CANCEL_TOKEN);
    }

    /// What the last request came to, bottom left.
    fn status(&mut self, frame: &Frame, staff: &StaffState) {
        let s = frame.s;
        let (line, colour) = if staff.busy {
            ("Working...".to_owned(), color::MUTED)
        } else if staff.failed {
            (cut(&staff.message, 90), color::EMBER)
        } else {
            (cut(&staff.message, 90), color::GOLD_BRIGHT)
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(LEFT_X, KEYS_Y, 900.0, 24.0),
            16.0 * s,
            colour,
            FontWeight::Regular,
            TextAlign::Start,
        );
    }

    /// The keys of what has the keyboard, right-aligned at the bottom.
    fn keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let enter = match self.focus {
            SEARCH_TOKEN => "search",
            NOTE_TOKEN | MERGE_FIELD_TOKEN => "",
            _ => "do",
        };
        let mut keys: Vec<(&[&str], &str)> = vec![(&["Tab"], "next")];
        if !enter.is_empty() {
            keys.push((&["Enter"], enter));
        }
        let escape = if self.pending_merge.is_some() || self.picking {
            "cancel"
        } else {
            "back"
        };
        keys.push((&["Esc"], escape));
        let gap = 30.0 * s;
        let width: f32 = keys
            .iter()
            .map(|(caps, action)| key_hint_width(caps, action, s))
            .sum::<f32>()
            + gap * (keys.len() - 1) as f32;
        let [right, y] = frame.point(1_824.0, KEYS_Y);
        let mut x = right - width;
        for (caps, action) in keys {
            x = key_hint(&mut self.ui, caps, action, x, y, s) + gap;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::profile;
    use super::*;
    use sjk_identity::{Achievement, Medal as WireMedal};

    fn crowded() -> Profile {
        Profile {
            staff: true,
            verified: true,
            name: "^1Sol^7Vulpes the Very Long Named One^5!".into(),
            medals: Medal::ALL
                .iter()
                .map(|medal| WireMedal {
                    id: medal.id().into(),
                    count: 3,
                    awarded: 1_791_336_225,
                    note: String::new(),
                })
                .collect(),
            achievements: achievements::ALL
                .iter()
                .map(|kind| Achievement {
                    id: kind.id.into(),
                    progress: kind.goal / 2 + 1,
                    goal: kind.goal,
                    unlocked: 0,
                })
                .collect(),
            unlocks: crate::unlockables::ALL
                .iter()
                .map(|unlockable| sjk_identity::Unlock {
                    id: unlockable.id.into(),
                    granted: 1_791_336_225,
                    note: "n".repeat(200),
                    count: 1,
                    medal: None,
                })
                .collect(),
            holocron_counts: sjk_identity::HolocronCounts {
                uncommon: 999,
                rare: 999,
                legendary: 999,
                mythical: 999,
            },
            holocrons: (0..30)
                .map(|id| sjk_identity::Holocron {
                    opened: None,
                    id: 1_000 - id,
                    tier: crate::holocrons::TIERS[id as usize % crate::holocrons::COUNT]
                        .id
                        .into(),
                    dropped: 1_791_641_100,
                    source: if id % 2 == 0 { "staff" } else { "play" }.into(),
                    note: "n".repeat(200),
                })
                .collect(),
            keys: (0..12).map(|index| format!("{index:016x}")).collect(),
            ..profile("aaaaaaaaaaaaaaaa", "x")
        }
    }

    /// Every unlockable of the catalogue has its row, with Relock when held and Unlock
    /// when not.
    #[test]
    fn every_unlockable_has_its_row() {
        let inter = crate::text::load_modern(1.0, None).expect("Inter");
        let staff = StaffState::default();
        let mut me = crowded();
        let mut panel = Panel::new();
        panel.open(true);
        panel.build(
            &Inputs {
                me: Some(&me),
                staff: &staff,
            },
            &inter.font,
            [1920.0, 1080.0],
        );
        let per_page = panel.shown.unlock_per_page;
        assert!(per_page >= 10, "a page shows several rows");
        let visible = |panel: &Panel, base: u16| -> Vec<u16> {
            (0..crate::unlockables::ALL.len() as u16)
                .filter(|index| panel.order.contains(&(base + index)))
                .collect()
        };
        // The first page holds the first rows; the pager reaches the others.
        assert_eq!(
            visible(&panel, RELOCK_BASE),
            (0..per_page.min(crate::unlockables::ALL.len()) as u16).collect::<Vec<_>>()
        );
        let mut seen = visible(&panel, RELOCK_BASE);
        let pages = crate::unlockables::ALL.len().div_ceil(per_page);
        for _ in 1..pages {
            panel.activate(UNLOCK_NEXT_TOKEN);
            panel.build(
                &Inputs {
                    me: Some(&me),
                    staff: &staff,
                },
                &inter.font,
                [1920.0, 1080.0],
            );
            seen.extend(visible(&panel, RELOCK_BASE));
        }
        assert_eq!(
            seen,
            (0..crate::unlockables::ALL.len() as u16).collect::<Vec<_>>(),
            "every row is on some page"
        );
        panel.activate(UNLOCK_NEXT_TOKEN);
        assert_eq!(panel.unlock_page, pages - 1, "stops at the last page");
        for _ in 0..pages {
            panel.activate(UNLOCK_PREV_TOKEN);
        }
        assert_eq!(panel.unlock_page, 0);
        me.unlocks.clear();
        panel.build(
            &Inputs {
                me: Some(&me),
                staff: &staff,
            },
            &inter.font,
            [1920.0, 1080.0],
        );
        for index in 0..per_page.min(crate::unlockables::ALL.len()) as u16 {
            assert!(panel.order.contains(&(UNLOCK_BASE + index)), "row {index}");
        }
    }

    /// Every focus, a full list and a crowded player fit the canvas at 1080p, 4K, 4:3
    /// and 21:9, in the families and in Inter, in both right-hand views and with a
    /// merge waiting for its confirmation.
    #[test]
    fn every_state_fits_the_canvas() {
        let load = |family| crate::text::load_family(family, 1.0, None).expect("a family");
        let body = load(&crate::text::BODY);
        let inter = crate::text::load_modern(1.0, None).expect("Inter");
        let me = crowded();
        let players: Vec<Profile> = (0..30)
            .map(|index| Profile {
                staff: index % 3 == 0,
                verified: index % 2 == 0,
                ..profile(
                    &format!("{index:016x}"),
                    "^3Someone^7 with a long name here",
                )
            })
            .collect();
        let long = "^3Someone^7 with a long name here".to_owned();
        let log_requests: Vec<sjk_identity::logs::LogRequestRow> = (0..40)
            .map(|index| sjk_identity::logs::LogRequestRow {
                id: index,
                key_id: format!("{index:016x}"),
                name: long.clone(),
                staff_name: long.clone(),
                state: ["sent", "open", "declined", "expired"][index as usize % 4].to_owned(),
                created: 1_791_720_000,
                answered: 1_791_720_000,
                size: 400_000,
                ..Default::default()
            })
            .collect();
        let crash_groups: Vec<sjk_identity::logs::CrashGroup> = (0..40)
            .map(|index| sjk_identity::logs::CrashGroup {
                signature: format!("{index:016x}"),
                kind: "exception".to_owned(),
                location: "crates/sjk-viewer/src/a_very_long_file_name_for_a_crash.rs:1:2"
                    .to_owned(),
                message: "m".repeat(300),
                count: 12_345,
                keys: 678,
                last: 1_791_720_000,
                ..Default::default()
            })
            .collect();
        let crashes: Vec<sjk_identity::logs::CrashSummary> = (0..60)
            .map(|index| sjk_identity::logs::CrashSummary {
                id: index,
                name: long.clone(),
                created: 1_791_720_000,
                build: "2026.1011.2-with-a-long-tail".to_owned(),
                ..Default::default()
            })
            .collect();
        let full = StaffState {
            players: players.clone(),
            message: "x".repeat(200),
            failed: true,
            log_requests,
            crash_groups,
            crashes,
            crashes_of: "0000000000000001".to_owned(),
            ..StaffState::default()
        };
        for staff in [full, StaffState::default()].into_iter().flat_map(|staff| {
            [
                (staff.clone(), RightView::Achievements, false),
                (staff.clone(), RightView::Keys, false),
                (staff.clone(), RightView::Keys, true),
                (staff.clone(), RightView::Logs, false),
                (staff.clone(), RightView::Crashes, false),
                (staff, RightView::Crashes, true),
            ]
        }) {
            let (staff, right, pending) = staff;
            let mut panel = Panel::new();
            panel.open(true);
            panel.note = "n".repeat(200);
            panel.query = "q".repeat(64);
            panel.right = right;
            panel.merge_from = "m".repeat(MERGE_FIELD_MAX);
            // In the Crashes view, `pending` shows one kind's reports.
            if pending && right == RightView::Crashes {
                panel.crash_group = Some("0000000000000001".into());
            } else if pending {
                panel.merge_from = "0000000000000003".into();
                panel.pending_merge = Some(PendingMerge {
                    kept: me.key_id.clone(),
                    from: "0000000000000003".into(),
                });
            }
            let inputs = Inputs {
                me: Some(&me),
                staff: &staff,
            };
            let font = &inter.font;
            panel.build(&inputs, font, [1920.0, 1080.0]);
            let order = panel.order.clone();
            for focus in order {
                for font in [&body.font, &inter.font] {
                    for viewport in [
                        [1920.0, 1080.0],
                        [3840.0, 2160.0],
                        [1440.0, 1080.0],
                        [2560.0, 1080.0],
                    ] {
                        panel.focus = focus;
                        panel.build(&inputs, font, viewport);
                        assert!(!panel.ui.overflowed(), "{focus} at {viewport:?}");
                    }
                }
            }
        }
    }
}
