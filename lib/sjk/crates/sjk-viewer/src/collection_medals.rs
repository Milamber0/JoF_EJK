//! The Collection's Medals tab: every medal the client knows hangs by its ribbon from a
//! lit rail, the ones the SJK team gave the player first, in colour on a soft gold
//! light, the others dark; the one chosen is marked on the rail and shown up close on
//! the right, with its name (and how many times for one given again), when it was
//! given, what it is for and the team's note. The medals come from the player's own hub
//! profile (`crate::medals::awards`), as the Players page and the profile card read
//! them.

use super::view::{LEFT_X, SHOWCASE_WIDTH, SHOWCASE_X, centred_lines, glow};
use super::*;
use crate::medals::{Award, Medal};
use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::TextFamily;
use sjk_ui::{Color, DrawCommand, FontWeight, TextAlign};

/// The rail: its height on the page and its length.
const RAIL_Y: f32 = 336.0;
const RAIL_WIDTH: f32 = 904.0;
/// A medal's room along the rail and its picture's size, at most.
const SLOT: f32 = 226.0;
const ART: f32 = 220.0;
/// The medal up close.
const BIG_ART: f32 = 400.0;

/// A medal as the tab shows it: the player's award, or the medal they do not hold.
pub(super) struct Showing {
    pub(super) medal: Medal,
    pub(super) award: Option<Award>,
}

/// How many medals the player holds, once the hub answered with the identity on.
pub(super) fn held(inputs: &Inputs<'_>) -> Option<usize> {
    inputs
        .snapshot
        .filter(|_| inputs.enabled)
        .and_then(|snapshot| snapshot.me.as_ref())
        .map(|me| crate::medals::awards(&me.medals).len())
}

/// What the tab says at its top, and the medals: the ones given first, in the
/// catalogue's order, then the others.
pub(super) fn showing(inputs: &Inputs<'_>) -> ((String, &'static str), Vec<Showing>) {
    let me = inputs
        .snapshot
        .filter(|_| inputs.enabled)
        .and_then(|snapshot| snapshot.me.as_ref());
    let awards = me.map(|me| crate::medals::awards(&me.medals));
    let held = awards.as_ref().map_or(0, Vec::len);
    let count = listed(awards.as_deref().unwrap_or_default()).count();
    let headline = match (inputs.enabled, &awards) {
        (false, _) => (
            "Identity is off".to_owned(),
            "Your medals are kept on the SJK hub: switch the identity on in Settings, Network, to see them.",
        ),
        (true, None) => (
            "Contacting the hub...".to_owned(),
            "Your medals show once the SJK hub has answered.",
        ),
        (true, Some(_)) if held == 0 => (
            format!("0 of {count} medals"),
            "The SJK team gives medals by hand, for what players do for SJK.",
        ),
        (true, Some(_)) => (
            format!("{held} of {count} medals"),
            "Given by the SJK team by hand, for what players do for SJK.",
        ),
    };
    let mut shown: Vec<Showing> = awards
        .unwrap_or_default()
        .into_iter()
        .map(|award| Showing {
            medal: award.medal,
            award: Some(award),
        })
        .collect();
    for medal in Medal::ALL {
        if !shown.iter().any(|showing| showing.medal == medal) && !shelved(medal) {
            shown.push(Showing { medal, award: None });
        }
    }
    (headline, shown)
}

/// Whether `medal` stays out of the tab until given: Supporter while donations are
/// shelved (`crate::donation_goal::SHELVED`).
fn shelved(medal: Medal) -> bool {
    crate::donation_goal::SHELVED && medal == Medal::Supporter
}

/// The medals the tab lists: every one but the shelved ones not in `awards`.
fn listed(awards: &[Award]) -> impl Iterator<Item = Medal> + '_ {
    Medal::ALL
        .into_iter()
        .filter(|&medal| !shelved(medal) || awards.iter().any(|award| award.medal == medal))
}

/// When it was given, as the medal's line says it.
fn given(award: Option<&Award>) -> String {
    let Some(award) = award else {
        return "Not given yet".to_owned();
    };
    let date = crate::medals::date_text(award.awarded);
    match (award.count, date.is_empty()) {
        (1, true) => "Given".to_owned(),
        (1, false) => format!("Given {date}"),
        (count, true) => format!("Given {count} times"),
        (count, false) => format!("Given {count} times, last on {date}"),
    }
}

