use std::sync::Arc;

use mlua::{Lua, Table};

use crate::{app_state::AppState, items::{item_types::ItemType, Item}, world::vector::Vector};

pub fn make_lua_api(lua: &Lua, state: Arc<AppState>) {
    let item_table = lua.create_table().unwrap();

    item_table.set("create", lua.create_function(move |_, (item_type, pos): (ItemType, Vector)| {
        let id = Item::create(item_type, pos, &state);

        Ok(id)
    }).unwrap()).unwrap();

    lua.globals().get::<Table>("_").unwrap().set("Items", item_table).unwrap();
}