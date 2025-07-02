pub struct AlexBufReader {
    buf: Vec<u8>,
    pos: usize,
    bit_pos: usize,
}

impl AlexBufReader {
    pub fn from_buf(buf: Vec<u8>) -> Self {
        Self { buf, pos: 0, bit_pos: 0 }
    }

    pub fn read_special_f32(&mut self) -> f32 {
        if self.pos + 3 > self.buf.len() {
            panic!("Not enough data in buffer to read special f32");
        }

        let mut arr = [0u8; 4];
        let bytes = self.read_bytes(3, 1);
        arr[0..3].copy_from_slice(&bytes);

        f32::from_le_bytes(arr)        
    }

    pub fn read_string(&mut self, size: usize) -> String {
        if self.pos + size > self.buf.len() {
            panic!("Not enough data in buffer to read string of size {size}");
        }

        let bytes = self.read_bytes(1, size);
        String::from_utf8(bytes.to_vec()).expect("Failed to convert bytes to String")
    }

    pub fn read_u8(&mut self) -> u8 {
        if self.pos >= self.buf.len() {
            panic!("Not enough data in buffer to read u8");
        }

        let bytes = self.read_bytes(1, 1);
        bytes[0]
    }

    pub fn read_u32(&mut self) -> u32 {
        if self.pos + 4 > self.buf.len() {
            panic!("Not enough data in buffer to read u32");
        }

        let bytes = self.read_bytes(4, 1);
        let value = u32::from_le_bytes(bytes.try_into().expect("Slice with incorrect length for u32"));
        value
    }

    pub fn read_bytes(&mut self, size: usize, count: usize) -> Vec<u8> {
        if self.pos > 65535 {
            panic!("Buffer overflow: Attempted to read beyond the buffer size");
        }

        let bytes_to_read = size * count;

        if self.pos + bytes_to_read > self.buf.len() {
            panic!("Not enough data in buffer to read {bytes_to_read} bytes");
        }

        if self.bit_pos > 0 {
            self.pos += 1;
            self.bit_pos = 0;
        }

        let data = self.buf[self.pos..self.pos + bytes_to_read].to_vec();
        self.pos += bytes_to_read;

        data
    }

    pub fn boundscheck_read_bits(&mut self, count: usize) -> u32 {
        if self.bit_pos <= 65535 && self.bit_pos + (count >> 3) <= self.buf.len() {
            return self.read_bits(count as u32).unwrap_or(0);
        }

        0
    }

    pub fn read_bits(&mut self, bit_count: u32) -> Option<u32> {
        if bit_count == 0 || bit_count > 32 {
            return Some(0);
        }

        if self.pos >= self.buf.len() {
            return None;
        }

        let mut data = 0u32;
        let mut remaining_bits = bit_count;
        let mut bits_read = 0u32;

        let bits_available_in_current_byte = 8 - self.bit_pos;
        let bits_to_read_from_current_byte = remaining_bits.min(bits_available_in_current_byte as u32);
        
        let current_byte = self.buf[self.pos] as u32;
        let mask = (1u32 << bits_to_read_from_current_byte) - 1;
        data = (current_byte >> self.bit_pos) & mask;
        
        remaining_bits -= bits_to_read_from_current_byte;
        bits_read += bits_to_read_from_current_byte;
        
        self.bit_pos += bits_to_read_from_current_byte as usize;
        if self.bit_pos >= 8 {
            self.bit_pos = 0;
            self.pos += 1;
        }

        while remaining_bits >= 8 && self.pos < self.buf.len() {
            let byte_data = self.buf[self.pos] as u32;
            data |= byte_data << bits_read;
            
            remaining_bits -= 8;
            bits_read += 8;
            self.pos += 1;
        }

        if remaining_bits > 0 && self.pos < self.buf.len() {
            let current_byte = self.buf[self.pos] as u32;
            let mask = (1u32 << remaining_bits) - 1;
            data |= (current_byte & mask) << bits_read;
            
            self.bit_pos = remaining_bits as usize;
        }

        let final_mask = if bit_count == 32 {
            0xFFFFFFFF
        } else {
            (1u32 << bit_count) - 1
        };
        
        Some(data & final_mask)
    }
}