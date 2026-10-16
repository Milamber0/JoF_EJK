//! JA+ tools: the client conveniences of JA+ and of the EternalJK family of
//! clients made for it, as an SJK mod (`japlus.*`, `docs/mods.md`).
//!
//! What a JA+ server needs to be played correctly stays in the core client (its
//! movement rules, `cjp_client`, `cp_pluginDisable`, saber colours and cosmetics):
//! this mod only adds commands a player types.

mod login;
mod options;
mod server_commands;
mod teleport;

use sjk_mod::{ClientMod, Command, Host, ServerKind, Setting, command};

/// The mod's id and prefix.
pub const ID: &str = "japlus";

const COMMANDS: &[Command] = &[
    command(
        "japlus.guntele",
        "Teleport to your crosshair, or a distance along your view: japlus.guntele [distance] [yaw offset] (JA+ admin)",
    ),
    command(
        "japlus.bring",
        "Bring a player in front of you: japlus.bring [id|name|gun] [distance] [yaw offset] (JA+ admin)",
    ),
    command(
        "japlus.goto",
        "Teleport in front of a player: japlus.goto [id|name|gun] [distance] [yaw offset] (JA+ admin)",
    ),
    command(
        "japlus.teleoffset",
        "Teleport by an offset from where you are: japlus.teleoffset x [y [z]] (JA+ admin)",
    ),
    command("japlus.mark", "Remember where you stand for japlus.recall"),
    command(
        "japlus.recall",
        "Return to the japlus.mark position (setviewpos; the server must allow it)",
    ),
    command(
        "japlus.autologin",
        "Log in as admin with the password saved for this server (japlus.loginServer1-3)",
    ),
    command(
        "japlus.serverconfig",
        "List the JA+ server's options (asked of jaPRO servers)",
    ),
    command(
        "japlus.plugin",
        "List or toggle JA+ plugin features (cp_pluginDisable): japlus.plugin [id]",
    ),
    command(
        "japlus.color",
        "Set your model's tint: japlus.color <red> <green> <blue> (0-255)",
    ),
];

const SETTINGS: &[Setting] = &[
    Setting {
        name: "japlus.loginServer1",
        default: "",
        help: "Server address for japlus.autologin (port 29070 when left out)",
    },
    Setting {
        name: "japlus.loginPass1",
        default: "",
        help: "Admin password sent to japlus.loginServer1 (saved as plain text)",
    },
    Setting {
        name: "japlus.loginServer2",
        default: "",
        help: "Server address for japlus.autologin (port 29070 when left out)",
    },
    Setting {
        name: "japlus.loginPass2",
        default: "",
        help: "Admin password sent to japlus.loginServer2 (saved as plain text)",
    },
    Setting {
        name: "japlus.loginServer3",
        default: "",
        help: "Server address for japlus.autologin (port 29070 when left out)",
    },
    Setting {
        name: "japlus.loginPass3",
        default: "",
        help: "Admin password sent to japlus.loginServer3 (saved as plain text)",
    },
];

/// The JA+ tools mod.
#[derive(Default)]
pub struct JaPlus {
    mark: Option<teleport::Mark>,
}

impl JaPlus {
    pub fn new() -> Self {
        Self::default()
    }
}

/// The local player, for the commands that need one.
fn me(host: &dyn Host) -> Result<sjk_mod::Me, String> {
    host.me().ok_or_else(|| "not in a game".to_owned())
}

/// `japlus.color <red> <green> <blue>`: EternalJK's `amColor`
/// (`cg_consolecmds.c:1245`), checked to 0-255.
fn color(args: &[String], host: &mut dyn Host) -> Result<Vec<String>, String> {
    let [red, green, blue] = args else {
        return Err(
            "usage: japlus.color <red> <green> <blue>, such as japlus.color 255 255 0".into(),
        );
    };
    let mut values = [0_u8; 3];
    for (value, text) in values.iter_mut().zip([red, green, blue]) {
        *value = text
            .trim()
            .parse()
            .map_err(|_| format!("japlus.color: {text} is not 0 to 255"))?;
    }
    for (name, value) in ["char_color_red", "char_color_green", "char_color_blue"]
        .iter()
        .zip(values)
    {
        host.set_cvar(name, &value.to_string())?;
    }
    Ok(Vec::new())
}

impl ClientMod for JaPlus {
    fn id(&self) -> &'static str {
        ID
    }

    fn title(&self) -> &'static str {
        "JA+ tools"
    }

    fn about(&self) -> &'static str {
        "Admin teleports, auto login, plugin options and command completion for JA+ servers"
    }

    fn servers(&self) -> &'static str {
        "JA+"
    }

    /// Every JA+ server, JoF's included.
    fn loads_on(&self, server: &sjk_mod::Server) -> bool {
        server.kind == ServerKind::JaPlus
    }

    fn commands(&self) -> &'static [Command] {
        COMMANDS
    }

    fn settings(&self) -> &'static [Setting] {
        SETTINGS
    }

    fn server_commands(&self, server: &sjk_mod::Server) -> &'static [Command] {
        if server.kind == ServerKind::JaPlus {
            server_commands::JA_PLUS
        } else {
            &[]
        }
    }

    fn run(
        &mut self,
        name: &str,
        args: &[String],
        host: &mut dyn Host,
    ) -> Result<Vec<String>, String> {
        let command = match name {
            "japlus.guntele" => teleport::gun_tele(args, me(host)?, host.crosshair_point())?,
            "japlus.bring" => teleport::bring(args, host)?,
            "japlus.goto" => {
                let (command, note) = teleport::go_to(args, host)?;
                host.send(&command)?;
                return Ok(note.into_iter().collect());
            }
            "japlus.teleoffset" => teleport::offset(args, me(host)?)?,
            "japlus.mark" => {
                let mark = teleport::Mark::of(me(host)?);
                self.mark = Some(mark);
                return Ok(vec![mark.describe()]);
            }
            "japlus.recall" => self.mark.ok_or("No mark set: japlus.mark first")?.recall(),
            "japlus.autologin" => return login::auto_login(host),
            "japlus.serverconfig" => return options::server_config(host),
            "japlus.plugin" => return options::plugin(args, host),
            "japlus.color" => return color(args, host),
            _ => return Err(format!("Unknown JA+ tools command {name}")),
        };
        host.send(&command)?;
        Ok(Vec::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_name_carries_the_prefix() {
        let mod_ = JaPlus::new();
        for name in mod_
            .commands()
            .iter()
            .map(|command| command.name)
            .chain(mod_.settings().iter().map(|setting| setting.name))
        {
            assert!(name.starts_with("japlus."), "{name}");
        }
        for slot in 1..=login::SLOTS {
            let (server, password) = login::names(slot);
            assert!(SETTINGS.iter().any(|setting| setting.name == server));
            assert!(SETTINGS.iter().any(|setting| setting.name == password));
        }
    }
}
