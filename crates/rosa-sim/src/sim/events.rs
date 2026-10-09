use std::net::SocketAddr;

use rosa_protocol::{Team, clientbound::game::{GameState, ItemKind, MenuType, events::{Event, ServerEvent, chat::ChatType, update_player::EventUpdatePlayer}}, serverbound::game::actions::{ChatAction, Menu, MenuAction}};

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
                earshots: [None; 8],
                pack_ring: Vec::new(),
                pack_count: 0,
                pack_ack: 0,
                packed: Default::default(),
                player_id,
            },
        );

        self.restore_account(player_id);
        let player = self.players.get_mut(player_id.idx()).unwrap();
        self.events.push(player.make_update_player_event(self.tick));
        self.events.push(player.make_update_round_event(self.tick));

        if self.gamestate == GameState::Intermission {
            player.menu = MenuType::Lobby
        } else {
            player.menu = MenuType::Empty
        }

        self.send_initial_sync(src);
    }

    pub(crate) fn on_leave(&mut self, conn: ConnId) {
        let client = self.clients.remove(&conn).unwrap();
        self.settle_leave(client.player_id);

        let player = self.players.remove(client.player_id.idx());

        self.events.push(Event {
            tick_created: self.tick,
            kind: ServerEvent::UpdatePlayer(EventUpdatePlayer {
                active: false,
                client_id: player.player_id.idx() as u32,
                is_bot: false,
                human_id: -1,
                team: player.team,
                customization: player.customization,
                name: player.username.clone()
            })
        });
    }

    pub(crate) fn on_menu_action(&mut self, player_id: PlayerId, action: MenuAction) {
        if self.players.get(player_id.idx()).is_some_and(|p| p.menu == MenuType::RoundCorpStock) {
            return self.stock_menu_selection(player_id, action.button);
        }
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

    /// `/item <name or number>`: spawns that item in front of you (`/item` alone lists the names).
    fn item_command(&mut self, player_id: PlayerId, message: &str) {
        let arg = message.trim_start_matches("/item").trim().to_lowercase();
        let kinds = (0..ItemKind::COUNT as u8).filter_map(|k| ItemKind::try_from(k).ok());
        let kind = kinds.clone().find(|k| arg.parse::<u8>().ok() == Some(*k as u8) || format!("{k:?}").to_lowercase() == arg);
        match kind {
            Some(k) => {
                self.spawn_item_for(player_id, k);
            }
            None => {
                let names: Vec<String> = kinds.map(|k| format!("{k:?}").to_lowercase()).collect();
                self.send_chat(&format!("/item {}", names.join(" ")), ChatType::Announce, -1, 0);
            }
        }
    }

    /// `/phone <number>`: spawns a phone with that number (9999 by default, like the binary's admin /phone).
    fn phone_command(&mut self, player_id: PlayerId, message: &str) {
        let number = message.trim_start_matches("/phone").trim().parse::<i32>().unwrap_or(9999);
        let Some(id) = self.spawn_item_for(player_id, ItemKind::Phone) else { return };
        if let Some(p) = self.items.get_mut(id).and_then(|i| i.state.phone_mut()) {
            p.number = number;
        }
        self.send_chat(&format!("Phone {number} spawned"), ChatType::Announce, -1, 0);
    }

    pub(crate) fn on_chat_action(&mut self, player_id: PlayerId, action: ChatAction) {
        match action.message.trim() {
            "/watermelon" => return self.spawn_watermelon_for(player_id),
            "/human" => return self.spawn_human_for(player_id),
            "/kill" => return self.kill_human_for(player_id),
            "/guns" => return self.spawn_guns_for(player_id),
            m if m.starts_with("/item") => return self.item_command(player_id, m),
            m if m.starts_with("/phone") => return self.phone_command(player_id, m),
            "/stocks" => {
                if let Some(p) = self.players.get_mut(player_id.idx()) {
                    p.menu = if p.menu == MenuType::RoundCorpStock { MenuType::Empty } else { MenuType::RoundCorpStock };
                }
                return;
            }
            _ => {}
        }

        match self.gamestate {
            GameState::Intermission => self.send_chat(&action.message, ChatType::Announce, player_id.idx() as i32, 0),
            _ => self.send_chat(&action.message, ChatType::Chat, player_id.idx() as i32, 0),
        }
    }
}