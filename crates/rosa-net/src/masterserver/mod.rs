mod auth;
mod discovery;

pub use auth::AuthStore;
pub use discovery::MsError;
use rosa_protocol::masterserver::AuthPacket;
use rosa_sim::Outbound;

use std::{net::SocketAddr, time::Duration};

pub struct MasterServer {
    pub address: SocketAddr,
    outbound: Outbound,
    auth: AuthStore
}

impl MasterServer {
    pub async fn connect(master_url: &str, master_ip: Option<&str>, outbound: Outbound) -> Result<Self, MsError> {
        let address = discovery::resolve_address(master_url, master_ip).await?;
        println!("[MasterServer] Resolved master server at {address} - online.");
        Ok(Self { address, auth: AuthStore::new(address), outbound })
    }

    pub fn send(&self, payload: &[u8]) {
        let _ = self.outbound.send((b"7DFP@".to_vec(), self.address));
    }

    pub fn keepalive(&self) {
        self.send(&[b'@']);
    }

    pub fn register_auth(&mut self, from: SocketAddr, pkt: AuthPacket) -> bool {
        if from != self.address {
            return false;
        }

        self.auth.stash(from, pkt)
    }

    pub fn verify_join(&mut self, account_id: u32, ticket: u32) -> Option<&AuthPacket> {
        self.auth.verify(account_id, ticket)
    }

    pub fn evict_stale(&mut self, max_age: Duration) {
        self.auth.evict(max_age);
    }
}