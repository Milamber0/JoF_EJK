//! The server's console variables, as `cvar.cpp` keeps them: every setting an operator
//! reads or changes by name — from the console, `rcon` or a config file — with the
//! reference's flags, latching, range checks, protections and messages.
//!
//! The table only holds text and the rules for changing it. What a value *does* is the
//! server's: it reads the variables it cares about after every console line and frame
//! (as `G_UpdateCvars` and `SV_CheckCvars` do) through [`Cvars::changed_since`].
//!
//! Output is handed over one `Com_Printf` message at a time, because the remote console
//! packs messages into datagrams.

mod commands;
mod numbers;
mod registry;
use numbers::{atof, atoi, c_int, format_value, is_a_number};

/// `CVAR_ARCHIVE`: saved to the configuration file.
pub const CVAR_ARCHIVE: u32 = 0x1;
/// `CVAR_USERINFO`.
pub const CVAR_USERINFO: u32 = 0x2;
/// `CVAR_SERVERINFO`: part of the server-info string browsers read.
pub const CVAR_SERVERINFO: u32 = 0x4;
/// `CVAR_SYSTEMINFO`: duplicated on every client.
pub const CVAR_SYSTEMINFO: u32 = 0x8;
/// `CVAR_INIT`: set on the command line only.
pub const CVAR_INIT: u32 = 0x10;
/// `CVAR_LATCH`: a change waits for the next registration (the next map).
pub const CVAR_LATCH: u32 = 0x20;
/// `CVAR_ROM`: set by the server itself only.
pub const CVAR_ROM: u32 = 0x40;
/// `CVAR_USER_CREATED`: made by a `set`.
pub const CVAR_USER_CREATED: u32 = 0x80;
/// `CVAR_TEMP`: never archived.
pub const CVAR_TEMP: u32 = 0x100;
/// `CVAR_CHEAT`: changed only while `sv_cheats` is on.
pub const CVAR_CHEAT: u32 = 0x200;
/// `CVAR_NORESTART`.
pub const CVAR_NORESTART: u32 = 0x400;
/// `CVAR_INTERNAL`: never shown in an info string.
pub const CVAR_INTERNAL: u32 = 0x800;
/// `CVAR_SERVER_CREATED`.
pub const CVAR_SERVER_CREATED: u32 = 0x2000;
/// `CVAR_VM_CREATED`.
pub const CVAR_VM_CREATED: u32 = 0x4000;
/// `CVAR_PROTECTED`.
pub const CVAR_PROTECTED: u32 = 0x8000;
/// `CVAR_NODEFAULT`: not archived while it holds its default.
pub const CVAR_NODEFAULT: u32 = 0x10000;

/// One variable.
#[derive(Clone, Debug)]
pub struct Cvar {
    pub name: Vec<u8>,
    pub string: Vec<u8>,
    /// `resetString`: the value `reset` returns to.
    pub reset: Vec<u8>,
    /// A change waiting for the next registration (`CVAR_LATCH`).
    pub latched: Option<Vec<u8>>,
    pub flags: u32,
    pub description: Option<Vec<u8>>,
    /// `Cvar_CheckRange`: minimum, maximum, whether it must be whole.
    pub range: Option<(f32, f32, bool)>,
    /// `modificationCount`: bumped by every change, latched ones included.
    pub modification_count: i32,
    /// The game announces every change to everyone (`trackChange`).
    pub announced: bool,
}

impl Cvar {
    /// `value`: the string read as `atof` reads it, stored as a float.
    pub fn value(&self) -> f32 {
        atof(&self.string) as f32
    }
    /// `integer`: the string read as `atoi` reads it.
    pub fn integer(&self) -> i32 {
        atoi(&self.string)
    }
}

/// The table, in the order the reference lists it: newest first.
#[derive(Clone, Debug, Default)]
pub struct Cvars {
    /// In creation order; listed in reverse.
    vars: Vec<Cvar>,
    /// `cvar_modifiedFlags`: the flags of every variable changed since the server last
    /// cleared them.
    pub modified_flags: u32,
    /// `cvar_sort`: a variable was created since the table was last sorted.
    sort_pending: bool,
}

/// `Cvar_ValidateString`.
fn valid_name(name: &[u8]) -> bool {
    !name.iter().any(|byte| matches!(byte, b'\\' | b'"' | b';'))
}

impl Cvars {
    /// `Cvar_Init`'s own variable.
    pub fn new() -> Self {
        let mut cvars = Self::default();
        cvars.get(
            b"sv_cheats",
            b"1",
            CVAR_ROM | CVAR_SYSTEMINFO,
            Some(b"Allow cheats on server if set to 1"),
        );
        cvars
    }

