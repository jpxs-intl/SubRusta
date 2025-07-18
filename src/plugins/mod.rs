use std::{fs::{self, File}, io::Read, path::Path};

use mlua::{ffi::lua, Lua};

#[derive(Default)]
pub struct PluginManager {
    pub lua: Lua
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            lua: Lua::new()
        }
    }

    pub fn add_globals(&self) {
        let print_func = self.lua.create_function(|_, data: String| {
            println!("[LUA] {data}");

            Ok(())
        }).unwrap();

        self.lua.globals().set("print", print_func);
    }

    pub fn load_plugins(&self) {
        self.add_globals();

        let plugins_folder = Path::new("./plugins");

        if !plugins_folder.exists() {
            fs::create_dir("./plugins").unwrap();
        }

        let mut lua_files = vec![];

        for file in fs::read_dir("./plugins").unwrap() {
            if let Ok(ref file) = file && file.file_name().to_string_lossy().ends_with(".lua") {
                lua_files.push(file.file_name().into_string().unwrap().clone())
            }
        }

        for lua_file in lua_files {
            let plugin_data = fs::read_to_string(format!("./plugins/{lua_file}")).unwrap();

            let err = self.lua.load(plugin_data).exec();
            println!("{:?}", err);
        }
    }
}