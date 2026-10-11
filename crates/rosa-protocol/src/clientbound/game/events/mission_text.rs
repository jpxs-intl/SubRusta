use crate::codec::WireWrite;

/// Event 0x16: one 64-byte line of a corporation mission's text (its result line), sent only to the corporation's
/// team.
#[derive(Clone, Debug, PartialEq)]
pub struct EventMissionText {
    pub corporation: i32,
    pub mission: i32,
    pub line: i32,
    /// At most 64 bytes, sent as they are (the text is NUL-terminated within its lines).
    pub text: Vec<u8>,
}

impl WireWrite for EventMissionText {
    fn write(&self, w: &mut crate::codec::Writer) {
        let len = self.text.len().min(64);
        w.bits((self.mission << 8) + self.corporation, 16);
        w.bits(self.line, 6);
        w.bits(len as i32, 7);
        w.bytes(&self.text[..len]);
    }
}
