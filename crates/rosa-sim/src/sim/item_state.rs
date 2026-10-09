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
    /// A stack of world cash.
    Cash(Cash),
    // TODO: computers, disks, doors, pay phone home positions
}

/// What each bill code is worth (raw 0x2e9e80).
pub const BILL_VALUES: [i32; 8] = [1, 5, 10, 20, 50, 100, 1000, 0];

/// A stack of up to ten bills: one less than the count (+0x2a4), which bill is picked out (+0x2a0) and each bill's
/// code, three bits apiece from the bottom (+0x2a8).
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Cash {
    pub spread: i32,
    pub bills: i32,
    pub codes: u32,
}

impl Cash {
    /// The code of bill `k`.
    pub fn code(&self, k: i32) -> u32 {
        (self.codes >> ((k * 3) & 31)) & 7
    }

    /// What the stack is worth.
    pub fn value(&self) -> i32 {
        (0..=self.bills).map(|k| BILL_VALUES[self.code(k) as usize]).sum()
    }

    /// cash_separate: puts a bill of `code` in at `index` (on top past the end), moving the bills above it up; a
    /// stack holds ten.
    pub fn insert(&mut self, index: i32, code: u32) -> bool {
        let c = self.bills;
        if c > 8 {
            return false;
        }
        self.bills = c + 1;
        let mut v = self.codes;
        let pos = if c + 1 <= index {
            3 * (c + 1)
        } else {
            let mut k = 3 * c;
            loop {
                let mask = !(7u32 << ((k + 3) & 31)) & v;
                v = (((v >> (k & 31)) & 7) << ((k + 3) & 31)) | mask;
                k -= 3;
                if k == 3 * index - 3 {
                    break;
                }
            }
            3 * index
        };
        v = (v & !(7u32 << (pos & 31))) | (code << (pos & 31));
        self.spread += 1;
        self.codes = v;
        true
    }

    /// cash_combine: takes out bill `index`, moving the bills above it down. Returns false when it was the last
    /// bill, which leaves the stack to despawn.
    pub fn remove(&mut self, index: i32) -> bool {
        let c = self.bills;
        if c == 0 {
            return false;
        }
        if c < 0 {
            return true;
        }
        self.bills = c - 1;
        if c - 1 < index {
            return true;
        }
        let (mut v, mut k) = (self.codes, 3 * index);
        loop {
            let mask = !(7u32 << (k & 31)) & v;
            v = (((mask >> ((k + 3) & 31)) & 7) << (k & 31)) | mask;
            k += 3;
            if k == 3 * c {
                break;
            }
        }
        self.codes = v;
        true
    }
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
            ItemKind::CashWorld => Self::Cash(Cash::default()),
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

    pub fn cash(&self) -> Option<&Cash> {
        if let Self::Cash(c) = self { Some(c) } else { None }
    }

    pub fn cash_mut(&mut self) -> Option<&mut Cash> {
        if let Self::Cash(c) = self { Some(c) } else { None }
    }

    pub fn phone(&self) -> Option<&Phone> {
        if let Self::Phone(p) = self { Some(p) } else { None }
    }

    pub fn phone_mut(&mut self) -> Option<&mut Phone> {
        if let Self::Phone(p) = self { Some(p) } else { None }
    }
}
