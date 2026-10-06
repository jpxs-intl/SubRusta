use std::{net::SocketAddr, sync::Arc};

use rosa_net::{Edge, ServerListing, masterserver::MasterServer};
use rosa_sim::{Inbound, sim::Sim};
use tokio::{net::UdpSocket, sync::mpsc};

use crate::config::ConfigMain;

pub mod config;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = ConfigMain::read_from_file();

    let (in_tx, in_rx) = mpsc::unbounded_channel::<Inbound>();
    let (out_tx, mut out_rx) = mpsc::unbounded_channel::<(Vec<u8>, SocketAddr)>();

    let socket = Arc::new(UdpSocket::bind(("0.0.0.0", config.port)).await?);
    println!("[net] listening on {}", socket.local_addr()?);

    {
        let socket = socket.clone();
        tokio::spawn(async move {
            while let Some((buf, addr)) = out_rx.recv().await {
                let _ = socket.send_to(&buf, addr).await;
            }
        });
    }

    let masterserver = MasterServer::connect(&config.master_server_url, config.master_server_ip.as_deref(), out_tx.clone()).await?;

    {
        let out_tx = out_tx.clone();
        let server_name = config.server_name.clone();
        std::thread::Builder::new().name("sim".into()).spawn(move || Sim::new(in_rx, out_tx, server_name, config.gamemode, config.max_players).run()).unwrap();
    }

    let server_listing = ServerListing {
        build: 0x26,
        address: [4, 43, 217, 32],
        gamemode: config.gamemode,
        max_players: config.max_players,
        password_protected: !config.server_password.is_empty(),
        server_password: config.server_password.clone(),
        port: config.port,
        server_id: 80085,
        server_name: config.server_name.clone()
    };

    Edge::new(socket, in_tx, out_tx, masterserver, server_listing).run().await;
    Ok(())
}