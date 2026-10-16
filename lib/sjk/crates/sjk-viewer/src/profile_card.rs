//! The player's profile card (`docs/sjk-ui.md`, "Profile card"): their picture, name
//! with its colours and what the SJK hub says of them (verified, medals, achievements)
//! in the bottom-left corner of the SJK UI's main page and in-game menu. A click on it
//! opens the Profile page.
//!
//! What the card says is gathered twice a second ([`refresh`], from the identity tick)
//! into a [`Summary`] the screens read without copying ([`with`]); the picture comes
//! from the picture cache (`avatars`), and until it is there, or when the player has
//! none, the card draws their initial on a colour of their own. [`avatar`] draws a
//! player's picture that way wherever a screen shows one.

use crate::menu::sjk::{Frame, color, text};
use crate::menu_widgets::{MenuCanvas, TextFamily};
use sjk_ui::{Color, DrawCommand, FontWeight, Rect, TextAlign};
use std::sync::{Mutex, MutexGuard};

/// How the player stands with the SJK hub.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum HubState {
    /// The identity is off (`cl_identity 0`) or has no hub.
    #[default]
    Off,
    /// Registering.
    Starting,
    /// The hub is out of reach.
    Offline,
    /// The hub answered.
    Online,
}

/// What the card shows of the player.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct Summary {
    /// The in-game name, with its colour codes.
    pub(crate) name: String,
    /// Their model and blade, as a line ("Kyle, blue saber", "Kyle, Sun blade").
    pub(crate) detail: String,
    /// The name of the blade skin they wear (`unlockables`), while they own it.
    pub(crate) skin: Option<&'static str>,
    /// Their key id, empty without an identity.
    pub(crate) key_id: String,
    /// Their picture's version, empty for none.
    pub(crate) avatar: String,
    pub(crate) hub: HubState,
    pub(crate) verified: bool,
    /// The hub made their key staff: the game menu offers Staff tools.
    pub(crate) staff: bool,
    /// Medals the SJK team gave them (those the client knows).
    pub(crate) medals: usize,
    /// Achievements unlocked, of how many.
    pub(crate) unlocked: usize,
    pub(crate) achievements: usize,
}

static SUMMARY: Mutex<Summary> = Mutex::new(Summary {
    name: String::new(),
    detail: String::new(),
    skin: None,
    key_id: String::new(),
    avatar: String::new(),
    hub: HubState::Off,
    verified: false,
    staff: false,
    medals: 0,
    unlocked: 0,
    achievements: 0,
});

/// A player with no identity and no counts, for screens' tests.
#[cfg(test)]
pub(crate) static NOBODY: Summary = Summary {
    name: String::new(),
    detail: String::new(),
    skin: None,
    key_id: String::new(),
    avatar: String::new(),
    hub: HubState::Off,
    verified: false,
    staff: false,
    medals: 0,
    unlocked: 0,
    achievements: 21,
};

fn lock() -> MutexGuard<'static, Summary> {
    SUMMARY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Read the card's summary without copying it.
pub(crate) fn with<R>(read: impl FnOnce(&Summary) -> R) -> R {
    read(&lock())
}

/// Gather the card's summary from the settings, the identity service and the
/// achievement counts; called twice a second.
pub(crate) fn refresh(console: &crate::console::ViewerConsole) {
    let name = console.text_value("name").unwrap_or("Padawan");
    let model = console
        .text_value("model")
        .and_then(|model| model.split('/').next())
        .filter(|model| !model.is_empty())
        .unwrap_or("kyle");
    let snapshot =
        crate::player_identity::collection_snapshot(crate::debug_collectibles::wanted(console));
    let enabled = console.bool_cvar("cl_identity") == Some(true);
    let skin = worn_skin(
        console
            .text_value(crate::unlockables::SABER_SKIN_CVAR)
            .unwrap_or_default(),
        enabled,
        snapshot.as_ref(),
    );
    let detail = Detail::Model(model, crate::menu::sjk::blade_name(console), skin).to_string();
    let held = snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.me.as_ref())
        .map(|me| me.achievements.clone())
        .unwrap_or_default();
    let standings = crate::achievements::standings(&held);
    let summary = Summary {
        skin,
        ..summarise(name, detail, enabled, snapshot.as_ref(), &standings)
    };
    let mut current = lock();
    if *current != summary {
        *current = summary;
    }
}