    /// A dedicated server's table: the engine's registrations (`SV_Init`), then the
    /// game's (`G_RegisterCvars` through `Cvar_Register`, which drops `CVAR_ROM` from an
    /// archived variable and marks every one the game's), in the reference's order.
    pub fn server() -> Self {
        let mut cvars = Self::new();
        cvars.register_server();
        cvars
    }

    /// The registrations of [`Self::server`] over a table the command line and the
    /// startup configs already wrote to, as the reference registers after them: their
    /// values stay, except that a read-only one is forced back to the code's value.
    pub fn register_server(&mut self) {
        let cvars = self;
        // `Com_Init`'s `dedicated` (`common.cpp:1224`): 1 for a LAN server, 2 for one that
        // announces itself to the master servers. The reference's dedicated server starts
        // at 2; this one starts at 1, so that no server lists itself publicly unless its
        // operator asks (`--set dedicated 2`).
        cvars.get(b"dedicated", b"1", CVAR_INIT, None);
        cvars.check_range(b"dedicated", 1.0, 2.0, true, &mut |_| {});
        for &(name, value, flags, description, range) in registry::ENGINE {
            cvars.get(name, value, flags, description);
            if let Some((min, max, integral)) = range {
                cvars.check_range(name, min, max, integral, &mut |_| {});
            }
        }
        for &(name, value, mut flags, announced) in registry::GAME {
            if flags & (CVAR_ARCHIVE | CVAR_ROM) == CVAR_ARCHIVE | CVAR_ROM {
                flags &= !CVAR_ROM;
            }
            let index = cvars.get(name, value, flags | CVAR_VM_CREATED, None);
            cvars.vars[index].announced = announced;
        }
    }

    fn find(&self, name: &[u8]) -> Option<usize> {
        self.vars
            .iter()
            .position(|var| var.name.eq_ignore_ascii_case(name))
    }

    /// A variable by name, any case.
    pub fn var(&self, name: &[u8]) -> Option<&Cvar> {
        self.find(name).map(|index| &self.vars[index])
    }

    /// Every variable, newest first, as `cvarlist` lists them.
    pub fn iter(&self) -> impl Iterator<Item = &Cvar> {
        self.vars.iter().rev()
    }

    /// `Cvar_VariableString`: empty for a variable that does not exist.
    pub fn string(&self, name: &[u8]) -> &[u8] {
        self.var(name).map_or(&[], |var| &var.string)
    }
    /// `Cvar_VariableValue`.
    pub fn value(&self, name: &[u8]) -> f32 {
        self.var(name).map_or(0.0, Cvar::value)
    }
    /// `Cvar_VariableIntegerValue`.
    pub fn integer(&self, name: &[u8]) -> i32 {
        self.var(name).map_or(0, Cvar::integer)
    }

    /// `Cvar_Get`: register a variable, or register it again. Again, it takes the new
    /// flags and description, a user's variable takes the code's default as its reset
    /// value, and a latched value takes effect.
    pub fn get(
        &mut self,
        name: &[u8],
        value: &[u8],
        flags: u32,
        description: Option<&[u8]>,
    ) -> usize {
        self.get_printing(name, value, flags, description, &mut |_| {})
    }

    /// [`Self::get`], with what the latched value's range check prints.
    pub fn get_printing(
        &mut self,
        name: &[u8],
        value: &[u8],
        mut flags: u32,
        description: Option<&[u8]>,
        print: &mut dyn FnMut(&[u8]),
    ) -> usize {
        let name: &[u8] = if valid_name(name) { name } else { b"BADNAME" };
        let Some(index) = self.find(name) else {
            self.vars.push(Cvar {
                name: name.to_vec(),
                string: value.to_vec(),
                reset: value.to_vec(),
                latched: None,
                flags,
                description: description
                    .filter(|text| !text.is_empty())
                    .map(<[u8]>::to_vec),
                range: None,
                modification_count: 1,
                announced: false,
            });
            self.modified_flags |= flags;
            self.sort_pending = true;
            return self.vars.len() - 1;
        };
        let value = self.validate(index, value, &mut |_| {});
        let var = &mut self.vars[index];
        if var.flags & CVAR_VM_CREATED != 0 {
            if flags & CVAR_VM_CREATED == 0 {
                var.flags &= !CVAR_VM_CREATED;
            }
        } else if var.flags & CVAR_USER_CREATED == 0 {
            flags &= !CVAR_VM_CREATED;
        }
        if var.flags & CVAR_USER_CREATED != 0 {
            var.flags &= !CVAR_USER_CREATED;
            var.reset = value.clone();
            if flags & CVAR_ROM != 0 {
                var.latched = Some(value.clone());
            }
        }
        if var.flags & CVAR_SERVER_CREATED != 0 {
            if flags & CVAR_SERVER_CREATED == 0 {
                var.flags &= !CVAR_SERVER_CREATED;
            }
        } else {
            flags &= !CVAR_SERVER_CREATED;
        }
        var.flags |= flags;
        if var.reset.is_empty() {
            var.reset = value;
        }
        if let Some(latched) = var.latched.take() {
            self.set2(name, Some(&latched), 0, true, print);
        }
        let var = &mut self.vars[index];
        if let Some(description) = description.filter(|text| !text.is_empty()) {
            var.description = Some(description.to_vec());
        }
        self.modified_flags |= flags;
        index
    }

