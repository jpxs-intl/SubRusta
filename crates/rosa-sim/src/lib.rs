use std::net::SocketAddr;

use rosa_protocol::{masterserver::AuthPacket, serverbound::{game::ClientGamePacket, join_request::JoinRequest}};
use tokio::sync::mpsc::UnboundedSender;

pub mod computer;
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
    /// Ticks since the client's last packet (the connection's timeoutTime): dropped at 1800, a kick sets 1200.
    timeout: i32,
    /// Whether the connection gets admin chat and the admin list (an admin's).
    admin_visible: bool,
    earshots: [Option<sim::Earshot>; 8],
    /// The human the client says it is watching (connection +0x19c), heard from while it has none of its own.
    spectating: Option<usize>,
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
    /// The line links this connection has been sent, per item (connection +0x1dac on), the 256 entry ring of links
    /// queued for it (+0x2ddd0 on), how far it is filled (+0x2ddc4) and acknowledged (+0x2ddcc).
    link_sent: std::collections::HashMap<usize, u64>,
    link_ring: Vec<i32>,
    link_count: u8,
    link_ack: u8,
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