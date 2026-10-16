//! Configstrings that change while clients are connected: OpenJK `sv_init.cpp`
//! `SV_SetConfigstring`, `SV_SendConfigstring` and `SV_UpdateConfigstrings`.
//!
//! A client that is in the world is told at once, by server command. One that holds a
//! gamestate but has not entered yet (primed) is told when it enters, so that it sees
//! each string once and in index order. Anyone earlier gets the value with its
//! gamestate. The values themselves stay with whoever owns them; this module only
//! decides who is told what, when, and in which words.

use crate::LegacyClientPhase;

/// `MAX_CONFIGSTRINGS`: the indices protocol 26 has.
const CONFIG_STRINGS: usize = 1_700;
/// `CS_SERVERINFO`, which a client may be spared (`SVF_NOSERVERINFO`, a bot's entity).
const CS_SERVERINFO: usize = 0;
/// `MAX_STRING_CHARS - 24`: a value this long or longer travels in chunks.
const CHUNK: usize = 1_000;

/// The roster as the configstring rules need it.
pub trait LegacyConfigStringHost {
    /// Wire client numbers in the roster.
    fn client_count(&self) -> usize;
    /// Phase of one wire client number.
    fn phase(&self, client: usize) -> LegacyClientPhase;
    /// Whether this client is not to be told `CS_SERVERINFO`.
    fn withholds_server_info(&self, client: usize) -> bool;
    /// The client's record of strings that changed while it was primed.
    fn marks(&mut self, client: usize) -> &mut LegacyConfigStringMarks;
    /// Queue one reliable server command for one client.
    fn send(&mut self, client: usize, command: &[u8]);
}

/// `client_t::csUpdated`: which configstrings changed since this client was primed.
#[derive(Clone)]
pub struct LegacyConfigStringMarks([u64; CONFIG_STRINGS.div_ceil(64)]);

impl Default for LegacyConfigStringMarks {
    fn default() -> Self {
        Self([0; CONFIG_STRINGS.div_ceil(64)])
    }
}

impl LegacyConfigStringMarks {
    /// Forget everything, for a new connection.
    pub fn clear(&mut self) {
        self.0.fill(0);
    }
    fn set(&mut self, index: usize, marked: bool) {
        let bit = 1 << (index % 64);
        if marked {
            self.0[index / 64] |= bit
        } else {
            self.0[index / 64] &= !bit
        }
    }
    /// The marked indices in ascending order.
    fn indices(&self) -> impl Iterator<Item = usize> + '_ {
        (0..CONFIG_STRINGS).filter(|index| self.0[index / 64] & (1 << (index % 64)) != 0)
    }
    /// How many strings are marked.
    pub fn count(&self) -> usize {
        self.0.iter().map(|word| word.count_ones() as usize).sum()
    }
}

/// `SV_SendConfigstring`: the server commands that carry one value, through `send`.
/// A value of 1,000 bytes or more goes as `bcs0`, any number of `bcs1` and a final
/// `bcs2`, 999 bytes each; anything shorter as one `cs`. `scratch` is reused.
pub fn legacy_config_string_commands(
    index: usize,
    value: &[u8],
    scratch: &mut Vec<u8>,
    mut send: impl FnMut(&[u8]),
) {
    let mut command = |verb: &str, part: &[u8]| {
        scratch.clear();
        scratch.extend_from_slice(verb.as_bytes());
        scratch.push(b' ');
        scratch.extend_from_slice(index.to_string().as_bytes());
        scratch.extend_from_slice(b" \"");
        scratch.extend_from_slice(part);
        scratch.extend_from_slice(b"\"\n");
        send(scratch);
    };
    if value.len() < CHUNK {
        return command("cs", value);
    }
    let mut sent = 0;
    while sent < value.len() {
        let remaining = value.len() - sent;
        let verb = if sent == 0 {
            "bcs0"
        } else if remaining < CHUNK {
            "bcs2"
        } else {
            "bcs1"
        };
        command(verb, &value[sent..value.len().min(sent + CHUNK - 1)]);
        sent += CHUNK - 1;
    }
}

/// `SV_SetConfigstring`'s telling: `previous` became `value`. Nothing happens for an
/// unchanged value, nor while no game is running (`running`: `SS_GAME`, or a restart in
/// progress) — then everyone gets it with their gamestate.
pub fn legacy_config_string_changed(
    host: &mut impl LegacyConfigStringHost,
    index: usize,
    previous: &[u8],
    value: &[u8],
    running: bool,
    scratch: &mut Vec<u8>,
) {
    if previous == value || !running || index >= CONFIG_STRINGS {
        return;
    }
    for client in 0..host.client_count() {
        match host.phase(client) {
            LegacyClientPhase::Active => {}
            LegacyClientPhase::Primed => {
                host.marks(client).set(index, true);
                continue;
            }
            _ => continue,
        }
        if index == CS_SERVERINFO && host.withholds_server_info(client) {
            continue;
        }
        legacy_config_string_commands(index, value, scratch, |command| host.send(client, command));
    }
}

/// `SV_UpdateConfigstrings`, as a client enters the world: every string marked while
/// it was primed, in index order, each with its current value. A withheld
/// `CS_SERVERINFO` stays marked, as in the reference.
pub fn legacy_config_strings_catch_up<'a>(
    host: &mut impl LegacyConfigStringHost,
    client: usize,
    value: impl Fn(usize) -> &'a [u8],
    scratch: &mut Vec<u8>,
) {
    let withheld = host.withholds_server_info(client);
    let marked: LegacyConfigStringMarks = host.marks(client).clone();
    for index in marked.indices() {
        if index == CS_SERVERINFO && withheld {
            continue;
        }
        legacy_config_string_commands(index, value(index), scratch, |command| {
            host.send(client, command)
        });
        host.marks(client).set(index, false);
    }
}
