//! The client mods built into this client (`docs/mods.md`): their switches
//! (`mod_<id>`, on by default), their saved settings, and their commands, which
//! exist only while a mod is on and the client is on a server it loads on. A mod's commands run on the main thread through [`host::ViewerHost`],
//! after the console has queued them like the other client commands.

use sjk_mod::{ClientMod, Server};
use sjk_shell::{CvarDefinition, CvarFlags, CvarValue, Shell};

#[path = "mods_host.rs"]
mod host;

/// The mods this build carries, in menu order.
// Each mod is pushed under its own feature, which `vec![]` cannot express.
#[allow(clippy::vec_init_then_push)]
fn builtin() -> Vec<Box<dyn ClientMod>> {
    #[cfg_attr(
        not(any(feature = "mod-japlus", feature = "mod-jof")),
        allow(unused_mut)
    )]
    let mut list: Vec<Box<dyn ClientMod>> = Vec::new();
    #[cfg(feature = "mod-japlus")]
    list.push(Box::new(sjk_mod_japlus::JaPlus::new()));
    #[cfg(feature = "mod-jof")]
    list.push(Box::new(sjk_mod_jof::JoF::new()));
    list
}

/// The setting that switches mod `id` on.
pub(crate) fn switch(id: &str) -> String {
    format!("mod_{id}")
}

/// Bumped when the switches' default changes: a profile saved before then gets
/// the new default once. 1: on by default (11/10/2026; they were off).
const DEFAULT_VERSION: &str = "mod_defaultVersion";
const CURRENT_DEFAULTS: i64 = 1;

struct Entry {
    id: &'static str,
    /// Lent out while one of its commands runs.
    module: Option<Box<dyn ClientMod>>,
    /// Whether the player leaves it on (`mod_<id>`).
    allowed: bool,
    /// Whether its commands are registered: allowed and on a server it loads on.
    on: bool,
}

/// The built-in mods and which are on.
pub(crate) struct Mods {
    entries: Vec<Entry>,
    /// The server the client is on, for the server commands mods complete.
    server: Option<Server>,
}

impl Mods {
    pub(crate) fn new() -> Self {
        Self {
            entries: builtin()
                .into_iter()
                .map(|module| Entry {
                    id: module.id(),
                    module: Some(module),
                    allowed: false,
                    on: false,
                })
                .collect(),
            server: None,
        }
    }

    /// Register every mod's switch and settings; settings are kept while a mod
    /// is off so its saved values survive.
    pub(crate) fn register(&self, shell: &mut Shell) -> Result<(), sjk_shell::CvarError> {
        for module in self
            .entries
            .iter()
            .filter_map(|entry| entry.module.as_ref())
        {
            shell.cvars.register(CvarDefinition::new(
                switch(module.id()),
                true,
                CvarFlags::ARCHIVE,
                format!(
                    "{} on {} servers: {}",
                    module.title(),
                    module.servers(),
                    module.about()
                ),
            ))?;
            for setting in module.settings() {
                shell.cvars.register(CvarDefinition::new(
                    setting.name,
                    setting.default,
                    CvarFlags::ARCHIVE,
                    setting.help,
                ))?;
            }
        }
        if shell.cvars.get(DEFAULT_VERSION).is_none() {
            shell.cvars.register(CvarDefinition::new(
                DEFAULT_VERSION,
                0_i64,
                CvarFlags::ARCHIVE,
                "Version of the mod switches' defaults this profile has",
            ))?;
        }
        Ok(())
    }

    /// Once per default change, after the profile loads: put every switch back
    /// to its default (mods were off by default before version 1).
    pub(crate) fn migrate_defaults(&self, shell: &mut Shell) {
        let saved = shell
            .cvars
            .get(DEFAULT_VERSION)
            .and_then(|cvar| cvar.value.as_text().trim().parse::<i64>().ok())
            .unwrap_or(0);
        if saved >= CURRENT_DEFAULTS {
            return;
        }
        for entry in &self.entries {
            let _ = shell.cvars.reset(&switch(entry.id));
        }
        let _ = shell
            .cvars
            .set_text(DEFAULT_VERSION, &CURRENT_DEFAULTS.to_string());
    }

