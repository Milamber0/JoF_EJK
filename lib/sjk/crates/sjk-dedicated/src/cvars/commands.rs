//! The console commands `Cvar_Init` registers, and `Cvar_Command` for a variable named
//! on its own (`cvar.cpp:939-1260`).
use super::{
    CVAR_ARCHIVE, CVAR_CHEAT, CVAR_INIT, CVAR_LATCH, CVAR_ROM, CVAR_SERVERINFO, CVAR_SYSTEMINFO,
    CVAR_USER_CREATED, CVAR_USERINFO, Cvar, Cvars, atof, atoi,
};
use sjk_network::LegacyTokens;

/// `^9`, `^7`: the colours `Cvar_Print` and `cvarlist` frame values with.
const GREY: &str = "^9";
const WHITE: &str = "^7";

/// `Cmd_ArgsFrom`: the words from `from` on, one space apart.
fn args_from(words: &[&[u8]], from: usize) -> Vec<u8> {
    words.get(from..).unwrap_or_default().join(&b' ')
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

impl Cvars {
    /// Run one console line if it is a cvar command or names a variable; `false` for a
    /// line that is neither. The commands match without regard to case, as
    /// `Cmd_ExecuteString` matches them; `Cvar_Command` comes after every command.
    pub fn command(&mut self, line: &[u8], print: &mut dyn FnMut(&[u8])) -> bool {
        let words: Vec<&[u8]> = LegacyTokens::new(line).collect();
        let Some(&name) = words.first() else {
            return false;
        };
        let is = |command: &[u8]| name.eq_ignore_ascii_case(command);
        if is(b"print") {
            self.print_command(&words, print);
        } else if is(b"toggle") {
            self.toggle(&words, print);
        } else if is(b"set") || is(b"sets") || is(b"setu") || is(b"seta") {
            self.set_command(&words, print);
        } else if [
            &b"cvarAdd"[..],
            b"cvarSub",
            b"cvarMult",
            b"cvarDiv",
            b"cvarMod",
        ]
        .iter()
        .any(|command| is(command))
        {
            self.math(&words, print);
        } else if is(b"reset") {
            if words.len() != 2 {
                print(b"usage: reset <variable>\n");
            } else {
                self.user_set_reset(words[1], print);
            }
        } else if is(b"cvarlist") {
            self.list(words.get(1).copied(), print);
        } else if is(b"cvar_modified") {
            self.list_modified(print);
        } else if is(b"cvar_usercreated") {
            self.list_user_created(print);
        } else if let Some(index) = self.find(name) {
            self.variable_command(index, &words, print);
        } else {
            return false;
        }
        true
    }

    /// `Cvar_Reset`: back to the reset value, through the protections.
    fn user_set_reset(&mut self, name: &[u8], print: &mut dyn FnMut(&[u8])) {
        self.set2(name, None, 0, false, print);
    }

    /// `Cvar_Command`: the variable printed, toggled by `!`, or set to the rest of the line.
    fn variable_command(&mut self, index: usize, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        if words.len() == 1 {
            print_var(&self.vars[index], print);
            return;
        }
        let name = self.vars[index].name.clone();
        if words[1] == b"!" {
            let value = self.vars[index].value();
            self.user_set_value(&name, if value == 0.0 { 1.0 } else { 0.0 }, print);
            return;
        }
        self.user_set(&name, Some(&args_from(words, 1)), print);
    }

    /// `Cvar_Print_f`.
    fn print_command(&self, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        if words.len() != 2 {
            print(b"usage: print <variable>\n");
            return;
        }
        match self.var(words[1]) {
            Some(var) => print_var(var, print),
            None => print(format!("Cvar {} does not exist.\n", text(words[1])).as_bytes()),
        }
    }

    /// `Cvar_Toggle_f`: 0 and 1, or on through a list of values.
    fn toggle(&mut self, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        match words.len() {
            0 | 1 => print(b"usage: toggle <variable> [value1, value2, ...]\n"),
            2 => {
                let value = self.value(words[1]);
                self.user_set_value(words[1], if value == 0.0 { 1.0 } else { 0.0 }, print);
            }
            3 => print(b"toggle: nothing to toggle to\n"),
            count => {
                let current = self.string(words[1]).to_vec();
                // The last value is not looked at: no match goes to the first anyway.
                let next = (2..count - 1)
                    .find(|&at| words[at] == current.as_slice())
                    .map_or(words[2], |at| words[at + 1]);
                self.user_set(words[1], Some(next), print);
            }
        }
    }

    /// `Cvar_Set_f`, also `seta`, `sets` and `setu`: the fourth letter as typed (so
    /// only lower case) adds the archive, serverinfo or userinfo flag.
    fn set_command(&mut self, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        if words.len() < 2 {
            print(format!("usage: {} <variable> <value>\n", text(words[0])).as_bytes());
            return;
        }
        if words.len() == 2 {
            self.print_command(words, print);
            return;
        }
        let Some(index) = self.user_set(words[1], Some(&args_from(words, 2)), print) else {
            return;
        };
        let flag = match words[0].get(3) {
            Some(b'a') => CVAR_ARCHIVE,
            Some(b'u') => CVAR_USERINFO,
            Some(b's') => CVAR_SERVERINFO,
            _ => return,
        };
        if self.vars[index].flags & flag == 0 {
            self.vars[index].flags |= flag;
            self.modified_flags |= flag;
        }
    }

    /// `Cvar_Math_f`, in the float arithmetic the reference does it in.
    fn math(&mut self, words: &[&[u8]], print: &mut dyn FnMut(&[u8])) {
        if words.len() != 3 {
            print(format!("usage: {} <variable> <value>\n", text(words[0])).as_bytes());
            return;
        }
        let (name, operand) = (words[1], words[2]);
        let current = f64::from(self.value(name));
        let result = match words[0].to_ascii_lowercase().as_slice() {
            b"cvaradd" => (current + atof(operand)) as f32,
            b"cvarsub" => (current - atof(operand)) as f32,
            b"cvarmult" => (current * atof(operand)) as f32,
            b"cvardiv" => {
                let divisor = atof(operand) as f32;
                if divisor == 0.0 {
                    print(b"Cannot divide by zero!\n");
                    return;
                }
                self.value(name) / divisor
            }
            _ => {
                // The reference traps on a zero or overflowing remainder; nothing changes here.
                let Some(remainder) = self.integer(name).checked_rem(atoi(operand)) else {
                    return;
                };
                remainder as f32
            }
        };
        self.user_set_value(name, result, print);
    }

    /// `Cvar_List_f`: the variables whose names `filter` matches (`Com_Filter`), then
    /// the count of all of them.
    fn list(&self, filter: Option<&[u8]>, print: &mut dyn FnMut(&[u8])) {
        for var in self.iter() {
            if filter.is_some_and(|filter| !com_filter(filter, &var.name)) {
                continue;
            }
            let marks = [
                (CVAR_SERVERINFO, b'S'),
                (CVAR_SYSTEMINFO, b's'),
                (CVAR_USERINFO, b'U'),
                (CVAR_ROM, b'R'),
                (CVAR_INIT, b'I'),
                (CVAR_ARCHIVE, b'A'),
                (CVAR_LATCH, b'L'),
                (CVAR_CHEAT, b'C'),
                (CVAR_USER_CREATED, b'?'),
            ];
            for (flag, mark) in marks {
                print(&[if var.flags & flag != 0 { mark } else { b' ' }]);
            }
            print(
                format!(
                    "{WHITE} {} = {GREY}\"{WHITE}{}{GREY}\"{WHITE}",
                    text(&var.name),
                    text(&var.string)
                )
                .as_bytes(),
            );
            if let Some(latched) = &var.latched {
                print(
                    format!(
                        ", latched = {GREY}\"{WHITE}{}{GREY}\"{WHITE}",
                        text(latched)
                    )
                    .as_bytes(),
                );
            }
            print(b"\n");
        }
        print(format!("\n{} total cvars\n", self.vars.len()).as_bytes());
    }

    /// `Cvar_ListModified_f`: every variable whose value (a latched one if waiting)
    /// is not its default.
    fn list_modified(&self, print: &mut dyn FnMut(&[u8])) {
        for var in self.iter() {
            let value = var.latched.as_ref().unwrap_or(&var.string);
            if var.modification_count == 0 || *value == var.reset {
                continue;
            }
            print(
                format!(
                    "{GREY}Cvar {WHITE}{} = {GREY}\"{WHITE}{}{GREY}\"{WHITE}, {WHITE}default = {GREY}\"{WHITE}{}{GREY}\"{WHITE}\n",
                    text(&var.name),
                    text(value),
                    text(&var.reset)
                )
                .as_bytes(),
            );
        }
    }
}

impl Cvars {
    /// `Cvar_ListUserCreated_f`: every variable a `set` made, and how many.
    fn list_user_created(&self, print: &mut dyn FnMut(&[u8])) {
        let mut count = 0;
        for var in self.iter().filter(|var| var.flags & CVAR_USER_CREATED != 0) {
            let value = var.latched.as_ref().unwrap_or(&var.string);
            print(
                format!(
                    "{GREY}Cvar {WHITE}{} = {GREY}\"{WHITE}{}{GREY}\"{WHITE}\n",
                    text(&var.name),
                    text(value)
                )
                .as_bytes(),
            );
            count += 1;
        }
        if count > 0 {
            print(
                format!("{GREY}Showing {WHITE}{count}{GREY} user created cvars{WHITE}\n")
                    .as_bytes(),
            );
        } else {
            print(format!("{GREY}No user created cvars{WHITE}\n").as_bytes());
        }
    }
}

/// `Cvar_Print`.
fn print_var(var: &Cvar, print: &mut dyn FnMut(&[u8])) {
    print(
        format!(
            "{GREY}Cvar {WHITE}{} = {GREY}\"{WHITE}{}{GREY}\"{WHITE}",
            text(&var.name),
            text(&var.string)
        )
        .as_bytes(),
    );
    if var.flags & CVAR_ROM == 0 {
        if var.string.eq_ignore_ascii_case(&var.reset) {
            print(format!(", {WHITE}the default").as_bytes());
        } else {
            print(
                format!(
                    ", {WHITE}default = {GREY}\"{WHITE}{}{GREY}\"{WHITE}",
                    text(&var.reset)
                )
                .as_bytes(),
            );
        }
    }
    print(b"\n");
    if let Some(latched) = &var.latched {
        print(format!("     latched = {GREY}\"{WHITE}{}{GREY}\"\n", text(latched)).as_bytes());
    }
    if let Some(description) = &var.description {
        print(&[description.as_slice(), b"\n"].concat());
    }
}

/// `Com_Filter`, without regard to case: `*` any run, `?` any one byte, `[a-z]` a set.
/// The name need only *begin* with a match, as the reference's loop ends with the filter.
pub(super) fn com_filter(filter: &[u8], name: &[u8]) -> bool {
    let (mut f, mut n) = (0, 0);
    let at = |text: &[u8], index: usize| text.get(index).copied().unwrap_or(0);
    let upper = |byte: u8| byte.to_ascii_uppercase();
    while f < filter.len() {
        match filter[f] {
            b'*' => {
                f += 1;
                let start = f;
                while f < filter.len() && !matches!(filter[f], b'*' | b'?') {
                    f += 1;
                }
                let piece = &filter[start..f];
                if !piece.is_empty() {
                    let rest = name.get(n..).unwrap_or_default();
                    let Some(found) = rest
                        .windows(piece.len())
                        .position(|window| window.eq_ignore_ascii_case(piece))
                    else {
                        return false;
                    };
                    n += found + piece.len();
                }
            }
            b'?' => {
                f += 1;
                n += 1;
            }
            b'[' if at(filter, f + 1) == b'[' => f += 1,
            b'[' => {
                f += 1;
                let mut found = false;
                while f < filter.len() && !found {
                    if filter[f] == b']' && at(filter, f + 1) != b']' {
                        break;
                    }
                    if at(filter, f + 1) == b'-'
                        && at(filter, f + 2) != 0
                        && (at(filter, f + 2) != b']' || at(filter, f + 3) == b']')
                    {
                        let byte = upper(at(name, n));
                        if byte >= upper(filter[f]) && byte <= upper(filter[f + 2]) {
                            found = true;
                        }
                        f += 3;
                    } else {
                        if upper(filter[f]) == upper(at(name, n)) {
                            found = true;
                        }
                        f += 1;
                    }
                }
                if !found {
                    return false;
                }
                while f < filter.len() {
                    if filter[f] == b']' && at(filter, f + 1) != b']' {
                        break;
                    }
                    f += 1;
                }
                f += 1;
                n += 1;
            }
            byte => {
                if upper(byte) != upper(at(name, n)) {
                    return false;
                }
                f += 1;
                n += 1;
            }
        }
    }
    true
}
