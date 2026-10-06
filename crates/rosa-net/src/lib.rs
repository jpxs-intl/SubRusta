use std::{collections::HashMap, net::SocketAddr, sync::Arc, time::Duration};

use rosa_protocol::{GameMode, ServerboundPacket, clientbound::{kick::KickClient, server_info::ServerInfo}, parse_frame, serverbound::info_request::InfoRequest};
use rosa_sim::{ConnId, Inbound, Outbound, SimJoinMsg, SimMsg};
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
    pub password_protected: bool,
    pub server_password: String,
}

pub struct Edge {
    socket: Arc<UdpSocket>,
    in_tx: mpsc::UnboundedSender<Inbound>,
    out_tx: Outbound,
    masterserver: MasterServer,
    sessions: HashMap<SocketAddr, ConnId>,
    next_conn: u32,
    listing: ServerListing,
    server_ip: Option<SocketAddr>,
}

impl Edge {
    pub fn new(socket: Arc<UdpSocket>, in_tx: mpsc::UnboundedSender<Inbound>, out_tx: Outbound, masterserver: MasterServer, listing: ServerListing) -> Self {
        Self { socket, in_tx, out_tx, masterserver, sessions: Default::default(), next_conn: 0, listing, server_ip: None }
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
            Err(e) => eprintln!("[Net] dropped packet from {src}: {e:?}"),
        }
    }

    fn route(&mut self, msg: ServerboundPacket, src: SocketAddr) {
        match msg {
            ServerboundPacket::InfoRequest(r) => self.reply_server_info(src, r),
            ServerboundPacket::AuthPacket(a) => { self.masterserver.register_auth(src, a); },
            ServerboundPacket::JoinRequest(j) => {
                if self.masterserver.verify_join(j.account_id, j.auth_ticket).is_none() {
                    return;
                }

                if j.password != self.listing.server_password {
                    let packet = KickClient {
                        reason: "Your password is incorrect!".to_string()
                    };

                    let _ = self.out_tx.send((rosa_protocol::frame_packet(packet), src));

                    return;
                }

                match self.sessions.get(&src) {
                    Some(_) => {}
                    None => {
                        let conn = ConnId(self.next_conn);

                        self.next_conn += 1;
                        self.sessions.insert(src, conn);

                        let _ = self.in_tx.send(Inbound { conn, src, msg: SimMsg::Join(SimJoinMsg {
                            join_packet: j.clone(),
                            auth_packet: self.masterserver.verify_join(j.account_id, j.auth_ticket).unwrap().clone()
                        }) });
                    }
                }
            },
            ServerboundPacket::LeaveGame => {
                if let Some(conn) = self.sessions.remove(&src) {
                    let _ = self.in_tx.send(Inbound { conn, src, msg: SimMsg::Leave });
                }
            },
            ServerboundPacket::GamePacket(g) => {
                if let Some(conn) = self.sessions.get(&src) {
                    let _ = self.in_tx.send(Inbound { conn: *conn, src, msg: SimMsg::Game(g) });
                }
            },
            ServerboundPacket::MasterServerPing(p) => self.server_ip = Some(p.addr)
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

        let _ = self.out_tx.send((rosa_protocol::frame_packet(res), src));
    }
}