    /// Follow the switches and the server: register the commands of mods that
    /// are on and load on this server, and remove the others'. Returns whether
    /// one changed.
    pub(crate) fn sync(&mut self, shell: &mut Shell) -> bool {
        let mut changed = false;
        for entry in &mut self.entries {
            let Some(module) = entry.module.as_ref() else {
                continue;
            };
            entry.allowed = shell
                .cvars
                .get(&switch(entry.id))
                .is_some_and(|cvar| matches!(cvar.value, CvarValue::Bool(true)));
            let on = entry.allowed
                && self
                    .server
                    .as_ref()
                    .is_some_and(|server| module.loads_on(server));
            if on == entry.on {
                continue;
            }
            for command in module.commands() {
                if on {
                    let _ = shell.commands.register(command.name, command.help, |_| {
                        Err(sjk_shell::CommandError::Handler(
                            "Viewer command dispatcher required".into(),
                        ))
                    });
                } else {
                    shell.commands.unregister(command.name);
                }
            }
            entry.on = on;
            changed = true;
        }
        changed
    }

    /// The mod a command belongs to, by its `<id>.` prefix.
    fn owner(&self, name: &str) -> Option<usize> {
        let (prefix, _) = name.split_once('.')?;
        self.entries
            .iter()
            .position(|entry| entry.id.eq_ignore_ascii_case(prefix))
    }

    /// For the console: `None` when `name` is no mod's, else whether its mod
    /// takes it now, or why not.
    pub(crate) fn accepts(&self, name: &str) -> Option<Result<(), String>> {
        let entry = &self.entries[self.owner(name)?];
        let known = entry.module.as_ref().is_none_or(|module| {
            module
                .commands()
                .iter()
                .any(|command| command.name.eq_ignore_ascii_case(name))
        });
        Some(if !known {
            Err(format!("Unknown command \"{name}\""))
        } else if !entry.allowed {
            Err(format!(
                "{name}: this mod is off; turn it on in Settings or with {} 1",
                switch(entry.id)
            ))
        } else if !entry.on {
            Err(format!(
                "{name}: works on {} servers only",
                entry
                    .module
                    .as_ref()
                    .map_or("its", |module| module.servers())
            ))
        } else {
            Ok(())
        })
    }

    /// Remember the server the client joined (`None` when it left).
    pub(crate) fn set_server(&mut self, server: Option<Server>) {
        self.server = server;
    }

    /// The server commands the mods that are on complete on the current server.
    pub(crate) fn server_help(&self) -> Vec<(String, String)> {
        let Some(server) = &self.server else {
            return Vec::new();
        };
        self.entries
            .iter()
            .filter(|entry| entry.on)
            .filter_map(|entry| entry.module.as_ref())
            .flat_map(|module| module.server_commands(server))
            .map(|command| (command.name.to_owned(), command.help.to_owned()))
            .collect()
    }

    /// Each mod's id, title and description, with its state in words.
    pub(crate) fn listing(&self) -> Vec<(&'static str, &'static str, &'static str, String)> {
        self.entries
            .iter()
            .filter_map(|entry| {
                let module = entry.module.as_ref()?;
                let state = if entry.on {
                    "^2loaded".to_owned()
                } else if entry.allowed {
                    format!("^3on, loads on {} servers", module.servers())
                } else {
                    "^1off".to_owned()
                };
                Some((entry.id, module.title(), module.about(), state))
            })
            .collect()
    }

    fn lend(&mut self, index: usize) -> Option<Box<dyn ClientMod>> {
        self.entries.get_mut(index)?.module.take()
    }

    fn give_back(&mut self, index: usize, module: Box<dyn ClientMod>) {
        self.entries[index].module = Some(module);
    }
}

/// `sjk_client`'s server profile as the mods see it.
pub(crate) fn server_kind(profile: &sjk_client::CompatProfile) -> sjk_mod::ServerKind {
    match profile {
        sjk_client::CompatProfile::BaseJka => sjk_mod::ServerKind::BaseJka,
        sjk_client::CompatProfile::JaPlus { .. } => sjk_mod::ServerKind::JaPlus,
        sjk_client::CompatProfile::TaystJk => sjk_mod::ServerKind::JaPro,
        sjk_client::CompatProfile::Unknown(_) => sjk_mod::ServerKind::Other,
    }
}

