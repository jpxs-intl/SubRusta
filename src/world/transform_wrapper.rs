use rapier3d::prelude::{RigidBody, RigidBodyHandle};

use crate::{app_state::AppState, world::{quaternion::Quaternion, transform::Transform, vector::Vector}};

#[derive(Debug)]
pub struct WrappedTransform {
    phys_transform: Option<RigidBodyHandle>,
    norm_transform: Option<Transform>,
}

impl WrappedTransform {
    pub fn new(rigidbody: Option<RigidBodyHandle>) -> Self {
        if rigidbody.is_some() {
            Self {
                phys_transform: rigidbody,
                norm_transform: None
            }
        } else {
            Self {
                phys_transform: None,
                norm_transform: Some(Transform::zero())
            }
        }
    }

    pub fn from_rigidbody(rigidbody: RigidBody, state: &AppState) -> Self {
        let mut writ = state.physics.rigidbodies.write().unwrap();

        Self {
            phys_transform: Some(writ.insert(rigidbody)),
            norm_transform: None
        }
    }

    pub fn get_rigidbody_handle(&self) -> Option<RigidBodyHandle> {
        self.phys_transform
    }

    pub fn pos(&self, state: &AppState) -> Vector {
        if let Some(phys_transform) = &self.phys_transform {
            let transform = state.physics.rigidbodies.read().unwrap();

            Vector::from_rapier(transform.get(*phys_transform).unwrap().translation())
        } else {
            self.norm_transform.as_ref().unwrap().pos
        }
    }

    pub fn set_pos(&mut self, pos: Vector, state: &AppState) {
        if let Some(rigidbody) = self.phys_transform {
            let mut writ = state.physics.rigidbodies.write().unwrap();
            let rigid = writ.get_mut(rigidbody).unwrap();

            rigid.set_translation(pos.to_rapier(), true);
        } else {
            self.norm_transform.as_mut().unwrap().pos = pos;
        }
    }

    pub fn rot(&self, state: &AppState) -> Quaternion {
        if let Some(phys_transform) = &self.phys_transform {
            let transform = state.physics.rigidbodies.read().unwrap();

            Quaternion::from_rapier(transform.get(*phys_transform).unwrap().rotation()).normalized()
        } else {
            self.norm_transform.as_ref().unwrap().rot.normalized()
        }
    }
}