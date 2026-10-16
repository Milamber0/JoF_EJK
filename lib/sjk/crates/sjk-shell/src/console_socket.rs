//! Loopback TCP bridge between the console and external programs.
//!
//! A program on the same computer connects, authenticates with a password line,
//! then receives every console line as it prints and has every line it sends
//! executed as a console command. The format is the one JoF EJK's
//! `cl_consoleSocket` used, so apps written for it (Sol's Archive) keep working:
//! plain text, `\n`-terminated lines, no framing.
//!
//! The socket is non-blocking and driven from the frame by [`ConsoleSocket::poll`]:
//! a slow or stalled reader never blocks the game, it is dropped once its unread
//! backlog passes [`MAX_CLIENT_BACKLOG`]. The listener binds `127.0.0.1` only.

use std::io::{ErrorKind, Read, Write};
use std::net::{Ipv4Addr, SocketAddr, TcpListener, TcpStream};

/// Connected programs at once; further connections are closed.
pub const MAX_CLIENTS: usize = 4;
/// Longest command line accepted. A longer line is discarded whole, never
/// truncated into a different command.
pub const MAX_COMMAND_LINE: usize = 1023;
/// Unread output a client may hold before it is dropped.
pub const MAX_CLIENT_BACKLOG: usize = 256 * 1024;
/// Bytes read from one client per poll, bounding the work of a frame.
const READ_BUDGET: usize = 16 * 1024;
/// Reply to a correct password.
pub const AUTHENTICATED_REPLY: &[u8] = b"cl_consoleSocket: authenticated\n";

struct Client {
    stream: TcpStream,
    /// Sent the password, or none is required.
    authed: bool,
    outbuf: Vec<u8>,
    line: Vec<u8>,
    overflow: bool,
}

/// What one [`ConsoleSocket::poll`] produced.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct Poll {
    /// Command lines received from authenticated programs, without line ends.
    pub commands: Vec<Vec<u8>>,
    /// Connection events for the console to print.
    pub notices: Vec<String>,
}

/// The listener and its connected programs.
#[derive(Default)]
pub struct ConsoleSocket {
    listener: Option<TcpListener>,
    clients: Vec<Client>,
}

impl ConsoleSocket {
    /// A socket that listens on nothing.
    pub fn new() -> Self {
        Self::default()
    }

    /// Stop listening and drop every program. Port 0 is not special here.
    pub fn close(&mut self) {
        self.listener = None;
        self.clients.clear();
    }

    /// Listen on `127.0.0.1:port`, replacing any earlier listener. Port 0 picks
    /// a free port (tests); callers treat their setting 0 as "off" and call
    /// [`Self::close`]. Returns the port bound.
    pub fn open(&mut self, port: u16) -> Result<u16, std::io::Error> {
        self.close();
        let listener = TcpListener::bind(SocketAddr::from((Ipv4Addr::LOCALHOST, port)))?;
        listener.set_nonblocking(true)?;
        let bound = listener.local_addr()?.port();
        self.listener = Some(listener);
        Ok(bound)
    }

    /// Whether a listener is open.
    pub fn is_listening(&self) -> bool {
        self.listener.is_some()
    }

    /// Programs connected, authenticated or not.
    pub fn client_count(&self) -> usize {
        self.clients.len()
    }

    /// Whether any authenticated program would receive output.
    pub fn has_reader(&self) -> bool {
        self.clients.iter().any(|client| client.authed)
    }

