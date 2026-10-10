use crate::codec::WireWrite;

/// Rows in the admin list (the game's admin_data table, 0x50 bytes each).
pub const ADMIN_LIST_ROWS: usize = 256;

/// One filled row of the admin list: an account id, its name and phone number.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminListEntry {
    pub account_id: i32,
    pub name: String,
    pub phone_number: i32,
}

/// Packet 9 (send_adminpacket): the admin list, sent to admin connections every 128 ticks. Each row is a set bit
/// and its entry, or a clear bit.
#[derive(Debug, Clone, PartialEq)]
pub struct AdminList {
    pub rows: Vec<Option<AdminListEntry>>,
}

impl WireWrite for AdminList {
    fn write(&self, w: &mut crate::codec::Writer) {
        w.byte(0x09);
        for row in &self.rows {
            w.bits(row.is_some() as i32, 1);
            if let Some(e) = row {
                w.i32(e.account_id);
                w.string(&e.name, 32);
                w.i32(e.phone_number);
            }
        }
    }
}
