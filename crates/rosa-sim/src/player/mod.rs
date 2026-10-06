use rosa_protocol::{CharacterCustomization, Team, clientbound::game::events::{Event, ServerEvent, update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound}, serverbound::game::{ClientGamePacket, InputFlags}};

use crate::{PlayerId, SimJoinMsg, player::actions::ActionQueue};

pub mod actions;

pub struct Player {
    pub player_id: PlayerId,
    pub username: String,
    pub team: Team,
    pub steam_id: u64,
    pub actions: ActionQueue,
    pub account_id: u32,
    pub input: InputFlags,
    pub money: i32,
    pub phone_number: u32,
    pub is_ready: bool
}

impl Player {
    pub fn new_from_join(player_id: PlayerId, account_id: u32, j: SimJoinMsg) -> Self {
        Self {
            player_id,
            steam_id: j.auth_packet.steam_id,
            actions: ActionQueue::default(),
            account_id,
            input: InputFlags::empty(),
            team: Team::Spectator,
            username: j.join_packet.player_name,
            money: 0,
            phone_number: j.auth_packet.phone_number,
            is_ready: false
        }
    }

    pub fn process_game_packet(&mut self, msg: Box<ClientGamePacket>) {
        self.actions.ingest(msg.total_actions, msg.actions);

        self.input = InputFlags::from_bits_retain(msg.input_flags);
    }

    pub fn make_update_player_event(&self, tick: u32) -> Event {
        Event {
            tick_created: tick,
            kind: ServerEvent::UpdatePlayer(EventUpdatePlayer {
                active: true,
                client_id: self.player_id.0,
                customization: CharacterCustomization::default(),
                human_id: -1,
                is_bot: false,
                team: self.team,
                name: self.username.clone()
            })
        }
    }

    pub fn make_update_round_event(&self, tick: u32) -> Event {
        Event {
            tick_created: tick,
            kind: ServerEvent::UpdatePlayerRound(EventUpdatePlayerRound {
                client_id: self.player_id.0,
                money: self.money,
                phone_number: self.phone_number,
                stocks: 0
            })
        }
    }
}
