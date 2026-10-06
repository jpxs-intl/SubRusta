use rosa_protocol::GameMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigMain {
    pub master_server_url: String,
    pub master_server_ip: Option<String>,
    pub port: u16,
    pub map_name: String,
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
            master_server_ip: None,
            port: 27584,
            map_name: "test2".to_string(),
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
        println!("[Config] Attempting to load config.toml...");

        match std::fs::read_to_string("config.toml") {
            Ok(s) => toml::from_str(&s).unwrap_or_else(|e| {
                eprintln!("[Config] parse error: {e} - using defaults");
                Self::default()
            }),
            Err(_) => {
                let c = Self::default();
                let _ = c.save();
                c
            }
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        std::fs::write("config.toml", toml::to_string_pretty(self).unwrap())
    }
}

pub fn init_config_dirs() {
    let folder = format!("{}/Sub Rosa", dirs_next::document_dir().unwrap().to_str().unwrap());

    if !std::path::Path::new(&folder).exists() {
        std::fs::create_dir_all(&folder).expect("Failed to create config directory");
    }
}