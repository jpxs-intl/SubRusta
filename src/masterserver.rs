use std::sync::Arc;

use crate::config::config_main::ConfigMain;

#[derive(Clone)]
pub struct MasterServer {
    pub address: String,
    server_socket: Option<Arc<tokio::net::UdpSocket>>,
}

impl MasterServer {
    pub async fn init(config: &ConfigMain) -> Self {
        println!("[MasterServer] Initializing MasterServer... Attempting to connect to {}...", config.master_server_url);

        let mut host = if config.master_server_url.ends_with('/') {
            format!("{}anewzero/serverinfo.php", config.master_server_url)
        } else {
            format!("{}/anewzero/serverinfo.php", config.master_server_url)
        };

        if !host.starts_with("http://") && !host.starts_with("https://") {
            host = format!("https://{host}");
        }

        let client = reqwest::ClientBuilder::new()
            .user_agent("SubRosa")
            .build()
            .unwrap();

        let res = client
            .get(host)
            .send()
            .await
            .expect("Failed to send request");
        let textual_data = res.text().await.expect("Failed to read response text");

        let split = textual_data
            .split('\t')
            .collect::<Vec<&str>>()
            .iter()
            .filter(|&s| !s.is_empty())
            .copied()
            .collect::<Vec<&str>>();

        let address = format!("{}:{}", split[1], split[2].parse::<i32>().unwrap() + 2);

        println!("[MasterServer] MasterServer initialized... Waiting for connection...");

        MasterServer {
            address,
            server_socket: None,
        }
    }

    pub async fn connect(&mut self) {
        if self.server_socket.is_none() {
            let socket = tokio::net::UdpSocket::bind("0.0.0.0:0")
                .await
                .expect("Failed to bind socket");

            socket.connect(&self.address)
                .await
                .expect("Failed to connect to server");

            println!("[MasterServer] Connected to MasterServer! - Now online!");

            self.server_socket = Some(Arc::new(socket));
        }
    }

    pub async fn send(&self, data: [u8; 1024]) {
        if let Some(socket) = &self.server_socket {
            let header = b"7DFP";

            let mut data_with_header = Vec::with_capacity(header.len() + data.len());
            data_with_header.extend_from_slice(header);
            data_with_header.extend_from_slice(&data);

            socket
                .send_to(&data_with_header, &self.address)
                .await
                .expect("Failed to send data");
        } else {
            panic!("[MASTERSERVER] Socket is not initialized");
        }
    }
}
