//! `cl_consoleSocket`: the console for programs on this computer.
//!
//! After JoF EJK's `cl_consoleSocket` (`cl_consoleSocket.cpp`), with the same
//! cvars and line protocol so apps written for it (Sol's Archive) work unchanged.
//! The transport is [`sjk_shell::console_socket`]; this module feeds it the
//! console's new lines every frame and queues the commands it receives.
//!
//! One difference from JoF EJK: the socket refuses to open without
//! `cl_consoleSocketPassword`. A web page can write to a loopback port, and with
//! no password its text would run as console commands.

use super::*;
use sjk_shell::console_socket::{ConsoleSocket, MAX_CLIENTS};
use std::collections::VecDeque;

const PORT_CVAR: &str = "cl_consoleSocket";
const PASSWORD_CVAR: &str = "cl_consoleSocketPassword";
/// Chat lines whose raw text waits for the pump; chat is a few lines per frame.
const MAX_RAW_CHAT: usize = 64;
/// Console lines streamed in one frame at most, after a long stall.
const MAX_LINES_PER_FRAME: u64 = 1024;

/// Socket state of one console.
pub(super) struct State {
    socket: ConsoleSocket,
    port: console_cvars::IntegerSetting,
    /// The port value the socket was last configured for.
    applied: i64,
    /// Refused to open for want of a password; retried once one is set.
    needs_password: bool,
    /// Number of the next scrollback line to stream.
    cursor: u64,
    /// Raw text of chat lines by scrollback number: the stock `0x19` separator the
    /// displayed line lacks, which is how apps tell chat from other output.
    raw_chat: VecDeque<(u64, String)>,
    /// Text of the `consolesocket` command's status line.
    status: Arc<RwLock<String>>,
    /// What `status` was last built from.
    shown: (bool, usize, bool, i64),
}

impl State {
    /// Register the cvars, before the config loads so archived values apply.
    pub(super) fn bind(cvars: &mut CvarRegistry) -> Result<Self, sjk_shell::CvarError> {
        cvars.register(CvarDefinition::new(
            PORT_CVAR,
            0_i64,
            CvarFlags::ARCHIVE,
            "TCP port on 127.0.0.1 that streams console output to external apps and runs \
             the commands they send (0 = off); needs cl_consoleSocketPassword",
        ))?;
        cvars.register(CvarDefinition::new(
            PASSWORD_CVAR,
            "",
            CvarFlags::ARCHIVE,
            "Apps connecting to cl_consoleSocket must send this as their first line",
        ))?;
        Ok(Self {
            socket: ConsoleSocket::new(),
            port: console_cvars::IntegerSetting::bind(cvars, PORT_CVAR, 0)?,
            applied: 0,
            needs_password: false,
            cursor: 0,
            raw_chat: VecDeque::new(),
            status: Arc::new(RwLock::new(String::new())),
            shown: (false, 0, false, -1),
        })
    }
}

/// The password cvar's text, empty when unset.
fn password(shell: &Shell) -> &str {
    match shell.cvars.get(PASSWORD_CVAR).map(|cvar| &cvar.value) {
        Some(CvarValue::Text(text)) => text,
        _ => "",
    }
}

/// Register `consolesocket`, which explains the feature and shows its state.
pub(super) fn register_command(shell: &mut Shell, state: &State) -> Result<(), Box<dyn Error>> {
    let status = Arc::clone(&state.status);
    shell.commands.register(
        "consolesocket",
        "Explain the console socket (external app access) and show its state",
        move |_| {
            let current = status
                .read()
                .map_or_else(|_| "unavailable".to_owned(), |text| text.clone());
            let mut lines = vec![
                "^5--- Console Socket: external app access ---".to_owned(),
                "Lets a program on ^2this computer^7 read everything this console prints \
                 (chat included) and send console commands back, over local TCP."
                    .to_owned(),
                format!("^5Status:^7 {current}"),
                "^5Settings^7 (both saved in your config):".to_owned(),
                "  ^3cl_consoleSocket <port>^7: port to listen on, 0 = off (default)".to_owned(),
                "  ^3cl_consoleSocketPassword <text>^7: apps send it as their first line; required"
                    .to_owned(),
                "^5Quick start:^7 ^3cl_consoleSocketPassword mySecret123^7, then \
                 ^3cl_consoleSocket 29071^7, then point the app at 127.0.0.1:29071."
                    .to_owned(),
                "Each console line streams to the app; each line it sends runs as a command \
                 (say hi chats)."
                    .to_owned(),
                "^1A connected app can run ANY console command as you:^7 only use apps you trust. \
                 Only this computer can connect (127.0.0.1)."
                    .to_owned(),
            ];
            lines.push(format!(
                "Up to {MAX_CLIENTS} apps at once; command lines up to 1023 characters."
            ));
            Ok(lines)
        },
    )?;
    Ok(())
}

