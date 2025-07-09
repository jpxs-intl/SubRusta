use crate::{connection::events::event_types::{bullet_hit::EventBulletHit, chat::EventChat, sound::EventSound, team_door_state::EventTeamDoorState, update_phone::EventUpdatePhone, update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound, update_vehicle::EventUpdateVehicle, update_vehicle_type_color::EventUpdateVehicleTypeColor}, packets::{buf_writer::AlexBufWriter, WriterEncodable}};

pub mod bullet_hit;
pub mod update_vehicle_type_color;
pub mod update_vehicle;
pub mod update_phone;
pub mod update_player;
pub mod team_door_state;
pub mod update_player_round;
pub mod chat;
pub mod sound;

#[derive(Clone)]
pub enum Event {
    BulletHit(EventBulletHit),
    UpdateVehicleTypeColor(EventUpdateVehicleTypeColor),
    UpdateVehicle(EventUpdateVehicle),
    UpdatePhone(EventUpdatePhone),
    UpdatePlayer(EventUpdatePlayer),
    UpdatePlayerRound(EventUpdatePlayerRound),
    TeamDoorState(EventTeamDoorState),
    Chat(EventChat),
    Sound(EventSound)
}

impl WriterEncodable for Event {
    fn encode(&self, state: &crate::AppState, writer: &mut AlexBufWriter) {
        match self {
            Event::BulletHit(event_bullet_hit) => event_bullet_hit.encode(state, writer),
            Event::UpdateVehicleTypeColor(event_update_vehicle_type_color) => event_update_vehicle_type_color.encode(state, writer),
            Event::UpdateVehicle(event_update_vehicle) => event_update_vehicle.encode(state, writer),
            Event::UpdatePhone(event_update_phone) => event_update_phone.encode(state, writer),
            Event::UpdatePlayer(event_update_player) => event_update_player.encode(state, writer),
            Event::UpdatePlayerRound(event_update_player_round) => event_update_player_round.encode(state, writer),
            Event::TeamDoorState(event_team_door_state) => event_team_door_state.encode(state, writer),
            Event::Chat(event_chat) => event_chat.encode(state, writer),
            Event::Sound(event_sound) => event_sound.encode(state, writer),
        }
    }
}