impl crate::GpuState {
    /// Run a queued mod command; `None` when `tokens` is no mod's.
    pub(crate) fn run_mod_command(
        &mut self,
        tokens: &[String],
    ) -> Option<Result<Vec<String>, String>> {
        let name = tokens.first()?.to_ascii_lowercase();
        let console = self.console.as_mut()?;
        let index = console.mods.owner(&name)?;
        if let Some(Err(error)) = console.mods.accepts(&name) {
            return Some(Err(error));
        }
        let mut module = console.mods.lend(index)?;
        let result = module.run(&name, &tokens[1..], &mut host::ViewerHost::new(self));
        if let Some(console) = self.console.as_mut() {
            console.mods.give_back(index, module);
        }
        Some(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell() -> (Shell, Mods) {
        let mut shell = Shell::new(sjk_shell::CvarRegistry::new(), Default::default());
        let mods = Mods::new();
        mods.register(&mut shell).unwrap();
        (shell, mods)
    }

    fn server(kind: sjk_mod::ServerKind, info: &str) -> Option<Server> {
        Some(Server {
            kind,
            address: "127.0.0.1:29070".into(),
            info: info.into(),
        })
    }

    #[test]
    fn mods_load_on_their_servers_only() {
        let (mut shell, mut mods) = shell();
        // On by default, but off a server nothing loads.
        assert!(!mods.sync(&mut shell));
        assert!(!shell.commands.contains("japlus.guntele"));
        assert!(
            matches!(mods.accepts("japlus.guntele"), Some(Err(error)) if error.contains("JA+ servers"))
        );
        assert_eq!(mods.accepts("connect"), None);
        // A base server loads neither.
        mods.set_server(server(sjk_mod::ServerKind::BaseJka, ""));
        assert!(!mods.sync(&mut shell));
        // A JA+ server loads JA+ tools, not JoF tools.
        mods.set_server(server(sjk_mod::ServerKind::JaPlus, r"\V\2.4B7"));
        assert!(mods.sync(&mut shell));
        assert!(shell.commands.contains("japlus.guntele"));
        assert_eq!(mods.accepts("JAPLUS.GunTele"), Some(Ok(())));
        assert!(matches!(mods.accepts("japlus.nothing"), Some(Err(_))));
        assert!(!shell.commands.contains("jof.commands"));
        // A JoF server is a JA+ one too: both load.
        mods.set_server(server(sjk_mod::ServerKind::JaPlus, r"\V\2.5B0"));
        assert!(mods.sync(&mut shell));
        assert!(shell.commands.contains("japlus.guntele"));
        assert!(shell.commands.contains("jof.commands"));
        // The switch still turns a mod off.
        shell.cvars.set_text("mod_japlus", "0").unwrap();
        assert!(mods.sync(&mut shell));
        assert!(!shell.commands.contains("japlus.guntele"));
        assert!(
            matches!(mods.accepts("japlus.guntele"), Some(Err(error)) if error.contains("mod_japlus 1"))
        );
        // Leaving the server unloads the rest.
        mods.set_server(None);
        assert!(mods.sync(&mut shell));
        assert!(!shell.commands.contains("jof.commands"));
    }

    #[test]
    fn an_old_profile_gets_the_mods_on_once() {
        let (mut shell, mods) = shell();
        // Saved while the mods were off by default.
        shell.cvars.set_text("mod_japlus", "0").unwrap();
        mods.migrate_defaults(&mut shell);
        assert_eq!(shell.cvars.get("mod_japlus").unwrap().value.as_text(), "1");
        // A choice made after that stays.
        shell.cvars.set_text("mod_japlus", "0").unwrap();
        mods.migrate_defaults(&mut shell);
        assert_eq!(shell.cvars.get("mod_japlus").unwrap().value.as_text(), "0");
    }

    #[test]
    fn settings_exist_while_the_mod_is_off() {
        let (shell, _) = shell();
        assert!(shell.cvars.get("japlus.loginServer1").is_some());
        assert!(shell.cvars.get("mod_jof").is_some());
    }

    #[test]
    fn server_commands_complete_only_while_loaded() {
        let (mut shell, mut mods) = shell();
        mods.set_server(server(sjk_mod::ServerKind::JaPlus, r"\V\2.5B0"));
        shell.cvars.set_text("mod_jof", "0").unwrap();
        assert!(mods.server_help().is_empty());
        mods.sync(&mut shell);
        let help = mods.server_help();
        assert!(help.iter().any(|(name, _)| name == "amtele"));
        assert!(!help.iter().any(|(name, _)| name == "gunkick"));
        shell.cvars.set_text("mod_jof", "1").unwrap();
        mods.sync(&mut shell);
        assert!(mods.server_help().iter().any(|(name, _)| name == "gunkick"));
    }
}
