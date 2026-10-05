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

    #[inline]
    fn take<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        Ok(self.bytes(N)?.try_into().unwrap())
    }

    pub fn u8(&mut self) -> Result<u8, CodecError> { Ok(self.take::<1>()?[0]) }
    pub fn u32(&mut self) -> Result<u32, CodecError> { Ok(u32::from_le_bytes(self.take::<4>()?)) }
    pub fn u64(&mut self) -> Result<u64, CodecError> { Ok(u64::from_le_bytes(self.take::<8>()?)) }
    pub fn f32(&mut self) -> Result<f32, CodecError> { Ok(f32::from_le_bytes(self.take::<4>()?)) }

    pub fn string(&mut self, len: usize) -> Result<String, CodecError> {
        Ok(
            String::from_utf8(self.bytes(len)?.into()).unwrap_or("".to_string()).split("\0").collect::<Vec<&str>>().first().unwrap().to_string()
        )
    }
}