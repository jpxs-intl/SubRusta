use std::net::SocketAddr;

use rosa_protocol::{Team, clientbound::game::{GameState, events::chat::ChatType}, serverbound::game::actions::{ChatAction, Menu, MenuAction}};

use crate::{Client, ConnId, PlayerId, SimJoinMsg};
use super::Sim;

impl Sim {
    pub(crate) fn on_join(&mut self, conn: ConnId, src: SocketAddr, j: SimJoinMsg) {
        let accout_data = self.saved_accounts.get_or_create(
                j.auth_packet.account_id,
                &j.auth_packet.name,
                j.auth_packet.phone_number,
                j.auth_packet.steam_id,
            ).clone();

        if accout_data.ban_time > 0 {
            return self.kick(src, "You are banned from this server");
        }

        if self.players.len() >= self.max_players as usize {
            return self.kick(src, "Server is full");
        }

        let Some(player_id) = self.alloc_player(accout_data.account_id, j) else {
            return self.kick(src, "Server is full");
        };

        self.clients.insert(
            conn,
            Client {
                addr: src,
                event_cursor: 0,
                last_sdl_tick: 0,
                player_id,
            },
        );

        let player = self.players.get(player_id.idx()).unwrap();
        self.events.push(player.make_update_player_event(self.tick));
        self.events.push(player.make_update_round_event(self.tick));

        self.send_initial_sync(src);
    }

    pub(crate) fn on_leave(&mut self, conn: ConnId) {
        let client = self.clients.remove(&conn).unwrap();

        self.players.remove(client.player_id.idx());
    }

    pub(crate) fn on_menu_action(&mut self, player_id: PlayerId, action: MenuAction) {
        let player = self.players.get_mut(player_id.idx()).unwrap();

        match action.menu {
            Menu::Lobby => {
                match action.button {
                    1 => player.team = Team::Goldmen,
                    2 => player.team = Team::Monsota,
                    3 => player.team = Team::OXS,
                    4 => player.team = Team::Spectator,
                    5 => player.is_ready = !player.is_ready,
                    other => println!("Got other {other}")
                }

                self.events.push(player.make_update_player_event(self.tick))
            }

            Menu::Other(other) => println!("Unknown menu option! {}", other)
        }
    }

    pub(crate) fn on_chat_action(&mut self, player_id: PlayerId, action: ChatAction) {
        match self.gamestate {
            GameState::Intermission => self.send_chat(&action.message, ChatType::Announce, player_id.idx() as i32, 0),
            _ => self.send_chat(&action.message, ChatType::Chat, player_id.idx() as i32, 0),
        }
    }
}