/// The name of the blade skin the player wears: the one `setting` (`cg_saberSkin`)
/// names, while their own profile holds it, the rule their look follows
/// (`unlockables::Holdings`, `player_identity::owns_unlock`).
pub(crate) fn worn_skin(
    setting: &str,
    enabled: bool,
    snapshot: Option<&sjk_identity::Snapshot>,
) -> Option<&'static str> {
    crate::unlockables::Holdings::of(enabled, snapshot)
        .worn_blade_skin(setting)
        .map(|skin| skin.name)
}

/// The summary of a player named `name` (their model and blade `detail`), with the
/// identity `enabled` and the service's `snapshot`, and their achievements' `standings`.
pub(crate) fn summarise(
    name: &str,
    detail: String,
    enabled: bool,
    snapshot: Option<&sjk_identity::Snapshot>,
    standings: &[crate::achievements::Standing],
) -> Summary {
    use sjk_identity::Status;
    let me = snapshot.and_then(|snapshot| snapshot.me.as_ref());
    let hub = match snapshot.map(|snapshot| &snapshot.status) {
        _ if !enabled => HubState::Off,
        None | Some(Status::Disabled | Status::NoHub) => HubState::Off,
        Some(Status::Registering) => HubState::Starting,
        Some(Status::Failed(_)) => HubState::Offline,
        Some(Status::Online) if me.is_some() => HubState::Online,
        Some(Status::Online) => HubState::Starting,
    };
    let online = hub == HubState::Online;
    Summary {
        name: name.to_owned(),
        detail,
        // The caller knows the setting (`refresh`).
        skin: None,
        key_id: snapshot
            .filter(|_| enabled)
            .map(|snapshot| snapshot.key_id.clone())
            .unwrap_or_default(),
        // A picture the hub answered with stays shown while it is out of reach.
        avatar: me
            .filter(|_| enabled)
            .map(|me| me.avatar.clone())
            .unwrap_or_default(),
        hub,
        verified: online && me.is_some_and(|me| me.verified),
        staff: online && me.is_some_and(|me| me.staff),
        medals: me
            .filter(|_| online)
            .map_or(0, |me| crate::medals::Medals::from_wire(&me.medals).len()),
        unlocked: standings
            .iter()
            .filter(|standing| standing.unlocked.is_some())
            .count(),
        achievements: standings.len(),
    }
}

/// The card's line about the SJK hub: "Verified · 2 medals · 12/21 achievements".
pub(crate) struct HubLine<'a>(pub(crate) &'a Summary);

impl std::fmt::Display for HubLine<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let summary = self.0;
        match summary.hub {
            HubState::Off => f.write_str("SJK identity off")?,
            HubState::Starting => f.write_str("Connecting to the hub")?,
            HubState::Offline => f.write_str("SJK hub out of reach")?,
            HubState::Online if summary.verified => f.write_str("Verified")?,
            HubState::Online => f.write_str("SJK player")?,
        }
        match summary.medals {
            0 => {}
            1 => f.write_str(" \u{b7} 1 medal")?,
            count => write!(f, " \u{b7} {count} medals")?,
        }
        write!(
            f,
            " \u{b7} {}/{} achievements",
            summary.unlocked, summary.achievements
        )
    }
}

/// The card's place on its screens, in frame pixels: the picture's top-left corner
/// and size, the text's start and width.
pub(crate) const PICTURE: [f32; 2] = [96.0, 944.0];
pub(crate) const PICTURE_SIZE: f32 = 72.0;
const TEXT_X: f32 = PICTURE[0] + PICTURE_SIZE + 18.0;
const TEXT_WIDTH: f32 = 360.0;
/// Room round the card that belongs to it: its band when lit, its pointer area.
const PAD: f32 = 12.0;

/// The card's whole area in frame pixels: where it is lit and clicked.
pub(crate) const AREA: [f32; 4] = [
    PICTURE[0] - PAD,
    PICTURE[1] - PAD,
    TEXT_X + TEXT_WIDTH - PICTURE[0] + PAD * 2.0,
    PICTURE_SIZE + PAD * 2.0,
];

