use std::net::{Ipv4Addr, SocketAddr};

use crate::codec::WireRead;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerAddress {
    pub addr: SocketAddr
}

impl WireRead for ServerAddress {
    fn read(r: &mut crate::codec::Reader) -> Result<Self, crate::codec::CodecError> {
        let first = r.u8()?;
        let second = r.u8()?;
        let third = r.u8()?;
        let fourth = r.u8()?;
        let port = r.u16()?;

        Ok(Self {
            addr: SocketAddr::new(std::net::IpAddr::V4(Ipv4Addr::new(fourth, third, second, first)), port)
        })
    }
}