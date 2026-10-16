//! Who the SJK chat page's side window lists (`docs/hub-chat.md`, "Who is online"):
//! the SJK players reading the chat now and the ones seen most recently.
//!
//! A hub that lists them (the feed's `people`, [`sjk_identity::People`]) gives both:
//! online is a key that read the feed in the last minute (the keys the online count
//! counts), playing one of them holding a live claim marked active; recent are the keys
//! that read it before, most recently seen first. A hub that does not list them gives
//! only the count, and the window falls back on the chat itself: the last senders,
//! newest first, with when they last spoke.
//!
//! The window shows each player as a face: a small disc with their picture, or their
//! initial on their colour ([`crate::profile_card::stand_in_colour`]). The faces wrap
//! in a grid laid out here ([`columns`], [`split`], [`centre`]), so the page can test
//! where each one goes without drawing.

use sjk_identity::{ChatState, normal_form};

/// The online faces' radius, the recent ones' (smaller) and the gap between two faces
/// (frame pixels).
pub(super) const ONLINE_RADIUS: f32 = 20.0;
pub(super) const RECENT_RADIUS: f32 = 15.0;
pub(super) const FACE_GAP: f32 = 10.0;
/// The window's room for faces across (frame pixels).
pub(super) const FACES_WIDTH: f32 = 484.0;
/// Rows of online faces at most; with more players, the last face says "+N".
pub(super) const ONLINE_FACE_ROWS: usize = 2;
/// Recent faces at most (the hub lists eight), in one row.
pub(super) const RECENT_FACES: usize = 8;

/// Faces of `radius` that fit across `width`, `FACE_GAP` between them (one at least).
pub(super) const fn columns(width: f32, radius: f32) -> usize {
    let across = ((width + FACE_GAP) / (radius * 2.0 + FACE_GAP)) as usize;
    if across == 0 { 1 } else { across }
}

/// Online faces the window has room for.
pub(super) const ONLINE_FACES: usize = columns(FACES_WIDTH, ONLINE_RADIUS) * ONLINE_FACE_ROWS;

/// Of `total` players in `capacity` faces: how many show, and how many the last face
/// counts ("+N") when they do not all fit; it takes a face of its own.
pub(super) fn split(total: usize, capacity: usize) -> (usize, usize) {
    if total > capacity {
        let shown = capacity.saturating_sub(1);
        (shown, total - shown)
    } else {
        (total, 0)
    }
}

/// Rows `count` faces take, `columns` to a row.
pub(super) fn rows(count: usize, columns: usize) -> usize {
    count.div_ceil(columns.max(1))
}

/// The centre of face `index` of a grid whose top left is `origin`, `columns` across,
/// faces of `radius` (frame pixels): left to right, then down.
pub(super) fn centre(index: usize, columns: usize, radius: f32, origin: [f32; 2]) -> [f32; 2] {
    let columns = columns.max(1);
    let pitch = radius * 2.0 + FACE_GAP;
    [
        origin[0] + radius + (index % columns) as f32 * pitch,
        origin[1] + radius + (index / columns) as f32 * pitch,
    ]
}

/// The height of a grid of `count` faces, `columns` across, of `radius`.
pub(super) fn grid_height(count: usize, columns: usize, radius: f32) -> f32 {
    let rows = rows(count, columns) as f32;
    if rows == 0.0 {
        0.0
    } else {
        rows * radius * 2.0 + (rows - 1.0) * FACE_GAP
    }
}

/// The letter a face without a picture shows: the first letter or digit of the name,
/// colour codes aside, in capitals.
pub(super) fn letter(name: &str) -> char {
    crate::profile_card::initial(name)
}

/// One player in the window.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct Row {
    pub(super) key_id: String,
    /// The name as the hub or the message gives it, colour codes kept.
    pub(super) name: String,
    pub(super) verified: bool,
    pub(super) staff: bool,
    /// Their picture's version as the hub listed it (`""` for none); `None` when the row
    /// comes from a message, which does not say.
    pub(super) avatar: Option<String>,
    /// Playing a match now (online rows only).
    pub(super) playing: bool,
    /// When they were last seen (recent rows), unix seconds.
    pub(super) seen: u64,
    /// Muted on this PC.
    pub(super) muted: bool,
}

/// What the window shows.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(super) struct Listed {
    /// The hub lists who is online; else only `count` is known and `recent` comes
    /// from the chat's senders.
    pub(super) listed: bool,
    /// How many read the chat now, as the hub says.
    pub(super) count: u32,
    /// The online players on show: playing first, then by name.
    pub(super) online: Vec<Row>,
    /// Online players left out for room (the last face says "+N" of them).
    pub(super) more: usize,
    /// The recently seen players, most recent first.
    pub(super) recent: Vec<Row>,
}