    /// One frame of work: accept connections, read and authenticate, queue
    /// `output` (console text, already encoded, lines ended by `\n`) to
    /// authenticated programs and write what they will take.
    ///
    /// An empty `password` authenticates nobody: callers refuse to open without
    /// one, since a web page can write to a loopback port and would otherwise
    /// run console commands.
    pub fn poll(&mut self, password: &[u8], output: &[u8]) -> Poll {
        let mut poll = Poll::default();
        let Some(listener) = &self.listener else {
            return poll;
        };
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    if self.clients.len() >= MAX_CLIENTS || stream.set_nonblocking(true).is_err() {
                        continue; // dropped, closing it
                    }
                    let _ = stream.set_nodelay(true);
                    self.clients.push(Client {
                        stream,
                        authed: false,
                        outbuf: Vec::new(),
                        line: Vec::new(),
                        overflow: false,
                    });
                    poll.notices
                        .push("Console socket: client connected (waiting for password)".into());
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        self.clients.retain_mut(|client| {
            let verdict = client.receive(password, &mut poll.commands);
            let verdict = verdict.or_else(|| client.queue_and_send(output));
            if let Some(reason) = verdict {
                poll.notices
                    .push(format!("Console socket: client dropped ({reason})"));
                false
            } else {
                true
            }
        });
        poll
    }
}

