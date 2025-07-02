use std::{collections::HashMap, fs::File, io::{Write, BufRead}};

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config::{decode_gamemode_str, get_bool_from_config, get_string_from_config, get_u32_from_config, GameMode};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigMain {
    pub master_server_url: String,
    pub port: u16,
    pub server_name: String,
    pub admin_password: String,
    pub server_password: String,
    pub gamemode: GameMode,
    pub max_players: u8,
    pub round_time: u32,
    pub voice_chat: bool,
    pub voice_min: u32,
    pub voice_boost: u32,
    pub help: bool,
    pub manual_hands: bool,
}

impl Default for ConfigMain {
    fn default() -> Self {
        ConfigMain {
            master_server_url: "www.crypticsea.com".to_string(),
            port: 27584,
            server_name: "Baro Serv".to_string(),
            admin_password: "admin".to_string(),
            server_password: "".to_string(),
            gamemode: GameMode::Round,
            max_players: 16,
            round_time: 300, // 5 minutes
            voice_chat: false,
            voice_min: 1000, // Default to 1 second
            voice_boost: 0, // No boost by default
            help: true,
            manual_hands: false,
        }
    }
}

impl ConfigMain {
    pub fn read_from_file() -> Self {
        println!("[CONFIG] Attempting to load config.txt...");
        let file = File::open("config.txt");

        if let Ok(file) = file {
            let mut data = HashMap::new();

            let reader = std::io::BufReader::new(file).lines();

            for line in reader.map_while(Result::ok) {
                let line_data = line.split_once('=');
                if let Some((key, value)) = line_data {
                    data.insert(key.trim().to_string(), value.trim().to_string());
                }
            }

            let val = serde_json::to_value(&data).unwrap_or(json!({}));

            let res = Self {
                master_server_url: get_string_from_config(&val, "master_server_url"),
                port: get_u32_from_config(&val, "port") as u16,
                server_name: get_string_from_config(&val, "server_name"),
                admin_password: get_string_from_config(&val, "admin_password"),
                server_password: get_string_from_config(&val, "server_password"),
                gamemode: decode_gamemode_str(&get_string_from_config(&val, "gamemode")),
                max_players: get_u32_from_config(&val, "max_players") as u8,
                round_time: get_u32_from_config(&val, "round_time"),
                voice_chat: get_bool_from_config(&val, "voice_chat"),
                voice_min: get_u32_from_config(&val, "voice_min"),
                voice_boost: get_u32_from_config(&val, "voice_boost"),
                help: get_bool_from_config(&val, "help"),
                manual_hands: get_bool_from_config(&val, "manual_hands"),
            };

            println!("[CONFIG] Successfully loaded config.txt!");
            res
        } else {
            let config = ConfigMain::default();
            config.save().expect("Failed to save default config");
            config
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        let mut file = File::create("config.txt")?;

        for (key, value) in serde_json::to_value(self.clone()).unwrap().as_object().unwrap() {
            writeln!(file, "{}={}", key, value.to_string().replace('"', ""))?;
        }

        Ok(())
    }
}