    /// `Cvar_CheckRange`: the variable is kept within `min`..`max` (whole if
    /// `integral`) from now on, starting with its current value.
    pub fn check_range(
        &mut self,
        name: &[u8],
        min: f32,
        max: f32,
        integral: bool,
        print: &mut dyn FnMut(&[u8]),
    ) {
        let Some(index) = self.find(name) else { return };
        self.vars[index].range = Some((min, max, integral));
        let (name, value) = (
            self.vars[index].name.clone(),
            self.vars[index].string.clone(),
        );
        self.set2(&name, Some(&value), 0, true, print);
    }

    /// `Cvar_Validate`: `value` as the variable's range allows it, warning through `print`.
    fn validate(&self, index: usize, value: &[u8], print: &mut dyn FnMut(&[u8])) -> Vec<u8> {
        let var = &self.vars[index];
        let Some((min, max, integral)) = var.range else {
            return value.to_vec();
        };
        let name = String::from_utf8_lossy(&var.name).into_owned();
        let mut changed = false;
        let mut valuef;
        if is_a_number(value) {
            valuef = atof(value) as f32;
            if integral && c_int(valuef) as f32 != valuef {
                print(format!("WARNING: cvar '{name}' must be integral").as_bytes());
                valuef = c_int(valuef) as f32;
                changed = true;
            }
        } else {
            print(format!("WARNING: cvar '{name}' must be numeric").as_bytes());
            valuef = atof(&var.reset) as f32;
            changed = true;
        }
        let bound = |bound: f32| {
            if c_int(bound) as f32 == bound {
                format!("{}", c_int(bound))
            } else {
                format!("{:.6}", f64::from(bound))
            }
        };
        if valuef < min || valuef > max {
            let (word, limit) = if valuef < min {
                ("min", min)
            } else {
                ("max", max)
            };
            print(
                if changed {
                    b" and is".to_vec()
                } else {
                    format!("WARNING: cvar '{name}'").into_bytes()
                }
                .as_slice(),
            );
            print(format!(" out of range ({word} {})", bound(limit)).as_bytes());
            valuef = limit;
            changed = true;
        }
        if !changed {
            return value.to_vec();
        }
        let text = if c_int(valuef) as f32 == valuef {
            format!("{}", c_int(valuef))
        } else {
            format!("{:.6}", f64::from(valuef))
        };
        print(format!(", setting to {text}\n").as_bytes());
        text.into_bytes()
    }

    /// `Cvar_Set2`: `value` (the reset value for `None`) into the variable, creating it
    /// with `default_flags` if it does not exist. Unless `force`, the variable's
    /// protections apply: read-only, write-protected, latched, cheat-protected. Returns
    /// the variable's index, `None` where there is none.
    pub fn set2(
        &mut self,
        name: &[u8],
        value: Option<&[u8]>,
        default_flags: u32,
        force: bool,
        print: &mut dyn FnMut(&[u8]),
    ) -> Option<usize> {
        let name: &[u8] = if valid_name(name) {
            name
        } else {
            print(&[&b"invalid cvar name string: "[..], name, b"\n"].concat());
            b"BADNAME"
        };
        let Some(index) = self.find(name) else {
            return value.map(|value| self.get(name, value, default_flags, None));
        };
        let value = value.map_or_else(|| self.vars[index].reset.clone(), <[u8]>::to_vec);
        let value = self.validate(index, &value, print);
        let cheats = self.integer(b"sv_cheats") != 0;
        let var = &mut self.vars[index];
        if var.flags & CVAR_LATCH != 0 && var.latched.is_some() {
            if value == var.string {
                var.latched = None;
                return Some(index);
            }
            if Some(&value) == var.latched.as_ref() {
                return Some(index);
            }
        } else if value == var.string {
            return Some(index);
        }
        self.modified_flags |= var.flags;
        let shown = String::from_utf8_lossy(name).into_owned();
        if !force {
            if var.flags & CVAR_ROM != 0 {
                print(format!("{shown} is read only.\n").as_bytes());
                return Some(index);
            }
            if var.flags & CVAR_INIT != 0 {
                print(format!("{shown} is write protected.\n").as_bytes());
                return Some(index);
            }
            if var.flags & CVAR_LATCH != 0 {
                match &var.latched {
                    Some(latched) if *latched == value => return Some(index),
                    None if value == var.string => return Some(index),
                    _ => {}
                }
                print(format!("{shown} will be changed upon restarting.\n").as_bytes());
                var.latched = Some(value);
                var.modification_count += 1;
                return Some(index);
            }
            if var.flags & CVAR_CHEAT != 0 && !cheats {
                print(format!("{shown} is cheat protected.\n").as_bytes());
                return Some(index);
            }
        } else {
            var.latched = None;
        }
        if value == var.string {
            return Some(index);
        }
        var.modification_count += 1;
        var.string = value;
        Some(index)
    }

