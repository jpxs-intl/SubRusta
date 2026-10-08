use crate::{clientbound::game::events::{bullet_hit::EventBulletHit, bullet_hole::EventBulletHole, chat::EventChat, sound::EventSound, team_door::EventTeamDoor, update_phone::EventUpdatePhone, update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound, update_vehicle::EventUpdateVehicle, update_vehicle_type_color::EventUpdateVehicleTypeColor}, codec::{WireWrite, Writer}};

pub mod bullet_hit;
pub mod bullet_hole;
pub mod chat;
pub mod sound;
pub mod team_door;
pub mod update_phone;
pub mod update_player_round;
pub mod update_player;
pub mod update_vehicle_type_color;
pub mod update_vehicle;

#[derive(Clone, Debug, PartialEq)]
pub struct Event {
    pub tick_created: u32,
    pub kind: ServerEvent
}

#[derive(Clone, PartialEq, Debug)]
pub enum ServerEvent {
    BulletHit(EventBulletHit),
    BulletHole(EventBulletHole),
    Chat(EventChat),
    Sound(EventSound),
    TeamDoor(EventTeamDoor),
    UpdatePhone(EventUpdatePhone),
    UpdatePlayerRound(EventUpdatePlayerRound),
    UpdatePlayer(EventUpdatePlayer),
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
            ServerEvent::BulletHit(e) => (1, e),
            ServerEvent::Chat(e) => (2, e),
            ServerEvent::UpdateVehicleTypeColor(e) => (3, e),
            ServerEvent::UpdateVehicle(e) => (4, e),
            ServerEvent::UpdatePhone(e) => (6, e),
            ServerEvent::UpdatePlayer(e) => (7, e),
            ServerEvent::UpdatePlayerRound(e) => (8, e),
            ServerEvent::Sound(e) => (9, e),
            ServerEvent::TeamDoor(e) => (10, e),
            ServerEvent::BulletHole(e) => (0x10, e),
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