/// The window's players from `chat`, `muted` being the keys muted on this PC.
pub(super) fn derive(chat: &ChatState, muted: &[String]) -> Listed {
    let is_muted = |key: &str| muted.iter().any(|muted| muted == key);
    let Some(people) = &chat.people else {
        // No list: the last senders, newest first, a holocron drop being no message.
        let mut recent: Vec<Row> = Vec::with_capacity(RECENT_FACES);
        for message in chat.messages.iter().rev() {
            if recent.len() == RECENT_FACES {
                break;
            }
            if message.holocron.is_some()
                || message.key_id.is_empty()
                || recent.iter().any(|row| row.key_id == message.key_id)
            {
                continue;
            }
            recent.push(Row {
                key_id: message.key_id.clone(),
                name: message.name.clone(),
                verified: message.verified,
                staff: message.staff,
                avatar: None,
                playing: false,
                seen: u64::try_from(message.at).unwrap_or(0),
                muted: is_muted(&message.key_id),
            });
        }
        return Listed {
            listed: false,
            count: chat.online,
            online: Vec::new(),
            more: 0,
            recent,
        };
    };
    let row = |person: &sjk_identity::Person| Row {
        key_id: person.key_id.clone(),
        name: person.name.clone(),
        verified: person.verified,
        staff: person.staff,
        avatar: Some(person.avatar.clone()),
        playing: person.playing,
        seen: u64::try_from(person.seen).unwrap_or(0),
        muted: is_muted(&person.key_id),
    };
    // Online: each key once, playing first, then by name as players read it.
    let mut online: Vec<&sjk_identity::Person> = Vec::with_capacity(people.online.len());
    for person in &people.online {
        if !person.key_id.is_empty() && !online.iter().any(|seen| seen.key_id == person.key_id) {
            online.push(person);
        }
    }
    online.sort_by_cached_key(|person| {
        (
            !person.playing,
            normal_form(&person.name),
            person.key_id.clone(),
        )
    });
    // A full window keeps its last face to say how many more there are.
    let (shown, more) = split(online.len(), ONLINE_FACES);
    // Recent: not online now, most recently seen first, each key once.
    let mut recent: Vec<&sjk_identity::Person> = people
        .recent
        .iter()
        .filter(|person| {
            !person.key_id.is_empty() && !online.iter().any(|on| on.key_id == person.key_id)
        })
        .collect();
    recent.sort_by_key(|person| std::cmp::Reverse(person.seen));
    let mut kept: Vec<Row> = Vec::with_capacity(RECENT_FACES);
    for person in recent {
        if kept.len() == RECENT_FACES {
            break;
        }
        if !kept.iter().any(|row| row.key_id == person.key_id) {
            kept.push(row(person));
        }
    }
    Listed {
        listed: true,
        count: chat.online,
        online: online[..shown].iter().map(|person| row(person)).collect(),
        more,
        recent: kept,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sjk_identity::{ChatMessage, DropMark, People, Person};

    fn person(key: &str, name: &str, seen: i64, playing: bool) -> Person {
        Person {
            key_id: key.repeat(16),
            name: name.to_owned(),
            seen,
            playing,
            ..Person::default()
        }
    }

    fn said(id: u64, key: &str, name: &str) -> ChatMessage {
        ChatMessage {
            id,
            at: 1_000 + id as i64,
            key_id: key.repeat(16),
            name: name.to_owned(),
            verified: key == "a",
            staff: false,
            text: "hi".to_owned(),
            holocron: None,
        }
    }

    fn names(rows: &[Row]) -> Vec<&str> {
        rows.iter().map(|row| row.name.as_str()).collect()
    }

    #[test]
    fn online_lists_players_playing_first_then_by_name_and_counts_the_rest() {
        let chat = ChatState {
            online: 8,
            people: Some(People {
                online: vec![
                    person("a", "^2zed", 10, false),
                    person("b", "Fox", 10, true),
                    person("c", "^1Alpha", 10, false),
                    person("a", "^2zed", 10, false),
                    person("d", "kyle", 10, false),
                    person("e", "Bastila", 10, true),
                    person("f", "Mara", 10, false),
                ],
                recent: Vec::new(),
            }),
            ..ChatState::default()
        };
        let listed = derive(&chat, &[]);
        assert!(listed.listed);
        assert_eq!(listed.count, 8);
        // Six keys (one listed twice), all in the window.
        assert_eq!(
            names(&listed.online),
            ["Bastila", "Fox", "^1Alpha", "kyle", "Mara", "^2zed"]
        );
        assert_eq!(listed.more, 0);
        assert!(listed.online[0].playing && !listed.online[2].playing);
    }

    #[test]
    fn a_full_window_keeps_its_last_face_for_the_others() {
        let crowd = |count: usize| ChatState {
            people: Some(People {
                online: (0..count)
                    .map(|n| Person {
                        key_id: format!("{n:016x}"),
                        name: format!("Player {n:02}"),
                        ..Person::default()
                    })
                    .collect(),
                recent: Vec::new(),
            }),
            ..ChatState::default()
        };
        // Exactly full: every face, no "+N".
        let listed = derive(&crowd(ONLINE_FACES), &[]);
        assert_eq!((listed.online.len(), listed.more), (ONLINE_FACES, 0));
        // One more: the last face says "+2", the two it stands for.
        let listed = derive(&crowd(ONLINE_FACES + 1), &[]);
        assert_eq!((listed.online.len(), listed.more), (ONLINE_FACES - 1, 2));
        // The hub's fifty.
        let listed = derive(&crowd(50), &[]);
        assert_eq!(listed.online.len() + listed.more, 50);
        assert_eq!(listed.online.len(), ONLINE_FACES - 1);
    }

    #[test]
    fn faces_wrap_in_a_grid_that_fits_the_window() {
        // Nine online faces and twelve recent ones across the window's room.
        assert_eq!(columns(FACES_WIDTH, ONLINE_RADIUS), 9);
        assert_eq!(columns(FACES_WIDTH, RECENT_RADIUS), 12);
        assert_eq!(ONLINE_FACES, 18);
        assert!(
            RECENT_FACES <= columns(FACES_WIDTH, RECENT_RADIUS),
            "one row"
        );
        // Never no column, however narrow.
        assert_eq!(columns(10.0, ONLINE_RADIUS), 1);
        let cols = columns(FACES_WIDTH, ONLINE_RADIUS);
        let origin = [100.0, 50.0];
        // Left to right, then the next row under the first face.
        assert_eq!(centre(0, cols, ONLINE_RADIUS, origin), [120.0, 70.0]);
        assert_eq!(centre(1, cols, ONLINE_RADIUS, origin), [170.0, 70.0]);
        assert_eq!(centre(cols, cols, ONLINE_RADIUS, origin), [120.0, 120.0]);
        // The last face of a row ends inside the room.
        let last = centre(cols - 1, cols, ONLINE_RADIUS, origin);
        assert!(last[0] + ONLINE_RADIUS <= origin[0] + FACES_WIDTH);
        // Heights: none, one row, two rows with a gap.
        assert_eq!(grid_height(0, cols, ONLINE_RADIUS), 0.0);
        assert_eq!(grid_height(cols, cols, ONLINE_RADIUS), 40.0);
        assert_eq!(grid_height(cols + 1, cols, ONLINE_RADIUS), 90.0);
        assert_eq!(rows(ONLINE_FACES, cols), ONLINE_FACE_ROWS);
        // Split: what fits shows; more takes the last face.
        assert_eq!(split(3, 18), (3, 0));
        assert_eq!(split(18, 18), (18, 0));
        assert_eq!(split(19, 18), (17, 2));
        assert_eq!(split(5, 0), (0, 5));
    }

    #[test]
    fn a_face_without_a_picture_shows_the_first_letter_of_the_name() {
        assert_eq!(letter("^1Sol^7Vulpes"), 'S');
        assert_eq!(letter("^5creyon"), 'C');
        assert_eq!(letter("{JoF}Fox"), 'J');
        assert_eq!(letter("^3^4 7even"), '7');
        assert_eq!(letter("éclair"), 'É');
        // Nothing to read: SJK's S.
        assert_eq!(letter("^1***"), 'S');
    }

    #[test]
    fn recent_leaves_out_who_is_online_and_comes_newest_first() {
        let chat = ChatState {
            people: Some(People {
                online: vec![person("a", "Sol", 100, false)],
                recent: vec![
                    person("b", "Old", 10, false),
                    person("a", "Sol", 50, false),
                    person("c", "Newest", 90, false),
                    person("d", "Middle", 60, false),
                    person("e", "Older", 20, false),
                    person("f", "Oldest", 5, false),
                ],
            }),
            ..ChatState::default()
        };
        let muted = ["d".repeat(16)];
        let listed = derive(&chat, &muted);
        assert_eq!(
            names(&listed.recent),
            ["Newest", "Middle", "Older", "Old", "Oldest"]
        );
        assert_eq!(listed.recent[0].seen, 90);
        assert!(listed.recent[1].muted && !listed.recent[0].muted);
        assert_eq!(listed.recent[0].avatar.as_deref(), Some(""));
    }

    #[test]
    fn a_hub_that_lists_nobody_falls_back_on_the_last_senders() {
        let mut drop = said(9, "z", "Lucky");
        drop.holocron = Some(DropMark {
            tier: "legendary".to_owned(),
            own: false,
        });
        let chat = ChatState {
            online: 4,
            messages: [
                said(1, "e", "Earliest"),
                said(2, "a", "^2Sol"),
                said(3, "b", "Fox"),
                said(4, "c", "Kyle"),
                said(5, "a", "^2Sol"),
                said(6, "d", "Mara"),
                drop,
            ]
            .into_iter()
            .collect(),
            ..ChatState::default()
        };
        let listed = derive(&chat, &[]);
        assert!(!listed.listed);
        assert_eq!(listed.count, 4);
        assert!(listed.online.is_empty());
        // Newest first, each sender once, the drop left out.
        assert_eq!(
            names(&listed.recent),
            ["Mara", "^2Sol", "Kyle", "Fox", "Earliest"]
        );
        assert_eq!(listed.recent[1].seen, 1_005, "when they last spoke");
        assert!(listed.recent[1].verified);
        assert_eq!(listed.recent[0].avatar, None, "a message does not say");
        // Nothing said: nobody.
        assert_eq!(derive(&ChatState::default(), &[]), Listed::default());
    }
}
