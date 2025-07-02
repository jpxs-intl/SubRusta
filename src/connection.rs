use std::net::SocketAddr;
use tokio::task::JoinHandle;
use crossbeam::channel::{Receiver, Sender};

#[derive(Debug)]
pub struct ClientConnection {
    pub socket: Sender<(Vec<u8>, SocketAddr)>,
    pub address: SocketAddr,
    sender: Sender<Vec<u8>>,
    receiver: Receiver<Vec<u8>>,
    handle: JoinHandle<()>
}

impl ClientConnection {
    pub fn from_address(address: SocketAddr, socket: Sender<(Vec<u8>, SocketAddr)>) -> Self {
        let (sender, receiver) = crossbeam::channel::unbounded();

        let r = receiver.clone();
        let s = socket.clone();
        let handle = tokio::spawn(async move {
            do_sending(r, s, address);
        });

        ClientConnection {
            socket,
            address,
            sender,
            receiver,
            handle
        }
    }

    pub fn send_data(&self, data: Vec<u8>) {
        let header = b"7DFP";
        let mut vec = Vec::with_capacity(header.len() + data.len());
        vec.extend_from_slice(header);
        vec.extend_from_slice(&data);

        let _ = self.sender.send(vec);
    }
}

fn do_sending(receiver: Receiver<Vec<u8>>, socket: Sender<(Vec<u8>, SocketAddr)>, address: SocketAddr) {
    loop {
        let bytes = receiver.recv();

        if let Ok(bytes) = bytes {
            println!("Sending bytes {:?}", bytes);
            socket.send((bytes, address)).unwrap();
        }
    }
}