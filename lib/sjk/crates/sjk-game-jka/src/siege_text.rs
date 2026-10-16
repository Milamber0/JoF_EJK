//! The text format of siege's files (OpenJK `codemp/game/bg_saga.c:176-575`): a map's
//! `.siege` file, the class files (`.scl`) and the team files (`.team`) are all read by
//! the same two routines, [`value_group`] and [`paired_value`], whose quirks decide what
//! a file means — a key must start a line, a line that starts with a tab is not a key
//! outside a group (a group's text has its tabs made spaces first), a quoted value
//! stops at a `//` inside the quotes, a key that is the head of a group is skipped with
//! the whole group. They are ported statement for statement over bytes, the end of the
//! slice standing in for the C string's terminator; what the reference answers with
//! `Com_Error(ERR_DROP, …)` is a [`TextError`].
//!
//! [`generic_table`] and [`force_powers`] turn a value into the numbers a class keeps
//! (`BG_SiegeTranslateGenericTable`, `BG_SiegeTranslateForcePowers`).

/// `SIEGECHAR_TAB`.
const TAB: u8 = 9;

/// Why a siege file could not be read: the reference drops to the console with this
/// error (`Com_Error(ERR_DROP, …)`); a server keeps running without the file instead.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TextError {
    /// The text ended while a group or a value was being read.
    UnexpectedEnd(String),
    /// A closing bracket without its opening one, or the reverse.
    Brackets(String),
    /// A comment where a value was expected.
    CommentForValue(String),
}

/// The byte at `index`, or the terminator past the end.
fn at(buf: &[u8], index: usize) -> u8 {
    buf.get(index).copied().unwrap_or(0)
}

/// `BG_SiegeStripTabs`: every tab becomes a space.
pub fn strip_tabs(buf: &mut [u8]) {
    for byte in buf.iter_mut() {
        if *byte == TAB {
            *byte = b' ';
        }
    }
}

/// Skips a group whose opening bracket is at or after `i`, stopping on its closing
/// bracket (the shared tail of `BG_SiegeGetValueGroup`'s two skips).
fn skip_group(buf: &[u8], mut i: usize, group: &[u8]) -> Result<usize, TextError> {
    let mut depth = 0i32;
    while at(buf, i) != 0 && (at(buf, i) != b'}' || depth != 0) {
        if at(buf, i) == b'{' {
            depth += 1;
        } else if at(buf, i) == b'}' {
            depth -= 1;
        }
        if depth < 0 {
            return Err(TextError::Brackets(
                String::from_utf8_lossy(group).into_owned(),
            ));
        }
        if at(buf, i) == b'}' && depth == 0 {
            break;
        }
        i += 1;
    }
    if at(buf, i) != b'}' {
        return Err(TextError::Brackets(
            String::from_utf8_lossy(group).into_owned(),
        ));
    }
    Ok(i)
}

/// `BG_SiegeGetValueGroup`: the text inside the group called `group` (case ignored),
/// its own brackets left out and its tabs made spaces; `None` when the text has no such
/// group at its top level.
pub fn value_group(buf: &[u8], group: &[u8]) -> Result<Option<Vec<u8>>, TextError> {
    let name = || String::from_utf8_lossy(group).into_owned();
    let mut i = 0usize;
    while at(buf, i) != 0 {
        let c = at(buf, i);
        if c != b' ' && c != b'{' && c != b'}' && c != b'\n' && c != b'\r' && c != TAB {
            if c == b'/' && at(buf, i + 1) == b'/' {
                // A comment: skipped to its end.
                while at(buf, i) != 0
                    && at(buf, i) != b'\n'
                    && at(buf, i) != b'\r'
                    && at(buf, i) != TAB
                {
                    i += 1;
                }
            } else {
                // The word, up to the next space, line end or bracket.
                let start = i;
                while at(buf, i) != b' '
                    && at(buf, i) != b'\n'
                    && at(buf, i) != b'\r'
                    && at(buf, i) != TAB
                    && at(buf, i) != b'{'
                    && at(buf, i) != 0
                {
                    if at(buf, i) == b'/' && at(buf, i + 1) == b'/' {
                        break;
                    }
                    i += 1;
                }
                let word = &buf[start..i];
                if at(buf, i) == b'/' && at(buf, i + 1) == b'/' {
                    while at(buf, i) != 0 && at(buf, i) != b'\n' && at(buf, i) != b'\r' {
                        i += 1;
                    }
                    while at(buf, i) == b'\n' || at(buf, i) == b'\r' {
                        i += 1;
                    }
                }
                if at(buf, i) == 0 {
                    return Err(TextError::UnexpectedEnd(name()));
                }
                while at(buf, i) != 0 && matches!(at(buf, i), b' ' | TAB | b'\n' | b'\r') {
                    i += 1;
                }
                let is_group = at(buf, i) == b'{';
                if is_group && word.eq_ignore_ascii_case(group) {
                    while at(buf, i) != b'{' && at(buf, i) != 0 {
                        i += 1;
                    }
                    if at(buf, i) == 0 {
                        return Err(TextError::UnexpectedEnd(name()));
                    }
                    // Everything to the matching bracket, the group's own two left out.
                    let mut out = Vec::new();
                    let mut depth = 0i32;
                    while (at(buf, i) != b'}' || depth != 0) && at(buf, i) != 0 {
                        let c = at(buf, i);
                        if c == b'{' {
                            depth += 1;
                        } else if c == b'}' {
                            depth -= 1;
                        }
                        if depth < 0 {
                            return Err(TextError::Brackets(name()));
                        }
                        if (c != b'{' || depth > 1) && (c != b'}' || depth > 0) {
                            out.push(c);
                        }
                        if c == b'}' && depth == 0 {
                            break;
                        }
                        i += 1;
                    }
                    if at(buf, i) != b'}' {
                        return Err(TextError::Brackets(name()));
                    }
                    strip_tabs(&mut out);
                    return Ok(Some(out));
                } else if !is_group {
                    // A value, not a group: to the end of its line.
                    while at(buf, i) != 0 && at(buf, i) != b'\n' && at(buf, i) != b'\r' {
                        i += 1;
                    }
                } else {
                    // Another group: past it.
                    i = skip_group(buf, i, group)?;
                    i += 1;
                }
            }
        } else if c == b'{' {
            // A group nobody named: past it.
            i = skip_group(buf, i, group)?;
        }
        if at(buf, i) == 0 {
            break;
        }
        i += 1;
    }
    Ok(None)
}

