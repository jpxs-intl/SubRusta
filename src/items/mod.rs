use dashmap::DashMap;
use rapier3d::prelude::*;

use crate::{app_state::AppState, connection::packets::buf_writer::AlexBufWriter, items::item_types::ItemType, world::{transform_wrapper::WrappedTransform, vector::IntVector}};

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

    pub fn tick(&self, state: &AppState) {
        for item in &self.items {
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
            ticking: true,
            collider_handle: handle,
            transform: WrappedTransform::new(rigid)
        };

        state.items.items.insert(id, item);

        id
    }

    pub fn tick(&self, state: &AppState) {
        /*let pos = self.transform.pos(state);

        let block_pos = pos / 4.0;

        for x in (block_pos.x.round() as i32 - 1)..=(block_pos.x.round() as i32 + 1) {
            for y in (block_pos.y.round() as i32 - 1)..=(block_pos.y.round() as i32 + 1) {
                for z in (block_pos.z.round() as i32 - 1)..=(block_pos.z.round() as i32 + 1) {
                    if state.map.added_coords.contains_key(&(x, y, z)) {
                        continue;
                    } 

                    let block_type = state.map.get_blocktype_at_blockpos(IntVector { x: x as u32, y: y as u32, z: z as u32 });

                    if let Some(typep) = block_type {
                        let block = state.map.get_block_in_csx(&typep.name.string());

                        if let Some(block) = block {
                            let rapier = block.to_rapier();

                            let mesh = ColliderBuilder::trimesh(rapier.0, rapier.1);
                            if let Ok(mesh) = mesh {
                                state.physics.insert_collider(mesh.translation(vector![x as f32 * 4.0, y as f32 * 4.0, z as f32 * 4.0]).build());
                            }

                            state.map.added_coords.insert((x, y, z), true);
                        } else if typep.name.string() == "nblock" {
                            // TODO: Diagnose this shit, I have NO idea why its like this.
                            // Its just a empty cube :shrug:
                            let cube = ColliderBuilder::cuboid(2.0, 2.0, 2.0).translation(vector![(x as f32 * 4.0) + 2.0, (y as f32 * 4.0) + 2.0, (z as f32 * 4.0) + 2.0]).build();

                            state.physics.insert_collider(cube);

                            state.map.added_coords.insert((x, y, z), true);
                        }
                    }
                }
            }
        }*/
    }

    pub fn destroy(id: u32, state: &AppState) {
        if let Some(item) = state.items.items.get_mut(&id) && let Some(phys) = item.transform.phys_transform {
                state.physics.destroy_object(phys);
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