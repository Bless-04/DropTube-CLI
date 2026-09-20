//! TCP connection tracking for the optional terminal interface.

use axum::serve::Listener;
use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use tokio::net::{TcpListener, TcpStream};

/// A point-in-time view of one client observed by the server.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClientSnapshot {
    ip_address: IpAddr,
    active_connections: usize,
}

impl ClientSnapshot {
    pub(crate) const fn new(ip_address: IpAddr, active_connections: usize) -> Self {
        Self {
            ip_address,
            active_connections,
        }
    }

    /// Returns the client's IP address.
    #[must_use]
    pub fn ip_address(self) -> IpAddr {
        self.ip_address
    }

    /// Returns whether the client currently has a live TCP connection.
    #[must_use]
    pub fn is_active(self) -> bool {
        self.active_connections > 0
    }

    /// Returns the number of live TCP connections for this client.
    #[must_use]
    pub fn active_connections(self) -> usize {
        self.active_connections
    }
}

/// Connection state shared between the server listener and the terminal interface.
///
/// Client entries remain present for the lifetime of the process. Their connection count reaches
/// zero after the last socket from that IP closes, allowing the interface to distinguish active
/// clients from clients observed earlier in the run.
#[derive(Debug, Default)]
pub struct ServerState {
    clients: BTreeMap<IpAddr, usize>,
}

impl ServerState {
    /// Returns an IP-sorted snapshot of every client observed since startup.
    #[must_use]
    pub fn clients(&self) -> Vec<ClientSnapshot> {
        self.clients
            .iter()
            .map(|(&ip_address, &active_connections)| {
                ClientSnapshot::new(ip_address, active_connections)
            })
            .collect()
    }

    fn connect(&mut self, ip_address: IpAddr) {
        let connection_count = self.clients.entry(ip_address).or_default();
        *connection_count = connection_count.saturating_add(1);
    }

    fn disconnect(&mut self, ip_address: IpAddr) {
        if let Some(connection_count) = self.clients.get_mut(&ip_address) {
            *connection_count = connection_count.saturating_sub(1);
        }
    }
}

/// An Axum listener that records TCP connection lifetimes in shared state.
#[derive(Debug)]
pub struct TrackingListener {
    listener: TcpListener,
    state: Arc<Mutex<ServerState>>,
}

impl TrackingListener {
    /// Wraps an existing Tokio TCP listener with connection tracking.
    #[must_use]
    pub fn new(listener: TcpListener, state: Arc<Mutex<ServerState>>) -> Self {
        Self { listener, state }
    }
}

impl Listener for TrackingListener {
    type Io = TrackedStream;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, address)) => {
                    return (
                        TrackedStream::new(stream, address.ip(), Arc::clone(&self.state)),
                        address,
                    );
                }
                Err(error) => {
                    let is_connection_error = matches!(
                        error.kind(),
                        io::ErrorKind::ConnectionRefused
                            | io::ErrorKind::ConnectionAborted
                            | io::ErrorKind::ConnectionReset
                    );
                    if !is_connection_error {
                        log::error!("TCP accept failed: {error}");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }

    fn local_addr(&self) -> io::Result<Self::Addr> {
        self.listener.local_addr()
    }
}

/// A TCP stream that releases its client's connection count when dropped.
#[doc(hidden)]
#[derive(Debug)]
pub struct TrackedStream {
    stream: TcpStream,
    _connection: ConnectionLease,
}

impl TrackedStream {
    fn new(stream: TcpStream, ip_address: IpAddr, state: Arc<Mutex<ServerState>>) -> Self {
        Self {
            stream,
            _connection: ConnectionLease::new(ip_address, state),
        }
    }
}

impl AsyncRead for TrackedStream {
    fn poll_read(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_read(context, buffer)
    }
}

impl AsyncWrite for TrackedStream {
    fn poll_write(
        mut self: Pin<&mut Self>,
        context: &mut Context<'_>,
        buffer: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.stream).poll_write(context, buffer)
    }

    fn poll_flush(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_flush(context)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, context: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.stream).poll_shutdown(context)
    }
}

#[derive(Debug)]
struct ConnectionLease {
    ip_address: IpAddr,
    state: Arc<Mutex<ServerState>>,
}

impl ConnectionLease {
    fn new(ip_address: IpAddr, state: Arc<Mutex<ServerState>>) -> Self {
        update_state(&state, |server_state| server_state.connect(ip_address));
        Self { ip_address, state }
    }
}

impl Drop for ConnectionLease {
    fn drop(&mut self) {
        update_state(&self.state, |server_state| {
            server_state.disconnect(self.ip_address);
        });
    }
}

fn update_state<T>(
    state: &Arc<Mutex<ServerState>>,
    update: impl FnOnce(&mut ServerState) -> T,
) -> T {
    let mut server_state = match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    update(&mut server_state)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn clients_remain_listed_after_their_last_connection_closes() {
        let state = Arc::new(Mutex::new(ServerState::default()));
        let ip_address = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 12));

        let first_connection = ConnectionLease::new(ip_address, Arc::clone(&state));
        let second_connection = ConnectionLease::new(ip_address, Arc::clone(&state));
        let active = update_state(&state, |server_state| server_state.clients());
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].active_connections(), 2);
        assert!(active[0].is_active());

        drop(first_connection);
        let one_remaining = update_state(&state, |server_state| server_state.clients());
        assert_eq!(one_remaining[0].active_connections(), 1);
        assert!(one_remaining[0].is_active());

        drop(second_connection);
        let disconnected = update_state(&state, |server_state| server_state.clients());
        assert_eq!(disconnected[0].active_connections(), 0);
        assert!(!disconnected[0].is_active());
    }

    #[test]
    fn client_snapshots_are_sorted_by_ip_address() {
        let mut state = ServerState::default();
        let later = IpAddr::V6(Ipv6Addr::LOCALHOST);
        let earlier = IpAddr::V4(Ipv4Addr::LOCALHOST);
        state.connect(later);
        state.connect(earlier);

        let clients = state.clients();
        assert_eq!(clients[0].ip_address(), earlier);
        assert_eq!(clients[1].ip_address(), later);
    }
}
