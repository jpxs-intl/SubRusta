
use std::{fs::File, path::Path};

use binrw::{BinRead, BinWrite};

#[derive(BinRead, BinWrite, Clone)]
#[brw(little)]
pub struct SrkData {
    pub version: u32,
    pub server_id: u32,
    pub player_count: u32,
    #[br(count = player_count)]
    pub players: Vec<SrkPlayerData>
}

#[derive(BinRead, BinWrite, Clone)]
#[brw(little)]
pub struct SrkPlayerData {
    pub subrosa_id: u32,
    pub phone_number: u32,
    pub steam_id: u64,
    pub unused_0: u32,
    pub unused_1: u32,
    pub player_name: [u8; 32],
    pub money: u32,
    pub corp_rating: u32,
    pub crim_rating: u32,
    pub spawn_timer: u32,
    pub play_time: u32,
    pub unused_2: u32,
    pub unused_3: u32,
    pub ban_time: u32,
}

impl SrkData {
    pub fn read_from_file() -> Self {
        println!("[SRK] Attempting to read server.srk...");

        let file_name = "server.srk";

        if !Path::new(file_name).exists() {
            let data = SrkData {
                player_count: 0,
                players: vec![],
                server_id: 800815,
                version: 1
            };

            let mut file = File::create(file_name).unwrap();
            let _ = data.write(&mut file);
        }

        let mut file = std::fs::File::open(file_name).expect("Failed to open server.srk");
        let data = SrkData::read(&mut file).unwrap();

        println!("[SRK] Loaded SRK successfully, found {} players.", data.player_count);

        data
    }
}