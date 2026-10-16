//! Bug reports, world notes and player reports sent to the hub (`PROTOCOL.md`, "Bug
//! reports", "World notes" and "Player reports"). The hub decides; these are the same
//! rules, so the game can refuse a character as it is typed and say what is wrong before
//! anything is sent.

/// Shortest and longest report text, in characters, after whitespace is normalised.
pub const TEXT_MIN: usize = 10;
/// Longest report text, in characters.
pub const TEXT_MAX: usize = 600;
/// The punctuation a report may use besides letters, digits and spaces.
pub const PUNCTUATION: &str = ".,!?'-:()";
const RUN_MAX: usize = 6;
const LETTERS_MIN: usize = 5;
const WORDS_MIN: usize = 2;
/// Shortest and longest world note text, in characters, after whitespace is normalised.
pub const NOTE_MIN: usize = 3;
/// Longest world note text, in characters: about a page (650 words), room for several
/// points about one place. In UTF-8 it is at most 16,000 bytes, which with the note's
/// context stays inside the hub's 48 KiB note body (`PROTOCOL.md`, "World notes").
pub const NOTE_MAX: usize = 4_000;
/// Most remaps a note carries to the hub: the ones of the noted shaders first.
pub const NOTE_REMAPS_MAX: usize = 64;
/// Most shaders of a noted model a note carries to the hub.
pub const NOTE_MODEL_SHADERS_MAX: usize = 32;
/// Longest shader name a note's context carries.
pub const NOTE_SHADER_MAX: usize = 64;
/// Longest model path or skin a note's context carries.
pub const NOTE_MODEL_PATH_MAX: usize = 96;
const NOTE_LETTERS_MIN: usize = 2;
/// Shortest and longest player report reason, in characters, after whitespace is
/// normalised.
pub const PLAYER_MIN: usize = 10;
/// Longest player report reason, in characters.
pub const PLAYER_MAX: usize = 300;

/// Why a player is reported (`PROTOCOL.md`, "Player reports").
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum Category {
    /// Aimbots, speed or wall hacks.
    Cheating,
    /// Insults, threats, hate.
    Harassment,
    /// Team-killing, blocking, ruining the match on purpose.
    Griefing,
    /// Winning through a map or game bug.
    Exploit,
    /// A name that insults or offends.
    Name,
    /// Flooding the chat, advertising.
    Spam,
    /// Anything else.
    #[default]
    Other,
}

impl Category {
    /// Every category, in the order the game lists them.
    pub const ALL: [Self; 7] = [
        Self::Cheating,
        Self::Harassment,
        Self::Griefing,
        Self::Exploit,
        Self::Name,
        Self::Spam,
        Self::Other,
    ];

    /// The hub's code for it.
    pub const fn code(self) -> &'static str {
        match self {
            Self::Cheating => "cheating",
            Self::Harassment => "harassment",
            Self::Griefing => "griefing",
            Self::Exploit => "exploit",
            Self::Name => "name",
            Self::Spam => "spam",
            Self::Other => "other",
        }
    }

    /// How the game names it.
    pub const fn label(self) -> &'static str {
        match self {
            Self::Cheating => "Cheating",
            Self::Harassment => "Harassment or hate",
            Self::Griefing => "Griefing",
            Self::Exploit => "Exploiting a bug",
            Self::Name => "Offensive name",
            Self::Spam => "Spam or advertising",
            Self::Other => "Something else",
        }
    }

    /// What it covers, in a line.
    pub const fn hint(self) -> &'static str {
        match self {
            Self::Cheating => "Aimbot, speed or wall hacks, other cheats",
            Self::Harassment => "Insults, threats or hate in chat or names",
            Self::Griefing => "Team-killing, blocking, ruining the match on purpose",
            Self::Exploit => "Winning through a map or game bug",
            Self::Name => "A name that insults or offends",
            Self::Spam => "Flooding the chat, advertising",
            Self::Other => "Anything the others do not cover",
        }
    }
}

/// A report about another player on the same game server, for the hub's operator.
/// Only a verified key may send one.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PlayerReport {
    /// Why.
    pub category: Category,
    /// What the reporter wrote ([`player_text`] checks it).
    pub text: String,
    /// The game server's `ip:port`, both players on it.
    pub server: String,
    /// The server's name as the game shows it (colour codes kept), or empty.
    pub server_name: String,
    /// The reported player's slot.
    pub slot: u8,
    /// The name the game shows for that slot (colour codes kept).
    pub target_name: String,
    /// The key the hub's presence list shows in that slot under that name, or empty.
    pub target_key_id: String,
    /// The map, `maps/<name>.bsp`, or empty.
    pub map: String,
    /// The client's build.
    pub build: String,
    /// Seconds since the map started (the match clock).
    pub level_time: Option<u32>,
    /// The in-game name the reporter wears; empty takes the one the service knows.
    pub name: String,
}

