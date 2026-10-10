use std::{fs::{File, OpenOptions}, path::Path};

use binrw::{BinRead, BinWrite};

use crate::file_types::LoaderError;


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
    pub account_id: u32,
    pub phone_number: u32,
    pub steam_id: u64,
    pub unused_0: u32,
    pub unused_1: u32,
    pub player_name: [u8; 32],
    pub money: u32,
    pub corp_rating: u32,
    pub crim_rating: u32,
    /// Play time: 5 is added at every world mode save (account record +0x4c); stats.txt counts it in tens.
    pub play_time: u32,
    /// The look the account was last seen with (account record +0x54, +0x58, +0x5c).
    pub eye_color: u32,
    pub hair_color: u32,
    pub skin_color: u32,
    pub ban_time: u32,
}

impl SrkPlayerData {
    pub fn new(account_id: u32, name: &str, phone_number: u32, steam_id: u64) -> Self {
        Self {
            account_id,
            player_name: SrkPlayerData::name_to_bytes(name),
            ban_time: 0,
            phone_number,
            steam_id,
            corp_rating: 0,
            crim_rating: 0,
            money: 0,
            play_time: 0,
            eye_color: 0,
            unused_0: 0,
            unused_1: 0,
            hair_color: 0,
            skin_color: 0
        }
    }

    fn name_to_bytes(name: &str) -> [u8; 32] {
        let mut buf = [0u8; 32];
        let src = name.as_bytes();
        let n = src.len().min(32);
        buf[..n].copy_from_slice(&src[..n]);
        buf
    }
}

impl SrkData {
    pub fn load(path: &Path) -> Result<Self, LoaderError> {
        if !Path::new(path).exists() {
            let data = SrkData {
                player_count: 0,
                players: vec![],
                server_id: 800815,
                version: 1
            };

            data.save(path)?;
            return Ok(data);
        }

        let mut file = File::open(path)?;
        Ok(SrkData::read(&mut file)?)
    }

    pub fn save(&self, path: &Path) -> Result<(), LoaderError> {
        let mut file = OpenOptions::new().write(true).create(true).truncate(true).open(path)?;

        self.write(&mut file)?;
        Ok(())
    }

    pub fn get_player_data(&mut self, account_id: u32) -> Option<&mut SrkPlayerData> {
        if let Some(i) = self.players.iter().position(|p| p.account_id == account_id) {
            Some(&mut self.players[i])
        } else {
            None
        }
    }

    pub fn get_or_create(&mut self, account_id: u32, name: &str, phone: u32, steam_id: u64) -> &mut SrkPlayerData {
        if let Some(i) = self.players.iter().position(|p| p.account_id == account_id) {
            &mut self.players[i]
        } else {
            self.players.push(SrkPlayerData::new(account_id, name, phone, steam_id));
            self.player_count = self.players.len() as u32;
            self.players.last_mut().unwrap()
        }
    }
}