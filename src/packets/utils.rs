pub fn least_significant(value: u8) -> u8 {
    value & 0b1111
}

pub fn most_significant(value: u8) -> u8 {
    value >> 4
}

pub fn limited_string(input: &str) -> [u8; 32] {
    let mut output = [0; 32];
    let bytes = input.as_bytes();
    let len = bytes.len().min(32);
    output[..len].copy_from_slice(&bytes[..len]);
    output
}

pub fn broken_bit(most_significant: u8, least_significant: u8) -> u8 {
    (most_significant & 0x0F) | (least_significant & 0x0F) << 4
}

pub fn read_u16_le(buf: &mut Vec<u8>) -> u16 {
    if buf.len() < 2 {
        panic!("Buffer too short to read u16");
    }
    let bytes = buf.drain(..2).collect::<Vec<u8>>();
    u16::from_le_bytes(bytes.try_into().expect("Slice with incorrect length for u16"))
}

pub fn read_u32_le(buf: &mut Vec<u8>) -> u32 {
    if buf.len() < 4 {
        panic!("Buffer too short to read u32");
    }
    let bytes = buf.drain(..4).collect::<Vec<u8>>();
    u32::from_le_bytes(bytes.try_into().expect("Slice with incorrect length for u32"))
}

pub fn read_u8(buf: &mut Vec<u8>) -> u8 {
    if buf.is_empty() {
        panic!("Buffer too short to read u8");
    }
    buf.drain(..1).as_slice().to_vec()[0]
}