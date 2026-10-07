use std::{
    collections::HashMap,
    net::SocketAddr,
    path::Path,
    time::{Duration, Instant},
};

use rosa_map::file_types::srk::SrkData;
use rosa_math::vector::Vector;
use rosa_protocol::{
    GameMode, clientbound::{
        game::{
            GameState, MenuType, ServerGamePacket, ServerVoiceData, events::{
                Event, ServerEvent,
                chat::{ChatType, EventChat},
            },
        }, initial_sync::InitialSync, kick::KickClient,
    }, frame_packet, serverbound::game::actions::GameAction,
};
use slab::Slab;
use tokio::sync::mpsc;

use crate::{Client, ConnId, Inbound, Outbound, PlayerId, SimJoinMsg, SimMsg, player::Player, world::World};

pub mod events;

#[allow(unused)]
struct TickCtx<'a> {
    players: &'a Slab<Player>,
    world: &'a World,
    events: &'a [Event],
    tick: u32,
    gamestate: GameState,
    ready_states: Option<[bool; 32]>
}

pub struct Sim {
    tick: u32,
    round_number: u32,
    gamemode: GameMode,
    gamestate: GameState,
    in_rx: mpsc::UnboundedReceiver<Inbound>,
    out_tx: Outbound,
    world: World,
    saved_accounts: SrkData,
    players: Slab<Player>,
    clients: HashMap<ConnId, Client>,
    max_players: u8,
    // TODO: Leaks. Just like restart your server or whatever, but this should be a ring.
    events: Vec<Event>,
}

impl Sim {
    pub fn new(
        in_rx: mpsc::UnboundedReceiver<Inbound>,
        out_tx: Outbound,
        map_name: String,
        gamemode: GameMode,
        max_players: u8,
    ) -> Self {
        let srk_data = SrkData::load(Path::new("server.srk")).unwrap();

        Self {
            tick: 0,
            in_rx,
            out_tx,
            world: World::new(map_name),
            gamemode,
            gamestate: GameState::Intermission,
            round_number: 0,
            saved_accounts: srk_data,
            players: Slab::with_capacity(256),
            clients: HashMap::new(),
            max_players,
            events: Vec::new(),
        }
    }

    fn apply(&mut self, input: Inbound) {
        match input.msg {
            SimMsg::Join(msg) => self.on_join(input.conn, input.src, msg),
            SimMsg::Game(g) => {
                let client = self.clients.get_mut(&input.conn).unwrap();
                let player = self.players.get_mut(client.player_id.idx()).unwrap();

                client.last_sdl_tick = g.sdl_tick;
                client.event_cursor = g.received_events as u16;

                player.process_game_packet(g);
            }
            SimMsg::Leave => self.on_leave(input.conn),
        }
    }

    fn send_initial_sync(&mut self, to: SocketAddr) {
        let packet = InitialSync {
            round_number: self.round_number,
            weekly_enabled: false,
            weekday: self.world.weekday as u8,
            sun_angle: self.world.sun_angle(),
            sun_axial_tilt: self.world.sun_axial_tilt(),
            versus_movedelay: None,
            gamemode: self.gamemode,
            map_name: self.world.map.map_name.clone(),
        };

        let _ = self.out_tx.send((rosa_protocol::frame_packet(packet), to));
    }

    fn send_chat(&mut self, message: &str, chat_type: ChatType, speaker_id: i32, volume: i32) {
        self.events.push(Event {
            tick_created: self.tick,
            kind: ServerEvent::Chat(EventChat {
                chat_type,
                speaker_id,
                volume,
                message: message.to_string()
            })
        });
    }

    fn kick(&mut self, src: SocketAddr, reason: &str) {
        let _ = self.out_tx.send((
            frame_packet(KickClient {
                reason: reason.to_string(),
            }),
            src,
        ));
    }

    pub fn announce_event(&mut self, event: Event) {
        self.events.push(event);
    }

    fn alloc_player(&mut self, account_id: u32, j: SimJoinMsg) -> Option<PlayerId> {
        if self.players.len() >= 255 {
            return None;
        }

        let entry = self.players.vacant_entry();
        let id = PlayerId(entry.key() as u32);

        entry.insert(Player::new_from_join(id, account_id, j));

        Some(id)
    }

    pub fn run(mut self) {
        let dt = Duration::from_secs_f64(1.0 / 60.0);
        let (mut acc, mut prev) = (Duration::ZERO, Instant::now());

        loop {
            while let Ok(inp) = self.in_rx.try_recv() {
                self.apply(inp);
            }

            acc += prev.elapsed();
            prev = Instant::now();

            while acc >= dt {
                // self.world.tick();

                let mut pending= HashMap::new();
                for (_, player) in self.players.iter_mut() {
                    pending.insert(player.player_id, player.actions.drain().collect::<Vec<GameAction>>());
                }

                self.process_actions(pending);
                self.broadcast_tick();

                self.tick += 1;
                acc -= dt;
            }

            std::thread::sleep(dt.saturating_sub(acc));
        }
    }