/// The card's last line.
#[derive(Clone, Copy, Debug)]
pub(crate) enum Detail<'a> {
    /// As written.
    Line(&'a str),
    /// The player's model, blade colour and the blade skin they wear, if any ("Kyle,
    /// blue saber", "Kyle, Sun blade").
    Model(&'a str, &'a str, Option<&'a str>),
}

impl std::fmt::Display for Detail<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Line(line) => f.write_str(line),
            Self::Model(model, _, Some(skin)) => {
                write!(f, "{}, {skin}", crate::menu::classic::view::Sentence(model))
            }
            Self::Model(model, blade, None) => write!(
                f,
                "{}, {blade} saber",
                crate::menu::classic::view::Sentence(model)
            ),
        }
    }
}

/// What one card shows.
pub(crate) struct Card<'a> {
    /// The in-game name, with its colour codes.
    pub(crate) name: &'a str,
    /// The line under the hub's line.
    pub(crate) detail: Detail<'a>,
    pub(crate) summary: &'a Summary,
    /// Hovered or chosen with the keys: it shows it opens the profile.
    pub(crate) lit: bool,
}

/// Draw `card` in the bottom-left corner of `frame`, its area answering `token`.
pub(crate) fn draw(canvas: &mut MenuCanvas, frame: &Frame, card: &Card<'_>, token: u16) {
    let s = frame.s;
    let [x, y, width, height] = AREA;
    let area = frame.rect(x, y, width, height);
    if card.lit {
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: area,
            radius: 18.0 * s,
            color: color::alpha(color::SPACE, 0.6),
        });
        let _ = canvas.draw_list_mut().push(DrawCommand::Border {
            rect: area,
            radius: 18.0 * s,
            width: 1.5 * s,
            color: color::alpha(color::GOLD, 0.6),
        });
    }
    let radius = PICTURE_SIZE * 0.5;
    avatar(
        canvas,
        frame.point(PICTURE[0] + radius, PICTURE[1] + radius),
        radius * s,
        &Avatar {
            key_id: &card.summary.key_id,
            version: &card.summary.avatar,
            name: card.name,
            verified: card.summary.verified,
            preview: false,
            lit: card.lit,
        },
    );
    text(
        canvas,
        TextFamily::Display,
        format_args!("{}", card.name),
        frame.rect(TEXT_X, PICTURE[1] - 4.0, TEXT_WIDTH, 34.0),
        27.0 * s,
        color::TEXT,
        FontWeight::Semibold,
        TextAlign::Start,
    );
    text(
        canvas,
        TextFamily::Body,
        format_args!("{}", HubLine(card.summary)),
        frame.rect(TEXT_X, PICTURE[1] + 30.0, TEXT_WIDTH, 22.0),
        16.0 * s,
        if card.summary.verified {
            color::GOLD_BRIGHT
        } else {
            color::MUTED
        },
        FontWeight::Regular,
        TextAlign::Start,
    );
    let (detail, tone) = if card.lit {
        (
            Detail::Line("Open your profile: picture, bio and record"),
            color::GOLD_BRIGHT,
        )
    } else {
        (card.detail, color::QUIET)
    };
    text(
        canvas,
        TextFamily::Body,
        format_args!("{detail}"),
        frame.rect(TEXT_X, PICTURE[1] + 52.0, TEXT_WIDTH, 20.0),
        15.0 * s,
        tone,
        FontWeight::Regular,
        TextAlign::Start,
    );
    canvas.hit_region(token, area);
}

/// A player's picture as [`avatar`] draws it.
pub(crate) struct Avatar<'a> {
    pub(crate) key_id: &'a str,
    /// The picture's version, empty for none.
    pub(crate) version: &'a str,
    /// Their name, for the stand-in's initial.
    pub(crate) name: &'a str,
    pub(crate) verified: bool,
    /// Draw the picture about to be uploaded instead (`avatars::set_preview`).
    pub(crate) preview: bool,
    /// Ring it in bright gold (hovered or chosen).
    pub(crate) lit: bool,
}

