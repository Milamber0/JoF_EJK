//! The Profile page's drawing, in the SJK UI's look over the map whatever the menu
//! style: the top bar, or on the Profile screen its title and row of tabs
//! ([`crate::profile_hub::header`]) with the page moved down under them; the player's
//! picture, who they are and their record on the left, the bio (or the picture panel)
//! in the middle and their medals (on its own only), achievements and collection on the
//! right, each with the way to the Collection.

use super::*;
use crate::menu::sjk::{
    Frame, TextTarget, color, key_hint, key_hint_width, kit, text, top_bar, wrap,
};
use crate::menu_widgets::TextFamily;
use crate::text::UiFont;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The columns of the Profile tab (frame pixels).
const LEFT_X: f32 = 96.0;
const LEFT_WIDTH: f32 = 520.0;
const MIDDLE_X: f32 = 680.0;
const MIDDLE_WIDTH: f32 = 620.0;
const RIGHT_X: f32 = 1_364.0;
const RIGHT_WIDTH: f32 = 460.0;
/// Where the columns start, under the top bar.
const TOP: f32 = 170.0;
/// The keys' line.
const KEYS_Y: f32 = 1_010.0;
/// The bio's box: its top, height, inner padding and line.
const BIO_TOP: f32 = 216.0;
const BIO_HEIGHT: f32 = 300.0;
const BIO_PAD: f32 = 20.0;
const BIO_LINE: f32 = 26.0;
const BIO_SIZE: f32 = 18.0;
/// Most medals the Profile tab lists.
const MEDALS_SHOWN: usize = 3;
/// Most unlocks the Profile tab lists.
const UNLOCKS_SHOWN: usize = 4;
/// The player's picture at the top of the left column, and the text beside it.
const PICTURE_SIZE: f32 = 104.0;
const WHO_X: f32 = LEFT_X + PICTURE_SIZE + 24.0;
const WHO_WIDTH: f32 = LEFT_WIDTH - PICTURE_SIZE - 24.0;
/// The picture panel's large picture.
const LARGE_PICTURE: f32 = 220.0;

/// What the page says when the player holds no medal yet.
const NO_MEDALS: &str =
    "No medals yet. The SJK team gives medals for testing, contributing and more.";

/// Who the player is, as the Profile tab's left column says it.
struct Who {
    headline: String,
    lines: Vec<(String, Color)>,
    /// Their key id and picture's version, empty when the hub has not answered.
    key_id: String,
    avatar: String,
    verified: bool,
}

fn who(inputs: &Inputs<'_>) -> Who {
    let plain = |headline: &str, lines: &[&str]| Who {
        headline: headline.to_owned(),
        lines: lines
            .iter()
            .map(|line| ((*line).to_owned(), color::MUTED))
            .collect(),
        key_id: String::new(),
        avatar: String::new(),
        verified: false,
    };
    if !inputs.enabled {
        return plain(
            "Identity is off",
            &[
                "Your profile lives on the SJK hub. Switch the identity on in Settings, Network, to show it to other players.",
                "Your achievements are counted on this PC meanwhile.",
            ],
        );
    }
    let Some(snapshot) = inputs.snapshot else {
        return plain("Starting...", &["Preparing your identity."]);
    };
    match &snapshot.status {
        Status::Disabled => plain("Identity is off", &["The settings were just changed."]),
        Status::NoHub => plain(
            "No hub is set",
            &["Set cl_hubUrl, or use the official hub: Settings, Network, SJK identity key."],
        ),
        Status::Registering => plain("Contacting the hub...", &[]),
        Status::Failed(error) => Who {
            headline: "Cannot reach the hub".to_owned(),
            lines: vec![
                (error.clone(), color::MUTED),
                ("Retrying automatically.".to_owned(), color::QUIET),
            ],
            key_id: snapshot.key_id.clone(),
            avatar: snapshot
                .me
                .as_ref()
                .map(|me| me.avatar.clone())
                .unwrap_or_default(),
            verified: false,
        },
        Status::Online => {
            let Some(me) = &snapshot.me else {
                return plain("Registered", &[]);
            };
            let mut lines = Vec::new();
            let since = crate::medals::date_text(me.created);
            let badge = if me.verified {
                ("Verified by the SJK team".to_owned(), color::GOLD_BRIGHT)
            } else {
                ("Not verified yet".to_owned(), color::QUIET)
            };
            lines.push(badge);
            if !since.is_empty() {
                lines.push((format!("Member since {since}"), color::MUTED));
            }
            let earlier: Vec<&str> = me
                .names
                .iter()
                .map(|worn| worn.name.as_str())
                .filter(|name| {
                    sjk_identity::normal_form(name) != sjk_identity::normal_form(&me.name)
                })
                .take(3)
                .collect();
            if !earlier.is_empty() {
                lines.push((
                    format!("Also known as {}", earlier.join("^7, ")),
                    color::MUTED,
                ));
            }
            Who {
                headline: if me.name.is_empty() {
                    "Registered".to_owned()
                } else {
                    me.name.clone()
                },
                lines,
                key_id: me.key_id.clone(),
                avatar: me.avatar.clone(),
                verified: me.verified,
            }
        }
    }
}

