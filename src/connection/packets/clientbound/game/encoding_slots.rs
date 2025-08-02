use std::sync::{Arc, Mutex};

use dashmap::DashMap;

#[derive(PartialEq, Eq)]
pub enum EncodingSlot {
    Human(u32),
    Item(u32),
    Empty
}

#[derive(Default)]
pub struct EncodingSlots {
    pub slots: DashMap<u32, EncodingSlot>,
    slots_updated_this_tick: Arc<Mutex<Vec<u32>>>
}

impl EncodingSlots {
    pub fn new() -> Self {
        Self {
            slots: DashMap::new(),
            slots_updated_this_tick: Arc::new(Mutex::new(Vec::new()))
        }
    }

    pub fn get_slot(&self) -> u32 {
        if let Some(slot) = self.find_empty_slot() {
            slot
        } else {
            let id = self.slots.len() as u32;
            self.slots.insert(id, EncodingSlot::Empty);

            id
        }
    }

    fn find_empty_slot(&self) -> Option<u32> {
        for slot in &self.slots {
            if let EncodingSlot::Empty = slot.value() {
                return Some(*slot.key());
            }
        }

        None
    }

    pub fn update_slot(&self, slot: u32) {
        if let Ok(mut lock) = self.slots_updated_this_tick.lock()
            && !lock.contains(&slot) {
                lock.push(slot)
            }
    }

    pub fn get_updated_slots(&self) -> Vec<u32> {
        if let Ok(lock) = self.slots_updated_this_tick.lock() {
            lock.clone()
        } else {
            vec![]
        }
    }

    pub fn clear_updated_slots(&self) {
        if let Ok(mut lock) = self.slots_updated_this_tick.lock() {
            lock.clear();
        }
    }

    pub fn remove_human_by_id(&self, item_id: u32) {
        self.slots.retain(|_, v| {
            *v != EncodingSlot::Human(item_id)
        });
    }

    pub fn remove_item_by_id(&self, item_id: u32) {
        self.slots.retain(|_, v| {
            *v != EncodingSlot::Item(item_id)
        });
    }
}