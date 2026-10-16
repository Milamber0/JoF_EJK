//! Info/profile commands: OpenJK codemp/client/cl_main.cpp:961-971,1278-1330,2552-2585.
use super::super::*;
use sjk_client::ClientSession;

impl ViewerConsole {
    pub(super) fn info_command(
        &mut self,
        name: &str,
        args: &[String],
        mut session: Option<&mut ClientSession>,
    ) -> Result<Vec<String>, String> {
        match name {
            "cmd" => {
                let session = session.as_deref_mut().ok_or("Not connected to a server.")?;
                if !args.is_empty() {
                    session
                        .send_reliable_command(args.join(" ").as_bytes())
                        .map_err(|error| error.to_string())?;
                }
                Ok(Vec::new())
            }
            "model" | "forcepowers" => {
                let value = match args {
                    [] => {
                        return Ok(vec![format!(
                            "{name} is set to {}",
                            self.text_value(name).unwrap_or("")
                        )]);
                    }
                    [value] => value.clone(),
                    [model, skin] if name == "model" => format!("{model}/{skin}"),
                    _ => return Err(format!("usage: {name} <value>")),
                };
                self.shell
                    .cvars
                    .set_text(name, &value)
                    .map_err(|error| error.to_string())?;
                self.persist();
                Ok(vec![format!("{name} is set to {value}")])
            }
            "clientinfo" | "userinfo" => {
                let mut lines = vec![
                    "--------- Client Information ---------".into(),
                    format!(
                        "state: {}",
                        if session.is_some() {
                            "active"
                        } else {
                            "disconnected"
                        }
                    ),
                    format!(
                        "Server: {}",
                        session
                            .as_ref()
                            .map(|s| s.server().to_string())
                            .unwrap_or_default()
                    ),
                ];
                let mut info = String::new();
                for var in self
                    .shell
                    .cvars
                    .iter()
                    .filter(|v| v.flags.contains(CvarFlags::USER_INFO))
                {
                    info.push('\\');
                    info.push_str(&var.name);
                    info.push('\\');
                    info.push_str(&var.value.as_text());
                }
                lines.push(info);
                Ok(lines)
            }
            "configstrings" => {
                let session = session.ok_or("Not connected to a server.")?;
                Ok(session
                    .game_state()
                    .config_strings()
                    .filter(|(_, value)| !value.is_empty())
                    .map(|(index, value)| format!("{index:4}: {}", String::from_utf8_lossy(value)))
                    .collect())
            }
            "fs_openedlist" => {
                let vfs = self
                    .script_vfs
                    .as_ref()
                    .ok_or("No asset search path attached")?;
                Ok(vfs
                    .mounts()
                    .map(|mount| format!("Opened: {}", mount.name))
                    .collect())
            }
            "fs_referencedlist" => {
                // The proof adapter does not retain FS_ReferencedPakNames. Never substitute
                // the server's advertised references for the client's actually-used packages.
                Err(
                    "Client referenced-pak tracking is unavailable; mounted paks: fs_openedList"
                        .into(),
                )
            }
            "afk" | "colorname" | "colorstring" => self.identity_command(name, args),
            "listemojis" => Ok(crate::chat::emoji::list_lines(
                self.script_vfs
                    .as_ref()
                    .ok_or("No asset search path attached")?,
            )),
            "mods" => Ok(self.mods_listing()),
            _ => Err(format!("Unsupported client command: {name}")),
        }
    }
}
