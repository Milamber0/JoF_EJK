//! TaystJK cl_main.cpp:2366-2374,2428-2450,3000-3243,3444-3451.
use super::super::*;
use std::sync::atomic::AtomicU64;

/// Timestamp of the most recent name edit for TaystJK's five-second guard.
pub(crate) struct NameClock {
    epoch: Instant,
    changed: Arc<AtomicU64>,
}

impl Default for NameClock {
    fn default() -> Self {
        Self {
            epoch: Instant::now(),
            changed: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub(super) fn register(
    cvars: &mut CvarRegistry,
    clock: &NameClock,
) -> Result<(), sjk_shell::CvarError> {
    cvars.register(CvarDefinition::new(
        "cl_afkPrefix",
        "[AFK]",
        CvarFlags::ARCHIVE,
        "Prefix toggled by afk",
    ))?;
    cvars.register(CvarDefinition::new(
        "cl_colorString",
        0_i64,
        CvarFlags::ARCHIVE,
        "Selected outgoing-chat color bits",
    ))?;
    cvars.register(CvarDefinition::new(
        "cl_colorStringRandom",
        2_i64,
        CvarFlags::ARCHIVE,
        "Color-change randomness; higher changes less often",
    ))?;
    let changed = Arc::clone(&clock.changed);
    let epoch = clock.epoch;
    cvars.on_change("name", move |_| {
        changed.store(epoch.elapsed().as_millis() as u64, Ordering::Relaxed);
    })
}

pub(super) fn selected_mask(args: &[String], current: u16, toggle: bool) -> Result<u16, String> {
    if args.len() > 10 {
        return Err("More than 10 colors supplied".into());
    }
    if args.is_empty() {
        return Ok(current);
    }
    if args == ["-1"] {
        return Ok(0);
    }
    if args == ["10"] {
        return Ok(1023);
    }
    let mut bits = 0;
    for arg in args {
        let index = arg
            .parse::<u8>()
            .ok()
            .filter(|n| *n < 10)
            .ok_or("Color index must be 0..9 (-1 clears, 10 selects all)")?;
        let bit = 1 << index;
        if bits & bit != 0 {
            return Err("Color entered more than once".into());
        }
        bits |= bit;
    }
    Ok(if toggle && args.len() == 1 {
        current ^ bits
    } else {
        bits
    })
}

pub(super) fn strip_colors(value: &str) -> String {
    let mut chars = value.chars().peekable();
    let mut result = String::new();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            chars.next();
        } else {
            result.push(c);
        }
    }
    result
}

pub(super) fn colorize(
    value: &str,
    mask: u16,
    randomness: u32,
    mut random: impl FnMut(u32) -> u32,
) -> String {
    let colors: Vec<_> = (0..10).filter(|i| mask & (1 << i) != 0).collect();
    if colors.is_empty() {
        return value.to_owned();
    }
    let mut result = String::new();
    let mut store = 0;
    let count = colors.len() as u32;
    let mut chars = value.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '^' && chars.peek().is_some_and(char::is_ascii_digit) {
            result.push(c);
            result.push(chars.next().unwrap());
            if let Some(c) = chars.next() {
                result.push(c);
            }
            store = 0;
            continue;
        }
        if count == 1 {
            if store == 0 {
                store = colors[0];
                result.push('^');
                result.push(char::from(b'0' + colors[0] as u8));
            }
        } else {
            let choice = 1 + random(count * if store == 0 { 1 } else { randomness.max(1) });
            if choice != store && choice <= count {
                store = choice;
                result.push('^');
                result.push(char::from(b'0' + colors[(choice - 1) as usize] as u8));
            }
        }
        result.push(c);
    }
    result
}

fn randomized(value: &str, mask: u16, randomness: u32) -> String {
    let mut seed = [0; 8];
    let _ = getrandom::fill(&mut seed);
    let mut state = u64::from_le_bytes(seed).max(1);
    colorize(value, mask, randomness, |max| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % u64::from(max.max(1))) as u32
    })
}

impl ViewerConsole {
    pub(super) fn identity_command(
        &mut self,
        name: &str,
        args: &[String],
    ) -> Result<Vec<String>, String> {
        let current = self.integer_cvar("cl_colorstring").unwrap_or(0) as u16 & 1023;
        if name == "colorstring" {
            let bits = selected_mask(args, current, true)?;
            self.shell
                .cvars
                .set_text("cl_colorString", &bits.to_string())
                .map_err(|e| e.to_string())?;
            self.persist();
            return Ok((0..10)
                .map(|index| {
                    format!(
                        "{index:2} [{}] ^{index}{}",
                        if bits & (1 << index) != 0 { "X" } else { " " },
                        [
                            "BLACK", "RED", "GREEN", "YELLOW", "BLUE", "CYAN", "MAGENTA", "WHITE",
                            "ORANGE", "GRAY"
                        ][index]
                    )
                })
                .collect());
        }
        let clock = &self.client_commands.name_clock;
        if clock.epoch.elapsed().as_millis() as u64 <= clock.changed.load(Ordering::Relaxed) + 5000
        {
            return Err("You must wait 5 seconds before changing your name again.".into());
        }
        let original = self.text_value("name").unwrap_or("Padawan");
        let value = if name == "afk" {
            let prefix = self.text_value("cl_afkPrefix").unwrap_or("[AFK]");
            original
                .strip_prefix(prefix)
                .map(str::to_owned)
                .unwrap_or_else(|| format!("{prefix}{original}"))
        } else {
            let mask = selected_mask(args, current, false)?;
            randomized(
                &strip_colors(original),
                mask,
                self.integer_cvar("cl_colorstringrandom")
                    .unwrap_or(2)
                    .clamp(1, 1000) as u32,
            )
        };
        self.shell
            .cvars
            .set_text("name", &value)
            .map_err(|e| e.to_string())?;
        self.persist();
        Ok(vec![value])
    }

    /// Color the existing safe chat command, preserving destination and escaping.
    pub(crate) fn color_chat_command(&self, command: &str) -> String {
        let mask = self.integer_cvar("cl_colorstring").unwrap_or(0) as u16 & 1023;
        let colored = color_chat(
            command,
            mask,
            self.integer_cvar("cl_colorstringrandom")
                .unwrap_or(2)
                .clamp(1, 1000) as u32,
        );
        super::super::client_options::style_chat(
            &colored,
            self.text_value("cl_chatstyleprefix").unwrap_or(""),
            self.text_value("cl_chatstylesuffix").unwrap_or(""),
        )
    }
}

/// Apply chat colors while retaining the existing destination and escaping rules.
pub(crate) fn color_chat(command: &str, mask: u16, randomness: u32) -> String {
    if mask == 0 {
        return command.to_owned();
    }
    let Ok(tokens) = sjk_shell::tokenize(command) else {
        return command.to_owned();
    };
    let (destination, message) = match tokens.as_slice() {
        [name, message] if name == "say" => (sjk_client::ChatDestination::Global, message),
        [name, message] if name == "say_team" => (sjk_client::ChatDestination::Team, message),
        [name, slot, message] if name == "tell" => {
            let Ok(slot) = slot.parse() else {
                return command.to_owned();
            };
            (sjk_client::ChatDestination::Player(slot), message)
        }
        _ => return command.to_owned(),
    };
    let text = randomized(message, mask, randomness);
    sjk_client::chat_command(destination, &text).unwrap_or_else(|| command.to_owned())
}