/// A bug report: the tester's text and where they were.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct BugReport {
    /// What the tester wrote ([`text`] checks it).
    pub text: String,
    /// The map, `maps/<name>.bsp`, or empty.
    pub map: String,
    /// The client's build.
    pub build: String,
    /// The game server's `ip:port`, or empty.
    pub server: String,
    /// The in-game name the player wears; empty takes the one the service knows.
    pub name: String,
}

/// A world note: the player's text about what they aimed at, and where.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct WorldNote {
    /// What the player wrote ([`note_text`] checks it).
    pub text: String,
    /// The map, `maps/<name>.bsp`, or empty.
    pub map: String,
    /// The client's build.
    pub build: String,
    /// The game server's `ip:port`, or empty.
    pub server: String,
    /// `setviewpos` back to the view: x, y, z and yaw.
    pub view: Option<[f32; 4]>,
    /// The aimed point and the surface's normal there.
    pub hit: Option<[f32; 3]>,
    /// The surface's normal at [`WorldNote::hit`].
    pub normal: Option<[f32; 3]>,
    /// The aimed shader ([`field`] empties a name the hub would refuse).
    pub shader: String,
    /// The BSP draw surface's index.
    pub surface: Option<u32>,
    /// How the surface is lit (`lightmapped`, `vertex-lit`).
    pub lighting: String,
    /// From the eye to the aimed point.
    pub distance: Option<f32>,
    /// The aimed entity's class name.
    pub entity: String,
    /// The shader a remap draws over [`WorldNote::shader`], what the player saw, or
    /// empty when none does.
    pub remapped: String,
    /// The shader remaps active when the note was taken, at most [`NOTE_REMAPS_MAX`].
    pub remaps: Vec<NoteRemap>,
    /// The model the note is about, when the player picked one.
    pub model: Option<NoteModel>,
    /// The in-game name the player wears; empty takes the one the service knows.
    pub name: String,
}

/// One shader remap active when a note was taken.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoteRemap {
    /// Who made it: `map` (the worldspawn's keys), `server` or `local` (the console).
    pub source: String,
    /// The shader the map or model names.
    pub from: String,
    /// The shader drawn instead.
    pub to: String,
}

/// A model a note is about: a map entity drawing an MD3.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NoteModel {
    /// The model's path (`models/map_objects/...md3`).
    pub path: String,
    /// The entity's class name (`misc_model_static`).
    pub classname: String,
    /// The entity's number in the map's entity list.
    pub entity: Option<u32>,
    /// Where the entity stands.
    pub origin: Option<[f32; 3]>,
    /// The entity's skin key, or empty.
    pub skin: String,
    /// Its surfaces' shaders, at most [`NOTE_MODEL_SHADERS_MAX`].
    pub shaders: Vec<NoteModelShader>,
}

/// One surface of a noted model and the shader it draws.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct NoteModelShader {
    /// The MD3 surface's name.
    pub surface: String,
    /// The shader the model names for it.
    pub shader: String,
    /// The shader a remap draws instead, or empty.
    pub remapped: String,
}

/// Whether a report may contain `c`.
pub fn allowed(c: char) -> bool {
    c.is_alphanumeric() || c == ' ' || PUNCTUATION.contains(c)
}

/// The report text as the hub stores it, or why the hub would refuse it.
pub fn text(raw: &str) -> Result<String, &'static str> {
    check(raw, TEXT_MIN, TEXT_MAX, LETTERS_MIN, WORDS_MIN).map_err(|refusal| match refusal {
        Refusal::Character => CHARACTERS,
        Refusal::Short => "a report needs at least 10 characters",
        Refusal::Long => "a report is at most 600 characters",
        Refusal::Noise => "a report needs a few real words",
    })
}

/// The note text as the hub stores it, or why the hub would refuse it.
pub fn note_text(raw: &str) -> Result<String, &'static str> {
    check(raw, NOTE_MIN, NOTE_MAX, NOTE_LETTERS_MIN, 1).map_err(|refusal| match refusal {
        Refusal::Character => CHARACTERS,
        Refusal::Short => "a note needs at least 3 characters",
        Refusal::Long => "a note is at most 4000 characters",
        Refusal::Noise => "a note needs a few letters",
    })
}

