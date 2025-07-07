pub mod lobby;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MenuTypes {
    Empty = 0,
    EnterCity = 1,
    Lobby = 2,
    EmptyBase = 3,
    WorldCarShop = 9,
    WorldStore = 10,
    WorldStoreDone = 11,
    WorldBank = 12,
    WorldBank2 = 13,
    RoundCorpWeapons = 14,
    RoundCorpAmmo = 15,
    RoundCorpEquip = 16,
    RoundCorpVehicle = 17,
    RoundCorpStock = 18,
    WorldEmptyCorp = 19,
    WorldCorpApplication = 20,
    WorldCorpHiring = 22,
    WorldCorpFiring = 23,
    WorldCorpTeam = 24,
    WorldCorpRequistion = 25,
}

pub fn menu_from_num(num: u8) -> MenuTypes {
    match num {
        0 => MenuTypes::Empty,
        1 => MenuTypes::EnterCity,
        2 => MenuTypes::Lobby,
        3 => MenuTypes::EmptyBase,
        9 => MenuTypes::WorldCarShop,
        10 => MenuTypes::WorldStore,
        11 => MenuTypes::WorldStoreDone,
        12 => MenuTypes::WorldBank,
        13 => MenuTypes::WorldBank2,
        14 => MenuTypes::RoundCorpWeapons,
        15 => MenuTypes::RoundCorpAmmo,
        16 => MenuTypes::RoundCorpEquip,
        17 => MenuTypes::RoundCorpVehicle,
        18 => MenuTypes::RoundCorpStock,
        19 => MenuTypes::WorldEmptyCorp,
        20 => MenuTypes::WorldCorpApplication,
        22 => MenuTypes::WorldCorpHiring,
        23 => MenuTypes::WorldCorpFiring,
        24 => MenuTypes::WorldCorpTeam,
        25 => MenuTypes::WorldCorpRequistion, 
        _ => MenuTypes::Empty
    }
}