    fn process_actions(&mut self, player_actions: HashMap<PlayerId, Vec<GameAction>>) {
        for (player, actions) in player_actions {
            for action in actions {
                match action {
                    GameAction::Menu(menu_action) => self.on_menu_action(player, menu_action),
                    GameAction::Chat(chat_action) => self.on_chat_action(player, chat_action),
                    GameAction::Item(item_action) => println!("Item {:?}", item_action),
                    GameAction::Inventory(inventory_action) => println!("Inventory {:?}", inventory_action),
                    GameAction::Admin(admin_action) => println!("Admin {:?}", admin_action),
                    GameAction::Unknown => {},
                }
            }
        }
    }

    fn calculate_ready_states(players: &mut Slab<Player>) -> [bool; 32] {
        let mut ready_vec = [false; 32];

        for (idx, player) in &mut *players {
            ready_vec[idx] = player.is_ready;
        }

        ready_vec
    }

    fn broadcast_tick(&mut self) {
        let Sim {
            clients,
            players,
            world,
            events,
            out_tx,
            tick,
            gamestate,
            ..
        } = self;

        let ready_states = if *gamestate == GameState::Intermission {
            Some(Self::calculate_ready_states(players))
        } else {
            None
        };

        let ctx = TickCtx {
            players,
            world,
            events: events.as_slice(),
            tick: *tick,
            gamestate: *gamestate,
            ready_states,
        };

        for client in clients.values_mut() {
            let Some(player) = players.get(client.player_id.idx()) else {
                continue;
            };

            let packet = Sim::build_game_packet(client, &ctx, player.player_id);

            let _ = out_tx.send((frame_packet(packet), client.addr));
        }
    }

    fn build_game_packet(
        client: &mut Client,
        ctx: &TickCtx,
        player_id: PlayerId
    ) -> ServerGamePacket {
        let (global_event_count, events) = Self::collect_events(client, ctx);
        let earshots = Self::calculate_earshot(client, player_id, ctx);
        let player = ctx.players.get(player_id.idx()).unwrap();

        let mut voice_data: [Option<ServerVoiceData>; 8] = [const { None }; 8];

        for (idx, earshot) in earshots.iter().enumerate() {
            let speaker = ctx.players.get(earshot.idx()).unwrap();

            let data = ServerVoiceData {
                human_id: -1,
                item_id: -1,
                player_id: earshot.idx() as i32,
                voice_frames: speaker.voice.recent4()
            };

            voice_data[idx] = Some(data);
        }

        ServerGamePacket {
            client_id: client.player_id.0,
            received_actions: player.actions.write as u32,
            round_number: 0,
            network_tick: ctx.tick,
            last_sdl_tick: client.last_sdl_tick,
            menu_type: MenuType::Lobby,
            money: 1000,
            gamestate: ctx.gamestate,
            ready_states: ctx.ready_states,
            follow_pos: Vector::new(0.0, 0.0, 0.0),
            global_event_count,
            events,
            voice: voice_data
        }
    }

    fn collect_events(
        client: &mut Client,
        ctx: &TickCtx
    ) -> (u32, Vec<(u32, Event)>) {
        let total = ctx.events.len() as u16;
        let pending = total.wrapping_sub(client.event_cursor);
        let to_send = pending.min(0x3f); // 63 cap

        let mut out = Vec::with_capacity(to_send as usize);
        for i in 0..to_send {
            let idx = client.event_cursor.wrapping_add(i);
            let ev = &ctx.events[idx as usize];

            let expired = ctx.tick.wrapping_sub(ev.tick_created) > 600;
            let ephemeral = matches!(ev.kind, ServerEvent::Sound(_));
            let kind = if expired && ephemeral {
                ServerEvent::Empty
            } else {
                ev.kind.clone()
            };

            out.push((
                idx as u32,
                Event {
                    tick_created: ev.tick_created,
                    kind,
                },
            ));
        }

        client.event_cursor = client.event_cursor.wrapping_add(to_send);
        (total as u32, out)
    }

    fn calculate_earshot(_client: &mut Client, player_id: PlayerId, ctx: &TickCtx) -> Vec<PlayerId> {
        let mut chars = Vec::new();

        if ctx.gamestate == GameState::Intermission {
            for player in ctx.players.iter().filter(|(_, p)| !p.voice.is_silenced && p.player_id != player_id) {
                if chars.len() >= 8 {
                    return chars
                }

                chars.push(player.1.player_id)
            }
        }

        chars
    }
}
