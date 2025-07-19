use crate::{app_state::AppState, plugins::types::PluginInfo};
use dashmap::DashMap;
use mlua::{
    Error, Function, Lua, LuaOptions, StdLib, Table, Value::{self, Nil},
};
use std::{
    fs::{self}, path::Path, sync::Arc
};

pub mod types;
pub mod api;

pub struct Plugin {
    pub lua: Lua,
    pub ticking: bool,
    pub did_init: bool,
    pub info: PluginInfo,
}

#[derive(Default)]
pub struct PluginManager {
    pub plugins: DashMap<String, Plugin>,
}

impl PluginManager {
    pub fn new() -> Self {
        Self { plugins: DashMap::new() }
    }

    pub fn add_globals(&self, plugin: &Lua, state: &Arc<AppState>) {
        plugin.globals().set("_", plugin.create_table().unwrap()).unwrap();

        self.mandatory_globals(plugin);
        api::make_lua_api(plugin, state.clone());

        let _ = plugin.globals().set(
            "print",
            plugin
                .create_function(move |_, data: String| {
                    println!("[LUA] {data}");

                    Ok(())
                })
                .unwrap(),
        );
    }

    pub fn mandatory_globals(&self, lua: &Lua) {
        lua.globals().set("_LOADED", lua.create_table().unwrap()).unwrap();

        lua.globals().set("print", Value::Nil).unwrap();

        let _ = lua.globals().set(
            "require",
            lua.create_function(move |lua, name: String| {
                let globals = lua.globals();
                let loaded: Table = globals.get("_LOADED")?;

                if let Ok(cached) = loaded.get::<Value>(name.clone())
                    && !matches!(cached, Nil)
                {
                    return Ok(cached);
                }

                let mut module_path = name.clone();

                module_path = module_path.replace("../", "");
                module_path = module_path.replace("./", "");

                let dir = format!("./plugins/{module_path}.lua");
                let file_path = Path::new(dir.as_str());

                if !file_path.exists() {
                    return Err(Error::RuntimeError(format!("Module '{module_path}' not found!")));
                }

                let source = fs::read_to_string(file_path).map_err(|_| Error::RuntimeError(format!("Error reading module '{module_path}'!")))?;

                let lua_data = lua.load(&source).set_name(&name);
                let result = lua_data.call::<Value>(())?;

                loaded.set(name, result.clone())?;

                Ok(result)
            })
            .unwrap(),
        );
    }

    pub fn tick(&self, state: &Arc<AppState>) {
        for mut plugin in self.plugins.iter_mut() {
            if plugin.ticking {
                let lua_instance = &plugin.lua;

                if !plugin.did_init {
                    self.add_globals(lua_instance, state);
                }

                let tick_func = lua_instance.globals().get::<Function>("tick");

                if let Ok(func) = tick_func {
                    if let Err(res) = func.call::<()>(()) {
                        println!("[PLUGINS] {:?} had an error calling tick! - {}", plugin.info.public_name, res);
                    }
                } else {
                    println!("[PLUGINS] {:?} had an error! Pausing ticks! - {:?}", plugin.info.public_name, tick_func.err());

                    plugin.ticking = false;
                }
            }
        }
    }

    pub fn make_lua() -> Lua {
        Lua::new_with(StdLib::ALL, LuaOptions::new()).unwrap()
    }

    pub fn init_plugin(&self, file_name: String) {
        let lua_instance = PluginManager::make_lua();
        self.mandatory_globals(&lua_instance);

        if let Ok(file) = fs::read_to_string(format!("./plugins/{file_name}")) {
            let res = lua_instance.load(file).exec();

            if let Err(res) = res {
                println!("[LUA] Failed to load {file_name} due to : {res:?}");

                return;
            }

            let globals = lua_instance.globals();

            if let Ok(data) = globals.get::<Table>("plugin_info") {
                if let Some(plugin_info) = PluginInfo::parse(data) {
                    println!("[PLUGINS] Loading {:?} by {:?}", plugin_info.public_name, plugin_info.author);

                    self.plugins.insert(
                        plugin_info.internal_name.clone(),
                        Plugin {
                            info: plugin_info,
                            lua: lua_instance,
                            did_init: false,
                            ticking: true,
                        },
                    );
                } else {
                    println!("[PLUGINS] Failed to load {file_name:?} - Couldnt parse plugin data!")
                }
            } else {
                println!(
                    "[PLUGINS] Failed to load {file_name:?} - Couldnt get plugin variable! {:?}",
                    globals.get::<Table>("plugin").err()
                );
            }
        } else {
            println!("[PLUGINS] Failed to load {file_name:?} - Coudlnt read file to string!");
        }
    }

    pub fn load_plugins(&self) {
        let plugins_folder = Path::new("./plugins");

        if !plugins_folder.exists() {
            fs::create_dir("./plugins").unwrap();
        }

        let mut lua_files = vec![];

        for file in fs::read_dir("./plugins").unwrap() {
            if let Ok(ref file) = file
                && file.file_name().to_string_lossy().ends_with(".lua")
            {
                lua_files.push(file.file_name().into_string().unwrap().clone())
            }
        }

        for lua_file in lua_files {
            self.init_plugin(lua_file);
        }
    }
}
