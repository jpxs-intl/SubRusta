use crate::{clientbound::game::events::{bullet::EventBullet, bullet_hit::EventBulletHit, bullet_hole::EventBulletHole, chat::EventChat, explosion::EventExplosion, mission::EventMission, phone_sound::EventPhoneSound, sound::EventSound, team_door::EventTeamDoor, update_corporation::EventUpdateCorporation, update_phone::EventUpdatePhone, update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound, update_stock::EventUpdateStock, update_vehicle::EventUpdateVehicle, update_vehicle_type_color::EventUpdateVehicleTypeColor}, codec::{WireWrite, Writer}};

pub mod bullet;
pub mod bullet_hit;
pub mod bullet_hole;
pub mod chat;
pub mod explosion;
pub mod mission;
pub mod phone_sound;
pub mod sound;
pub mod team_door;
pub mod update_corporation;
pub mod update_phone;
pub mod update_player_round;
pub mod update_player;
pub mod update_stock;
pub mod update_vehicle_type_color;
pub mod update_vehicle;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub tick_created: u32,
    pub kind: ServerEvent
}

#[derive(Clone, PartialEq, Debug)]
pub enum ServerEvent {
    Bullet(EventBullet),
    BulletHit(EventBulletHit),
    BulletHole(EventBulletHole),
    Chat(EventChat),
    Explosion(EventExplosion),
    Mission(EventMission),
    PhoneSound(EventPhoneSound),
    Sound(EventSound),
    TeamDoor(EventTeamDoor),
    UpdateCorporation(EventUpdateCorporation),
    UpdatePhone(EventUpdatePhone),
    UpdatePlayerRound(EventUpdatePlayerRound),
    UpdatePlayer(EventUpdatePlayer),
    UpdateStock(EventUpdateStock),
    UpdateVehicleTypeColor(EventUpdateVehicleTypeColor),
    UpdateVehicle(EventUpdateVehicle),
    Empty
}

pub trait EventWrite {
    fn write(&self, w: &mut Writer);
    fn type_id(&self) -> i32;
}

impl WireWrite for Event {
    fn write(&self, w: &mut Writer) {
        let (type_id, body): (i32, &dyn WireWrite) = match &self.kind {
            ServerEvent::Bullet(e) => (0, e),
            ServerEvent::BulletHit(e) => (1, e),
            ServerEvent::Chat(e) => (2, e),
            ServerEvent::UpdateVehicleTypeColor(e) => (3, e),
            ServerEvent::UpdateVehicle(e) => (4, e),
            ServerEvent::UpdatePhone(e) => (6, e),
            ServerEvent::UpdatePlayer(e) => (7, e),
            ServerEvent::UpdatePlayerRound(e) => (8, e),
            ServerEvent::Sound(e) => (9, e),
            ServerEvent::TeamDoor(e) => (10, e),
            ServerEvent::UpdateCorporation(e) => (0xc, e),
            ServerEvent::UpdateStock(e) => (0xd, e),
            ServerEvent::BulletHole(e) => (0x10, e),
            ServerEvent::PhoneSound(e) => (0x13, e),
            ServerEvent::Explosion(e) => (0x14, e),
            ServerEvent::Mission(e) => (0x15, e),
            ServerEvent::Empty => {
                w.bits(0x17, 6);

                return;
            }
        };

        w.bits(type_id, 6);
        w.bits(self.tick_created as i32, 28);

        body.write(w)
    }
}