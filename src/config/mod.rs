
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::packets::GameMode;
pub mod config_main;

pub fn decode_gamemode_str(mode: &str) -> GameMode {
    match mode.to_lowercase().as_str() {
        "round" => GameMode::Round,
        "world" => GameMode::World,
        "eliminator" => GameMode::Eliminator,
        _ => GameMode::Round, // Default to Round if unknown
    }
}

pub fn init_config_dirs() {
    let folder = format!("{}/Sub Rosa", dirs_next::document_dir().unwrap().to_str().unwrap());

    if !std::path::Path::new(&folder).exists() {
        std::fs::create_dir_all(&folder).expect("Failed to create config directory");
    }
}

pub fn get_bool_from_config(config: &Value, key: &str) -> bool {
    let value = config.get(key).and_then(Value::as_bool);
    value.unwrap_or(false)
}

pub fn get_u8_from_config(config: &Value, key: &str) -> u8 {
    let value = config.get(key).and_then(Value::as_str).unwrap().parse::<u8>().ok();
    value.map_or(0, |v| v)
}

pub fn get_u32_from_config(config: &Value, key: &str) -> u32 {
    let value = config.get(key).and_then(Value::as_str).unwrap().parse::<u32>().ok();
    value.map_or(0, |v| v)
}

pub fn get_string_from_config(config: &Value, key: &str) -> String {
    let value = config.get(key).and_then(Value::as_str);
    value.map_or(String::new(), |v| v.to_string())
}