impl Panel {
    /// The Medals tab.
    pub(super) fn medals(&mut self, frame: &Frame, inputs: &Inputs<'_>) {
        let ((headline, line), shown) = showing(inputs);
        self.lead(frame, &headline, line);
        self.medals_shown = shown.len();
        self.medal = self.medal.min(shown.len().saturating_sub(1));
        let s = frame.s;
        // The rail, with a bead at each end.
        let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
            rect: frame.rect(LEFT_X, RAIL_Y, RAIL_WIDTH, 1.5),
            color: color::alpha(color::HOLO, 0.45),
        });
        for x in [LEFT_X, LEFT_X + RAIL_WIDTH] {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(x - 4.0, RAIL_Y - 3.0, 8.0, 8.0),
                radius: 4.0 * s,
                color: color::HOLO,
            });
        }
        let slot = (RAIL_WIDTH / shown.len().max(1) as f32).min(SLOT);
        let art = (slot - 6.0).min(ART);
        for (index, showing) in shown.iter().enumerate() {
            let centre = LEFT_X + slot * (index as f32 + 0.5);
            self.hanging_medal(frame, showing, index, centre, art);
        }
        // Room for the medals to come.
        let mut x = LEFT_X;
        while x < LEFT_X + RAIL_WIDTH {
            let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
                rect: frame.rect(x, 720.0, 8.0, 1.0),
                color: color::alpha(color::HOLO, 0.22),
            });
            x += 16.0;
        }
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("More medals will hang here as the SJK team adds them."),
            frame.rect(LEFT_X, 736.0, RAIL_WIDTH, 24.0),
            15.0 * s,
            color::QUIET,
            FontWeight::Regular,
            TextAlign::Start,
        );
        if let Some(chosen) = shown.get(self.medal) {
            self.medal_up_close(frame, chosen);
        }
    }

    /// Medal `index` hanging from the rail, its middle at `centre`, its picture `art`
    /// across.
    fn hanging_medal(
        &mut self,
        frame: &Frame,
        showing: &Showing,
        index: usize,
        centre: f32,
        art: f32,
    ) {
        let s = frame.s;
        let held = showing.award.is_some();
        let chosen = index == self.medal;
        let token = MEDAL_BASE + index as u16;
        if held {
            glow(
                &mut self.ui,
                frame,
                [centre, RAIL_Y + art * 0.68],
                art * 0.62,
                color::GOLD,
                if chosen { 0.42 } else { 0.18 },
            );
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: frame.rect(centre - art * 0.5, RAIL_Y - art * 0.04, art, art),
            texture: showing.medal.art(),
            color: if held {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                Color::new(0.42, 0.45, 0.52, 0.4)
            },
        });
        if chosen {
            let _ = self.ui.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: frame.rect(centre - 2.0, RAIL_Y - 9.0, 4.0, 18.0),
                radius: 2.0 * s,
                color: color::GOLD_BRIGHT,
            });
        }
        let name = showing
            .award
            .as_ref()
            .map_or_else(|| showing.medal.name().to_owned(), Award::label);
        let caption = RAIL_Y + art * 0.98 + 14.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{name}"),
            frame.rect(centre - 110.0, caption, 220.0, 30.0),
            23.0 * s,
            match (chosen, held) {
                (true, _) => color::GOLD_BRIGHT,
                (false, true) => color::TEXT,
                (false, false) => color::QUIET,
            },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        let status = match &showing.award {
            Some(award) if award.count > 1 => format!("Given {} times", award.count),
            Some(_) => {
                let date =
                    crate::medals::date_text(showing.award.as_ref().map_or(0, |a| a.awarded));
                if date.is_empty() {
                    "Given".to_owned()
                } else {
                    format!("Given {date}")
                }
            }
            None => "Not given yet".to_owned(),
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{status}"),
            frame.rect(centre - 110.0, caption + 32.0, 220.0, 22.0),
            15.0 * s,
            if held { color::GOLD } else { color::QUIET },
            FontWeight::Regular,
            TextAlign::Center,
        );
        self.ui.hit_region(
            token,
            frame.rect(centre - art * 0.5, RAIL_Y - 10.0, art, art + 70.0),
        );
    }

    /// The medal chosen, up close on the right.
    fn medal_up_close(&mut self, frame: &Frame, showing: &Showing) {
        let s = frame.s;
        let held = showing.award.is_some();
        let middle = SHOWCASE_X + SHOWCASE_WIDTH * 0.5;
        let top = 214.0;
        if held {
            glow(
                &mut self.ui,
                frame,
                [middle, top + BIG_ART * 0.62],
                280.0,
                color::GOLD,
                0.4,
            );
        }
        let _ = self.ui.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: frame.rect(middle - BIG_ART * 0.5, top, BIG_ART, BIG_ART),
            texture: showing.medal.art(),
            color: if held {
                Color::new(1.0, 1.0, 1.0, 1.0)
            } else {
                Color::new(0.45, 0.48, 0.56, 0.45)
            },
        });
        let name = showing
            .award
            .as_ref()
            .map_or_else(|| showing.medal.name().to_owned(), Award::label);
        let mut y = top + BIG_ART + 24.0;
        text(
            &mut self.ui,
            TextFamily::Display,
            format_args!("{name}"),
            frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 60.0),
            54.0 * s,
            if held {
                color::GOLD_BRIGHT
            } else {
                color::alpha(color::TEXT, 0.75)
            },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        y += 66.0;
        let line = format!(
            "{}  \u{b7}  {}",
            given(showing.award.as_ref()),
            crate::editions::name(showing.medal.edition())
        );
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{line}"),
            frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 24.0),
            18.0 * s,
            if held { color::GOLD } else { color::QUIET },
            FontWeight::Semibold,
            TextAlign::Center,
        );
        y += 36.0;
        let about = match showing.medal.blade() {
            Some(id) => {
                let blade =
                    crate::unlockables::find(id).map_or("a saber shader", |blade| blade.name);
                format!(
                    "{} Comes with the {blade}, a saber shader.",
                    showing.medal.description()
                )
            }
            None => showing.medal.description().to_owned(),
        };
        y = centred_lines(
            &mut self.ui,
            frame,
            &about,
            [SHOWCASE_X, y, SHOWCASE_WIDTH],
            56,
            2,
            20.0,
            if held { color::TEXT } else { color::MUTED },
        );
        let note = showing
            .award
            .as_ref()
            .map(|award| award.note.as_str())
            .filter(|note| !note.is_empty());
        if let Some(note) = note {
            y += 10.0;
            let _ = self.ui.draw_list_mut().push(DrawCommand::SolidRect {
                rect: frame.rect(middle - 32.0, y, 64.0, 1.0),
                color: color::alpha(color::HOLO, 0.45),
            });
            y += 14.0;
            text(
                &mut self.ui,
                TextFamily::Body,
                format_args!(
                    "{}",
                    if note == crate::debug_collectibles::NOTE {
                        "Not really yours"
                    } else {
                        "From the SJK team"
                    }
                ),
                frame.rect(SHOWCASE_X, y, SHOWCASE_WIDTH, 20.0),
                14.0 * s,
                color::QUIET,
                FontWeight::Regular,
                TextAlign::Center,
            );
            y += 24.0;
            let quoted = format!("\u{201c}{note}\u{201d}");
            y = centred_lines(
                &mut self.ui,
                frame,
                &quoted,
                [SHOWCASE_X, y, SHOWCASE_WIDTH],
                58,
                2,
                18.0,
                color::TEXT,
            );
        }
        let footer = if held {
            "Every SJK player sees it on your card, on the scoreboard and in SJK chat."
        } else {
            "The SJK team gives it by hand."
        };
        text(
            &mut self.ui,
            TextFamily::Body,
            format_args!("{footer}"),
            frame.rect(SHOWCASE_X, (y + 14.0).max(910.0), SHOWCASE_WIDTH, 22.0),
            15.0 * s,
            color::MUTED,
            FontWeight::Regular,
            TextAlign::Center,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::Profile;

    fn medal(id: &str, count: u32, note: &str) -> sjk_identity::Medal {
        sjk_identity::Medal {
            id: id.to_owned(),
            count,
            awarded: 1_791_336_225,
            note: note.to_owned(),
        }
    }

    fn holding(medals: Vec<sjk_identity::Medal>) -> Snapshot {
        crate::console::profile_panel::tests::snapshot(
            Some(Profile {
                medals,
                ..crate::console::profile_panel::tests::me("")
            }),
            None,
        )
    }

    fn with<'a>(enabled: bool, snapshot: Option<&'a Snapshot>) -> Inputs<'a> {
        Inputs {
            enabled,
            snapshot,
            ..super::super::tests::inputs(Holdings::Known(&[]), "")
        }
    }

    /// The medals given come first, lit, then every other medal the client knows; a
    /// medal the client does not know is left out.
    #[test]
    fn the_medals_given_come_first_then_the_others() {
        let shot = holding(vec![
            medal("bug_hunter", 3, "three fixes"),
            medal("unknown", 1, ""),
            medal("early_tester", 1, ""),
        ]);
        let ((headline, _), shown) = showing(&with(true, Some(&shot)));
        let listed = Medal::ALL
            .into_iter()
            .filter(|&medal| !shelved(medal))
            .count();
        assert_eq!(headline, format!("2 of {listed} medals"));
        assert_eq!(held(&with(true, Some(&shot))), Some(2));
        let order: Vec<(Medal, bool)> = shown
            .iter()
            .map(|showing| (showing.medal, showing.award.is_some()))
            .collect();
        let mut expected = vec![
            (Medal::EarlyTester, true),
            (Medal::BugHunter, true),
            (Medal::EarlyContributor, false),
        ];
        if !shelved(Medal::Supporter) {
            expected.push((Medal::Supporter, false));
        }
        assert_eq!(order, expected);
        assert_eq!(
            given(shown[1].award.as_ref()),
            format!(
                "Given 3 times, last on {}",
                crate::medals::date_text(1_791_336_225)
            )
        );
        // Without the hub every listed medal shows, none given, and none is counted.
        for (enabled, snapshot) in [(false, Some(&shot)), (true, None)] {
            let (_, shown) = showing(&with(enabled, snapshot));
            assert_eq!(shown.len(), listed);
            assert!(shown.iter().all(|showing| showing.award.is_none()));
            assert_eq!(held(&with(enabled, snapshot)), None);
        }
    }

    /// Every medal hangs within the canvas over the keys, notes at their longest, and
    /// answers the pointer, at 1080 lines, 4K, 4:3 and 21:9.
    #[test]
    fn every_medal_hangs_over_the_keys() {
        let note = "a very long note from the team ".repeat(10);
        let shot = holding(
            Medal::ALL
                .iter()
                .map(|kind| medal(kind.id(), 12, &note))
                .collect(),
        );
        let font = crate::text::load_modern(1.0, None).expect("Inter").font;
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [2_560.0, 1_080.0],
        ] {
            let mut panel = Panel::new();
            panel.open(Tab::Medals, true, true, ReturnTarget::MainMenu);
            panel.build(&with(true, Some(&shot)), &font, viewport);
            assert!(!panel.ui.overflowed(), "{viewport:?}");
            let frame = Frame::new(viewport);
            let keys = frame.point(0.0, super::super::view::KEYS_Y)[1];
            let arts = panel
                .ui
                .draw_list()
                .commands()
                .iter()
                .filter(|command| {
                    matches!(command, DrawCommand::TexturedQuad { texture, .. }
                        if Medal::from_art(*texture).is_some())
                })
                .count();
            // Each on the rail, and the chosen one up close.
            assert_eq!(arts, Medal::COUNT + 1, "{viewport:?}");
            for index in 0..Medal::COUNT {
                assert!(panel.ui.rect_for(MEDAL_BASE + index as u16).is_some());
            }
            for command in panel.ui.draw_list().commands() {
                if let DrawCommand::Text { rect, .. } = command {
                    assert!(rect.bottom() <= keys || rect.y >= keys, "{rect:?}");
                    assert!(rect.right() <= viewport[0] + 1.0, "{rect:?}");
                }
            }
        }
    }
}
