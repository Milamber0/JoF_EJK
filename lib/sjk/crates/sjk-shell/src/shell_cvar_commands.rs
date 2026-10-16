//! OpenJK cvar.cpp:1005-1100,1240-1268,1337-1424; cmd.cpp:932-946.
//! Arithmetic and script substitutions extend OpenJK using TaystJK
//! cvar.cpp:1117-1155 and common.cpp:346-487.
use super::*;
use crate::CvarFlags;

pub(super) const COMMANDS: &[(&str, &str)] = &[
    ("sets", "Set a serverinfo cvar"),
    ("setu", "Set a userinfo cvar"),
    ("unset", "Remove a user-created cvar"),
    ("unset_usercreated", "Remove all user-created cvars"),
    (
        "cvar_restart",
        "Reset writable defaults and discard user cvars",
    ),
    ("cvar_modified", "List non-default cvars"),
    ("cvar_usercreated", "List user-created cvars"),
    ("cvarAdd", "Add a number"),
    ("cvarSub", "Subtract a number"),
    ("cvarMult", "Multiply by a number"),
    ("cvarDiv", "Divide by a nonzero number"),
    ("cvarMod", "Integer remainder"),
    ("strSub", "Execute with $cvar$ substitutions"),
    ("ifCvar", "Execute the first matching cvar branch"),
    ("help", "Describe a command or cvar"),
    ("execq", "Execute a cfg quietly"),
];

impl Shell {
    pub(super) fn extended_cvar_command(
        &mut self,
        command: &str,
        args: &[String],
    ) -> Result<Vec<String>, ShellError> {
        match command {
            "unset" => {
                let [name] = args else {
                    return Err(ShellError::Usage("unset <name>"));
                };
                self.cvars.unset(name)?;
            }
            "unset_usercreated" | "cvar_restart" => {
                self.cvars.restart(command == "unset_usercreated")?;
            }
            "cvar_modified" | "cvar_usercreated" => {
                return Ok(self
                    .cvars
                    .iter()
                    .filter(|v| {
                        if command == "cvar_usercreated" {
                            v.flags.contains(CvarFlags::USER_CREATED)
                        } else {
                            v.value != v.default
                        }
                    })
                    .map(|v| {
                        format!(
                            "{} = {} (default {})",
                            v.name,
                            v.value.as_text(),
                            v.default.as_text()
                        )
                    })
                    .collect());
            }
            "help" => {
                let [name] = args else {
                    return Err(ShellError::Usage("help <name>"));
                };
                if let Some(v) = self.cvars.get(name) {
                    return Ok(vec![format!("{}: {}", v.name, v.description)]);
                }
                let found = builtin_commands()
                    .find(|(key, _)| key.eq_ignore_ascii_case(name))
                    .map(|(key, help)| format!("{key}: {help}"))
                    .or_else(|| {
                        self.commands
                            .iter()
                            .find(|(key, _)| key.eq_ignore_ascii_case(name))
                            .map(|(key, help)| format!("{key}: {help}"))
                    });
                return found
                    .map(|line| vec![line])
                    .ok_or_else(|| ShellError::UnknownCommand(name.clone()));
            }
            "ifcvar" => self.if_cvar(args)?,
            "strsub" => {
                if args.is_empty() {
                    return Err(ShellError::Usage("strSub <command ...>"));
                }
                let expanded: Vec<_> = args.iter().map(|text| self.substitute(text)).collect();
                self.command_buffer.append(&quoted_command(&expanded))?;
            }
            _ => {
                let [name, operand] = args else {
                    return Err(ShellError::Usage("cvar arithmetic <name> <number>"));
                };
                let a = number(&self.cvar_text(name));
                let b = number(operand);
                let value = match command {
                    "cvaradd" => a + b,
                    "cvarsub" => a - b,
                    "cvarmult" => a * b,
                    "cvardiv" if b != 0.0 => a / b,
                    "cvarmod" if b as i32 != 0 => {
                        (a as i32).checked_rem(b as i32).unwrap_or(0) as f32
                    }
                    _ => return Err(ShellError::Application("Cannot divide by zero".into())),
                };
                if !value.is_finite() {
                    return Err(ShellError::Application(
                        "Non-finite arithmetic result".into(),
                    ));
                }
                return self.set_command(&[name.clone(), value.to_string()], CvarFlags::NONE);
            }
        }
        Ok(Vec::new())
    }

