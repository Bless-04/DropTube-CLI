use std::net::{IpAddr, SocketAddr};
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
}
