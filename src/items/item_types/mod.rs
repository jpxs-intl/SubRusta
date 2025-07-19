use mlua::{FromLua, Lua, Value};
use rapier3d::prelude::*;
use serde::Deserialize;

use crate::world::vector::Vector;

pub trait ItemColliders {
    fn create_collider(&self) -> Option<Collider> {Some(ColliderBuilder::cuboid(0.5, 0.5, 0.5).build())}

    fn create_rigidbody(&self, transform: Vector) -> Option<RigidBody> {Some(RigidBodyBuilder::dynamic().translation(vector![transform.x, transform.y, transform.z]).build())}
}

#[derive(Default, Debug, Clone, Copy, Deserialize)]
pub enum ItemType {
    Watermelon = 45,
    Box = 37,
    BigBox = 38,
    #[default]
    Unknown = 0
}

impl ItemColliders for ItemType {
    fn create_collider(&self) -> Option<Collider> {
        match self {
            ItemType::Watermelon => Some(ColliderBuilder::capsule_y(0.10, 0.20).density(2.0).restitution(0.0).friction(0.78).build()),
            ItemType::Box => None,
            ItemType::BigBox => None,
            ItemType::Unknown => None,
        }
    }

    fn create_rigidbody(&self, transform: Vector) -> Option<RigidBody> {
        match self {
            ItemType::Watermelon => Some(RigidBodyBuilder::dynamic().translation(vector![transform.x, transform.y, transform.z]).angular_damping(0.8).linear_damping(0.2).build()),
            ItemType::Box => None,
            ItemType::BigBox => None,
            ItemType::Unknown => None,
        }
    }
}

impl FromLua for ItemType {
    fn from_lua(value: Value, _lua: &Lua) -> mlua::Result<Self> {
        match value {
            Value::Integer(n) => match n {
                45 => Ok(ItemType::Watermelon),
                _ => Ok(ItemType::Watermelon)
            },
            _ => Err(mlua::Error::FromLuaConversionError { from: value.type_name(), to: "ItemType".to_string(), message: Some("Expected integer for itme type".to_string()) })
        }
    }
}