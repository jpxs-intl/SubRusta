#[derive(Default)]
pub struct Writer {
    buf: Vec<u8>,
    bit_pos: usize
}

impl Writer {
    pub fn new() -> Self { Self::default() }

    pub fn bits(&mut self, value: i32, count: u32) {
        if count == 0 { return; }

        let mut value = value as u32;
        if count < 32 { value &= (1u32 << count) - 1; }

        let mut remaining = count;
        while remaining > 0 {
            let byte_index = self.bit_pos / 8;
            if byte_index >= self.buf.len() { self.buf.push(0); }

            let off = (self.bit_pos % 8) as u32;
            let n = remaining.min(8 - off);
            let chunk = (value & ((1u32 << n) - 1)) as u8;
            self.buf[byte_index] |= chunk << off;

            value >>= n;
            remaining -= n;
            self.bit_pos += n as usize;
        }
    }

    pub fn pad_to_byte(&mut self) {
        let off = self.bit_pos % 8;
        if off != 0 { self.bits(0, (8 - off) as u32) }
    }

    pub fn byte(&mut self, b: u8) { self.pad_to_byte(); self.bits(b as i32, 8); }
    pub fn bytes(&mut self, bytes: &[u8]) { self.pad_to_byte(); for &b in bytes { self.bits(b as i32, 8); } }
    pub fn f32(&mut self, v: f32) { self.bytes(&v.to_le_bytes()) }
    pub fn u16(&mut self, v: u16) { self.bytes(&v.to_le_bytes()) }
    pub fn u32(&mut self, v: u32) { self.bytes(&v.to_le_bytes()) }
    pub fn u64(&mut self, v: u64) { self.bytes(&v.to_le_bytes()) }
    pub fn i32(&mut self, v: i32) { self.bytes(&v.to_le_bytes()) }

    pub fn string(&mut self, s: &str, len: usize) {
        self.pad_to_byte();
        let src = s.as_bytes();
        for i in 0..len { self.bits(*src.get(i).unwrap_or(&0) as i32, 8); }
    }

    pub fn least_significant(value: u8) -> u8 {
        value & 0xf
    }

    pub fn most_significant(value: u8) -> u8 {
        value >> 4
    }

    pub fn into_vec(self) -> Vec<u8> { self.buf }
    pub fn as_slice(&self) -> &[u8] { &self.buf }
}