/// Draw `avatar` as a disc of `radius` window pixels round `centre`: the picture once
/// it is loaded, else the initial of their name on a colour of their own, ringed (gold
/// when verified) and with the verified badge at its foot.
pub(crate) fn avatar(canvas: &mut MenuCanvas, centre: [f32; 2], radius: f32, avatar: &Avatar<'_>) {
    let texture = if avatar.preview {
        crate::avatars::preview_ready().then(crate::avatars::preview_texture)
    } else {
        crate::avatars::texture(avatar.key_id, avatar.version)
    };
    let square = Rect::new(
        centre[0] - radius,
        centre[1] - radius,
        radius * 2.0,
        radius * 2.0,
    );
    match texture {
        Some(texture) => {
            let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
                rect: square,
                texture,
                color: Color::new(1.0, 1.0, 1.0, 1.0),
            });
        }
        None => {
            let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
                rect: square,
                radius,
                color: stand_in_colour(avatar.key_id, avatar.name),
            });
            text(
                canvas,
                TextFamily::Display,
                format_args!("{}", initial(avatar.name)),
                square,
                radius * 1.1,
                color::TEXT,
                FontWeight::Semibold,
                TextAlign::Center,
            );
        }
    }
    let ring = if avatar.lit {
        color::GOLD_BRIGHT
    } else if avatar.verified {
        color::alpha(color::GOLD, 0.9)
    } else {
        color::alpha(color::HOLO, 0.55)
    };
    let width = (radius * 0.055).max(1.5);
    let _ = canvas.draw_list_mut().push(DrawCommand::Arc {
        center: centre,
        radius: radius + width,
        width,
        start: 0.0,
        sweep: std::f32::consts::TAU,
        color: ring,
        knockout: None,
    });
    if avatar.verified {
        // The badge at the picture's foot, right, on a disc of the ground.
        let badge = radius * 0.36;
        let at = [centre[0] + radius * 0.72, centre[1] + radius * 0.72];
        let _ = canvas.draw_list_mut().push(DrawCommand::RoundedRect {
            rect: Rect::new(at[0] - badge, at[1] - badge, badge * 2.0, badge * 2.0),
            radius: badge,
            color: color::alpha(color::SPACE, 0.92),
        });
        let mark = badge * 0.82;
        let _ = canvas.draw_list_mut().push(DrawCommand::TexturedQuad {
            rect: Rect::new(at[0] - mark, at[1] - mark, mark * 2.0, mark * 2.0),
            texture: crate::ui_renderer::VERIFIED_TEXTURE,
            color: Color::new(1.0, 1.0, 1.0, 1.0),
        });
    }
}

/// The colours a stand-in is drawn on, dark enough for the light initial.
const STAND_INS: [u32; 8] = [
    0x34_56_9E, 0x6E_48_9C, 0x9A_45_66, 0xA1_5B_33, 0x2F_7F_72, 0x4A_76_35, 0x86_70_2E, 0x51_5D_74,
];