/// `text` cut to `chars` characters with an ellipsis, colour codes counted as text.
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
        // The fonts outlive the target's vertex lists: measure with them, then append.
        let body = match &target {
            TextTarget::Families(fonts, _) => fonts.body.1,
            TextTarget::Inter(_, font) => *font,
        };
        self.build(inputs, body, viewport);
        target.append(&self.ui, viewport);
    }

    /// Lay the page out, measuring the bio's lines in `body`.
    pub(super) fn build(&mut self, inputs: &Inputs<'_>, body: &UiFont, viewport: [f32; 2]) {
        self.sync(inputs);
        let frame = Frame::new(viewport);
        self.ui.begin_transparent(viewport);
        crate::settings::sjk_view::backdrop(&mut self.ui, viewport);
        match self.mode {
            Mode::Pages => top_bar(&mut self.ui, &frame, "Back", BACK_TOKEN, "Profile", None),
            Mode::Hub => crate::profile_hub::header(
                &mut self.ui,
                &frame,
                &self.hub_header,
                BACK_TOKEN,
                HubTab::Profile,
            ),
        }
        // Under the Profile screen's row of tabs the page moves down.
        let page = frame.shifted(0.0, self.shift());
        let who = who(inputs);
        self.left_column(&page, inputs, &who);
        match self.middle {
            Middle::Bio => self.bio_column(&page, body),
            Middle::Picture => self.picture_column(&page, &who),
        }
        self.right_column(&page, inputs);
        self.keys(&frame);
        self.ui.finish(self.focus_token());
    }

    /// How far the page is moved down: under the Profile screen's row of tabs.
    fn shift(&self) -> f32 {
        if self.mode == Mode::Hub {
            crate::profile_hub::SHIFT
        } else {
            0.0
        }
    }

    /// The last frame line the page's own parts may reach, in their moved frame: over
    /// the keys.
    pub(super) fn bottom(&self) -> f32 {
        KEYS_Y - 20.0 - self.shift()
    }

    fn left_column(&mut self, frame: &Frame, inputs: &Inputs<'_>, who: &Who) {
        let s = frame.s;
        // The player's picture: a click (or Enter) opens the picture panel.
        let lit = self.focus == Focus::Picture || self.ui.token_hovered(PICTURE_TOKEN);
        let radius = PICTURE_SIZE * 0.5;
        crate::profile_card::avatar(
            &mut self.ui,
            frame.point(LEFT_X + radius, TOP + radius),
            radius * s,
            &crate::profile_card::Avatar {
                key_id: &who.key_id,
                version: &who.avatar,
                name: &who.headline,
                verified: who.verified,
                preview: false,
                lit,
            },
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "{}",
                if self.has_picture {
                    "Change picture"
                } else {
                    "Add a picture"
                }
            ),
            frame.rect(
                LEFT_X - 8.0,
                TOP + PICTURE_SIZE + 8.0,
                PICTURE_SIZE + 16.0,
                22.0,
            ),
            14.0 * s,
            if lit {
                color::GOLD_BRIGHT
            } else {
                color::QUIET
            },
            FontWeight::Regular,
            TextAlign::Center,
        );
        self.ui.hit_region(
            PICTURE_TOKEN,
            frame.rect(
                LEFT_X - 8.0,
                TOP - 4.0,
                PICTURE_SIZE + 16.0,
                PICTURE_SIZE + 38.0,
            ),
        );
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{}", cut(&who.headline, 28)),
            frame.rect(WHO_X, TOP, WHO_WIDTH, 50.0),
            38.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let mut y = TOP + 56.0;
        for (line, colour) in &who.lines {
            for part in wrap(line, 44).take(3) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(WHO_X, y, WHO_WIDTH, 26.0),
                    17.0 * s,
                    *colour,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 26.0;
            }
        }
        y = y.max(TOP + PICTURE_SIZE + 38.0) + 8.0;
        if self.staff {
            kit::button(
                &mut self.ui,
                frame,
                [LEFT_X, y, 180.0, 42.0],
                "Staff tools",
                true,
                true,
                self.focus == Focus::Staff,
                STAFF_TOKEN,
            );
            y += 42.0;
        }
        y += 34.0;
        kit::heading(&mut self.ui, frame, LEFT_X, y, LEFT_WIDTH, "Your record");
        y += 30.0;
        for (index, (label, value)) in inputs.record.iter().take(8).enumerate() {
            let x = LEFT_X + (index % 2) as f32 * 260.0;
            let row_y = y + (index / 2) as f32 * 74.0;
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{value}"),
                frame.rect(x, row_y, 250.0, 40.0),
                34.0 * s,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{label}"),
                frame.rect(x, row_y + 40.0, 250.0, 22.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let mut y = y + 4.0 * 74.0 + 14.0;
        let mut unlocked: Vec<&Standing> = inputs
            .standings
            .iter()
            .filter(|standing| standing.unlocked.is_some())
            .collect();
        unlocked.sort_by_key(|standing| std::cmp::Reverse(standing.unlocked.unwrap_or(0)));
        let bottom = self.bottom();
        if unlocked.is_empty() || y + 60.0 > bottom {
            return;
        }
        kit::heading(
            &mut self.ui,
            frame,
            LEFT_X,
            y,
            LEFT_WIDTH,
            "Unlocked lately",
        );
        y += 26.0;
        for standing in unlocked.into_iter().take(UNLOCKS_SHOWN) {
            if y + 28.0 > bottom {
                break;
            }
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", standing.kind.name),
                frame.rect(LEFT_X, y, LEFT_WIDTH - 140.0, 28.0),
                21.0 * s,
                color::GOLD_BRIGHT,
                FontWeight::Regular,
                TextAlign::Start,
            );
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{}",
                    crate::medals::date_text(standing.unlocked.unwrap_or(0))
                ),
                frame.rect(LEFT_X + LEFT_WIDTH - 140.0, y, 140.0, 28.0),
                15.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::End,
            );
            y += 30.0;
        }
    }

    /// The bio's box, its counts, buttons and the rules, measured in `body`.
    fn bio_column(&mut self, frame: &Frame, body: &UiFont) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            MIDDLE_X,
            TOP + 20.0,
            MIDDLE_WIDTH,
            "About you",
        );
        let focused = self.focus == Focus::Bio;
        let rect = frame.rect(MIDDLE_X, BIO_TOP, MIDDLE_WIDTH, BIO_HEIGHT);
        let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
            rect,
            radius: 16.0 * s,
            color: color::alpha(color::SPACE, 0.6),
        });
        let _ = self.ui.draw_list_mut().push(DrawCommand::Border {
            rect,
            radius: 16.0 * s,
            width: 1.5 * s,
            color: if focused {
                Color::new(1.0, 1.0, 1.0, 0.7)
            } else {
                color::alpha(color::HOLO, 0.4)
            },
        });
        self.ui.hit_region(BIO_TOKEN, rect);
        let caret = focused && (self.epoch.elapsed().as_millis() / 500).is_multiple_of(2);
        let room = MIDDLE_WIDTH - BIO_PAD * 2.0;
        let shown = if self.bio.is_empty() && !focused {
            None
        } else {
            Some(self.bio.clone())
        };
        let rows = BIO_HEIGHT - BIO_PAD * 2.0;
        let most = (rows / BIO_LINE) as usize;
        match shown {
            None => {
                let prompt = if self.writable {
                    "Say something about yourself: your clan, your style, where you play."
                } else {
                    "Your bio is kept on the SJK hub: switch the identity on to write one."
                };
                for (index, part) in wrap(prompt, 60).enumerate() {
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("{part}"),
                        frame.rect(
                            MIDDLE_X + BIO_PAD,
                            BIO_TOP + BIO_PAD + index as f32 * BIO_LINE,
                            room,
                            BIO_LINE,
                        ),
                        BIO_SIZE * s,
                        color::QUIET,
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
            }
            Some(bio_text) => {
                let lines = bio_lines(&bio_text, body, room);
                // The end being written stays in view.
                let skip = lines.len().saturating_sub(most);
                let last = lines.len().saturating_sub(1);
                for (index, line) in lines.iter().enumerate().skip(skip) {
                    let row = (index - skip) as f32;
                    let mark = if caret && index == last { "|" } else { "" };
                    text(
                        &mut self.ui,
                        TextFamily::Body,
                        format_args!("{line}{mark}"),
                        frame.rect(
                            MIDDLE_X + BIO_PAD,
                            BIO_TOP + BIO_PAD + row * BIO_LINE,
                            room,
                            BIO_LINE,
                        ),
                        BIO_SIZE * s,
                        if self.writable {
                            color::TEXT
                        } else {
                            color::MUTED
                        },
                        FontWeight::Regular,
                        TextAlign::Start,
                    );
                }
            }
        }
        let below = BIO_TOP + BIO_HEIGHT + 18.0;
        let characters = self.bio.chars().count();
        let lines = if self.bio.is_empty() {
            0
        } else {
            self.bio.split('\n').count()
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!(
                "{characters} / {} characters, {lines} / {} lines",
                bio::BIO_MAX,
                bio::LINES_MAX
            ),
            frame.rect(MIDDLE_X, below, 330.0, 26.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
        let mut x = MIDDLE_X + MIDDLE_WIDTH;
        for (label, primary, token, focus) in [
            ("Save", true, SAVE_TOKEN, Focus::Save),
            ("Revert", false, REVERT_TOKEN, Focus::Revert),
        ] {
            let width = 130.0;
            x -= width;
            kit::button(
                &mut self.ui,
                frame,
                [x, below - 10.0, width, 46.0],
                label,
                primary,
                self.writable && (primary || self.edited),
                self.focus == focus,
                token,
            );
            x -= 14.0;
        }
        let mut y = below + 56.0;
        if !self.message.is_empty() {
            let message = cut(&self.message, 80);
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{message}"),
                frame.rect(MIDDLE_X, y, MIDDLE_WIDTH, 26.0),
                17.0 * s,
                if self.message == "Saved" {
                    color::GOLD_BRIGHT
                } else {
                    color::EMBER
                },
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 34.0;
        }
        for part in wrap(
            "Letters, digits, spaces and simple punctuation, up to 6 lines. No emoji or symbols. A caret and a digit colour the text. Everyone can read it.",
            92,
        ) {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(MIDDLE_X, y, MIDDLE_WIDTH, 24.0),
                15.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 24.0;
        }
    }

    /// The picture panel, in the bio's place: the picture large (the one about to be
    /// sent, else the player's), what is happening, Browse... and how else to choose
    /// one, what a picture may be, and Use this picture, Remove picture and Done.
    fn picture_column(&mut self, frame: &Frame, who: &Who) {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            MIDDLE_X,
            TOP + 20.0,
            MIDDLE_WIDTH,
            "Your picture",
        );
        let preview = self.ready.is_some();
        let radius = LARGE_PICTURE * 0.5;
        crate::profile_card::avatar(
            &mut self.ui,
            frame.point(MIDDLE_X + radius, BIO_TOP + radius),
            radius * s,
            &crate::profile_card::Avatar {
                key_id: &who.key_id,
                version: &who.avatar,
                name: &who.headline,
                verified: who.verified,
                preview,
                lit: preview,
            },
        );
        let side_x = MIDDLE_X + LARGE_PICTURE + 30.0;
        let side_width = MIDDLE_WIDTH - LARGE_PICTURE - 30.0;
        let headline = match (&self.reading, &self.ready, self.changing) {
            (_, _, Some(changing)) if changing.removing => "Taking it down...",
            (_, _, Some(_)) => "Sending...",
            (Some(_), _, _) => "Reading the picture...",
            (_, Some(_), _) => "Your new picture",
            _ if self.has_picture => "Your picture",
            _ => "No picture yet",
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{headline}"),
            frame.rect(side_x, BIO_TOP + 8.0, side_width, 36.0),
            30.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        let file = self
            .reading
            .as_ref()
            .map(|reading| reading.name.as_str())
            .or_else(|| self.ready.as_ref().map(|(name, _)| name.as_str()));
        let mut y = BIO_TOP + 50.0;
        let detail = match file {
            Some(name) => cut(name, 40),
            None => "Everyone sees it beside your name".to_owned(),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{detail}"),
            frame.rect(side_x, y, side_width, 24.0),
            16.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Start,
        );
        y += 36.0;
        let message = match &self.picture_message {
            Some(message) => Some(message.clone()),
            None if !self.writable => Some((
                "Pictures need the SJK identity on and the SJK hub answering".to_owned(),
                false,
            )),
            None => None,
        };
        if let Some((message, good)) = message {
            for part in wrap(&message, 44).take(4) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(side_x, y, side_width, 24.0),
                    16.0 * s,
                    if good {
                        color::GOLD_BRIGHT
                    } else {
                        color::EMBER
                    },
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 24.0;
            }
        }
        let browse_y = BIO_TOP + LARGE_PICTURE + 24.0;
        let browsing = self.is_browsing();
        kit::button(
            &mut self.ui,
            frame,
            [MIDDLE_X, browse_y, 200.0, 46.0],
            if browsing { "Choosing..." } else { "Browse..." },
            false,
            !browsing,
            self.focus == Focus::Browse,
            BROWSE_TOKEN,
        );
        for (index, part) in wrap(
            "Choose a picture file on this PC, or drop one on this window.",
            46,
        )
        .take(2)
        .enumerate()
        {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("{part}"),
                frame.rect(
                    MIDDLE_X + 220.0,
                    browse_y + index as f32 * 22.0,
                    MIDDLE_WIDTH - 220.0,
                    22.0,
                ),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
        }
        let mut y = browse_y + 46.0 + 20.0;
        for line in [
            "A PNG, JPEG or TGA of up to 16 MB, 64 to 8192 pixels a side, at most four times as long as it is wide.",
            "It is cropped to a square from its middle and made 128 pixels across. Everyone can see it, so keep it friendly: the SJK team takes down pictures that are not.",
        ] {
            for part in wrap(line, 80) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(MIDDLE_X, y, MIDDLE_WIDTH, 24.0),
                    16.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 24.0;
            }
            y += 10.0;
        }
        let buttons_y = y + 14.0;
        let busy = self.changing.is_some() || self.reading.is_some();
        kit::button(
            &mut self.ui,
            frame,
            [MIDDLE_X, buttons_y, 220.0, 46.0],
            "Use this picture",
            true,
            self.writable && self.ready.is_some() && !busy,
            self.focus == Focus::UsePicture,
            USE_TOKEN,
        );
        let armed = self
            .remove_armed
            .is_some_and(|at| at.elapsed() < REMOVE_CONFIRM);
        kit::button(
            &mut self.ui,
            frame,
            [MIDDLE_X + 236.0, buttons_y, 220.0, 46.0],
            if armed {
                "Press again"
            } else {
                "Remove picture"
            },
            false,
            self.writable && self.has_picture && !busy,
            self.focus == Focus::RemovePicture,
            REMOVE_TOKEN,
        );
        kit::button(
            &mut self.ui,
            frame,
            [MIDDLE_X + MIDDLE_WIDTH - 130.0, buttons_y, 130.0, 46.0],
            "Done",
            false,
            true,
            self.focus == Focus::BioBack,
            BIO_BACK_TOKEN,
        );
    }

    /// The medals (on its own only: the Collection has them), the achievements and the
    /// collection.
    fn right_column(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let y = if self.mode == Mode::Hub {
            TOP + 20.0
        } else {
            self.medals_block(frame, inputs).max(TOP + 300.0) + 20.0
        };
        self.achievements_block(frame, inputs, y);
        self.unlockables(frame, inputs, y + 176.0);
        self.holocrons(frame, inputs, y + 322.0);
    }

    /// The medals given, up to [`MEDALS_SHOWN`], from the column's top; returns the y
    /// under them.
    fn medals_block(&mut self, frame: &Frame, inputs: &Inputs<'_>) -> f32 {
        let s = frame.s;
        kit::heading(
            &mut self.ui,
            frame,
            RIGHT_X,
            TOP + 20.0,
            RIGHT_WIDTH,
            "Medals",
        );
        let mut y = TOP + 48.0;
        let medals = inputs
            .snapshot
            .and_then(|snapshot| snapshot.me.as_ref())
            .map(|me| crate::medals::awards(&me.medals))
            .unwrap_or_default();
        if medals.is_empty() {
            for part in wrap(NO_MEDALS, 48) {
                text(
                    &mut self.ui,
                    TextFamily::Body,
                    format_args!("{part}"),
                    frame.rect(RIGHT_X, y, RIGHT_WIDTH, 24.0),
                    16.0 * s,
                    color::MUTED,
                    FontWeight::Regular,
                    TextAlign::Start,
                );
                y += 24.0;
            }
            y += 16.0;
        }
        for award in medals.iter().take(MEDALS_SHOWN) {
            let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: frame.rect(RIGHT_X, y, 84.0, 84.0),
                texture: award.medal.art(),
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
            text(
                &mut self.ui,
                TextFamily::Display,
                format_args!("{}", award.label()),
                frame.rect(RIGHT_X + 98.0, y + 8.0, RIGHT_WIDTH - 98.0, 30.0),
                23.0 * s,
                color::GOLD_BRIGHT,
                FontWeight::Semibold,
                TextAlign::Start,
            );
            let given = award.given();
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{}",
                    if given.is_empty() {
                        award.medal.description()
                    } else {
                        &given
                    }
                ),
                frame.rect(RIGHT_X + 98.0, y + 40.0, RIGHT_WIDTH - 98.0, 24.0),
                15.0 * s,
                color::MUTED,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 96.0;
        }
        if medals.len() > MEDALS_SHOWN {
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!("and {} more", medals.len() - MEDALS_SHOWN),
                frame.rect(RIGHT_X, y, RIGHT_WIDTH, 24.0),
                15.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Start,
            );
            y += 30.0;
        }
        y
    }

    /// How many achievements are unlocked, and the way to the board, from `y`.
    fn achievements_block(&mut self, frame: &Frame, inputs: &Inputs<'_>, y: f32) {
        let s = frame.s;
        kit::heading(&mut self.ui, frame, RIGHT_X, y, RIGHT_WIDTH, "Achievements");
        let total = inputs.standings.len();
        let unlocked = inputs
            .standings
            .iter()
            .filter(|standing| standing.unlocked.is_some())
            .count();
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{unlocked} of {total} unlocked"),
            frame.rect(RIGHT_X, y + 26.0, RIGHT_WIDTH, 40.0),
            30.0 * s,
            color::TEXT,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        bar(
            &mut self.ui,
            frame,
            [RIGHT_X, y + 76.0, RIGHT_WIDTH, 8.0],
            if total == 0 {
                0.0
            } else {
                unlocked as f32 / total as f32
            },
            color::GOLD,
        );
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X, y + 104.0, 240.0, 46.0],
            "See the board",
            false,
            true,
            self.focus == Focus::Board,
            BOARD_TOKEN,
        );
    }

    /// How many unlockables the player owns, and the way to their page, from `y`.
    fn unlockables(&mut self, frame: &Frame, inputs: &Inputs<'_>, y: f32) {
        let s = frame.s;
        let (heading, button) = (crate::profile_hub::COLLECTION, "See the collection");
        kit::heading(&mut self.ui, frame, RIGHT_X, y, RIGHT_WIDTH, heading);
        let holdings = crate::unlockables::Holdings::of(inputs.enabled, inputs.snapshot);
        let (line, size, colour) = if holdings.reason().is_none() {
            let owned = crate::unlockables::ALL
                .iter()
                .filter(|unlockable| holdings.unlock(unlockable.id).is_some())
                .count();
            (
                format!("{owned} of {} shaders owned", crate::unlockables::ALL.len()),
                30.0,
                color::TEXT,
            )
        } else {
            (
                "Medals, shaders, toys and more, kept on the SJK hub".to_owned(),
                17.0,
                color::MUTED,
            )
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{line}"),
            frame.rect(RIGHT_X, y + 26.0, RIGHT_WIDTH, 40.0),
            size * s,
            colour,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X, y + 76.0, 240.0, 46.0],
            button,
            false,
            true,
            self.focus == Focus::Unlockables,
            UNLOCKABLES_TOKEN,
        );
    }

    /// How many holocrons the player holds, and the way to their page, from `y`.
    fn holocrons(&mut self, frame: &Frame, inputs: &Inputs<'_>, y: f32) {
        let s = frame.s;
        let heading = if self.mode == Mode::Hub {
            crate::profile_hub::HOLOCRONS
        } else {
            "Holocrons"
        };
        kit::heading(&mut self.ui, frame, RIGHT_X, y, RIGHT_WIDTH, heading);
        let known = inputs
            .snapshot
            .filter(|_| inputs.enabled)
            .and_then(|snapshot| snapshot.me.as_ref())
            .map(|me| {
                crate::holocrons::counts_of(&me.holocron_counts)
                    .iter()
                    .sum::<u32>()
            });
        let (line, size, colour) = match known {
            Some(1) => ("1 holocron".to_owned(), 30.0, color::TEXT),
            Some(total) => (format!("{total} holocrons"), 30.0, color::TEXT),
            None => (
                "Found by playing, kept on the SJK hub".to_owned(),
                17.0,
                color::MUTED,
            ),
        };
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{line}"),
            frame.rect(RIGHT_X, y + 26.0, RIGHT_WIDTH, 40.0),
            size * s,
            colour,
            FontWeight::Semibold,
            TextAlign::Start,
        );
        kit::button(
            &mut self.ui,
            frame,
            [RIGHT_X, y + 76.0, 240.0, 46.0],
            "See holocrons",
            false,
            true,
            self.focus == Focus::Holocrons,
            HOLOCRONS_TOKEN,
        );
    }

    /// The keys of what has the keyboard, right-aligned at the bottom.
    fn keys(&mut self, frame: &Frame) {
        let s = frame.s;
        let typing = self.focus == Focus::Bio && self.writable;
        let enter = match self.focus {
            Focus::Staff => "open",
            Focus::Bio | Focus::Save => "save",
            Focus::Revert => "revert",
            Focus::Board | Focus::Unlockables | Focus::Holocrons => "open",
            Focus::Picture => "change picture",
            Focus::Browse => "choose a file",
            Focus::UsePicture => "use this picture",
            Focus::RemovePicture => "remove picture",
            Focus::BioBack => "back to your bio",
        };
        let mut keys: Vec<(&[&str], &str)> = vec![(&["Tab"], "next"), (&["Enter"], enter)];
        if typing {
            keys.push((&["Shift", "Enter"], "new line"));
        }
        if self.mode == Mode::Hub {
            keys.push((&["Ctrl", "Tab"], HubTab::Profile.next(true).label()));
        }
        keys.push((&["Esc"], "back"));
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

/// A progress bar over `rect` (frame pixels), filled to `fraction` in `fill`.
fn bar(
    canvas: &mut crate::menu_widgets::MenuCanvas,
    frame: &Frame,
    rect: [f32; 4],
    fraction: f32,
    fill: Color,
) {
    let [x, y, width, height] = rect;
    let s = frame.s;
    let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
        rect: frame.rect(x, y, width, height),
        radius: height * 0.5 * s,
        color: color::alpha(color::HOLO, 0.12),
    });
    let fraction = fraction.clamp(0.0, 1.0);
    if fraction > 0.0 {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: frame.rect(x, y, (width * fraction).max(height), height),
            radius: height * 0.5 * s,
            color: fill,
        });
    }
}