impl Client {
    /// Read what the program sent. `Some(reason)` means drop it.
    fn receive(&mut self, password: &[u8], commands: &mut Vec<Vec<u8>>) -> Option<&'static str> {
        let mut buffer = [0_u8; 1024];
        let mut budget = READ_BUDGET;
        while budget > 0 {
            let read = match self.stream.read(&mut buffer) {
                Ok(0) => return Some("disconnected"),
                Ok(read) => read,
                Err(error) if error.kind() == ErrorKind::WouldBlock => return None,
                Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                Err(_) => return Some("receive error"),
            };
            budget = budget.saturating_sub(read);
            for &byte in &buffer[..read] {
                match byte {
                    b'\r' => {}
                    b'\n' => {
                        let line = std::mem::take(&mut self.line);
                        let overflow = std::mem::take(&mut self.overflow);
                        if !self.authed {
                            // The first line must be the password; anything else,
                            // an HTTP request from a hostile page included, closes
                            // the connection before anything runs.
                            if overflow || password.is_empty() || line != password {
                                return Some("bad password");
                            }
                            self.authed = true;
                            self.outbuf.extend_from_slice(AUTHENTICATED_REPLY);
                        } else if !overflow && !line.is_empty() {
                            commands.push(line);
                        }
                    }
                    _ if self.line.len() < MAX_COMMAND_LINE => self.line.push(byte),
                    _ => self.overflow = true,
                }
            }
        }
        None
    }

    /// Queue console output and write what the socket accepts.
    fn queue_and_send(&mut self, output: &[u8]) -> Option<&'static str> {
        if self.authed && !output.is_empty() {
            if self.outbuf.len() + output.len() > MAX_CLIENT_BACKLOG {
                return Some("send backlog overflow");
            }
            self.outbuf.extend_from_slice(output);
        }
        while !self.outbuf.is_empty() {
            match self.stream.write(&self.outbuf) {
                Ok(0) => return Some("send error"),
                Ok(sent) => {
                    self.outbuf.drain(..sent);
                }
                Err(error) if error.kind() == ErrorKind::WouldBlock => return None,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(_) => return Some("send error"),
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn connect(port: u16) -> TcpStream {
        let stream = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_millis(20)))
            .unwrap();
        stream
    }

    /// Poll until `done` holds or a second passes, collecting commands and notices.
    fn run(
        socket: &mut ConsoleSocket,
        password: &[u8],
        output: &[u8],
        mut done: impl FnMut(&Poll) -> bool,
    ) -> Poll {
        let mut all = Poll::default();
        let start = Instant::now();
        let mut output = output;
        while start.elapsed() < Duration::from_secs(1) {
            let poll = socket.poll(password, output);
            output = b"";
            all.commands.extend(poll.commands);
            all.notices.extend(poll.notices);
            if done(&all) {
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        all
    }

    fn read_available(stream: &mut TcpStream) -> Vec<u8> {
        let mut got = Vec::new();
        let mut buffer = [0_u8; 256];
        while let Ok(read) = stream.read(&mut buffer) {
            if read == 0 {
                break;
            }
            got.extend_from_slice(&buffer[..read]);
        }
        got
    }

    #[test]
    fn password_then_commands_run_and_output_streams() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let mut app = connect(port);
        app.write_all(b"secret\r\nsay hi\nsay\xf8 two\n").unwrap();
        let poll = run(&mut socket, b"secret", b"line one\n", |poll| {
            poll.commands.len() == 2
        });
        assert_eq!(poll.commands, [b"say hi".to_vec(), b"say\xf8 two".to_vec()]);
        assert!(socket.has_reader());
        let got = run_read(&mut socket, &mut app, b"secret");
        assert!(got.starts_with(AUTHENTICATED_REPLY), "{got:?}");
        assert!(got.ends_with(b"line one\n"), "{got:?}");
    }

    fn run_read(socket: &mut ConsoleSocket, app: &mut TcpStream, password: &[u8]) -> Vec<u8> {
        let mut got = Vec::new();
        for _ in 0..20 {
            socket.poll(password, b"");
            got.extend(read_available(app));
        }
        got
    }

    #[test]
    fn wrong_password_or_a_web_request_is_dropped_before_anything_runs() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let mut app = connect(port);
        app.write_all(b"POST / HTTP/1.1\nquit\n").unwrap();
        let poll = run(&mut socket, b"secret", b"", |poll| {
            poll.notices.iter().any(|n| n.contains("dropped"))
        });
        assert!(poll.commands.is_empty());
        assert_eq!(socket.client_count(), 0);
        assert!(!socket.has_reader());
    }

    #[test]
    fn an_empty_password_never_authenticates() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let mut app = connect(port);
        app.write_all(b"\nquit\n").unwrap();
        let poll = run(&mut socket, b"", b"", |poll| {
            poll.notices.iter().any(|n| n.contains("dropped"))
        });
        assert!(poll.commands.is_empty());
        assert_eq!(socket.client_count(), 0);
    }

    #[test]
    fn an_oversized_line_is_discarded_whole() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let mut app = connect(port);
        let mut input = b"pw\n".to_vec();
        input.extend(std::iter::repeat_n(b'x', MAX_COMMAND_LINE + 50));
        input.extend_from_slice(b"\nsay ok\n");
        app.write_all(&input).unwrap();
        let poll = run(&mut socket, b"pw", b"", |poll| !poll.commands.is_empty());
        assert_eq!(poll.commands, [b"say ok".to_vec()]);
    }

    #[test]
    fn a_line_of_exactly_the_limit_is_kept() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let mut app = connect(port);
        let mut input = b"pw\n".to_vec();
        input.extend(std::iter::repeat_n(b'x', MAX_COMMAND_LINE));
        input.push(b'\n');
        app.write_all(&input).unwrap();
        let poll = run(&mut socket, b"pw", b"", |poll| !poll.commands.is_empty());
        assert_eq!(poll.commands[0].len(), MAX_COMMAND_LINE);
    }

    #[test]
    fn connections_beyond_the_limit_are_closed() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let apps: Vec<_> = (0..MAX_CLIENTS + 2).map(|_| connect(port)).collect();
        run(&mut socket, b"pw", b"", |_| false);
        assert_eq!(socket.client_count(), MAX_CLIENTS);
        drop(apps);
    }

    #[test]
    fn output_before_authentication_is_withheld() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let mut app = connect(port);
        run(&mut socket, b"pw", b"secret console text\n", |_| false);
        assert!(read_available(&mut app).is_empty());
    }

    #[test]
    fn close_stops_listening_and_drops_programs() {
        let mut socket = ConsoleSocket::new();
        let port = socket.open(0).unwrap();
        let _app = connect(port);
        run(&mut socket, b"pw", b"", |_| false);
        socket.close();
        assert!(!socket.is_listening());
        assert_eq!(socket.client_count(), 0);
        assert!(TcpStream::connect((Ipv4Addr::LOCALHOST, port)).is_err());
    }
}
