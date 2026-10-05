use crate::codec::WireRead;

#[derive(Debug, Clone, PartialEq)]
pub struct InfoRequest {
    pub version: u8,
    pub timestamp: u32,
}

impl WireRead for InfoRequest {
    fn read(r: &mut crate::codec::Reader) -> Result<Self, crate::codec::CodecError> {
        let version = r.u8()?;
        let timestamp = r.u32()?;

        Ok(InfoRequest {
            version,
            timestamp
        })
    }
}