impl ViewerConsole {
    /// Add a chat line to the scrollback without notifying, as `push_log_quiet`
    /// does, and keep `raw` (the server's text, `0x19` separators included) for
    /// the console socket.
    pub(crate) fn push_chat_line(&mut self, display: String, raw: &str) {
        let number = self.shell.lines_written();
        // Chat stays out of crash reports.
        crate::crash_report::privately(|| self.shell.push_log_quiet(display));
        // A line `cl_noprint` suppressed has no number to name.
        if self.socket.socket.has_reader() && self.shell.lines_written() != number {
            if self.socket.raw_chat.len() == MAX_RAW_CHAT {
                self.socket.raw_chat.pop_front();
            }
            self.socket.raw_chat.push_back((number, raw.to_owned()));
        }
    }

    /// Once per frame: follow `cl_consoleSocket`, stream the console's new lines to
    /// connected apps and queue the commands they sent.
    pub(crate) fn pump_console_socket(&mut self) {
        let wanted = self.socket.port.value();
        if wanted != self.socket.applied
            || (self.socket.needs_password && !password(&self.shell).is_empty())
        {
            self.reconfigure_console_socket(wanted);
        }
        if !self.socket.socket.is_listening() {
            return;
        }
        if password(&self.shell).is_empty() {
            self.socket.socket.close();
            self.socket.needs_password = true;
            self.shell
                .push_log("^3Console socket: closed, cl_consoleSocketPassword was cleared");
            self.update_console_socket_status(wanted);
            return;
        }

        let end = self.shell.lines_written();
        let mut output = Vec::new();
        if self.socket.socket.has_reader() {
            for number in self
                .socket
                .cursor
                .max(end.saturating_sub(MAX_LINES_PER_FRAME))..end
            {
                let Some(line) = self.shell.line(number) else {
                    continue;
                };
                let raw = self
                    .socket
                    .raw_chat
                    .iter()
                    .find(|(raw_number, _)| *raw_number == number)
                    .map(|(_, text)| text.as_str());
                let text = sjk_protocol::encode_legacy_text(raw.unwrap_or(&line.text));
                output.extend(text.iter().map(|&byte| match byte {
                    b'\n' | b'\r' => b' ',
                    byte => byte,
                }));
                output.push(b'\n');
            }
        }
        self.socket.cursor = end;
        self.socket.raw_chat.retain(|(number, _)| *number >= end);

        let poll = self
            .socket
            .socket
            .poll(password(&self.shell).as_bytes(), &output);
        for notice in poll.notices {
            self.shell.push_log(notice);
        }
        for command in poll.commands {
            let text = sjk_client::decode_legacy(&command);
            if let Err(error) = self.shell.queue_script(&text) {
                self.shell
                    .push_log(format!("^1Console socket: command refused: {error}"));
            }
        }
        self.update_console_socket_status(wanted);
    }

    /// Close the socket and open it again for `wanted`, if that is a usable port.
    fn reconfigure_console_socket(&mut self, wanted: i64) {
        self.socket.applied = wanted;
        self.socket.needs_password = false;
        self.socket.socket.close();
        self.socket.raw_chat.clear();
        self.socket.cursor = self.shell.lines_written();
        if wanted == 0 {
            self.update_console_socket_status(wanted);
            return;
        }
        let Some(port) = u16::try_from(wanted).ok().filter(|port| *port != 0) else {
            self.shell.push_log(format!(
                "^3Console socket: {wanted} is not a port (use 1 to 65535, or 0 for off)"
            ));
            return;
        };
        if password(&self.shell).is_empty() {
            self.socket.needs_password = true;
            self.shell.push_log(
                "^3Console socket: set cl_consoleSocketPassword and it will start on port "
                    .to_owned()
                    + &port.to_string(),
            );
            self.update_console_socket_status(wanted);
            return;
        }
        match self.socket.socket.open(port) {
            Ok(_) => self
                .shell
                .push_log(format!("Console socket: listening on 127.0.0.1:{port}")),
            Err(error) => self.shell.push_log(format!(
                "^1Console socket: couldn't listen on 127.0.0.1:{port}: {error}"
            )),
        }
        self.update_console_socket_status(wanted);
    }