/// A player report's reason as the hub stores it, or why the hub would refuse it.
pub fn player_text(raw: &str) -> Result<String, &'static str> {
    check(raw, PLAYER_MIN, PLAYER_MAX, LETTERS_MIN, WORDS_MIN).map_err(|refusal| match refusal {
        Refusal::Character => CHARACTERS,
        Refusal::Short => "say what happened in at least 10 characters",
        Refusal::Long => "a player report is at most 300 characters",
        Refusal::Noise => "say what happened in a few real words",
    })
}

/// A context field as the hub takes it: `raw` trimmed when it is at most `max` ASCII
/// letters, digits and `_ - . /` and spaces, else empty (an odd name is left out
/// rather than the whole note refused).
pub fn field(raw: &str, max: usize) -> String {
    let value = raw.trim();
    if value.len() <= max
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-./ ".contains(&b))
    {
        value.to_owned()
    } else {
        String::new()
    }
}

const CHARACTERS: &str = "only letters, digits, spaces and . , ! ? ' - : ( ) are allowed";

enum Refusal {
    Character,
    Short,
    Long,
    Noise,
}

fn check(
    raw: &str,
    min: usize,
    max: usize,
    letters_min: usize,
    words_min: usize,
) -> Result<String, Refusal> {
    let text = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if !text.chars().all(allowed) {
        return Err(Refusal::Character);
    }
    let length = text.chars().count();
    if length < min {
        return Err(Refusal::Short);
    }
    if length > max {
        return Err(Refusal::Long);
    }
    let letters = text.chars().filter(|c| c.is_alphabetic()).count();
    let words = text
        .split(' ')
        .filter(|word| word.chars().any(char::is_alphanumeric))
        .count();
    let mut run = (None, 0);
    let longest = text.chars().fold(0, |longest, c| {
        run = if run.0 == Some(c) {
            (Some(c), run.1 + 1)
        } else {
            (Some(c), 1)
        };
        longest.max(run.1)
    });
    if letters < letters_min || words < words_min || longest > RUN_MAX {
        return Err(Refusal::Noise);
    }
    Ok(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_rules_match_the_hubs() {
        assert_eq!(
            text(" The door\nflickers on ffa3! ").as_deref(),
            Ok("The door flickers on ffa3!")
        );
        assert!(text("Le sol brille trop, été").is_ok());
        assert!(text("<b>bold</b> text here").is_err());
        assert!(text("short").is_err());
        assert!(text("aaaaaaaaaaaa bb").is_err());
        assert!(text(&"word ".repeat(200)).is_err());
        assert!(!allowed('<') && !allowed('"') && !allowed('/') && !allowed('\u{202e}'));
        assert!(allowed('é') && allowed('7') && allowed('?'));
    }

    #[test]
    fn player_reports_take_the_alphabet_and_a_closed_list_of_reasons() {
        assert_eq!(
            player_text(" speed hacking\nall round ").as_deref(),
            Ok("speed hacking all round")
        );
        assert!(player_text("hacker").is_err());
        assert!(player_text(&"word ".repeat(70)).is_err());
        assert!(player_text("<b>wallhack</b> obviously").is_err());
        let codes: Vec<&str> = Category::ALL.iter().map(|c| c.code()).collect();
        // The hub's list, in its order (`PROTOCOL.md`, "Player reports").
        assert_eq!(
            codes,
            [
                "cheating",
                "harassment",
                "griefing",
                "exploit",
                "name",
                "spam",
                "other"
            ]
        );
        assert!(
            Category::ALL
                .iter()
                .all(|c| !c.label().is_empty() && !c.hint().is_empty())
        );
    }

    #[test]
    fn notes_take_the_alphabet_with_their_own_bounds() {
        assert_eq!(note_text(" too  shiny ").as_deref(), Ok("too shiny"));
        // Up to a page: 4000 characters pass, one more does not.
        let page = "word ".repeat(800);
        assert_eq!(note_text(&page).map(|t| t.chars().count()), Ok(3_999));
        assert!(note_text(&format!("{page}ab")).is_err());
        assert!(note_text("Where video ???").is_ok());
        assert!(note_text("ok").is_err());
        assert!(note_text("1234 5678").is_err());
        assert!(note_text("a \"quoted\" word").is_err());
        assert_eq!(
            field(" textures/mp/s_ylight_red ", 64),
            "textures/mp/s_ylight_red"
        );
        assert_eq!(field("models/x+y", 64), "");
        assert_eq!(field(&"a".repeat(65), 64), "");
    }
}
