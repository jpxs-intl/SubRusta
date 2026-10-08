use glam::Vec3;
use rosa_protocol::clientbound::game::ItemKind;

use rosa_physics::RigidBodies;

use super::{Human, InventorySlot, QueuedAction};
use crate::sim::items::Touchables;

const PICKUP_REACH: f32 = 1.5;
const PICKUP_RADIUS: f32 = 0.5;
const HAND_DROP_DISTANCE: f32 = 1.75;

/// point_segment_distance: how far `p` is from the segment `a`..`b`.
pub fn point_segment_distance(a: Vec3, b: Vec3, p: Vec3) -> f32 {
    let e = Vec3::new(b.x - a.x, b.y - a.y, b.z - a.z);
    if ((p.x - b.x) * e.x + (p.y - b.y) * e.y) + (p.z - b.z) * e.z >= 0.0 {
        let d = Vec3::new(b.x - p.x, b.y - p.y, b.z - p.z);
        return ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
    }
    let w = Vec3::new(p.x - a.x, p.y - a.y, p.z - a.z);
    let d = if 0.0 >= (e.x * w.x + e.y * w.y) + e.z * w.z {
        Vec3::new(a.x - p.x, a.y - p.y, a.z - p.z)
    } else {
        let len = ((e.x * e.x + e.y * e.y) + e.z * e.z).sqrt();
        let u = if len == 0.0 {
            Vec3::ZERO
        } else {
            let inv = 1.0 / len;
            Vec3::new(e.x * inv, e.y * inv, e.z * inv)
        };
        let t = -((w.y * u.y + w.x * u.x) + w.z * u.z);
        Vec3::new(a.x - (p.x + u.x * t), a.y - (p.y + u.y * t), a.z - (p.z + u.z * t))
    };
    ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt()
}

/// link_item with a human parent: puts the item into the human's inventory slot, taking it out of wherever it was.
/// A hand holds one item and a pocket up to eight.
pub fn link_item_to_human(h: &mut Human, human_id: usize, bodies: &mut RigidBodies, touch: &mut Touchables, item_id: usize, slot: usize) -> bool {
    let count = h.inventory[slot].count;
    if count > 1 || (slot <= 1 && count == 1) {
        return false;
    }
    let Some(item) = touch.items.get(item_id) else { return false };
    let is_gun = touch.types[item.item_type as usize].is_gun;
    if (slot == 2 && !is_gun) || (slot > 2 && is_gun) {
        return false;
    }
    // TODO: item_removelink when the item hangs off another item
    if item.parent_human != -1 {
        // TODO: item_detach_human from another human
        if item.parent_human as usize == human_id {
            detach_item(h, bodies, touch, item_id, item.parent_slot as usize);
        }
    }
    let item = touch.items.get_mut(item_id).unwrap();
    item.parent_item = -1;
    let n = h.inventory[slot].count;
    if n <= 7 {
        h.inventory[slot].count = n + 1;
        item.parent_human = human_id as i32;
        item.parent_slot = slot as i32;
        h.inventory[slot].items[n as usize] = item_id as i32;
    }
    true
}

/// link_item with no parent: drops the item from the human's inventory.
pub fn unlink_item(h: &mut Human, bodies: &mut RigidBodies, touch: &mut Touchables, item_id: usize) {
    let Some(item) = touch.items.get(item_id) else { return };
    if item.parent_human != -1 {
        detach_item(h, bodies, touch, item_id, item.parent_slot as usize);
    }
}

/// item_detach_human: takes the item out of the slot (the last item takes its place); items leaving a pocket appear
/// in front of the chest.
fn detach_item(h: &mut Human, bodies: &mut RigidBodies, touch: &mut Touchables, item_id: usize, slot: usize) {
    if slot > 1 {
        let chest = &h.bones[2];
        let (p, r1) = (chest.pos, chest.rot[1]);
        // the binary also sets the item's previous position to the chest, which its next position sync overwrites
        let at = Vec3::new(r1.x * -0.25 + p.x, r1.y * -0.25 + p.y, -0.25 * r1.z + p.z);
        if let Some(item) = touch.items.get_mut(item_id) {
            if let Some(body) = bodies.get_mut(item.body) {
                body.pos = at;
            }
            item.pos2 = at;
        }
    }
    let s: &mut InventorySlot = &mut h.inventory[slot];
    let mut i = 0;
    while i < s.count {
        if s.items[i as usize] == item_id as i32 {
            s.count -= 1;
            if let Some(item) = touch.items.get_mut(item_id) {
                item.parent_human = -1;
            }
            s.items[i as usize] = s.items[s.count as usize];
        } else {
            i += 1;
        }
    }
}

