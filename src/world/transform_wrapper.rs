use std::sync::Arc;

use rapier3d::prelude::{RigidBody, RigidBodyHandle};

use crate::{app_state::AppState, world::{quaternion::Quaternion, transform::Transform, vector::Vector}};

#[derive(Debug, Clone)]
pub struct WrappedTransform {
    pub phys_transform: Option<RigidBodyHandle>,
    pub norm_transform: Option<Transform>,
    last_tick_updated: i32,
}

impl WrappedTransform {
    pub fn new(rigidbody: Option<RigidBodyHandle>) -> Self {
        if rigidbody.is_some() {
            Self {
                phys_transform: rigidbody,
                norm_transform: None,
                last_tick_updated: 0
            }
        } else {
            Self {
                phys_transform: None,
                norm_transform: Some(Transform::zero()),
                last_tick_updated: 0
            }
        }
    }

    pub fn updated_this_tick(&self, state: &Arc<AppState>) -> bool {
        self.last_tick_updated == state.network_tick()
    }

    pub fn sleeping(&self, state: &Arc<AppState>) -> bool {
        if let Some(phys_handle) = self.phys_transform {
            let rigidbodies = state.physics.rigidbodies.read().unwrap();

            let handle = rigidbodies.get(phys_handle);

            if let Some(handle) = handle {
                handle.is_sleeping()
            } else {
                false
            }
        } else {
            false
        }
    }

    pub fn eq(&self, transform: &WrappedTransform, state: &Arc<AppState>) -> bool {
        self.pos(state) == transform.pos(state) && self.rot(state) == transform.rot(state)
    }

    pub fn from_rigidbody(rigidbody: RigidBody, state: &Arc<AppState>) -> Self {
        let mut writ = state.physics.rigidbodies.write().unwrap();

        Self {
            phys_transform: Some(writ.insert(rigidbody)),
            norm_transform: None,
            last_tick_updated: 0
        }
    }

    pub fn get_rigidbody_handle(&self) -> Option<RigidBodyHandle> {
        self.phys_transform
    }

    pub fn pos(&self, state: &Arc<AppState>) -> Vector {
        if let Some(phys_transform) = &self.phys_transform {
            let transform = state.physics.rigidbodies.read().unwrap();

            Vector::from_rapier(transform.get(*phys_transform).unwrap().translation())
        } else {
            self.norm_transform.as_ref().unwrap().pos
        }
    }

    pub fn set_pos(&mut self, pos: Vector, state: &Arc<AppState>) {
        self.last_tick_updated = state.network_tick();

        if let Some(rigidbody) = self.phys_transform {
            let mut writ = state.physics.rigidbodies.write().unwrap();
            let rigid = writ.get_mut(rigidbody).unwrap();

            rigid.set_translation(pos.to_rapier(), true);
        } else {
            self.norm_transform.as_mut().unwrap().pos = pos;
        }
    }

    pub fn rot(&self, state: &Arc<AppState>) -> Quaternion {
        if let Some(phys_transform) = &self.phys_transform {
            let transform = state.physics.rigidbodies.read().unwrap();

            Quaternion::from_rapier(transform.get(*phys_transform).unwrap().rotation()).normalized()
        } else {
            self.norm_transform.as_ref().unwrap().rot.normalized()
        }
    }
}