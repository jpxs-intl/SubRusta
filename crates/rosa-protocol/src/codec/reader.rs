use crate::codec::CodecError;

pub struct Reader<'a> { buf: &'a [u8], pos: usize, bit_pos: usize }

impl<'a> Reader<'a> {
    pub fn new(buf: &'a [u8]) -> Self { Self { buf, pos: 0, bit_pos: 0 } }

    #[inline]
    fn align(&mut self) {
        if self.bit_pos != 0 {
            self.pos += 1;
            self.bit_pos = 0;
        }
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        self.align();
        let end = self.pos.checked_add(n).ok_or(CodecError::EoF)?;
        if end > self.buf.len() {
            return Err(CodecError::EoF);
        }
        let slice = &self.buf[self.pos..end];
        self.pos = end;
        Ok(slice)
    }

    pub fn bits(&mut self, count: u32) -> Result<u32, CodecError> {
        if count == 0 { return Ok(0); }

        let abs = self.pos * 8 + self.bit_pos;
        if abs + count as usize > self.buf.len() * 8 {
            return Err(CodecError::EoF);
        }

        let mut read = 0u32;
        let mut remaining = count;

        let first = remaining.min(8 - self.bit_pos as u32);
        let mut data = (self.buf[self.pos] as u32 >> self.bit_pos) & ((1u32 << first) - 1);
        remaining -= first;
        read += first;
        self.bit_pos += first as usize;
        if self.bit_pos >= 8 { self.bit_pos = 0; self.pos += 1; }

        while remaining >= 8 {
            data |= (self.buf[self.pos] as u32) << read;
            remaining -= 8;
            read += 8;
            self.pos += 1;
        }

        if remaining > 0 {
            data |= ((self.buf[self.pos] as u32) & ((1u32 << remaining) - 1)) << read;
            self.bit_pos = remaining as usize;
        }

        Ok(data)
    }

    #[inline]
    fn take<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> Result<u8, CodecError> { Ok(self.take::<1>()?[0]) }
    pub fn u16(&mut self) -> Result<u16, CodecError> { Ok(u16::from_le_bytes(self.take::<2>()?)) }
    pub fn u32(&mut self) -> Result<u32, CodecError> { Ok(u32::from_le_bytes(self.take::<4>()?)) }
    pub fn u64(&mut self) -> Result<u64, CodecError> { Ok(u64::from_le_bytes(self.take::<8>()?)) }
    pub fn f32(&mut self) -> Result<f32, CodecError> { Ok(f32::from_le_bytes(self.take::<4>()?)) }

    pub fn fixed_float(&mut self) -> Result<f32, CodecError> {
        let raw = self.bits(24)?;
        let sign = 1u32 << 23;
        let mag = sign - 1;
        let angle = (raw & mag) as f32 / mag as f32 * std::f32::consts::TAU;
        Ok(if raw & sign != 0 { -angle } else { angle })
    }

    pub fn string(&mut self, len: usize) -> Result<String, CodecError> {
        Ok(
            String::from_utf8(self.bytes(len)?.into()).unwrap_or("".to_string()).split("\0").collect::<Vec<&str>>().first().unwrap().to_string()
        )
    }
}