    /// Rebuild the `consolesocket` status line when what it shows changed.
    fn update_console_socket_status(&mut self, wanted: i64) {
        let key = (
            self.socket.socket.is_listening(),
            self.socket.socket.client_count(),
            !password(&self.shell).is_empty(),
            wanted,
        );
        if key == self.socket.shown {
            return;
        }
        self.socket.shown = key;
        let (listening, clients, password_set, port) = key;
        let text = if listening {
            format!(
                "^2enabled^7, listening on 127.0.0.1:{port}, apps connected: ^3{clients}^7 (max \
                 {MAX_CLIENTS}), password ^2set"
            )
        } else if port != 0 && !password_set {
            "^1off^7: waiting for cl_consoleSocketPassword".to_owned()
        } else {
            format!(
                "^1disabled^7, password {}",
                if password_set { "^2set" } else { "^1not set" }
            )
        };
        if let Ok(mut status) = self.socket.status.write() {
            *status = text;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::net::{Ipv4Addr, TcpListener, TcpStream};
    use std::time::Duration;

    fn console() -> (tempfile::TempDir, ViewerConsole) {
        let directory = tempfile::tempdir().unwrap();
        let console = ViewerConsole::new(directory.path().join("config.cfg")).unwrap();
        (directory, console)
    }

    fn free_port() -> u16 {
        TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    fn open(console: &mut ViewerConsole) -> TcpStream {
        console.shell.cvars.set_text(PASSWORD_CVAR, "pw").unwrap();
        let port = free_port();
        console
            .shell
            .cvars
            .set_text(PORT_CVAR, &port.to_string())
            .unwrap();
        console.pump_console_socket();
        assert!(console.socket.socket.is_listening());
        let app = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        app.set_read_timeout(Some(Duration::from_millis(2)))
            .unwrap();
        app
    }

    /// Pump until the app has read `wanted` or about a second has passed.
    fn read_until(console: &mut ViewerConsole, app: &mut TcpStream, wanted: &[u8]) -> Vec<u8> {
        let mut got = Vec::new();
        let mut buffer = [0_u8; 512];
        for _ in 0..300 {
            console.pump_console_socket();
            if let Ok(read) = app.read(&mut buffer) {
                got.extend_from_slice(&buffer[..read]);
            }
            if got.windows(wanted.len()).any(|window| window == wanted) {
                break;
            }
            std::thread::sleep(Duration::from_millis(3));
        }
        got
    }

    #[test]
    fn registered_with_the_old_names_and_off_by_default() {
        let (_directory, console) = console();
        assert_eq!(console.integer_cvar(PORT_CVAR), Some(0));
        assert_eq!(console.text_value(PASSWORD_CVAR), Some(""));
        assert!(!console.socket.socket.is_listening());
    }

    #[test]
    fn a_port_waits_for_a_password_then_opens() {
        let (_directory, mut console) = console();
        let port = free_port();
        console
            .shell
            .cvars
            .set_text(PORT_CVAR, &port.to_string())
            .unwrap();
        console.pump_console_socket();
        assert!(!console.socket.socket.is_listening());
        assert!(console.socket.needs_password);
        console.shell.cvars.set_text(PASSWORD_CVAR, "pw").unwrap();
        console.pump_console_socket();
        assert!(console.socket.socket.is_listening());
        console.shell.cvars.set_text(PORT_CVAR, "0").unwrap();
        console.pump_console_socket();
        assert!(!console.socket.socket.is_listening());
    }

    #[test]
    fn an_app_sees_chat_with_its_separator_and_its_commands_run() {
        let (_directory, mut console) = console();
        let mut app = open(&mut console);
        app.write_all(b"pw\necho from the app\n").unwrap();
        let got = read_until(&mut console, &mut app, b"authenticated\n");
        assert!(
            got.starts_with(b"cl_consoleSocket: authenticated\n"),
            "{got:?}"
        );

        // Console output and chat arrive as lines; chat keeps the 0x19 separator,
        // and a high character is one Windows-1252 byte, as apps expect.
        console.push_log("^5plain output");
        console.push_chat_line("T\u{f8}x^7: hello".into(), "T\u{f8}x^7\u{19}: hello");
        let got = read_until(&mut console, &mut app, b"hello\n");
        assert!(
            got.windows(14).any(|window| window == b"^5plain output"),
            "{:?}",
            String::from_utf8_lossy(&got)
        );
        assert!(got.ends_with(b"T\xf8x^7\x19: hello\n"), "{got:?}");

        // The command the app sent runs as if typed: `echo` prints its text.
        console.execute_buffered_frame(None, |_, _| false);
        assert!(
            console
                .shell
                .lines()
                .any(|line| line.text == "from the app")
        );
    }

    #[test]
    fn a_wrong_password_gets_nothing_and_runs_nothing() {
        let (_directory, mut console) = console();
        let mut app = open(&mut console);
        app.write_all(b"nope\necho evil\n").unwrap();
        let got = read_until(&mut console, &mut app, b"never arrives");
        assert!(got.is_empty(), "{got:?}");
        console.execute_buffered_frame(None, |_, _| false);
        assert!(!console.shell.lines().any(|line| line.text == "evil"));
    }
}
