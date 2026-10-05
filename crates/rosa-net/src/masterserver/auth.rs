use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use rosa_protocol::masterserver::AuthPacket;
use tokio::time::Instant;

struct Pending {
    received: Instant,
    packet: AuthPacket
}

pub struct AuthStore {
    ms_addr: SocketAddr,
    pending: HashMap<u32, Pending>
}

impl AuthStore {
    pub fn new(ms_addr: SocketAddr) -> Self {
        Self { ms_addr, pending: HashMap::new() }
    }

    pub fn stash(&mut self, from: SocketAddr, packet: AuthPacket) -> bool {
        if from != self.ms_addr {
            return false;
        }
        self.pending.insert(packet.account_id, Pending { received: Instant::now(), packet });
        true
    }

    pub fn verify(&self, account_id: u32, ticket: u32) -> Option<&AuthPacket> {
        self.pending.get(&account_id).filter(|p| p.packet.auth_ticket == ticket).map(|p| &p.packet)
    }

    pub fn evict(&mut self, max_age: Duration) {
        let now = Instant::now();
        self.pending.retain(|_, p| now.duration_since(p.received) <= max_age);
    }
}