#[derive(Clone, Default)]
pub struct AlexBufWriter {
    buf: Vec<u8>,
    bit_pos: usize,
}

impl AlexBufWriter {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            bit_pos: 0,
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            buf: Vec::with_capacity(capacity),
            bit_pos: 0,
        }
    }

    pub fn write_bits(&mut self, value: i32, bit_count: u32) {
        if bit_count == 0 || bit_count > 32 {
            return;
        }

        // Mask the value to only include the bits we want to write
        let masked_value = if bit_count == 32 {
            value
        } else {
            value & ((1i32 << bit_count) - 1)
        };

        let mut remaining_bits = bit_count;
        let mut data_to_write = masked_value;

        while remaining_bits > 0 {
            // Ensure we have a byte to write to
            if self.buf.len() * 8 <= self.bit_pos {
                self.buf.push(0);
            }

            let byte_index = self.bit_pos / 8;
            let bit_offset_in_byte = self.bit_pos % 8;
            let bits_available_in_byte = 8 - bit_offset_in_byte;
            let bits_to_write = remaining_bits.min(bits_available_in_byte as u32);

            // Extract the bits to write to this byte
            let bits_for_this_byte = data_to_write & ((1i32 << bits_to_write) - 1);
            
            // Write the bits to the current byte
            self.buf[byte_index] |= (bits_for_this_byte as u8) << bit_offset_in_byte;

            // Update state for next iteration
            remaining_bits -= bits_to_write;
            data_to_write >>= bits_to_write;
            self.bit_pos += bits_to_write as usize;
        }
    }

    pub fn write_string(&mut self, string: String) {
        let mut output = [0; 32];
        let bytes = string.as_bytes();
        let len = bytes.len().min(32);
        output[..len].copy_from_slice(&bytes[..len]);
        
        self.write_bytes(&output);
    }

    pub fn write_byte(&mut self, byte: u8) {
        self.write_bits(byte as i32, 8);
    }

    pub fn write_bytes(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_byte(byte);
        }
    }

    pub fn pad_to_byte_boundary(&mut self) {
        let bit_offset = self.bit_pos % 8;
        if bit_offset != 0 {
            let padding_bits = 8 - bit_offset;
            self.write_bits(0, padding_bits as u32);
        }
    }

    pub fn into_vec(self) -> Vec<u8> {
        self.buf
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.buf
    }

    pub fn len(&self) -> usize {
        self.buf.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }

    pub fn bit_len(&self) -> usize {
        self.bit_pos
    }

    pub fn clear(&mut self) {
        self.buf.clear();
        self.bit_pos = 0;
    }

    pub fn get_position(&self) -> (usize, usize) {
        (self.bit_pos / 8, self.bit_pos % 8)
    }
}