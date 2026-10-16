//! The interface between SJK and its client mods.
//!
//! A mod is a set of console commands and settings that live outside the core
//! client. Every name a mod owns starts with its id and a dot (`japlus.guntele`,
//! `japlus.loginServer1`), and its commands exist only on the servers it is made
//! for ([`ClientMod::loads_on`]) and while the player leaves it on (`mod_<id>`, on
//! by default). A mod reaches the game through [`Host`] alone: it sends server
//! commands and reads what the client already knows (the crosshair point, the
//! predicted player, the drawn players), and never touches the renderer, the
//! prediction or the wire directly.
//!
//! The mods are built into the client (see `docs/mods.md`); this crate is the line
//! they are written against.

pub mod known_servers;
mod player;

pub use known_servers::{KNOWN_SERVERS, KnownServer};
pub use player::{Drawn, Player, resolve_player, strip_colours};

/// A console command a mod adds, or a server command it completes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Command {
    /// The name as typed; a mod's own commands start with `<id>.`.
    pub name: &'static str,
    /// One line for the console's help and completion.
    pub help: &'static str,
}

/// Shorthand for a [`Command`] in a table.
pub const fn command(name: &'static str, help: &'static str) -> Command {
    Command { name, help }
}

/// A saved setting a mod owns; its name starts with `<id>.`.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Setting {
    pub name: &'static str,
    pub default: &'static str,
    pub help: &'static str,
}

/// The game module a server runs, as far as mods care.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ServerKind {
    BaseJka,
    JaPlus,
    /// jaPRO and its TaystJK lineage.
    JaPro,
    Other,
}

/// The server the client is on.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Server {
    pub kind: ServerKind,
    /// Its address, `host:port`.
    pub address: String,
    /// `CS_SERVERINFO`, a `\key\value` info string.
    pub info: String,
}

impl Server {
    /// The family this server belongs to, if SJK knows it ([`known_servers`]).
    pub fn family(&self) -> Option<&'static KnownServer> {
        known_servers::identify(self)
    }

    /// The value of `key` in the server info (keys ignore case).
    pub fn info(&self, key: &str) -> Option<&str> {
        let mut parts = self
            .info
            .strip_prefix('\\')
            .unwrap_or(&self.info)
            .split('\\');
        while let (Some(name), Some(value)) = (parts.next(), parts.next()) {
            if name.eq_ignore_ascii_case(key) {
                return Some(value);
            }
        }
        None
    }
}

/// The local player as the client predicts it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Me {
    pub client: u16,
    pub origin: [f32; 3],
    /// Pitch, yaw, roll in degrees, as the game sends them.
    pub view_angles: [f32; 3],
    pub intermission: bool,
}

/// What the client lends a mod while one of its commands runs.
pub trait Host {
    /// Send a reliable command to the server.
    fn send(&mut self, command: &str) -> Result<(), String>;
    /// The server the client is on, if it is in a game.
    fn server(&self) -> Option<Server>;
    /// The local player, if it is in a game.
    fn me(&self) -> Option<Me>;
    /// Where the crosshair's trace ended this frame.
    fn crosshair_point(&self) -> Option<[f32; 3]>;
    /// The player under the crosshair (retained for a second, as the game does).
    fn crosshair_player(&self) -> Option<u16>;
    /// Every player in the roster, with where each is drawn when in view.
    fn players(&self) -> Vec<Player>;
    /// A setting's value as text.
    fn cvar(&self, name: &str) -> Option<String>;
    /// Change and save a setting.
    fn set_cvar(&mut self, name: &str, value: &str) -> Result<(), String>;
}

/// A client mod.
pub trait ClientMod: Send {
    /// Lower-case id: the prefix of its names and of its switch, `mod_<id>`.
    fn id(&self) -> &'static str;
    /// Its name in menus, such as "JA+ tools".
    fn title(&self) -> &'static str;
    /// One line on what it adds.
    fn about(&self) -> &'static str;
    /// The servers it loads on, for messages: "JA+" in "JA+ servers".
    fn servers(&self) -> &'static str;
    /// Whether it loads on `server`: its commands exist only there.
    fn loads_on(&self, server: &Server) -> bool;
    /// The commands it adds while loaded.
    fn commands(&self) -> &'static [Command];
    /// Its saved settings, kept whether it is on or not.
    fn settings(&self) -> &'static [Setting] {
        &[]
    }
    /// Server commands it completes on `server` while loaded (none by default).
    fn server_commands(&self, _server: &Server) -> &'static [Command] {
        &[]
    }
    /// Run one of [`ClientMod::commands`]; `name` is in lower case.
    fn run(
        &mut self,
        name: &str,
        args: &[String],
        host: &mut dyn Host,
    ) -> Result<Vec<String>, String>;
}

/// The unit vector of `angles` (pitch, yaw, roll in degrees), `AngleVectors`' forward.
pub fn forward(angles: [f32; 3]) -> [f32; 3] {
    let (pitch, yaw) = (angles[0].to_radians(), angles[1].to_radians());
    [
        pitch.cos() * yaw.cos(),
        pitch.cos() * yaw.sin(),
        -pitch.sin(),
    ]
}

/// `atof`: the leading number of `text`, 0 when there is none.
pub fn number(text: &str) -> f32 {
    let text = text.trim_start();
    let end = text
        .char_indices()
        .take_while(|&(index, character)| {
            character.is_ascii_digit()
                || matches!(character, '.' | 'e' | 'E')
                || (index == 0 && matches!(character, '-' | '+'))
        })
        .map(|(index, character)| index + character.len_utf8())
        .last()
        .unwrap_or(0);
    // A trailing exponent marker or sign is not part of the number.
    (0..=end)
        .rev()
        .find_map(|end| text[..end].parse::<f32>().ok())
        .unwrap_or(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_server_info_keys_in_any_case() {
        let server = Server {
            kind: ServerKind::JaPlus,
            address: "127.0.0.1:29070".into(),
            info: r"\gamename\JA+ Mod v2.5\V\2.5B0\jp_cinfo\196819".into(),
        };
        assert_eq!(server.info("v"), Some("2.5B0"));
        assert_eq!(server.info("JP_CINFO"), Some("196819"));
        assert_eq!(server.info("mapname"), None);
    }

    #[test]
    fn forward_follows_angle_vectors() {
        let [x, y, z] = forward([0.0, 90.0, 0.0]);
        assert!(x.abs() < 1e-6 && (y - 1.0).abs() < 1e-6 && z.abs() < 1e-6);
        // Looking down (positive pitch) points below the horizon.
        assert!(forward([45.0, 0.0, 0.0])[2] < 0.0);
    }

    #[test]
    fn number_reads_like_atof() {
        assert_eq!(number("150"), 150.0);
        assert_eq!(number(" -32.5abc"), -32.5);
        assert_eq!(number("1e"), 1.0);
        assert_eq!(number("gun"), 0.0);
    }
}
