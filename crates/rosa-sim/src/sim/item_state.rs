use rosa_protocol::clientbound::game::ItemKind;

use super::item_types::ItemType;
use crate::PlayerId;

/// The part of an item record that depends on its type. The binary keeps all of these in one record and reuses
/// fields across types (a radio's channel sits in the computer's line fields); here each type carries only its own.
#[derive(Clone, Debug, PartialEq)]
pub enum ItemState {
    /// Nothing beyond the common item fields.
    Plain,
    /// Something that is used up (item +0x144, starting at the type's magazine_ammo): a magazine's or the Auto5's
    /// rounds, the soccer ball's charge.
    Stock { left: i32 },
    /// A gun: rounds ready to fire (+0x144, the Auto5's own 300 shells, otherwise the round chambered from the
    /// magazine), ticks until it can fire again (+0x13c) and how long the trigger has been held (+0x14c).
    Gun { rounds: i32, cooldown: i32, trigger_ticks: i32 },
    /// Uses left (+0x144) and how far the current patching has got (+0x13c, done past 255).
    Bandage { left: i32, progress: i32 },
    /// Bites left (+0x144).
    Burger { left: i32 },
    /// The pin (+0x144, 1 while in), the fuse (+0x13c: 240 while held unpinned, counting down once thrown) and the
    /// player who pulled the pin (+0x20).
    Grenade { pin: i32, fuse: i32, primer: Option<PlayerId> },
    /// Phones and pay phones.
    Phone(Phone),
    /// A walkie-talkie: its channel (+0x368, low 7 bits) and whether the use key is held to talk (+0x36c).
    Radio { channel: i32, transmitting: bool },
    /// An arcade machine's screen: the frame counter (+0x368) and top line (+0x36c).
    Arcade { frame: i32, top_line: i32 },
    /// A car key and its vehicle (+0x280).
    Key { vehicle: Option<usize> },
    // TODO: cash (spread, bill count, values), computers, disks, doors, pay phone home positions
}

/// Where a phone is in a call (item +0x27c).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[repr(i32)]
pub enum PhoneStatus {
    #[default]
    Idle = 0,
    Dialing = 1,
    /// Calling out, or being called while not yet answered.
    Ringing = 2,
    Connected = 3,
    Busy = 4,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Phone {
    /// +0x160
    pub number: i32,
    /// +0x27c
    pub status: PhoneStatus,
    /// The phone this one called or is talking to (+0x15c).
    pub connected: Option<usize>,
    /// Ticks spent ringing or busy (+0x164).
    pub ring_timer: i32,
    /// The number on the screen (+0x168).
    pub display_number: i32,
    /// The digits dialled so far (+0x16c).
    pub entered_number: i32,
    /// Ticks before the keypad works again after a call is placed (+0x13c).
    pub cooldown: i32,
    /// +0x278
    pub texture: i32,
}

impl ItemState {
    /// The state a freshly created item of this type starts in.
    pub fn new(kind: ItemKind, ty: &ItemType) -> Self {
        let left = ty.magazine_ammo;
        match kind {
            ItemKind::Bandage => Self::Bandage { left, progress: 0 },
            ItemKind::Burger => Self::Burger { left },
            ItemKind::Grenade => Self::Grenade { pin: left, fuse: 0, primer: None },
            ItemKind::Phone | ItemKind::PhonePay => Self::Phone(Phone::default()),
            ItemKind::Radio => Self::Radio { channel: 0, transmitting: false },
            ItemKind::Arcade => Self::Arcade { frame: 0, top_line: 0 },
            ItemKind::Key => Self::Key { vehicle: None },
            _ if ty.is_gun => Self::Gun { rounds: left, cooldown: 0, trigger_ticks: 0 },
            _ if left > 0 => Self::Stock { left },
            _ => Self::Plain,
        }
    }

    /// Item +0x144: what is left of a used-up item, 0 for the rest.
    pub fn left(&self) -> i32 {
        match self {
            Self::Stock { left } | Self::Bandage { left, .. } | Self::Burger { left } => *left,
            Self::Gun { rounds, .. } => *rounds,
            Self::Grenade { pin, .. } => *pin,
            _ => 0,
        }
    }

    pub fn phone(&self) -> Option<&Phone> {
        if let Self::Phone(p) = self { Some(p) } else { None }
    }

    pub fn phone_mut(&mut self) -> Option<&mut Phone> {
        if let Self::Phone(p) = self { Some(p) } else { None }
    }
}
