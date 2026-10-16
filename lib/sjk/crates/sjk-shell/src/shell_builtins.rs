use super::*;
use crate::{CvarDefinition, CvarError, CvarFlags, CvarValue, ShellError, save_config};

impl Shell {
    pub(super) fn set_command(
        &mut self,
        arguments: &[String],
        dynamic_flags: CvarFlags,
    ) -> Result<Vec<String>, ShellError> {
        let [name, values @ ..] = arguments else {
            return Err(ShellError::Usage("set[a] <name> <value>"));
        };
        if values.is_empty() {
            return self.describe_cvar(name);
        }
        let value = values.join(" ");
        if self.cvars.get(name).is_none() {
            self.cvars.register(CvarDefinition::new(
                name,
                value.clone(),
                dynamic_flags | CvarFlags::USER_CREATED,
                "User-created cvar",
            ))?;
        } else {
            self.cvars.set_text(name, &value)?;
            self.cvars.add_flags(name, dynamic_flags)?;
        }
        self.describe_cvar(name)
    }

    pub(super) fn describe_cvar(&self, name: &str) -> Result<Vec<String>, ShellError> {
        let cvar = self
            .cvars
            .get(name)
            .ok_or_else(|| CvarError::Unknown(name.to_owned()))?;
        Ok(vec![format!(
            "{} = \"{}\" (default \"{}\")",
            cvar.name,
            cvar.value.as_text(),
            cvar.default.as_text()
        )])
    }

    pub(super) fn reset_command(
        &mut self,
        arguments: &[String],
    ) -> Result<Vec<String>, ShellError> {
        let [name] = arguments else {
            return Err(ShellError::Usage("reset <name>"));
        };
        self.cvars.reset(name)?;
        self.describe_cvar(name)
    }

    pub(super) fn toggle_command(
        &mut self,
        arguments: &[String],
    ) -> Result<Vec<String>, ShellError> {
        let [name, values @ ..] = arguments else {
            return Err(ShellError::Usage("toggle <name> [value1 value2 ...]"));
        };
        // OpenJK cvar.cpp:1005-1036: exact string cycling, first value on no match.
        if !values.is_empty() {
            if values.len() < 2 {
                return Err(ShellError::Usage("toggle needs at least two values"));
            }
            let current = self
                .cvars
                .get(name)
                .map(|v| v.value.as_text())
                .unwrap_or_default();
            let next = values
                .iter()
                .position(|value| *value == current)
                .map_or(0, |index| (index + 1) % values.len());
            return self.set_command(&[name.clone(), values[next].clone()], CvarFlags::NONE);
        }
        if self.cvars.get(name).is_none() {
            return self.set_command(&[name.clone(), "1".into()], CvarFlags::NONE);
        }
        let current = self
            .cvars
            .get(name)
            .ok_or_else(|| CvarError::Unknown(name.clone()))?
            .value
            .clone();
        let next = match current {
            CvarValue::Bool(value) => CvarValue::Bool(!value),
            CvarValue::Integer(value) => CvarValue::Integer(i64::from(value == 0)),
            CvarValue::Float(value) => CvarValue::Float(f64::from(value == 0.0)),
            CvarValue::Text(value) => {
                CvarValue::Text(i32::from(value.parse::<f32>().unwrap_or(0.0) == 0.0).to_string())
            }
        };
        self.cvars.set_value(name, next)?;
        self.describe_cvar(name)
    }

    pub(super) fn bind_command(&mut self, arguments: &[String]) -> Result<Vec<String>, ShellError> {
        let Some(key) = arguments.first() else {
            return Err(ShellError::Usage("bind <key> [command]"));
        };
        let key = crate::key_names::canonical_key(key)
            .ok_or_else(|| crate::BindError::InvalidKey(key.clone()))?;
        if arguments.len() == 1 {
            return Ok(vec![self.binds.get(key).map_or_else(
                || format!("{} is not bound", crate::key_names::display_key(key)),
                |command| format!("{} = \"{command}\"", crate::key_names::display_key(key)),
            )]);
        }
        let command = arguments[1..].join(" ");
        if command.is_empty() {
            self.binds.unbind(key);
            return Ok(Vec::new());
        }
        self.binds.bind(key, command.clone())?;
        Ok(vec![format!(
            "{} = \"{command}\"",
            crate::key_names::display_key(key)
        )])
    }

    pub(super) fn unbind_command(
        &mut self,
        arguments: &[String],
    ) -> Result<Vec<String>, ShellError> {
        let [key] = arguments else {
            return Err(ShellError::Usage("unbind <key>"));
        };
        let key = crate::key_names::canonical_key(key)
            .ok_or_else(|| crate::BindError::InvalidKey(key.clone()))?;
        self.binds.unbind(key);
        Ok(vec![format!(
            "{} unbound",
            crate::key_names::display_key(key)
        )])
    }

    pub(super) fn write_config_command(
        &mut self,
        arguments: &[String],
    ) -> Result<Vec<String>, ShellError> {
        let path = match arguments {
            [] => self.config_path.clone().ok_or(ShellError::NoConfigPath)?,
            [path] => PathBuf::from(path),
            _ => return Err(ShellError::Usage("writeconfig [path]")),
        };
        if self.config_load_failed {
            return Err(ShellError::ConfigLoadFailed);
        }
        // The profile's own file goes through the saver, after its queued writes.
        if self.config_path.as_deref() == Some(path.as_path()) {
            self.save()?;
        } else {
            save_config(&path, &self.cvars, &self.binds)?;
        }
        Ok(vec![format!("wrote {}", path.display())])
    }
}
