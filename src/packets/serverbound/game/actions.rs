use crate::packets::buf_reader::AlexBufReader;

#[derive(Debug, Clone, PartialEq)]
pub enum ServerboundGameAction {
    Menu(ServerboundGameActionTypeMenu),
    Chat(ServerboundGameActionTypeChat),
    Item(ServerboundGameActionTypeItem),
    Inventory(ServerboundGameActionTypeInventory),
    Admin(ServerboundGameActionTypeAdmin),
    Unknown
}

pub fn decode_actions(reader: &mut AlexBufReader, num_actions: u8) -> Option<Vec<ServerboundGameAction>> {
    let mut actions = Vec::with_capacity(num_actions as usize);

    for _ in 0..num_actions {
        let action_type = reader.boundscheck_read_bits(4)? as u8;

        let action = Some(match action_type {
            0 => ServerboundGameAction::Menu(ServerboundGameActionTypeMenu { 
                a: reader.read_u8()?, 
                b: reader.read_u32()?, 
                c: reader.read_bytes(16, 1)? 
            }),
            1 => {
                let len = reader.boundscheck_read_bits(6)? as usize;
                ServerboundGameAction::Chat(ServerboundGameActionTypeChat {
                    message: reader.read_string(len)?,
                    volume: reader.boundscheck_read_bits(4)? as u8,
                })
            }
            2 => ServerboundGameAction::Item(ServerboundGameActionTypeItem {
                a: reader.boundscheck_read_bits(16)? as u16,
                b: reader.boundscheck_read_bits(16)? as u16,
            }),
            3 => ServerboundGameAction::Inventory(ServerboundGameActionTypeInventory {
                a: reader.boundscheck_read_bits(16)? as u16,
                b: reader.boundscheck_read_bits(16)? as u16,
                c: reader.boundscheck_read_bits(16)? as u16,
            }),
            4 => ServerboundGameAction::Admin(ServerboundGameActionTypeAdmin {
                a: reader.read_u32()?,
                b: reader.read_u32()?,
            }),
            _ => {
                println!("Received invalid action type {action_type:?}");

                ServerboundGameAction::Unknown
            }
        });

        if let Some(action) = action {
            actions.push(action)
        }
    }

    Some(actions)
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameActionTypeMenu {
    pub a: u8,
    pub b: u32,
    pub c: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameActionTypeChat {
    pub message: String,
    pub volume: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameActionTypeItem {
    pub a: u16,
    pub b: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameActionTypeInventory {
    pub a: u16,
    pub b: u16,
    pub c: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ServerboundGameActionTypeAdmin {
    pub a: u32,
    pub b: u32,
}