/// The hand part of human_update_hand_grab_and_inventory: a hand lets go of an item that is too far away, or when
/// the human is badly hurt, and everything carried is kept awake.
pub fn hand_grab_and_inventory(h: &mut Human, human_id: usize, bodies: &mut RigidBodies, touch: &mut Touchables) {
    for (slot, hand) in [(0, 9), (1, 6)] {
        while h.inventory[slot].count > 0 {
            let item_id = h.inventory[slot].items[0] as usize;
            let Some(item) = touch.items.get(item_id) else { break };
            let (a, b) = (h.bones[hand].pos, item.pos2);
            let d = Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
            if !(((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt() <= HAND_DROP_DISTANCE) {
                unlink_item(h, bodies, touch, item_id);
                continue;
            }
            if h.health <= 49 {
                unlink_item(h, bodies, touch, item_id);
            }
            break;
        }
        // TODO: grabbing other humans and vehicles, the use key and item input flags
    }
    if h.input_flags & 0x2000 != 0 && h.last_input_flags & 0x2000 == 0 {
        swap_hands(h, human_id, bodies, touch);
    }
    for slot in &h.inventory {
        for &item_id in &slot.items[..slot.count as usize] {
            if let Some(item) = touch.items.get_mut(item_id as usize) {
                item.physics_settled = false;
                // TODO: carrying a gun ends spawn protection
            }
        }
    }
}

/// The hand swap key (input 0x2000): the right hand's item goes to the left hand and the left hand's to the right.
fn swap_hands(h: &mut Human, human_id: usize, bodies: &mut RigidBodies, touch: &mut Touchables) {
    let (right, left) = (h.inventory[0].count, h.inventory[1].count);
    if right > 0 {
        let a = h.inventory[0].items[0] as usize;
        if left != 0 {
            let b = h.inventory[1].items[0] as usize;
            unlink_item(h, bodies, touch, a);
            link_item_to_human(h, human_id, bodies, touch, b, 0);
        }
        link_item_to_human(h, human_id, bodies, touch, a, 1);
    } else if left > 0 {
        let b = h.inventory[1].items[0] as usize;
        link_item_to_human(h, human_id, bodies, touch, b, 0);
    }
}

/// Queues a player's inventory action (the action type 3 part of logic_playerinteractions).
pub fn queue_inventory_action(h: &mut Human, a: i32, b: i32) {
    let q = h.actions_queued as usize;
    match a {
        3..=7 => {
            h.actions[q] = QueuedAction { kind: 2, progress: 0.0, slot: b, arg: a - 3 };
        }
        8 => h.actions[q] = QueuedAction { kind: 3, progress: 0.0, slot: b, arg: 0 },
        1 => h.actions[q] = QueuedAction { kind: 1, progress: 0.0, slot: b, arg: 0 },
        2 => {
            // TODO: the binary also keeps 8 times a human float (physScratch + 4) as this action's argument; the drop
            // action never reads it
            h.actions[q] = QueuedAction { kind: 0, progress: 0.0, slot: b, arg: 0 };
        }
        _ => return,
    }
    h.actions_queued = (h.actions_queued + 1) & 7;
}

/// human_action_simulation: runs the oldest queued inventory action.
pub fn action_simulation(h: &mut Human, human_id: usize, bodies: &mut RigidBodies, touch: &mut Touchables) {
    h.action_type = -1;
    if h.actions_finished == h.actions_queued {
        return;
    }
    let f = h.actions_finished as usize;
    let a = h.actions[f];
    h.action_progress = a.progress;
    h.action_type = a.kind;
    h.action_duration = (a.progress * 100.0) as i32;
    match a.kind {
        0 => {
            if !drop(h, bodies, touch, f) {
                return;
            }
        }
        2 => {
            if !move_item(h, human_id, bodies, touch, f) {
                return;
            }
        }
        3 => pickup(h, human_id, bodies, touch, f),
        // TODO: loading and unloading magazines (1) and using a computer (4)
        _ => {}
    }
    h.actions_finished = (h.actions_finished + 1) & 7;
}

/// human_action_possible for action 2: a full hand can put its item in the pocket when the item fits there and the
/// pocket holds at most one item; an empty hand can take from a pocket that holds something.
fn can_move(h: &Human, touch: &Touchables, hand: i32, pocket: i32) -> bool {
    let held = &h.inventory[hand as usize];
    let in_pocket = h.inventory[(pocket + 2) as usize].count;
    if held.count > 0 {
        let Some(item) = touch.items.get(held.items[0] as usize) else { return false };
        touch.types[item.item_type as usize].pockets[pocket as usize] != 0 && in_pocket <= 1
    } else {
        // TODO: a hand holding on to something (grab state, record 0x6c48 + hand * 0x38) cannot take from a pocket
        in_pocket > 0
    }
}

/// Action 2: moves the item in a hand into a pocket, or the pocket's item into an empty hand, over 15 ticks. Hands 2
/// and 3 pick one: the right hand for a magazine in the pocket, otherwise the left, preferring a full hand.
/// Returns whether the action is finished.
fn move_item(h: &mut Human, human_id: usize, bodies: &mut RigidBodies, touch: &mut Touchables, f: usize) -> bool {
    let (slot, pocket) = (h.actions[f].slot, h.actions[f].arg);
    if !(0..5).contains(&pocket) {
        return true;
    }
    let hand = if slot <= 1 {
        if slot == -1 {
            h.action_hand = -1;
            h.action_slot = pocket;
            return true;
        }
        slot
    } else {
        let mut hand = 1;
        let s = &h.inventory[(pocket + 2) as usize];
        if s.count > 0 {
            let kind = touch.items.get(s.items[0] as usize).map_or(0, |i| i.item_type as i32);
            let magazine = touch.types[kind as usize].magazine_ammo > 0 && (kind - 0xd) as u32 > 1;
            hand = (magazine as i32) ^ 1;
        }
        if h.inventory[hand as usize].count == 0 {
            hand ^= 1;
        }
        if slot == 2 {
            hand ^= 1;
        }
        if !can_move(h, touch, hand, pocket) {
            hand ^= 1;
            if !can_move(h, touch, hand, pocket) {
                h.actions[f].slot = -1;
                h.action_hand = -1;
                h.action_slot = pocket;
                return true;
            }
        }
        h.actions[f].slot = hand;
        hand
    };
    let progress = 0.06666667 + h.actions[f].progress;
    h.actions[f].progress = progress;
    h.action_hand = hand;
    h.action_slot = pocket;
    if can_move(h, touch, hand, pocket) {
        if progress < 1.0 {
            return false;
        }
    } else {
        h.actions[f].progress = 1.0;
    }
    let held = h.inventory[hand as usize];
    if held.count > 0 {
        let item = held.items[0] as usize;
        let kind = touch.items.get(item).map_or(0, |i| i.item_type as usize);
        if touch.types[kind].pockets[pocket as usize] != 0 && item <= 0x3ff {
            link_item_to_human(h, human_id, bodies, touch, item, (pocket + 2) as usize);
        }
    } else if held.count == 0 {
        let s = h.inventory[(pocket + 2) as usize];
        if s.count > 0 && s.items[0] <= 0x3ff {
            link_item_to_human(h, human_id, bodies, touch, s.items[0] as usize, hand as usize);
        }
    }
    true
}

/// Action 0, drop or throw: the throw pitch built up while aiming swings back towards zero, and the hand lets go
/// once it is inside the release window, leaving the item with the swing's speed. Slots 2 and 3 pick a hand.
/// Returns whether the action is finished.
fn drop(h: &mut Human, bodies: &mut RigidBodies, touch: &mut Touchables, f: usize) -> bool {
    let mut slot = h.actions[f].slot;
    if slot > 1 {
        let first = (slot != 2) as i32;
        let pick = if h.inventory[first as usize].count == 0 { (slot == 2) as i32 } else { first };
        if h.inventory[first as usize].count == 0 && h.inventory[pick as usize].count == 0 {
            h.actions[f].slot = -1;
            return true;
        }
        slot = pick;
        h.actions[f].slot = slot;
    } else if slot == -1 {
        return true;
    }
    let tp = h.throw_pitch as f64;
    if !(tp <= -0.196349540849375) && !(0.0981747704246875 <= tp) {
        let s = &h.inventory[slot as usize];
        if s.count > 0 {
            let item = s.items[0] as usize;
            unlink_item(h, bodies, touch, item);
        }
        return true;
    }
    h.throw_pitch = if -0.159_534_001_940_117_2 > tp {
        (tp + 0.159_534_001_940_117_2) as f32
    } else if tp <= 0.14726215563703127 {
        0.0
    } else {
        (tp - 0.14726215563703127) as f32
    };
    false
}

/// Action 3: picks up the nearest free item along the line of sight, into the requested hand (3 picks one).
fn pickup(h: &mut Human, human_id: usize, bodies: &mut RigidBodies, touch: &mut Touchables, f: usize) {
    if h.vehicle.is_some() {
        return;
    }
    let head = &h.bones[3];
    let (p, r2) = (head.pos, head.rot[2]);
    let end = Vec3::new(r2.x * -PICKUP_REACH + p.x, r2.y * -PICKUP_REACH + p.y, -PICKUP_REACH * r2.z + p.z);
    let mut best = PICKUP_RADIUS;
    let mut found = None;
    for (id, item) in touch.items.iter() {
        if item.parent_human != -1 || item.parent_item != -1 {
            continue;
        }
        let d = point_segment_distance(p, end, item.pos2);
        if best > d {
            best = d;
            found = Some(id);
        }
    }
    let Some(item_id) = found else {
        // TODO: with no item in reach, the human gets into the nearest vehicle seat within 1.375
        return;
    };
    let kind = touch.items.get(item_id).unwrap().item_type;
    if matches!(kind, ItemKind::Computer | ItemKind::Table | ItemKind::TableTest) {
        return;
    }
    let mut slot = h.actions[f].slot;
    if slot == 3 {
        let t = &touch.types[kind as usize];
        slot = (t.magazine_ammo > 0 && !t.is_gun) as i32;
        if (kind as u32) <= 0x24 && (0x10_0201_e000u64 >> kind as u32) & 1 != 0 {
            slot = 0;
        }
        if h.inventory[slot as usize].count > 0 {
            slot ^= 1;
        }
        h.actions[f].slot = slot;
    }
    if !(0..super::INVENTORY_SLOTS as i32).contains(&slot) || h.inventory[slot as usize].count != 0 {
        return;
    }
    link_item_to_human(h, human_id, bodies, touch, item_id, slot as usize);
}
