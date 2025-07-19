use mlua::Table;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct PluginInfo {
    pub public_name: String,
    pub internal_name: String,
    pub author: String,
    pub version: String,
}

impl PluginInfo {
    pub fn parse(table: Table) -> Option<PluginInfo> {
        Some(Self {
            public_name: table.get("public_name").ok()?,
            internal_name: table.get("internal_name").ok()?,
            author: table.get("author").ok()?,
            version: table.get("version").ok()?
        })
    }
}