/// `BG_SiegeGetPairedValue`: the value written after `key` (case ignored) at the text's
/// own level — a quoted one up to its closing quote — or `None`. Keys inside groups are
/// not seen.
pub fn paired_value(buf: &[u8], key: &[u8]) -> Result<Option<Vec<u8>>, TextError> {
    let name = || String::from_utf8_lossy(key).into_owned();
    let mut i = 0usize;
    while at(buf, i) != 0 {
        let c = at(buf, i);
        if c != b' ' && c != b'{' && c != b'}' && c != b'\n' && c != b'\r' {
            if c == b'/' && at(buf, i + 1) == b'/' {
                while at(buf, i) != 0 && at(buf, i) != b'\n' && at(buf, i) != b'\r' {
                    i += 1;
                }
            } else {
                let start = i;
                while at(buf, i) != b' '
                    && at(buf, i) != b'\n'
                    && at(buf, i) != b'\r'
                    && at(buf, i) != TAB
                    && at(buf, i) != 0
                {
                    if at(buf, i) == b'/' && at(buf, i + 1) == b'/' {
                        break;
                    }
                    i += 1;
                }
                let word = &buf[start..i];
                let mut k = i;
                while at(buf, k) != 0 && matches!(at(buf, k), b' ' | b'\n' | b'\r') {
                    k += 1;
                }
                if at(buf, k) == b'{' {
                    // The head of a group: the whole group is skipped.
                    let mut depth = 0i32;
                    while at(buf, i) != 0 && (at(buf, i) != b'}' || depth != 0) {
                        if at(buf, i) == b'{' {
                            depth += 1;
                        } else if at(buf, i) == b'}' {
                            depth -= 1;
                        }
                        if depth < 0 {
                            return Err(TextError::Brackets(
                                String::from_utf8_lossy(word).into_owned(),
                            ));
                        }
                        if at(buf, i) == b'}' && depth == 0 {
                            break;
                        }
                        i += 1;
                    }
                    if at(buf, i) == b'}' {
                        i += 1;
                    }
                } else if at(buf, i) != b'/' || at(buf, i + 1) != b'/' {
                    if word.eq_ignore_ascii_case(key) {
                        while at(buf, i) != 0 && matches!(at(buf, i), b' ' | b'\n' | b'\r' | TAB) {
                            i += 1;
                        }
                        if at(buf, i) == 0 {
                            return Err(TextError::UnexpectedEnd(name()));
                        }
                        let quoted = at(buf, i) == b'"';
                        if quoted {
                            i += 1;
                        }
                        let mut out = Vec::new();
                        while (!quoted
                            && at(buf, i) != b' '
                            && at(buf, i) != b'\n'
                            && at(buf, i) != b'\r')
                            || (quoted && at(buf, i) != b'"')
                        {
                            if at(buf, i) == b'/' && at(buf, i + 1) == b'/' {
                                break;
                            }
                            out.push(at(buf, i));
                            i += 1;
                            if at(buf, i) == 0 {
                                return Err(TextError::UnexpectedEnd(name()));
                            }
                        }
                        return Ok(Some(out));
                    }
                    // Not the key: the rest of the line, so a value is never taken for a key.
                    while at(buf, i) != 0 && at(buf, i) != b'\n' {
                        i += 1;
                    }
                } else {
                    return Err(TextError::CommentForValue(name()));
                }
            }
        }
        if at(buf, i) == 0 {
            break;
        }
        i += 1;
    }
    Ok(None)
}

