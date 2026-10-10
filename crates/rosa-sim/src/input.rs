//! The client's keys (ClientGamePacket input_flags) as masks over the raw u32 humans and bots keep,
//! and the use flags a human sets on the item in its hand.
use rosa_protocol::serverbound::game::InputFlags;

pub const LEFT_MOUSE: u32 = InputFlags::LEFT_MOUSE.bits();
pub const RIGHT_MOUSE: u32 = InputFlags::RIGHT_MOUSE.bits();
pub const JUMP: u32 = InputFlags::JUMP.bits();
pub const CROUCH: u32 = InputFlags::CROUCH.bits();
pub const SHIFT: u32 = InputFlags::SHIFT.bits();
pub const DROP_ITEM: u32 = InputFlags::DROP_ITEM.bits();
pub const MODE_KEYS: u32 = InputFlags::MODE_KEYS.bits();
pub const GRAB: u32 = InputFlags::GRAB.bits();
pub const RELOAD: u32 = InputFlags::RELOAD.bits();
pub const SWITCH_HANDS: u32 = InputFlags::SWITCH_HANDS.bits();
pub const RAGDOLL: u32 = InputFlags::RAGDOLL.bits();

/// The item is being used (left mouse with this hand).
pub const ITEM_USE: u32 = 1;
/// The item's second action (reload).
pub const ITEM_SECONDARY: u32 = 2;
