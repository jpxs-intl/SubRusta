use std::net::SocketAddr;

use glam::Vec3;
use rosa_protocol::{
    GameMode, Team,
    clientbound::game::{
        GameState, ItemKind, MenuType,
        events::{Event, ServerEvent, chat::ChatType, update_player::EventUpdatePlayer},
    },
    serverbound::game::actions::{ChatAction, Menu, MenuAction},
};

use super::Sim;
use crate::{Client, ConnId, PlayerId, SimJoinMsg};

/// The gravity the game is tuned for, in m/s², which /gravity values are relative to.
const EARTH_GRAVITY: f32 = 9.8;

impl Sim {
    pub(crate) fn on_join(&mut self, conn: ConnId, src: SocketAddr, j: SimJoinMsg) {
        let accout_data = self
            .saved_accounts
            .get_or_create(
                j.auth_packet.account_id,
                &j.auth_packet.name,
                j.auth_packet.phone_number,
                j.auth_packet.steam_id,
            )
            .clone();

        if accout_data.ban_time > 0 {
            return self.kick(src, "You are banned from this server");
        }

        let admin = self.is_admin_phone(accout_data.phone_number);

        if admin {
            println!("[Sim] Admin {} is joining!", j.join_packet.player_name);
        }

        if self.players.len() >= self.max_players as usize && !admin {
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
                round_number: u32::MAX,
                timeout: 0,
                admin_visible: admin,
                last_sdl_tick: 0,
                earshots: [None; 8],
                pack_ring: Vec::new(),
                pack_count: 0,
                pack_ack: 0,
                packed: Default::default(),
                traffic_priority: Vec::new(),
                signal_cursor: 0,
                player_id,
            },
        );