/// A player's stand-in colour, the same on every PC: from their key, else their name.
pub(crate) fn stand_in_colour(key_id: &str, name: &str) -> Color {
    let seed = if key_id.is_empty() {
        sjk_identity::normal_form(name)
    } else {
        key_id.to_ascii_lowercase()
    };
    // FNV-1a.
    let hash = seed.bytes().fold(0x811c_9dc5_u32, |hash, byte| {
        (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
    });
    let rgb = STAND_INS[(hash % STAND_INS.len() as u32) as usize];
    Color::new(
        ((rgb >> 16) & 0xff) as f32 / 255.0,
        ((rgb >> 8) & 0xff) as f32 / 255.0,
        (rgb & 0xff) as f32 / 255.0,
        1.0,
    )
}

/// The first letter of `name` past its colour codes and clan tags' symbols, in
/// capitals; `S` for a name with none.
pub(crate) fn initial(name: &str) -> char {
    let mut characters = name.chars().peekable();
    while let Some(character) = characters.next() {
        if character == '^' && characters.peek().is_some_and(char::is_ascii_digit) {
            characters.next();
            continue;
        }
        if character.is_alphanumeric() {
            return character.to_uppercase().next().unwrap_or(character);
        }
    }
    'S'
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::{Medal, Profile, Snapshot, Status};

    fn me() -> Profile {
        Profile {
            credits: 0,
            cards: Vec::new(),
            keys: Vec::new(),
            key_id: "0123456789abcdef".to_owned(),
            key: String::new(),
            name: "Sol".to_owned(),
            bio: String::new(),
            verified: true,
            staff: false,
            created: 0,
            names: Vec::new(),
            medals: ["early_tester", "bug_hunter", "from_the_future"]
                .iter()
                .map(|id| Medal {
                    id: (*id).to_owned(),
                    count: 1,
                    awarded: 0,
                    note: String::new(),
                })
                .collect(),
            achievements: Vec::new(),
            unlocks: Vec::new(),
            avatar: "fedcba9876543210".to_owned(),
            holocron_counts: Default::default(),
            holocrons: Vec::new(),
        }
    }

    fn snapshot(status: Status, me: Option<Profile>) -> Snapshot {
        Snapshot {
            loot: Vec::new(),
            opening: None,
            reveal: None,
            local_key_id: "0123456789abcdef".to_owned(),
            status,
            key_id: "0123456789abcdef".to_owned(),
            me,
            server: None,
            players: Vec::new(),
            profiles: std::collections::HashMap::new(),
            model_checks: std::collections::HashMap::new(),
            profiles_revision: 0,
            notice: None,
            revision: 0,
            report: None,
            note: None,
            player_report: None,
            avatar: None,
            look_outcome: None,
            packs_revision: 0,
            assets_note: None,
            crashes_sent: 0,
            crash_note: None,
            holocrons: None,
            log_requests: Vec::new(),
            log_answer: None,
        }
    }

    fn standings() -> Vec<crate::achievements::Standing> {
        crate::achievements::ALL
            .iter()
            .enumerate()
            .map(|(index, kind)| crate::achievements::Standing {
                kind,
                progress: 0,
                unlocked: (index < 3).then_some(1),
            })
            .collect()
    }

    #[test]
    fn the_line_says_what_the_hub_knows() {
        let standings = standings();
        let total = standings.len();
        let online = snapshot(Status::Online, Some(me()));
        let summary = summarise(
            "^1Sol",
            "Kyle, blue saber".into(),
            true,
            Some(&online),
            &standings,
        );
        assert_eq!(summary.avatar, "fedcba9876543210");
        assert_eq!(summary.key_id, "0123456789abcdef");
        assert_eq!(
            HubLine(&summary).to_string(),
            format!("Verified \u{b7} 2 medals \u{b7} 3/{total} achievements")
        );
        let plain = snapshot(
            Status::Online,
            Some(Profile {
                verified: false,
                medals: Vec::new(),
                ..me()
            }),
        );
        let summary = summarise("Sol", String::new(), true, Some(&plain), &standings);
        assert_eq!(
            HubLine(&summary).to_string(),
            format!("SJK player \u{b7} 3/{total} achievements")
        );
        let failed = snapshot(Status::Failed("down".into()), Some(me()));
        let summary = summarise("Sol", String::new(), true, Some(&failed), &standings);
        assert!(
            !summary.verified,
            "unconfirmed while the hub is out of reach"
        );
        assert_eq!(summary.avatar, "fedcba9876543210", "the picture stays");
        assert!(
            HubLine(&summary)
                .to_string()
                .starts_with("SJK hub out of reach")
        );
        let off = summarise("Sol", String::new(), false, Some(&online), &standings);
        assert_eq!(
            (off.hub, off.avatar.as_str(), off.key_id.as_str()),
            (HubState::Off, "", "")
        );
        assert!(
            HubLine(&off)
                .to_string()
                .starts_with("SJK identity off \u{b7} 3/")
        );
        let starting = snapshot(Status::Registering, None);
        let summary = summarise("Sol", String::new(), true, Some(&starting), &standings);
        assert_eq!(summary.hub, HubState::Starting);
    }

    #[test]
    fn the_line_fits_its_width_at_its_longest() {
        // Medals show only while the hub answers (`summarise`).
        for (hub, medals, verified) in [
            (HubState::Off, 0, false),
            (HubState::Starting, 0, false),
            (HubState::Offline, 0, false),
            (HubState::Online, 12, true),
            (HubState::Online, 12, false),
        ] {
            let summary = Summary {
                hub,
                verified,
                medals,
                unlocked: 120,
                achievements: 120,
                ..Summary::default()
            };
            // Exo 2 at 16 is about 8 pixels a character, wide ones included.
            let line = HubLine(&summary).to_string();
            assert!(line.chars().count() as f32 * 8.0 <= TEXT_WIDTH, "{line}");
        }
    }

    #[test]
    fn the_card_names_the_blade_skin_worn_while_it_is_owned() {
        let owned = Profile {
            unlocks: vec![sjk_identity::Unlock {
                id: "saber_sun".to_owned(),
                granted: 1,
                note: String::new(),
                count: 1,
                medal: None,
            }],
            ..me()
        };
        let online = snapshot(Status::Online, Some(owned));
        assert_eq!(
            worn_skin("saber_sun", true, Some(&online)),
            Some("Sun blade")
        );
        assert_eq!(
            Detail::Model("kyle", "blue", Some("Sun blade")).to_string(),
            "Kyle, Sun blade"
        );
        assert_eq!(
            Detail::Model("kyle", "blue", None).to_string(),
            "Kyle, blue saber"
        );
        // Not owned, none chosen, or the identity off: the stock blade's colour.
        let plain = snapshot(Status::Online, Some(me()));
        assert_eq!(worn_skin("saber_sun", true, Some(&plain)), None);
        assert_eq!(worn_skin("", true, Some(&online)), None);
        assert_eq!(worn_skin("saber_sun", false, Some(&online)), None);
    }

    #[test]
    fn the_initial_skips_colour_codes_and_symbols() {
        assert_eq!(initial("^5JoF^7 Jedi"), 'J');
        assert_eq!(initial("{JoF}solol"), 'J');
        assert_eq!(initial("^1^2"), 'S');
        assert_eq!(initial("sol"), 'S');
        assert_eq!(initial("ёж"), 'Ё');
    }

    #[test]
    fn a_stand_in_colour_belongs_to_its_key() {
        let a = stand_in_colour("0123456789abcdef", "Sol");
        assert_eq!(a, stand_in_colour("0123456789ABCDEF", "Other"));
        let by_name = stand_in_colour("", "^1Sol");
        assert_eq!(by_name, stand_in_colour("", "sol"));
        // Not all keys share one colour.
        let colours: std::collections::HashSet<[u32; 3]> = (0..64)
            .map(|n| {
                let c = stand_in_colour(&format!("{n:016x}"), "");
                [c.r, c.g, c.b].map(|v| (v * 255.0) as u32)
            })
            .collect();
        assert!(colours.len() >= 6, "{}", colours.len());
    }

    /// The card stays inside its corner of the window, clear of the keys at the bottom
    /// centre and the version at the bottom right, at every window shape; its pointer
    /// area is the card.
    #[test]
    fn the_card_fits_its_corner_at_every_window() {
        for viewport in [
            [1_920.0, 1_080.0],
            [3_840.0, 2_160.0],
            [1_440.0, 1_080.0],
            [1_024.0, 768.0],
            [2_560.0, 1_080.0],
            [3_440.0, 1_440.0],
        ] {
            let frame = Frame::new(viewport);
            let mut canvas = MenuCanvas::new();
            canvas.begin_transparent(viewport);
            let summary = Summary {
                name: "^1A Very Long Name^7 [JoF]".to_owned(),
                hub: HubState::Online,
                verified: true,
                medals: 4,
                ..Summary::default()
            };
            draw(
                &mut canvas,
                &frame,
                &Card {
                    name: &summary.name,
                    detail: Detail::Model("kyle", "blue", None),
                    summary: &summary,
                    lit: true,
                },
                50,
            );
            canvas.finish(50);
            assert!(!canvas.overflowed());
            let area = canvas.rect_for(50).expect("the card answers the pointer");
            assert!(area.x >= 0.0 && area.y >= 0.0, "{viewport:?}");
            assert!(area.right() <= viewport[0] * 0.5, "{viewport:?}: left half");
            assert!(area.bottom() <= viewport[1], "{viewport:?}");
            // The keys start at x 960 less half their width (some 360 frame pixels).
            let keys_left = frame.point(960.0 - 360.0, 0.0)[0];
            assert!(area.right() < keys_left, "{viewport:?}");
            for command in canvas.draw_list().commands() {
                let rect = match command {
                    DrawCommand::RoundedRect { rect, .. }
                    | DrawCommand::Border { rect, .. }
                    | DrawCommand::Text { rect, .. }
                    | DrawCommand::TexturedQuad { rect, .. } => *rect,
                    _ => continue,
                };
                assert!(
                    rect.x >= area.x - 0.5 && rect.right() <= area.right() + 0.5,
                    "{viewport:?}"
                );
                assert!(
                    rect.y >= area.y - 0.5 && rect.bottom() <= area.bottom() + 0.5,
                    "{viewport:?}"
                );
            }
        }
    }
}
