use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use rosa_protocol::{GameMode, ServerboundPacket, clientbound::server_info::ServerInfo, codec::{WireWrite, Writer}, parse_frame, serverbound::info_request::InfoRequest};
use rosa_sim::{ConnId, Inbound, Outbound};
use tokio::{net::UdpSocket, sync::mpsc};

use crate::masterserver::MasterServer;

pub mod masterserver;

pub struct ServerListing {
    pub build: u8,
    pub gamemode: GameMode,
    pub max_players: u8,
    pub server_name: String,
    pub server_id: u32,
    pub address: [u8; 4],
    pub port: u16,
    pub password_protected: bool
}

pub struct Edge {
    socket: Arc<UdpSocket>,
    in_tx: mpsc::UnboundedSender<Inbound>,
    out_tx: Outbound,
    masterserver: MasterServer,
    sessions: HashMap<SocketAddr, ConnId>,
    next_conn: u32,
    listing: ServerListing
}

impl Edge {
    pub fn new(socket: Arc<UdpSocket>, in_tx: mpsc::UnboundedSender<Inbound>, out_tx: Outbound, masterserver: MasterServer, listing: ServerListing) -> Self {
        Self { socket, in_tx, out_tx, masterserver, sessions: Default::default(), next_conn: 0, listing }
    }

    pub async fn run(mut self) {
        let mut buf = [0u8; 2048];
        let mut keepalive = tokio::time::interval(Duration::from_secs(16));
        let mut evict = tokio::time::interval(Duration::from_secs(10));
        loop {
            tokio::select! {
                r = self.socket.recv_from(&mut buf) => {
                    if let Ok((n, src)) = r { self.on_datagram(&buf[..n], src); }
                }
                _ = keepalive.tick() => self.masterserver.keepalive(),
                _ = evict.tick() => self.masterserver.evict_stale(Duration::from_secs(10)),
            }
        }
    }

    fn on_datagram(&mut self, data: &[u8], src: SocketAddr) {
        let Some((type_byte, body)) = parse_frame(data) else { return; };
        match rosa_protocol::decode_packet(type_byte, body) {
            Ok(msg) => self.route(msg, src),
            Err(e) => eprintln!("[net] dropped packet from {src}: {e:?}"),
        }
    }

    fn route(&mut self, msg: ServerboundPacket, src: SocketAddr) {
        match msg {
            ServerboundPacket::InfoRequest(r) => self.reply_server_info(src, r),
            ServerboundPacket::AuthPacket(a) => { self.masterserver.register_auth(src, a); },
            ServerboundPacket::JoinRequest(j) => {
                let is_valid = self.masterserver.verify_join(j.account_id, j.auth_ticket);

                if let Some(auth_packet) = is_valid {

                }
            },
        }
    }

    fn reply_server_info(&self, src: SocketAddr, req: InfoRequest) {
        let res = ServerInfo {
            timestamp: req.timestamp,
            gamemode: self.listing.gamemode,
            current_players: self.sessions.len() as u8,
            max_players: self.listing.max_players,
            address: self.listing.address,
            port: self.listing.port,
            password_protected: self.listing.password_protected,
            build: self.listing.build,
            server_name: self.listing.server_name.clone()
        };

        let mut w = Writer::new();
        w.bytes(b"7DFP");
        res.write(&mut w);

        let _ = self.out_tx.send((w.into_vec(), src));
    }
}