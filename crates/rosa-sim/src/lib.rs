use std::net::SocketAddr;

use rosa_protocol::{masterserver::AuthPacket, serverbound::{game::ClientGamePacket, join_request::JoinRequest}};
use tokio::sync::mpsc::UnboundedSender;

pub mod human;
pub mod player;
pub mod rng;
pub mod world;
pub mod sim;
pub mod traffic;
pub mod vehicle;

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct PlayerId(pub u32);
impl PlayerId {
    #[inline]
    fn idx(self) -> usize {
        self.0 as usize
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnId(pub u32);
pub struct Inbound {
    pub conn: ConnId,
    pub src: SocketAddr,
    pub msg: SimMsg,
}
pub type Outbound = UnboundedSender<(Vec<u8>, SocketAddr)>;

pub struct SimJoinMsg {
    pub join_packet: JoinRequest,
    pub auth_packet: AuthPacket,
}
pub enum SimMsg {
    Join(SimJoinMsg),
    Game(Box<ClientGamePacket>),
    Leave,
}

pub struct Client {
    player_id: PlayerId,
    addr: SocketAddr,
    event_cursor: u16,
    /// The round the client last said it is in (-1 on join): until it is the current one the
    /// client gets the initial sync each tick instead of game packets, and what it sends is ignored.
    round_number: u32,
    last_sdl_tick: u32,
    earshots: [Option<sim::Earshot>; 8],
    /// The client's object slot ring (2048 entries): what was queued, how far, and how far the client has
    /// acknowledged (the 11 bits after the spectated human in its game packet).
    pack_ring: Vec<rosa_protocol::clientbound::game::ObjectPack>,
    pack_count: u16,
    pack_ack: u16,
    /// What each slot holds as far as the queued entries go.
    packed: std::collections::HashMap<u16, rosa_protocol::clientbound::game::ObjectPack>,
    /// How overdue each traffic car is for this client (connection +0x6da4), and which intersection's lights it gets
    /// next (connection +0x54).
    traffic_priority: Vec<i32>,
    signal_cursor: i32,
}

impl Client {
    /// The connection part of reset_game: the event and object packet state start over with the new round.
    fn reset_for_round(&mut self) {
        self.event_cursor = 0;
        self.pack_ring.clear();
        self.pack_count = 0;
        self.pack_ack = 0;
        self.packed.clear();
        self.traffic_priority.clear();
        self.signal_cursor = 0;
        self.earshots = [None; 8];
    }
}