use crossbeam::channel::{Receiver, Sender};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::SystemTime,
};
use tokio::task::JoinHandle;

#[derive(Debug, Clone)]
pub struct ClientConnection {
    pub tx_socket: Sender<(Vec<u8>, SocketAddr)>,
    pub last_packet: SystemTime,
    pub address: SocketAddr,
    tx_sender: Sender<Vec<u8>>,
    tx_receiver: Receiver<Vec<u8>>,
    tx_handle: Arc<Option<JoinHandle<()>>>,
}

impl ClientConnection {
    pub fn from_address(address: SocketAddr, tx_socket: Sender<(Vec<u8>, SocketAddr)>) -> Self {
        let (tx_sender, tx_receiver) = crossbeam::channel::unbounded();

        let mut conn = ClientConnection {
            tx_socket,
            last_packet: SystemTime::now(),
            address,
            tx_sender,
            tx_receiver,
            tx_handle: Arc::new(None),
        };

        let c = conn.clone();
        conn.tx_handle = Arc::new(Some(tokio::spawn(async move { c.do_sending() })));

        conn
    }

    pub fn kill_thread(&self) {
        if let Some(handle) = self.tx_handle.as_ref() {
            handle.abort();
        }
    }

    fn do_sending(&self) {
        loop {
            let bytes = self.tx_receiver.recv();

            if let Ok(bytes) = bytes {
                self.tx_socket.send((bytes, self.address)).unwrap();
            }
        }
    }

    pub fn send_data(&self, data: Vec<u8>) {
        let header = b"7DFP";
        let mut vec = Vec::with_capacity(header.len() + data.len());
        vec.extend_from_slice(header);
        vec.extend_from_slice(&data);

        let _ = self.tx_sender.send(vec);
    }
}