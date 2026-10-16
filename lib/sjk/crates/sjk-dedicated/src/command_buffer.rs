//! The console's command buffer (`cmd.cpp`): text queued by the terminal, config files
//! and `vstr`, cut into command lines as `Cbuf_Execute` cuts it, and the commands that
//! feed it — `exec`, `execq`, `vstr`, `wait`.
//!
//! The buffer only holds and cuts text. Each line it hands over runs through the whole
//! console (the endpoint's commands, then the server's), exactly as a line typed at the
//! console does; the process drains it once a frame, as `Com_Frame` does.

use crate::cvars::Cvars;

/// `MAX_CMD_BUFFER`.
const BUFFER_BYTES: usize = 128 * 1024;
/// `MAX_CMD_LINE`: the longest line handed over; the rest becomes the next line.
const LINE_BYTES: usize = 1024;
/// `MAX_QPATH`.
const QPATH_BYTES: usize = 64;

/// Queued command text and the `wait` still to sit out.
#[derive(Default)]
pub struct CommandBuffer {
    text: Vec<u8>,
    wait: i32,
}

/// One `Cbuf_Execute` call's comment state, which runs from line to line within it.
#[derive(Default)]
pub struct CommandPass {
    star_comment: bool,
    slash_comment: bool,
}

/// The text up to its first NUL, as `strlen` sees it.
fn c_string(text: &[u8]) -> &[u8] {
    &text[..text
        .iter()
        .position(|&byte| byte == 0)
        .unwrap_or(text.len())]
}

impl CommandBuffer {
    /// `Cbuf_AddText`: at the end, or nothing and a complaint if it would overflow.
    pub fn add_text(&mut self, text: &[u8], print: &mut dyn FnMut(&[u8])) {
        let text = c_string(text);
        if self.text.len() + text.len() >= BUFFER_BYTES {
            print(b"Cbuf_AddText: overflow\n");
            return;
        }
        self.text.extend_from_slice(text);
    }

    /// `Cbuf_InsertText`: at the front, ended by a newline, so that it runs next.
    pub fn insert_text(&mut self, text: &[u8], print: &mut dyn FnMut(&[u8])) {
        let text = c_string(text);
        if text.len() + 1 + self.text.len() > BUFFER_BYTES {
            print(b"Cbuf_InsertText overflowed\n");
            return;
        }
        self.text.splice(0..0, text.iter().copied().chain([b'\n']));
    }

    /// Whether nothing is queued.
    pub fn is_empty(&self) -> bool {
        self.text.is_empty()
    }

    /// The next line of `Cbuf_Execute`'s loop, or `None` when this pass is over: the
    /// buffer is empty, or a `wait` holds the rest for a later frame. A line ends at a
    /// `;` outside quotes and comments or at a line break outside a block comment; a
    /// `*/` ends one too (its `/` is lost). A line longer than 1,023 bytes is cut and
    /// its tail, less one byte, becomes the next.
    pub fn next_line(&mut self, pass: &mut CommandPass) -> Option<Vec<u8>> {
        if self.text.is_empty() {
            return None;
        }
        if self.wait > 0 {
            self.wait -= 1;
            return None;
        }
        let text = &self.text;
        let (mut quotes, mut i) = (0, 0);
        while i < text.len() {
            if text[i] == b'"' {
                quotes += 1;
            }
            if quotes & 1 == 0 {
                if i < text.len() - 1 {
                    let next = text[i + 1];
                    if !pass.star_comment && text[i] == b'/' && next == b'/' {
                        pass.slash_comment = true;
                    } else if !pass.slash_comment && text[i] == b'/' && next == b'*' {
                        pass.star_comment = true;
                    } else if pass.star_comment && text[i] == b'*' && next == b'/' {
                        pass.star_comment = false;
                        i += 1;
                        break;
                    }
                }
                if !pass.slash_comment && !pass.star_comment && text[i] == b';' {
                    break;
                }
            }
            if !pass.star_comment && matches!(text[i], b'\n' | b'\r') {
                pass.slash_comment = false;
                break;
            }
            i += 1;
        }
        i = i.min(LINE_BYTES - 1);
        let line = text[..i].to_vec();
        if i == self.text.len() {
            self.text.clear();
        } else {
            self.text.drain(..i + 1);
        }
        Some(line)
    }

    /// The buffer's own commands: `exec`/`execq` (a file's text run next, `.cfg` added
    /// to a name without an extension), `vstr` (a variable's text run next) and `wait`
    /// (the rest held for that many frames). `read` finds a file by its game path.
    /// Returns whether the line was one of them.
    pub fn command(
        &mut self,
        words: &[&[u8]],
        cvars: &Cvars,
        read: &mut dyn FnMut(&str) -> Option<Vec<u8>>,
        print: &mut dyn FnMut(&[u8]),
    ) -> bool {
        let Some(&name) = words.first() else {
            return false;
        };
        let is = |command: &[u8]| name.eq_ignore_ascii_case(command);
        if is(b"exec") || is(b"execq") {
            let quiet = is(b"execq");
            if words.len() != 2 {
                let (q, notice) = if quiet {
                    ("q", " without notification")
                } else {
                    ("", "")
                };
                print(format!("exec{q} <filename> : execute a script file{notice}\n").as_bytes());
                return true;
            }
            let file = config_file_name(words[1]);
            // `FS_FOpenFileRead`: nothing that could leave the search paths.
            let text = if file.contains("..") || file.contains("::") {
                None
            } else {
                read(&file)
            };
            let Some(text) = text else {
                print(format!("couldn't exec {file}\n").as_bytes());
                return true;
            };
            if !quiet {
                print(format!("execing {file}\n").as_bytes());
            }
            self.insert_text(&text, print);
        } else if is(b"vstr") {
            if words.len() != 2 {
                print(b"vstr <variablename> : execute a variable command\n");
                return true;
            }
            let text = [cvars.string(words[1]), b"\n"].concat();
            self.insert_text(&text, print);
        } else if is(b"wait") {
            self.wait = if words.len() == 2 {
                sjk_game_jka::userinfo::atoi(words[1])
            } else {
                1
            };
            if self.wait < 0 {
                self.wait = 1;
            }
        } else {
            return false;
        }
        true
    }
}

/// `Q_strncpyz` into `MAX_QPATH` and `COM_DefaultExtension(".cfg")`: the extension is
/// added only where the last `.` is not after the last `/`, and only if it fits.
fn config_file_name(argument: &[u8]) -> String {
    let mut name = argument[..argument.len().min(QPATH_BYTES - 1)].to_vec();
    let dot = name.iter().rposition(|&byte| byte == b'.');
    let slash = name.iter().rposition(|&byte| byte == b'/');
    let has_extension = dot.is_some_and(|dot| slash.is_none_or(|slash| slash < dot));
    if !has_extension && name.len() + 4 < QPATH_BYTES {
        name.extend_from_slice(b".cfg");
    }
    String::from_utf8_lossy(&name).into_owned()
}