        self.restore_account(player_id);
        let player = self.players.get_mut(player_id.idx()).unwrap();
        player.is_admin = admin;
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
                name: player.username.clone(),
            }),
        });
    }

    pub(crate) fn on_menu_action(&mut self, player_id: PlayerId, action: MenuAction) {
        let id = match action.menu {
            Menu::Lobby => 2,
            Menu::Other(m) => m,
        };

        let Some(player) = self.players.get_mut(player_id.idx()) else {
            return;
        };

        if player.menu as u8 == id
            && (player.human.is_some() || player.ghost_human)
            && matches!(id, 14..=19)
        {
            return self.round_buy_menu(player_id, action.button);
        }

        if player.menu as u8 == id && matches!(id, 9..=11) {
            return self.shop_menu_action(player_id, action.button);
        }

        if self.gamemode == GameMode::World
            && player.menu as u8 == id
            && player.human.is_some()
            && matches!(id, 20..=26)
        {
            return self.corp_menu_action(player_id, id, action.button as i32);
        }

        if matches!(self.gamemode, GameMode::Round | GameMode::Eliminator)
            && matches!(action.menu, Menu::Lobby)
        {
            return self.round_lobby(player_id, action.button);
        }

        match action.menu {
            Menu::Lobby => {
                match action.button {
                    1 => player.team = Team::Goldmen,
                    2 => player.team = Team::Monsota,
                    3 => player.team = Team::OXS,
                    4 => player.team = Team::Spectator,
                    5 => player.is_ready = !player.is_ready,
                    other => println!("Got other {other}"),
                }

                self.events.push(player.make_update_player_event(self.tick))
            }

            Menu::Other(other) => println!("Unknown menu option! {}", other),
        }
    }

    /// `/item <name or number>`: spawns that item in front of you (`/item` alone lists the names).
    fn item_command(&mut self, player_id: PlayerId, message: &str) {
        let arg = message.trim_start_matches("/item").trim().to_lowercase();
        let kinds = (0..ItemKind::COUNT as u8).filter_map(|k| ItemKind::try_from(k).ok());
        let kind = kinds.clone().find(|k| {
            arg.parse::<u8>().ok() == Some(*k as u8) || format!("{k:?}").to_lowercase() == arg
        });
        match kind {
            Some(k) => {
                self.spawn_item_for(player_id, k, Vec3::Z);
            }
            None => {
                let names: Vec<String> = kinds.map(|k| format!("{k:?}").to_lowercase()).collect();
                self.send_chat(
                    &format!("/item {}", names.join(" ")),
                    ChatType::Announce,
                    -1,
                    0,
                );
            }
        }
    }

    /// `/phone <number>`: spawns a phone with that number (9999 by default, like the binary's admin /phone).
    /// Test command (/gravity [m/s² | reset]): sets gravity for everyone, as Earth's 9.8 by default; with no value it
    /// announces the current gravity.
    fn gravity_command(&mut self, message: &str) {
        let arg = message.split_whitespace().nth(1);
        let scale = match arg {
            None => None,
            Some("reset") => Some(1.0),
            Some(v) => match v.parse::<f32>() {
                Ok(g) if g.is_finite() && (-1000.0..=1000.0).contains(&g) => {
                    Some(g / EARTH_GRAVITY)
                }
                _ => {
                    self.send_chat("Usage: /gravity [m/s² | reset]", ChatType::Announce, -1, 0);
                    return;
                }
            },
        };
        if let Some(scale) = scale {
            self.bodies.gravity_scale = scale;
            for (_, item) in self.items.iter_mut() {
                item.physics_settled = false;
                item.settled_timer = 0;
            }
        }
        let g = self.bodies.gravity_scale * EARTH_GRAVITY;
        self.send_chat(&format!("Gravity is {g} m/s²"), ChatType::Announce, -1, 0);
    }

    fn phone_command(&mut self, player_id: PlayerId, message: &str) {
        let number = message
            .trim_start_matches("/phone")
            .trim()
            .parse::<i32>()
            .unwrap_or(9999);
        let Some(id) = self.spawn_item_for(player_id, ItemKind::Phone, Vec3::Z) else {
            return;
        };
        if let Some(p) = self.items.get_mut(id).and_then(|i| i.state.phone_mut()) {
            p.number = number;
        }
        self.send_chat(
            &format!("Phone {number} spawned"),
            ChatType::Announce,
            -1,
            0,
        );
    }

    pub(crate) fn on_chat_action(&mut self, player_id: PlayerId, action: ChatAction) {
        if action.message.trim() == "/tps" {
            return self.tps_command();
        }

        if self.gamemode == GameMode::Sandbox {
            match action.message.trim() {
                "/watermelon" => return self.spawn_watermelon_for(player_id),
                "/human" => return self.spawn_human_for(player_id),
                "/kill" => return self.kill_human_for(player_id),
                "/godmode" => return self.godmode_command(player_id),
                "/guns" => return self.spawn_guns_for(player_id),
                "/clear" => return self.clear_command(),
                m if m.starts_with("/car") => return self.car_command(player_id, m),
                m if m.starts_with("/item") => return self.item_command(player_id, m),
                m if m.starts_with("/phone") => return self.phone_command(player_id, m),
                m if m.starts_with("/gravity") => return self.gravity_command(m),
                "/stocks" => {
                    if let Some(p) = self.players.get_mut(player_id.idx()) {
                        p.menu = if p.menu == MenuType::RoundCorpStock {
                            MenuType::Empty
                        } else {
                            MenuType::RoundCorpStock
                        };
                    }
                    return;
                }
                "/heal" => {
                    if let Some(p) = self.players.get(player_id.idx())
                        && let Some(human) = p.human
                    {
                        let human = self.humans.get_mut(human).unwrap();

                        human.chest_hp = 100;
                        human.head_hp = 100;
                        human.right_arm_hp = 100;
                        human.left_arm_hp = 100;
                        human.left_leg_hp = 100;
                        human.right_leg_hp = 100;
                        human.old_health = 100;
                        human.blood_level = 100;
                    }
                    return;
                }
                _ => {}
            }
        }

        if action.message.starts_with('/') {
            return self.admin_message(player_id, &action.message);
        }

        let speaker = self
            .players
            .get(player_id.idx())
            .and_then(|p| p.human)
            .filter(|&h| self.humans.get(h).is_some_and(|h| h.old_health > 0));
        match (self.gamestate, speaker) {
            (GameState::Intermission, _) | (_, None) => self.send_chat(
                &action.message,
                ChatType::Announce,
                player_id.idx() as i32,
                0,
            ),
            (_, Some(human)) => self.send_chat(&action.message, ChatType::Chat, human as i32, 0),
        }
    }
}