    /// `Cvar_Set`: the code's own change, past every protection.
    pub fn set(&mut self, name: &[u8], value: &[u8]) {
        self.set2(name, Some(value), 0, true, &mut |_| {});
    }

    /// `Cvar_User_Set`: an operator's change, which respects the protections and makes
    /// a new variable a user's.
    pub fn user_set(
        &mut self,
        name: &[u8],
        value: Option<&[u8]>,
        print: &mut dyn FnMut(&[u8]),
    ) -> Option<usize> {
        self.set2(name, value, CVAR_USER_CREATED, false, print)
    }

    /// `Cvar_User_SetValue`: a number written as the reference writes a float.
    fn user_set_value(&mut self, name: &[u8], value: f32, print: &mut dyn FnMut(&[u8])) {
        self.user_set(name, Some(format_value(value).as_bytes()), print);
    }

    /// `Cvar_WriteVariables`: a `seta` line for every archived variable (its latched
    /// value if one waits), except a `CVAR_NODEFAULT` one at its default. Sorts the table
    /// by name first if a variable was created since the last time — which is also the
    /// order `cvarlist` shows from then on.
    pub fn archive_lines(&mut self, print: &mut dyn FnMut(&[u8])) -> Vec<u8> {
        if self.sort_pending {
            self.sort_pending = false;
            // Listed ascending by `strcmp`; stored newest-last, so descending.
            self.vars.sort_by(|a, b| b.name.cmp(&a.name));
        }
        let mut out = Vec::new();
        for var in self.iter() {
            if var.name.eq_ignore_ascii_case(b"cl_cdkey") || var.flags & CVAR_ARCHIVE == 0 {
                continue;
            }
            let value = var.latched.as_ref().unwrap_or(&var.string);
            if var.name.len() + value.len() + 10 > 1024 {
                print(
                    format!(
                        "^3WARNING: value of variable \"{}\" too long to write to file\n",
                        String::from_utf8_lossy(&var.name)
                    )
                    .as_bytes(),
                );
                continue;
            }
            if var.flags & CVAR_NODEFAULT != 0 && *value == var.reset {
                continue;
            }
            out.extend_from_slice(&[&b"seta "[..], &var.name, b" \"", value, b"\"\n"].concat());
        }
        out
    }

    /// `Cvar_InfoString`: every variable with `bit` (and not `CVAR_INTERNAL`) as an info
    /// string, oldest last, as `Info_SetValueForKey` builds it.
    pub fn info_string(&self, bit: u32) -> Vec<u8> {
        self.iter()
            .filter(|var| var.flags & CVAR_INTERNAL == 0 && var.flags & bit != 0)
            .fold(Vec::new(), |info, var| {
                sjk_protocol::info_set_value(&info, &var.name, &var.string)
            })
    }

    /// The variables whose `modificationCount` differs from `seen`'s record, which is
    /// brought up to date: how the server learns what a console line changed.
    pub fn changed_since(&self, seen: &mut Vec<(Vec<u8>, i32)>) -> Vec<Vec<u8>> {
        let mut changed = Vec::new();
        for var in &self.vars {
            match seen.iter_mut().find(|(name, _)| *name == var.name) {
                Some((_, count)) if *count == var.modification_count => {}
                Some((_, count)) => {
                    *count = var.modification_count;
                    changed.push(var.name.clone());
                }
                None => {
                    seen.push((var.name.clone(), var.modification_count));
                    changed.push(var.name.clone());
                }
            }
        }
        changed
    }
}
