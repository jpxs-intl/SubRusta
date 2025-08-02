use std::sync::Arc;

use dashmap::DashMap;
use rapier3d::prelude::ColliderHandle;

use crate::{app_state::AppState, connection::packets::{buf_writer::AlexBufWriter, clientbound::game::encoding_slots::EncodingSlot}, items::item_types::{ItemColliders, ItemType}, world::{transform_wrapper::WrappedTransform, vector::Vector}};

pub mod item_types;

#[derive(Default)]
pub struct ItemManager {
    pub items: DashMap<u32, Item>
}

impl ItemManager {
    pub fn next_item_id(&self) -> u32 {
        for i in 0..64 {
            if !self.items.contains_key(&i) {
                return i;
            }
        }

        0
    }

    pub fn tick(&self, state: &Arc<AppState>) {
        for mut item in self.items.iter_mut() {
            if item.ticking {
                item.tick(state);
            }
        }
    }
}

#[derive(Debug)]
pub struct Item {
    pub item_type: ItemType,
    pub ticking: bool,
    pub item_id: u32,
    pub transform: WrappedTransform,
    pub collider_handle: Option<ColliderHandle>,
    encoding_slot: Option<u32>
}

impl Item {
    pub fn create(item_type: ItemType, transform: Vector, state: &Arc<AppState>) -> u32 {
        let id = state.items.next_item_id();

        let (handle, rigid) = if let Some(collider) = item_type.create_collider() && let Some(rigidbody) = item_type.create_rigidbody(transform) {
            let mut colliders = state.physics.colliders.write().unwrap();
            let mut rigidbodies = state.physics.rigidbodies.write().unwrap();

            let rigidbody_handle = rigidbodies.insert(rigidbody);

            (Some(colliders.insert_with_parent(collider, rigidbody_handle, &mut rigidbodies)), Some(rigidbody_handle))
        } else {
            (None, None)
        };

        let item = Self {
            item_type,
            item_id: id,
            ticking: true,
            collider_handle: handle,
            transform: WrappedTransform::new(rigid),
            encoding_slot: Some(state.encoding_slots.get_slot())
        };

        state.encoding_slots.slots.insert(item.encoding_slot.unwrap(), EncodingSlot::Item(id));
        item.update(state);
        state.items.items.insert(id, item);

        id
    }

    pub fn tick(&mut self, state: &Arc<AppState>) {
        if !self.transform.updated_this_tick(state) || !self.transform.sleeping(state) {
            self.update(state);
        }

        let mut pos = self.transform.pos(state);

        if pos.y <= 0.0 {
            pos.y += 1000.0;

            self.transform.set_pos(pos, state);
            self.update(state);
        }
    }

    // TODO: Actually make this disable the slot.
    pub fn destroy(id: u32, state: &Arc<AppState>) {
        if let Some(item) = state.items.items.get_mut(&id) && let Some(phys) = item.transform.phys_transform {
                state.physics.destroy_object(phys);
            }

        state.encoding_slots.remove_item_by_id(id);
        state.items.items.remove(&id);
    }

    pub fn update(&self, state: &Arc<AppState>) {
        if let Some(encoding_slot) = self.encoding_slot {
            println!("Updating item!");
            state.encoding_slots.update_slot(encoding_slot);
        }
    }

    pub fn encode_obj_header(&self, slot: i32, writer: &mut AlexBufWriter) {
        writer.write_bits(slot, 10);
        writer.write_bits(0, 2);
        writer.write_bits(1, 3);
        writer.write_bits(self.item_type as i32, 10);
        writer.write_bits(self.item_id as i32, 16);
    }

    pub fn encode_transform(&self, state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        self.transform.pos(state).encode_delta(writer);

        self.transform.rot(state).encode_xyz(writer);
    }

    pub fn encode_obj(&self, state: &Arc<AppState>, writer: &mut AlexBufWriter) {
        writer.write_bits(1, 1);
        writer.write_bits(1, 1);

        writer.write_bits(self.item_type as i32, 8);
        writer.write_bits(-1, 10);
        writer.write_bits(-1, 10);
        writer.write_bits(0, 4);
        
        writer.write_bits(self.item_id as i32, 8);

        self.encode_transform(state, writer);
    }
}

impl ItemManager {
    pub fn new() -> Self {
        Self {
            items: DashMap::new()
        }
    }
}