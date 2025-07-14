use dashmap::DashMap;
use rapier3d::prelude::{Collider, ColliderHandle, RigidBody};

use crate::{app_state::AppState, connection::packets::buf_writer::AlexBufWriter, items::item_types::ItemType, world::transform_wrapper::WrappedTransform};

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
}

#[derive(Debug)]
pub struct Item {
    pub item_type: ItemType,
    pub item_id: u32,
    pub transform: WrappedTransform,
    pub collider_handle: Option<ColliderHandle>,
}

impl Item {
    pub fn create(item_type: ItemType, collider: Option<(Collider, RigidBody)>, state: &AppState) -> u32 {
        let id = state.items.next_item_id();

        let (handle, rigid) = if let Some(collider) = collider {
            let mut colliders = state.physics.colliders.write().unwrap();
            let mut rigidbodies = state.physics.rigidbodies.write().unwrap();

            let rigidbody_handle = rigidbodies.insert(collider.1);

            (Some(colliders.insert_with_parent(collider.0, rigidbody_handle, &mut rigidbodies)), Some(rigidbody_handle))
        } else {
            (None, None)
        };

        let item = Self {
            item_type,
            item_id: id,
            collider_handle: handle,
            transform: WrappedTransform::new(rigid)
        };

        state.items.items.insert(id, item);

        id
    }

    pub fn destroy(id: u32, state: &AppState) {
        if let Some(item) = state.items.items.get_mut(&id) {
            if let Some(phys) = item.transform.phys_transform {
                state.physics.destroy_object(phys);
            }
        }

        state.items.items.remove(&id);
    }

    pub fn encode_obj_header(&self, writer: &mut AlexBufWriter) {
        writer.write_bits(self.item_id as i32, 10);
        writer.write_bits(0, 2);
        writer.write_bits(1, 3);
        writer.write_bits(self.item_type as i32, 10);
        writer.write_bits(self.item_id as i32, 16);
    }

    pub fn encode_transform(&self, state: &AppState, writer: &mut AlexBufWriter) {
        self.transform.pos(state).encode_delta(writer);

        self.transform.rot(state).encode_xyz(writer);
    }

    pub fn encode_obj(&self, state: &AppState, writer: &mut AlexBufWriter) {
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