/// The bio's lines as the box draws them: each of its lines broken at spaces to fit
/// `room` pixels in `body` at the box's size; an empty line stays a line.
fn bio_lines(bio_text: &str, body: &UiFont, room: f32) -> Vec<String> {
    let mut lines = Vec::new();
    for line in bio_text.split('\n') {
        if line.is_empty() {
            lines.push(String::new());
            continue;
        }
        lines.extend(crate::text_dialog::wrap_to(line, body, BIO_SIZE, 0.0, room));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::super::tests::snapshot;
    use super::*;
    use crate::achievements;
    use sjk_identity::{Medal, Profile, WornName};

    struct Fonts {
        display: crate::text::FontAtlas,
        body: crate::text::FontAtlas,
    }

    fn fonts() -> Fonts {
        let load = |family| crate::text::load_family(family, 1.0, None).expect("a bundled family");
        Fonts {
            display: load(&crate::text::DISPLAY),
            body: load(&crate::text::BODY),
        }
    }

    fn full_profile() -> Profile {
        Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "0123456789abcdef".to_owned(),
            key: String::new(),
            name: "^1Ш^7Sol^3Vulpes^7 the Long Named".to_owned(),
            bio: format!(
                "{}\n\nline three\n{}",
                "Äöüß word ".repeat(30),
                "x".repeat(60)
            ),
            verified: true,
            staff: false,
            created: 1_759_708_800,
            names: ["^2Fox", "Sol", "^4Another Long Name", "Fourth"]
                .iter()
                .map(|name| WornName {
                    name: (*name).to_owned(),
                    first_seen: 0,
                    last_seen: 0,
                })
                .collect(),
            medals: ["early_tester", "early_contributor", "bug_hunter", "unknown"]
                .iter()
                .map(|id| Medal {
                    id: (*id).to_owned(),
                    count: 3,
                    awarded: 1_759_900_000,
                    note: "a note".to_owned(),
                })
                .collect(),
            achievements: Vec::new(),
            avatar: String::new(),
            unlocks: Vec::new(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        }
    }

    fn record() -> Vec<(&'static str, String)> {
        [
            "Players defeated",
            "Saber kills",
            "Best streak",
            "Duels won",
            "Flags captured",
            "Maps played",
            "Servers",
            "Time played",
        ]
        .iter()
        .map(|label| (*label, "123456".to_owned()))
        .collect()
    }

    /// Half unlocked, half part-way.
    fn standings() -> Vec<Standing> {
        achievements::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| Standing {
                kind,
                progress: if index % 2 == 0 {
                    kind.goal
                } else {
                    kind.goal / 3
                },
                unlocked: (index % 2 == 0).then_some(1_759_900_000),
            })
            .collect()
    }

    /// Every tab, focus and state, at its longest, fits the canvas at 1080 lines, 4K,
    /// 4:3 and 21:9, in the families and in Inter.
    #[test]
    fn every_state_fits_the_canvas() {
        let families = fonts();
        let inter = crate::text::load_modern(1.0, None).expect("Inter");
        let record = record();
        let standings = standings();
        let online = snapshot(Some(full_profile()), Some("saved"));
        let offline = Snapshot {
            status: Status::Failed("the hub took too long to answer, again and again".into()),
            ..snapshot(None, None)
        };
        for (enabled, shot) in [(true, Some(&online)), (true, Some(&offline)), (false, None)] {
            {
                for focus in [
                    Focus::Picture,
                    Focus::Staff,
                    Focus::Browse,
                    Focus::Bio,
                    Focus::Save,
                    Focus::Revert,
                    Focus::Board,
                    Focus::Unlockables,
                    Focus::Holocrons,
                ] {
                    for body in [&families.body.font, &inter.font] {
                        for viewport in [
                            [1920.0, 1080.0],
                            [3840.0, 2160.0],
                            [1440.0, 1080.0],
                            [2560.0, 1080.0],
                        ] {
                            for mode in [Mode::Pages, Mode::Hub] {
                                let mut panel = Panel::new();
                                panel.open_as(true, mode);
                                panel.focus = focus;
                                panel.message = "a bio uses letters, digits, spaces and simple punctuation (no emoji or symbols)".into();
                                let inputs = Inputs {
                                    enabled,
                                    snapshot: shot,
                                    standings: &standings,
                                    record: &record,
                                };
                                panel.build(&inputs, body, viewport);
                                assert!(
                                    !panel.ui.overflowed(),
                                    "{focus:?} {mode:?} {enabled} at {viewport:?}"
                                );
                                // The Profile screen's tabs answer where they are drawn.
                                let strip = panel.ui.rect_for(crate::profile_hub::TOKEN);
                                assert_eq!(strip.is_some(), mode == Mode::Hub);
                                if mode == Mode::Hub {
                                    clear_of_the_row(&panel, viewport);
                                }
                            }
                        }
                    }
                }
            }
        }
        let _ = &families.display;
    }

    /// The picture panel fits the canvas in each of its states, keeps to the middle
    /// column, and its controls (and the picture that opens it) answer the pointer
    /// above the keys, at 1080 lines, 4K, 4:3 and 21:9.
    #[test]
    fn the_picture_panel_fits_its_column_and_answers_the_pointer() {
        let families = fonts();
        let record = record();
        let standings = standings();
        let online = snapshot(Some(full_profile()), Some("saved"));
        for viewport in [
            [1920.0, 1080.0],
            [3840.0, 2160.0],
            [1440.0, 1080.0],
            [2560.0, 1080.0],
        ] {
            let frame = Frame::new(viewport);
            for (state, mode) in (0..5).flat_map(|state| [(state, Mode::Pages), (state, Mode::Hub)])
            {
                let mut panel = Panel::new();
                panel.open_as(true, mode);
                panel.middle = Middle::Picture;
                match state {
                    0 => {}
                    1 => {
                        panel.ready = Some((
                            "a picture file whose name goes on and on and on.png".into(),
                            vec![1],
                        ));
                    }
                    2 => {
                        panel.changing = Some(Changing {
                            serial_before: 0,
                            removing: true,
                        });
                    }
                    3 => {
                        panel.picture_message = Some((
                            "That picture is too big: at most 16 MB and 8192 pixels a side. "
                                .repeat(3),
                            false,
                        ));
                    }
                    _ => panel.remove_armed = Some(Instant::now()),
                }
                for focus in [
                    Focus::Picture,
                    Focus::Browse,
                    Focus::UsePicture,
                    Focus::RemovePicture,
                    Focus::BioBack,
                ] {
                    panel.focus = focus;
                    let inputs = Inputs {
                        enabled: state != 4,
                        snapshot: Some(&online),
                        standings: &standings,
                        record: &record,
                    };
                    panel.build(&inputs, &families.body.font, viewport);
                    assert!(!panel.ui.overflowed(), "{state} {viewport:?}");
                    let keys = frame.point(0.0, KEYS_Y)[1];
                    for token in [
                        PICTURE_TOKEN,
                        BROWSE_TOKEN,
                        USE_TOKEN,
                        REMOVE_TOKEN,
                        BIO_BACK_TOKEN,
                    ] {
                        let rect = panel.ui.rect_for(token).expect("the control's area");
                        assert!(rect.x >= 0.0 && rect.right() <= viewport[0], "{token}");
                        assert!(rect.y >= 0.0 && rect.bottom() < keys, "{token} {state}");
                    }
                    let (middle, right) =
                        (frame.point(MIDDLE_X, 0.0)[0], frame.point(RIGHT_X, 0.0)[0]);
                    for command in panel.ui.draw_list().commands() {
                        // The keys' line runs under every column.
                        if let DrawCommand::Text { rect, .. } = command
                            && rect.x >= middle - 0.5
                            && rect.x < right
                            && rect.y < keys
                        {
                            assert!(rect.right() <= right - 20.0 * frame.s, "{state} {rect:?}");
                        }
                    }
                }
            }
        }
    }

    /// On the Profile screen nothing of the page stands in the row of tabs: between the
    /// title and the page's top only the tabs' names are drawn.
    fn clear_of_the_row(panel: &Panel, viewport: [f32; 2]) {
        let frame = Frame::new(viewport);
        let (above, below) = (frame.point(0.0, 120.0)[1], frame.point(0.0, 192.0)[1]);
        let tabs = frame.point(0.0, 146.0)[1];
        for command in panel.ui.draw_list().commands() {
            if let DrawCommand::Text { rect, .. } = command
                && rect.y > above
                && rect.y < below
            {
                // A tab's name is set a little under the row's top.
                assert!(
                    rect.y >= tabs && rect.y - tabs < 10.0 * frame.s,
                    "{rect:?} in the row"
                );
            }
        }
    }

    /// The bio's lines fit the box's width and a long bio shows its end.
    #[test]
    fn the_bio_wraps_inside_its_box() {
        let fonts = fonts();
        let body = &fonts.body.font;
        let room = MIDDLE_WIDTH - BIO_PAD * 2.0;
        let lines = bio_lines(&full_profile().bio, body, room);
        assert!(lines.len() > 4);
        assert_eq!(lines.iter().filter(|line| line.is_empty()).count(), 1);
        let scale = BIO_SIZE / body.height;
        for line in &lines {
            assert!(
                crate::text::visible_text_width(body, line, scale) <= room,
                "{line}"
            );
        }
    }
}
