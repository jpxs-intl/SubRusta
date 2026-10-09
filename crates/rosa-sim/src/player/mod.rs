use rosa_protocol::{CharacterCustomization, Team, clientbound::game::{MenuType, events::{Event, ServerEvent, update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound}}, serverbound::game::{ClientGamePacket, InputFlags}};

use crate::{PlayerId, SimJoinMsg, player::{actions::ActionQueue, voice::PlayerVoice}};

pub mod actions;
pub mod voice;

pub struct Player {
    pub player_id: PlayerId,
    pub username: String,
    pub team: Team,
    pub steam_id: u64,
    pub actions: ActionQueue,
    pub menu: MenuType,
    /// The building whose shop or bank menu is open (player +0x168).
    pub menu_tab: i32,
    /// How many things the player has bought from gun stores and burger shops (player +0x68), cleared when they get a
    /// new human, at reset_game, and every 3600 ticks in round mode.
    // TODO: the round mode clear every 3600 ticks of the round timer (logic_round), once round mode is ported
    pub items_bought: i32,
    pub account_id: u32,
    pub input: InputFlags,
    pub money: i32,
    /// Shares held in the player's corporation (`team`).
    pub stocks: i32,
    pub corp_rating: i32,
    pub crim_rating: i32,
    pub spawn_timer: i32,
    pub phone_number: u32,
    pub is_ready: bool,
    pub voice: PlayerVoice,
    pub customization: CharacterCustomization,
    pub camera_pos: rosa_math::vector::Vector,
    pub view_yaw: f32,
    pub view_pitch: f32,
    pub human: Option<usize>,
    // TODO: name the control floats (player +0xa0..+0xdc, the first 9 read from the game packet) and the four after
    // them (player +0xe0..+0xec, the first 2 read from the game packet and used as the jump's spin)
    pub controls: [f32; 16],
    pub controls_extra: [f32; 4],
    pub input_bits: u32,
    pub control_mode: u32,
    pub zoom_level: u32,
}

impl Player {
    pub fn new_from_join(player_id: PlayerId, account_id: u32, j: SimJoinMsg) -> Self {
        let avatar_info = j.join_packet.avatar_info;

        Self {
            player_id,
            steam_id: j.auth_packet.steam_id,
            actions: ActionQueue::default(),
            account_id,
            menu: MenuType::Empty,
            menu_tab: 0,
            items_bought: 0,
            input: InputFlags::empty(),
            team: Team::Spectator,
            username: j.join_packet.player_name,
            money: 0,
            stocks: 0,
            corp_rating: 0,
            crim_rating: 0,
            spawn_timer: 0,
            phone_number: j.auth_packet.phone_number,
            is_ready: false,
            voice: PlayerVoice::new(),
            customization: CharacterCustomization {
                eye_color: avatar_info.eye_color,
                gender: avatar_info.gender,
                hair_color: avatar_info.hair_color,
                hair_style: avatar_info.hair,
                head: avatar_info.head,
                model: 0,
                necklace: 0,
                skin: avatar_info.skin_color,
                suit_color: 0,
                tie_color: 0
            },
            camera_pos: Default::default(),
            view_yaw: 0.0,
            view_pitch: 0.0,
            human: None,
            controls: [0.0; 16],
            controls_extra: [0.0; 4],
            input_bits: 0,
            control_mode: 0,
            zoom_level: 0,
        }
    }

    pub fn process_game_packet(&mut self, msg: Box<ClientGamePacket>) {
        self.actions.ingest(msg.total_actions, msg.actions);
        self.voice.ingest(msg.voice_data);

        self.input = InputFlags::from_bits_retain(msg.input_flags);
        let c = [msg.gear_x, msg.left_right, msg.gear_y, msg.forward_back, msg.view_yaw_delta, msg.view_pitch, msg.free_look_yaw, msg.free_look_pitch, msg.view_yaw];
        self.controls[..c.len()].copy_from_slice(&c);
        self.controls_extra[0] = msg.unknown;
        self.controls_extra[1] = msg.view_pitch_delta;
        self.input_bits = msg.input_flags;
        self.control_mode = msg.input_type as u32;
        self.zoom_level = msg.zoom_level as u32;
        self.camera_pos = msg.camera_pos;
        self.view_yaw = msg.view_yaw;
        self.view_pitch = msg.view_pitch;
    }

    pub fn make_update_player_event(&self, tick: u32) -> Event {
        Event {
            tick_created: tick,
            kind: ServerEvent::UpdatePlayer(EventUpdatePlayer {
                active: true,
                client_id: self.player_id.0,
                customization: self.customization,
                human_id: self.human.map_or(-1, |h| h as i32),
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
                stocks: self.stocks
            })
        }
    }
}
