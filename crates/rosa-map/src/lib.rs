use binrw::BinRead;
use std::fmt::Debug;

pub mod file_types;

#[derive(BinRead, Clone)]
pub struct Char64 {
    pub string: [u8; 64],
}

impl Default for Char64 {
    fn default() -> Self { Self { string: [0; 64] } }
}

impl Debug for Char64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Char64").field("string", &self.as_str()).finish()
    }
}

impl Char64 {
    pub fn new(s: &str) -> Self {
        let mut buf = [0u8; 64];
        let b = s.as_bytes();
        let n = b.len().min(64);
        buf[..n].copy_from_slice(&b[..n]);
        Self { string: buf }
    }
    pub fn as_str(&self) -> &str {
        let end = self.string.iter().position(|&b| b == 0).unwrap_or(64);
        std::str::from_utf8(&self.string[..end]).unwrap_or("")
    }
}