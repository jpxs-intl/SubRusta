use crate::packets::buf_reader::AlexBufReader;

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameVoiceData {
    pub frames: [ServerboundGameVoiceFrame; 6],
    pub is_silenced: bool
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameVoiceFrame {
    pub index: u8,
    pub size: u16,
    pub volume: u8,
    pub data: Vec<u8>,
}

pub fn decode_voice_data(reader: &mut AlexBufReader) -> ServerboundGameVoiceData {
    let mut frames = core::array::from_fn(|_| ServerboundGameVoiceFrame {
        index: 0,
        size: 0,
        volume: 0,
        data: Vec::new(),
    });

    for frame in &mut frames {
        println!("Frame num");

        frame.index = reader.boundscheck_read_bits(6) as u8;
        frame.size = reader.boundscheck_read_bits(11) as u16;
        frame.volume = reader.boundscheck_read_bits(2) as u8;

        println!("Frame size {}", frame.size);

        if frame.size > 0 {
            frame.data = reader.read_bytes(frame.size as usize, 1);
        } else {
            frame.data.clear();
        }

        println!("Read frame!");
    }

    let is_silenced = reader.boundscheck_read_bits(1) != 0;

    ServerboundGameVoiceData {
        frames,
        is_silenced,
    }
}