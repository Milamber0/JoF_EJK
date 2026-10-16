//! A client slot as the game's commands see it, and `ClientNumberFromString`
//! (`codemp/game/g_cmds.c:184`), which several commands use to name another client.

/// `clientConnected_t` for an occupied slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Connection {
    /// `CON_CONNECTING`: in a slot, not yet in the game.
    Connecting,
    /// `CON_CONNECTED`.
    Connected,
}

/// One occupied client slot, as a command sees it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClientView {
    /// The slot's client number.
    pub client: usize,
    /// How far into the game it is.
    pub connection: Connection,
    /// `sess.sessionTeam`.
    pub team: i32,
    /// Whether it is a bot (`SVF_BOT`).
    pub bot: bool,
    /// `pers.netname`, colours and all.
    pub name: Vec<u8>,
    /// `client->tempSpectate`: until when a siege player who died out of a round watches.
    pub temp_spectate: i32,
}

/// `ClientNumberFromString`: a slot number, then a name compared without its colours and
/// without case. `allow_connecting` also accepts a slot not yet in the game. `None` is
/// the reference's `-1`; the reference also tells the asker `User %s is not on the
/// server`, which is the caller's to say.
pub fn client_number_from_string<'a>(
    clients: &'a [ClientView],
    text: &[u8],
    allow_connecting: bool,
) -> Option<&'a ClientView> {
    let admitted =
        |view: &&ClientView| allow_connecting || view.connection == Connection::Connected;
    // `StringIsInteger`: digits only, at least one. A number naming nobody falls through
    // to the names, so a player called "7" is found.
    if !text.is_empty()
        && text.iter().all(u8::is_ascii_digit)
        && let Some(view) = usize::try_from(crate::userinfo::atoi(text))
            .ok()
            .and_then(|number| clients.iter().find(|view| view.client == number))
            .filter(admitted)
    {
        return Some(view);
    }
    let wanted = strip_colours(text);
    clients
        .iter()
        .filter(admitted)
        .find(|view| strip_colours(&view.name).eq_ignore_ascii_case(&wanted))
}

/// `Q_StripColor`: every `^` followed by a digit goes, with the digit, pass after pass
/// until none is left (`^^11x` is `x`).
pub fn strip_colours(text: &[u8]) -> Vec<u8> {
    let mut out = text.to_vec();
    loop {
        let before = out.len();
        let mut stripped = Vec::with_capacity(out.len());
        let mut bytes = out.iter().copied().peekable();
        while let Some(byte) = bytes.next() {
            if byte == b'^' && bytes.peek().is_some_and(u8::is_ascii_digit) {
                bytes.next();
                continue;
            }
            stripped.push(byte);
        }
        out = stripped;
        if out.len() == before {
            return out;
        }
    }
}
