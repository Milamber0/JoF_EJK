//! TaystJK cg_servercmds.c:1695-1734 suppression, before notification playback.

/// Bounded notification filter shared by the local sound adapter.
pub(crate) struct Filter {
    /// Whether spam and duplicate notification suppression is enabled.
    pub(crate) enabled: bool,
    previous: [u8; 1024],
    len: usize,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            enabled: false,
            previous: [0; 1024],
            len: 0,
        }
    }
}

impl Filter {
    /// Record an incoming command and decide whether its notification should be omitted.
    pub(crate) fn suppress(&mut self, command: &[u8], global: bool) -> bool {
        let same = command == &self.previous[..self.len];
        self.len = command.len().min(self.previous.len());
        self.previous[..self.len].copy_from_slice(&command[..self.len]);
        if !self.enabled {
            return false;
        }
        if same {
            return true;
        }
        if !global {
            return false;
        }
        let mut plain = [0; 1024];
        let mut len = 0;
        let mut input = command.iter().copied().peekable();
        while let Some(byte) = input.next() {
            if byte == b'^' && input.peek().is_some_and(u8::is_ascii_digit) {
                input.next();
            } else if len < plain.len() {
                plain[len] = byte;
                len += 1;
            }
        }
        [b"media - currently playing: ".as_slice(), b"hi everybody!"]
            .iter()
            .any(|needle| {
                plain[..len]
                    .windows(needle.len())
                    .any(|part| part.eq_ignore_ascii_case(needle))
            })
    }
}