/// [`paired_value`] as text, the way every caller uses it.
pub fn paired(buf: &[u8], key: &str) -> Result<Option<String>, TextError> {
    Ok(
        paired_value(buf, key.as_bytes())?
            .map(|value| String::from_utf8_lossy(&value).into_owned()),
    )
}

/// `BG_SiegeTranslateGenericTable`: names separated by `|` or spaces, looked up in
/// `table` (case ignored). With `bitflag` each found name sets bit `1 << id` and the
/// bits are returned; without it the first found name's id is. `"0"` alone is nothing.
pub fn generic_table(buf: &[u8], table: &[(&str, i32)], bitflag: bool) -> i32 {
    if buf == b"0" {
        return 0;
    }
    let mut items = 0i32;
    let mut i = 0usize;
    while at(buf, i) != 0 {
        if at(buf, i) != b' ' && at(buf, i) != b'|' {
            let start = i;
            while at(buf, i) != 0 && at(buf, i) != b' ' && at(buf, i) != b'|' {
                i += 1;
            }
            let item = &buf[start..i];
            if !item.is_empty()
                && let Some((_, id)) = table
                    .iter()
                    .find(|(name, _)| name.as_bytes().eq_ignore_ascii_case(item))
            {
                if !bitflag {
                    return *id;
                }
                items |= 1i32.wrapping_shl(*id as u32);
            }
        }
        if at(buf, i) == 0 {
            break;
        }
        i += 1;
    }
    items
}

/// `NUM_FORCE_POWERS` in multiplayer.
pub const FORCE_POWER_COUNT: usize = 18;

/// `FPTable`, in `forcePowers_t` order.
pub const FORCE_POWER_NAMES: [&str; FORCE_POWER_COUNT] = [
    "FP_HEAL",
    "FP_LEVITATION",
    "FP_SPEED",
    "FP_PUSH",
    "FP_PULL",
    "FP_TELEPATHY",
    "FP_GRIP",
    "FP_LIGHTNING",
    "FP_RAGE",
    "FP_PROTECT",
    "FP_ABSORB",
    "FP_TEAM_HEAL",
    "FP_TEAM_FORCE",
    "FP_DRAIN",
    "FP_SEE",
    "FP_SABER_OFFENSE",
    "FP_SABER_DEFENSE",
    "FP_SABERTHROW",
];

/// `BG_SiegeTranslateForcePowers`: `FP_NAME,rank` entries separated by `|` or spaces —
/// a missing rank is 3, ranks are held to 0..=5, `FP_JUMP` is levitation's other name;
/// `FP_ALL` is every power at 3 and `0` is none.
pub fn force_powers(buf: &[u8]) -> [i32; FORCE_POWER_COUNT] {
    let all = buf.eq_ignore_ascii_case(b"FP_ALL");
    let mut levels = [if all { 3 } else { 0 }; FORCE_POWER_COUNT];
    if all || buf == b"0" {
        return levels;
    }
    let mut i = 0usize;
    while at(buf, i) != 0 {
        if at(buf, i) != b' ' && at(buf, i) != b'|' {
            let start = i;
            while at(buf, i) != 0 && at(buf, i) != b' ' && at(buf, i) != b'|' && at(buf, i) != b','
            {
                i += 1;
            }
            let power = &buf[start..i];
            let level = if at(buf, i) == b',' {
                i += 1;
                let from = i;
                while at(buf, i) != 0 && at(buf, i) != b' ' && at(buf, i) != b'|' {
                    i += 1;
                }
                crate::userinfo::atoi(&buf[from..i]).clamp(0, 5)
            } else {
                3
            };
            if !power.is_empty() {
                let power: &[u8] = if power.eq_ignore_ascii_case(b"FP_JUMP") {
                    b"FP_LEVITATION"
                } else {
                    power
                };
                if let Some(index) = FORCE_POWER_NAMES
                    .iter()
                    .position(|name| name.as_bytes().eq_ignore_ascii_case(power))
                {
                    levels[index] = level;
                }
            }
        }
        if at(buf, i) == 0 {
            break;
        }
        i += 1;
    }
    levels
}
