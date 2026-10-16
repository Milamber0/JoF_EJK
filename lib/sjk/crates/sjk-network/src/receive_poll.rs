//! Immediate receive, separate from blocking handshake/download deadlines.
use std::io;
use std::net::{SocketAddr, UdpSocket};

/// Poll an exclusively owned blocking socket, restoring its mode before returning.
/// `codemp/qcommon/net_ip.cpp:250-273,442-448`: non-blocking receive returns on EAGAIN.
/// Unlike stock's shared event socket, this connection also serves blocking bootstrap calls;
/// scope non-blocking mode to the receive so sends and deliberate waits retain their contract.
pub(crate) fn poll(
    socket: &UdpSocket,
    packet: &mut [u8],
) -> io::Result<Option<(usize, SocketAddr)>> {
    socket.set_nonblocking(true)?;
    let received = once(|| socket.recv_from(packet));
    // Restore even on WouldBlock or another receive error; no cloned fd, allocation or lock.
    socket.set_nonblocking(false)?;
    received
}

fn once<T>(receive: impl FnOnce() -> io::Result<T>) -> io::Result<Option<T>> {
    match receive() {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.kind() == io::ErrorKind::WouldBlock => Ok(None),
        Err(error) => Err(error),
    }
}
