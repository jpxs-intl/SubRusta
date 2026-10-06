use crate::codec::WireWrite;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChatType {
    Announce = 0,
    Chat = 1,
    ItemSpeak = 2,
    EliminatorAnnouncement = 3,
    AdminChat = 4,
    PrivateMessage = 6
}

#[derive(Clone, Debug, PartialEq)]
pub struct EventChat {
    pub chat_type: ChatType,
    pub message: String,
    pub speaker_id: i32,
    pub volume: i32
}

impl WireWrite for EventChat {
    fn write(&self, w: &mut crate::codec::Writer) {
        let bytes = self.message.as_bytes();
        let len = bytes.len().min(0x3f);

        w.bits(len as i32, 6);
        w.bits(self.chat_type as i32, 4);
        w.bits(self.speaker_id, 10);
        w.bits(self.volume, 4);

        for &b in &bytes[..len] { w.bits(b as i32, 7); }
    }
}