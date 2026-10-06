use crate::codec::{CodecError, Reader};

#[derive(Debug, Clone, PartialEq)]
pub enum GameAction {
    Menu(MenuAction),
    Chat(ChatAction),
    Item(ItemAction),
    Inventory(InventoryAction),
    Admin(AdminAction),
    Unknown
}

#[derive(Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum Menu {
    Lobby = 2,
    Other(u8)
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuAction {
    pub menu: Menu,
    pub button: u32,
    pub c: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChatAction {
    pub message: String,
    pub volume: u8,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ItemAction {
    pub a: u16,
    pub b: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct InventoryAction {
    pub a: u16,
    pub b: u16,
    pub c: u16,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AdminAction {
    pub a: u32,
    pub b: u32,
}

pub fn read_actions(r: &mut Reader, num_actions: u32) -> Result<Vec<GameAction>, CodecError> {
    let mut actions = Vec::with_capacity(num_actions as usize);

    for _ in 0..num_actions {
        let action_type = r.bits(4)? as u8;

        let action = Some(match action_type {
            0 => {
                let menu_type = match r.bits(8)? as u8 {
                    2 => Menu::Lobby,
                    other => Menu::Other(other)
                };

                GameAction::Menu(MenuAction {
                    menu: menu_type,
                    button: r.u32()?,
                    c: r.bytes(16)?.to_vec(),
                })
            },
            1 => {
                let len = r.bits(6)? as usize;

                let message = if len > 0 {
                    r.string(len)?
                } else {
                    "".to_string()
                };

                GameAction::Chat(ChatAction { message, volume: r.bits(4)? as u8 })
            },
            2 => GameAction::Item(ItemAction {
                a: r.bits(16)? as u16,
                b: r.bits(16)? as u16
            }),
            3 => GameAction::Inventory(InventoryAction {
                a: r.bits(16)? as u16,
                b: r.bits(16)? as u16,
                c: r.bits(16)? as u16
            }),
            4 | 5 => GameAction::Admin(AdminAction {
                a: r.u32()?,
                b: r.u32()?
            }),
            _ => {
                println!("Received invalid action type {action_type:?}");

                GameAction::Unknown
            }
        });

        if let Some(action) = action {
            actions.push(action)
        }
    }

    Ok(actions)
}