    fn cvar_text(&self, name: &str) -> String {
        self.cvars
            .get(name)
            .map(|v| v.value.as_text())
            .unwrap_or_default()
    }

    fn substitute(&self, text: &str) -> String {
        let mut rest = text;
        let mut out = String::new();
        while let Some(index) = rest.find('$') {
            out.push_str(&rest[..index]);
            rest = &rest[index + 1..];
            if rest.is_empty() {
                out.push('$');
                break;
            }
            if let Some(tail) = rest.strip_prefix('$') {
                out.push('$');
                rest = tail;
                continue;
            }
            let end = rest.find('$').unwrap_or(rest.len());
            out.push_str(&self.cvar_text(&rest[..end]));
            rest = rest.get(end + 1..).unwrap_or("");
        }
        out.push_str(rest);
        out
    }

    fn if_cvar(&mut self, args: &[String]) -> Result<(), ShellError> {
        let [name, branches @ ..] = args else {
            return Err(ShellError::Usage(
                "ifCvar <cvar> <condition> <argc> <command ...>",
            ));
        };
        let mut branches = branches;
        let value = self.cvar_text(name);
        while !branches.is_empty() {
            let [condition, count, tail @ ..] = branches else {
                return Err(ShellError::Usage("incomplete ifCvar branch"));
            };
            let count = count
                .parse::<usize>()
                .ok()
                .filter(|n| (1..1024).contains(n))
                .filter(|n| *n <= tail.len())
                .ok_or(ShellError::Usage("invalid ifCvar argc"))?;
            if self.condition(&value, condition) {
                self.command_buffer
                    .append(&quoted_command(&tail[..count]))?;
                return Ok(());
            }
            branches = &tail[count..];
        }
        Ok(())
    }

    fn condition(&self, value: &str, condition: &str) -> bool {
        let lower = condition.to_ascii_lowercase();
        if lower.starts_with("$else") {
            return true;
        }
        for op in [
            "$!=",
            "$>=",
            "$<=",
            "$=",
            "$>",
            "$<",
            "$contains",
            "$beginswith",
            "$startswith",
            "$endswith",
            "",
        ] {
            if !lower.starts_with(op) {
                continue;
            }
            let rhs = &condition[op.len()..];
            let rhs = rhs
                .strip_prefix('$')
                .map(|name| self.cvar_text(name))
                .unwrap_or_else(|| rhs.to_owned());
            return match op {
                "$=" => number(value) == number(&rhs),
                "$!=" => number(value) != number(&rhs),
                "$>=" => number(value) >= number(&rhs),
                "$<=" => number(value) <= number(&rhs),
                "$>" => number(value) > number(&rhs),
                "$<" => number(value) < number(&rhs),
                "$contains" => value
                    .to_ascii_lowercase()
                    .contains(&rhs.to_ascii_lowercase()),
                "$beginswith" | "$startswith" => value
                    .to_ascii_lowercase()
                    .starts_with(&rhs.to_ascii_lowercase()),
                "$endswith" => value
                    .to_ascii_lowercase()
                    .ends_with(&rhs.to_ascii_lowercase()),
                _ => value.eq_ignore_ascii_case(&rhs),
            };
        }
        false
    }
}

// atof-style prefix parsing, without C's undefined arithmetic or overflow.
fn number(text: &str) -> f32 {
    let text = text.trim_start();
    text.char_indices()
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .filter_map(|end| text[..end].parse::<f32>().ok())
        .last()
        .unwrap_or(0.0)
}

fn quoted_command(args: &[String]) -> String {
    args.iter()
        .map(|arg| format!("\"{}\"", arg.replace('\\', "\\\\").replace('"', "\\\"")))
        .collect::<Vec<_>>()
        .join(" ")
}
