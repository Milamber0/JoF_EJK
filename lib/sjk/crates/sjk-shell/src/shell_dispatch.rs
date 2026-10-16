//! Command dispatch and script expansion; queue ownership stays in Shell.
use super::*;

impl Shell {
    pub(super) fn execute_buffered_one<R: CommandFileResolver>(
        &mut self,
        name: &str,
        arguments: &[String],
        exec_depth: u8,
        resolver: &mut R,
    ) -> Result<Vec<String>, ShellError> {
        let lower = name.to_ascii_lowercase();
        match lower.as_str() {
            "exec" | "execq" => self
                .exec_command(arguments, exec_depth, resolver)
                .map(|lines| if lower == "execq" { Vec::new() } else { lines }),
            "vstr" => self.vstr_command(arguments, exec_depth),
            "wait" => self.wait_command(arguments),
            "delay" | "waitf" | "delaycancel" | "waitfcancel" => {
                self.schedule_command(&lower, arguments)
            }
            "echo" => Ok(vec![arguments.join(" ")]),
            // OpenJK cvar.cpp:1589: print describes a cvar; it is not echo.
            "print" => {
                let [name] = arguments else {
                    return Err(ShellError::Usage("print <cvar>"));
                };
                self.describe_cvar(name)
            }
            "path" | "dir" | "fdir" | "touchfile" | "which" | "condump" => {
                let dump = if lower == "condump" {
                    self.lines()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    String::new()
                };
                resolver
                    .file_command(&lower, arguments, &dump)
                    .map_err(ShellError::Application)
            }
            "sets" => self.set_command(arguments, crate::CvarFlags::SERVER_INFO),
            "setu" => self.set_command(arguments, crate::CvarFlags::USER_INFO),
            "unset" | "unset_usercreated" | "cvar_restart" | "cvar_modified" | "cvaradd"
            | "cvar_usercreated" | "cvarsub" | "cvarmult" | "cvardiv" | "cvarmod" | "ifcvar"
            | "strsub" | "help" => self.extended_cvar_command(&lower, arguments),
            "set" => self.set_command(arguments, crate::CvarFlags::NONE),
            "seta" => self.set_command(arguments, crate::CvarFlags::ARCHIVE),
            "reset" => self.reset_command(arguments),
            "toggle" => self.toggle_command(arguments),
            "bind" => self.bind_command(arguments),
            "unbind" => self.unbind_command(arguments),
            "unbindall" => {
                self.binds.clear();
                Ok(vec!["all bindings cleared".to_owned()])
            }
            "bindlist" => Ok(self
                .binds
                .iter()
                .map(|(key, command)| {
                    format!("{:<16} {command}", crate::key_names::display_key(key))
                })
                .collect()),
            "cvarlist" => Ok(self
                .cvars
                .iter()
                .filter(|cvar| list_matches(&cvar.name, arguments))
                .map(|cvar| format!("{:<24} {}", cvar.name, cvar.value.as_text()))
                .collect()),
            "cmdlist" => {
                let mut lines = builtin_commands()
                    .map(|(name, description)| format!("{name:<16} {description}"))
                    .collect::<Vec<_>>();
                lines.extend(
                    self.commands
                        .iter()
                        .map(|(name, description)| format!("{name:<16} {description}")),
                );
                lines.extend(
                    self.external_command_help
                        .iter()
                        .map(|(name, description)| format!("{name:<16} {description} [server]")),
                );
                lines.sort();
                lines.retain(|line| {
                    list_matches(line.split_whitespace().next().unwrap_or(""), arguments)
                });
                Ok(lines)
            }
            "write" | "writeconfig" => self.write_config_command(arguments),
            "clear" => {
                self.clear_lines();
                Ok(Vec::new())
            }
            _ => {
                if let Some(result) = self.commands.dispatch(name, arguments) {
                    return result.map_err(Into::into);
                }
                if self.cvars.get(name).is_some() {
                    if arguments.is_empty() {
                        return self.describe_cvar(name);
                    }
                    self.cvars.set_text(name, &arguments.join(" "))?;
                    return self.describe_cvar(name);
                }
                Err(ShellError::UnknownCommand(name.to_owned()))
            }
        }
    }

    fn exec_command<R: CommandFileResolver>(
        &mut self,
        arguments: &[String],
        exec_depth: u8,
        resolver: &mut R,
    ) -> Result<Vec<String>, ShellError> {
        let [requested] = arguments else {
            return Err(ShellError::Usage("exec <filename>"));
        };
        let filename = default_cfg_extension(requested);
        // `exec config.cfg` reads what is saved so far, as when every frame saved.
        if self
            .config_path
            .as_deref()
            .and_then(Path::file_name)
            .is_some_and(|name| name.eq_ignore_ascii_case(Path::new(&filename).as_os_str()))
        {
            let _ = self.save_if_dirty();
        }
        let Some(text) = resolver
            .read_command_file(&filename)
            .map_err(ShellError::ScriptResolver)?
        else {
            return Err(ShellError::ScriptNotFound(filename));
        };
        // Cmd_Exec_f inserts the bytes at the front (`cmd.cpp:272-297`).
        self.command_buffer
            .insert_after_current(&text, exec_depth.saturating_add(1))?;
        Ok(vec![format!("execing {filename}")])
    }

    fn vstr_command(
        &mut self,
        arguments: &[String],
        exec_depth: u8,
    ) -> Result<Vec<String>, ShellError> {
        let [name] = arguments else {
            return Err(ShellError::Usage("vstr <variablename>"));
        };
        let value = self
            .cvars
            .get(name)
            .map(|cvar| cvar.value.as_text())
            .unwrap_or_default();
        // Cmd_Vstr_f feeds the value through Cbuf_InsertText (`cmd.cpp:303-317`).
        self.command_buffer
            .insert_after_current(&value, exec_depth)?;
        Ok(Vec::new())
    }

    fn wait_command(&mut self, arguments: &[String]) -> Result<Vec<String>, ShellError> {
        let frames = if let [value] = arguments {
            value.parse::<i32>().unwrap_or(0)
        } else {
            1
        };
        self.command_buffer
            .set_wait(if frames < 0 { 1 } else { frames as u32 });
        Ok(Vec::new())
    }
}
