use crate::codec::{CodecError, Reader};

#[derive(Debug, Clone, PartialEq)]
pub struct VoiceData {
    pub frames: [VoiceFrame; 6],
    pub is_silenced: bool,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct VoiceFrame {
    pub index: u8,
    pub size: u16,
    pub volume: u8,
    pub data: Vec<u8>,
}

pub fn decode_voice_data(reader: &mut Reader) -> Result<VoiceData, CodecError> {
    let mut frames = core::array::from_fn(|_| {
        VoiceFrame {
            index: 0,
            size: 0,
            volume: 0,
            data: Vec::new()
        }
    });

    for frame in &mut frames {
        frame.index = reader.bits(6)? as u8;
        frame.size = reader.bits(11)? as u16;
        frame.volume = reader.bits(2)? as u8;

        frame.data = reader.bytes(frame.size as usize)?.to_vec();
    }

    let is_silenced = reader.bits(1)? != 0;

    Ok(VoiceData {
        frames,
        is_silenced
    })
}