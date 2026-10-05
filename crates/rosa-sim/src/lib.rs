use std::{net::SocketAddr, time::{Duration, Instant}};

use tokio::sync::mpsc::{self, UnboundedSender};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ConnId(pub u32);
pub struct Inbound { pub conn: ConnId, pub src: SocketAddr, pub msg: SimMsg}
pub type Outbound = UnboundedSender<(Vec<u8>, SocketAddr)>;
pub enum SimMsg { Join(), Game(), Leave }

pub struct Sim {
    tick: u32,
    in_rx: mpsc::UnboundedReceiver<Inbound>,
    out_tx: Outbound
}

impl Sim {
    pub fn new(in_rx: mpsc::UnboundedReceiver<Inbound>, out_tx: Outbound) -> Self {
        Self { tick: 0, in_rx, out_tx }
    }

    pub fn run(mut self) {
        let dt = Duration::from_secs_f64(1.0 / 60.0);
        let (mut acc, mut prev) = (Duration::ZERO, Instant::now());

        loop {
            acc += prev.elapsed();
            prev = Instant::now();

            while acc >= dt {
                // self.world.tick();
                self.tick += 1;
                acc -= dt;
            }

            std::thread::sleep(dt.saturating_sub(acc));
        }
    }
}