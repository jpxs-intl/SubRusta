use std::net::SocketAddr;

use rosa_protocol::{masterserver::AuthPacket, serverbound::{game::ClientGamePacket, join_request::JoinRequest}};
use tokio::sync::mpsc::UnboundedSender;

pub mod human;
pub mod player;
pub mod rng;
pub mod world;
pub mod sim;

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
    last_sdl_tick: u32,
    earshots: [Option<sim::Earshot>; 8],
}