//! Explicit connectionless queries. Reuses the browser's OOB builders/parsers.
//! OpenJK codemp/client/cl_main.cpp:1108-1149,3337-3371,3386-3458,3618-3644,3756-3805.
use crate::*;
use std::net::{Ipv4Addr, SocketAddr, UdpSocket};

/// Bounded diagnostic text; unknown UDP senders cannot inject console lines.
#[derive(Debug, Default)]
pub(crate) struct PrintInbox(std::collections::VecDeque<String>);

impl PrintInbox {
    pub(crate) fn receive(&mut self, expected: SocketAddr, source: SocketAddr, bytes: &[u8]) {
        if source != expected {
            return;
        }
        if let Some(text) = parse_print_response(bytes) {
            if self.0.len() == 32 {
                self.0.pop_front();
            }
            self.0.push_back(text);
        }
    }
}

impl LegacyConnection {
    /// Take one validated in-session OOB print without disturbing sequenced decoding.
    pub fn pop_server_print(&mut self) -> Option<String> {
        self.server_prints.0.pop_front()
    }

    /// Time since the most recent packet from the sequenced peer.
    pub fn packet_silence(&self) -> Option<Duration> {
        self.last_received.map(|last| last.elapsed())
    }
}

/// A stock connectionless request, independent of the sequenced client codec.
pub enum Request {
    /// Server information and ping.
    Ping,
    /// Full server/player status.
    Status,
    /// Remote console command. Never retransmitted automatically.
    Rcon {
        /// Remote console credential, never included in diagnostics.
        password: String,
        /// Unmodified command argument text.
        command: String,
    },
    /// Master query, including optional stock filters.
    Master {
        /// Requested server protocol number.
        protocol: u32,
        /// Optional space-separated master filters.
        keywords: String,
    },
    /// IPv4 broadcast discovery on PORT_SERVER through PORT_SERVER+3.
    Lan,
}

/// Decoded OOB response.
pub enum Answer {
    /// Published server information.
    Info(InfoString),
    /// Full server status.
    Status(ServerStatus),
    /// Remote console text.
    Print(String),
    /// Master-listed addresses.
    Servers(Vec<SocketAddr>),
}

/// A response and its receipt timing.
pub struct Response {
    /// Validated reply source.
    pub source: SocketAddr,
    /// Time since the request was sent.
    pub elapsed: Duration,
    /// Parsed response.
    pub answer: Answer,
}

/// Build stock bytes. Rcon includes the terminating NUL (CL_Rcon_f strlen+1).
pub fn packet(request: &Request) -> Result<Vec<u8>, NetworkError> {
    let command = match request {
        Request::Ping | Request::Lan => "getinfo xxx".into(),
        Request::Status => "getstatus".into(),
        Request::Rcon { password, command } => format!("rcon {password} {command}"),
        Request::Master { protocol, keywords } => {
            format!(
                "getservers {protocol}{}",
                if keywords.is_empty() {
                    String::new()
                } else {
                    format!(" {keywords}")
                }
            )
        }
    };
    let mut bytes = connectionless_packet(&command)?;
    if matches!(request, Request::Rcon { .. }) {
        bytes.push(0);
    }
    Ok(bytes)
}

/// Parse replies using the same decoders used by browser discovery.
pub fn parse(request: &Request, bytes: &[u8]) -> Result<Answer, NetworkError> {
    match request {
        Request::Ping | Request::Lan => parse_info_response(bytes).map(Answer::Info),
        Request::Status => parse_status_response(bytes).map(Answer::Status),
        Request::Master { .. } => parse_master_response(bytes).map(Answer::Servers),
        Request::Rcon { .. } => parse_print_response(bytes)
            .map(Answer::Print)
            .ok_or(NetworkError::UnexpectedResponse),
    }
}

/// Receive one expected-source packet; also used by browser info/status queries.
pub(crate) fn first_response(
    server: SocketAddr,
    bytes: &[u8],
    timeout: Duration,
) -> Result<Vec<u8>, NetworkError> {
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_read_timeout(Some(timeout.min(Duration::from_millis(100))))?;
    socket.send_to(bytes, server)?;
    let deadline = Instant::now() + timeout;
    let mut packet = [0; MAX_UDP_PACKET_BYTES];
    loop {
        match socket.recv_from(&mut packet) {
            Ok((length, source)) if source == server => return Ok(packet[..length].to_vec()),
            Ok(_) => {}
            Err(error) if timeout_error(&error) => {}
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            return Err(NetworkError::TimedOut("OOB query"));
        }
    }
}

/// Execute off the render thread, sharing one socket across replies and status retries.
pub fn run(
    server: SocketAddr,
    request: Request,
    timeout: Duration,
    resend: Duration,
) -> Result<Vec<Response>, NetworkError> {
    let socket = UdpSocket::bind(if server.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    })?;
    socket.set_read_timeout(Some(Duration::from_millis(50)))?;
    let bytes = packet(&request)?;
    let lan = matches!(request, Request::Lan);
    if lan {
        socket.set_broadcast(true)?;
        for _ in 0..2 {
            for port in 29070..29074 {
                socket.send_to(&bytes, (Ipv4Addr::BROADCAST, port))?;
            }
        }
    } else {
        socket.send_to(&bytes, server)?;
    }
    let started = Instant::now();
    let mut deadline = started + timeout;
    let mut retry = started + resend;
    let mut responses = Vec::new();
    let mut packet = [0; MAX_UDP_PACKET_BYTES];
    loop {
        match socket.recv_from(&mut packet) {
            Ok((length, source))
                if source == server || (lan && (29070..29074).contains(&source.port())) =>
            {
                if let Ok(answer) = parse(&request, &packet[..length]) {
                    responses.push(Response {
                        source,
                        elapsed: started.elapsed(),
                        answer,
                    });
                    if matches!(request, Request::Ping | Request::Status) {
                        break;
                    }
                    if !lan {
                        deadline = deadline.min(Instant::now() + Duration::from_millis(200));
                    }
                }
            }
            Ok(_) => {}
            Err(error) if timeout_error(&error) => {}
            Err(error) => return Err(error.into()),
        }
        let now = Instant::now();
        if now >= deadline || responses.len() >= 4096 {
            break;
        }
        if matches!(request, Request::Status) && now >= retry {
            socket.send_to(&bytes, server)?;
            retry = now + resend;
        }
    }
    if responses.is_empty() {
        return Err(NetworkError::TimedOut("OOB query"));
    }
    Ok(responses)
}

fn timeout_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}
