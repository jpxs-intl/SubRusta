use rosa_protocol::{CharacterCustomization, Team, clientbound::game::{MenuButton, MenuType, events::{Event, ServerEvent, update_player::EventUpdatePlayer, update_player_round::EventUpdatePlayerRound}}, serverbound::game::{ClientGamePacket, InputFlags}};

use glam::Vec3;

use crate::{PlayerId, SimJoinMsg, player::{actions::ActionQueue, voice::PlayerVoice}};

pub mod actions;
pub mod voice;

/// create_player's starting money.
const BOT_MONEY: i32 = 50000;

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
    pub items_bought: i32,
    /// The buttons of the open menu, rebuilt every tick (player +0x1b14).
    pub menu_buttons: Vec<MenuButton>,
    /// The extra value of each button slot, which stays from the last button that set one (+0x1b5c in each).
    pub button_extras: Vec<i32>,
    /// Whether the player sees the round manager page (player +0x90), given to each corporation's richest member at the
    /// end of a round.
    pub manager_tab: bool,
    /// A player fired with their human: the binary keeps the deleted human's id until they get a new one, so
    /// logic_player keeps closing their menu (they are out of every corporation) and the buy pages still take tab
    /// buttons.
    pub ghost_human: bool,
    /// What the binary still has of that deleted human (its record stays behind): purchases go into it.
    pub ghost: Option<crate::sim::round_menus::GhostHuman>,
    /// Ticks before the player can change team in the lobby again (player +0x88).
    pub team_switch_timer: i32,
    /// Vehicles bought this round day (player +0x6c) and the $5 and $10 bills taken from a bank since (+0x70).
    pub vehicles_bought: i32,
    pub bills_withdrawn: i32,
    /// What the player's human held at the end of the last round, slot by slot.
    pub saved_inventory: [Vec<crate::sim::round::SavedItem>; 7],
    /// Where the player's human stood when the world day ended (player +0x3818 on).
    pub saved_body: Option<crate::sim::world::SavedBody>,
    /// The money and credit of the player's corporation, copied every tick (player +0x50 and +0x54).
    pub corp_money: i32,
    pub corp_credit: i32,
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
    /// A player the server drives (+0x2d18): the crews of round car chases.
    pub is_bot: bool,
    /// God mode (+0x60), toggled by /godmode: bullets pass through the player and team damage is not punished.
    pub god_mode: bool,
    /// An admin (+0x34): from serveradmin.txt, or after /admin with the password.
    pub is_admin: bool,
    /// /admin attempts so far; the fifth on is refused.
    pub admin_tries: i32,
    /// A mission bot's deadline on the world clock (+0x36e4) and the deal it belongs to (+0x36e8).
    pub bot_deadline: i32,
    pub bot_mission: i32,
    /// The bot AI's state (player_ai): see [`BotBrain`].
    pub bot: BotBrain,
}

/// An enemy a bot has seen (player +0x3564, 0x18 each): the human, where the bot thinks it is, how sure it is (0 to 16)
/// and a timer set to 600 when first seen.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct BotTarget {
    pub human: i32,
    pub pos: Vec3,
    pub awareness: f32,
    pub timer: i32,
}

/// What player_ai keeps for a bot.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct BotBrain {
    /// A zombie (+0x2d1c) walks at what it sees, through walls.
    pub is_zombie: bool,
    // TODO: name once its readers are ported: player +0x2d24, set for a car chase's driver
    pub unk_2d24: i32,
    /// The heading to face with nothing else to do (+0x2d28).
    pub idle_yaw: f32,
    /// The waypoint being walked to (+0x2d34), how many there are (+0x2d38) and the waypoints (+0x2d3c, 0x20 each).
    pub waypoint: i32,
    pub waypoint_count: i32,
    pub waypoints: Vec<Vec3>,
    /// Ticks the bot does nothing for (+0x3550).
    pub delay: i32,
    /// Ticks the bot's vehicle has been wrecked (+0x3558); past 179 it gets out.
    pub wrecked_ticks: i32,
    /// The enemies seen (+0x355c count, up to 16) and the one aimed at (+0x3560).
    pub targets: Vec<BotTarget>,
    pub target: i32,
}

impl BotBrain {
    pub fn waypoint_pos(&self, i: i32) -> Vec3 {
        usize::try_from(i).ok().and_then(|i| self.waypoints.get(i)).copied().unwrap_or(Vec3::ZERO)
    }

    pub fn set_waypoint(&mut self, i: usize, p: Vec3) {
        if self.waypoints.len() <= i {
            self.waypoints.resize(i + 1, Vec3::ZERO);
        }
        self.waypoints[i] = p;
    }
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
            menu_buttons: Vec::new(),
            button_extras: Vec::new(),
            manager_tab: false,
            ghost_human: false,
            ghost: None,
            team_switch_timer: 0,
            vehicles_bought: 0,
            bills_withdrawn: 0,
            saved_inventory: Default::default(),
            saved_body: None,
            corp_money: 0,
            corp_credit: 0,
            input: InputFlags::empty(),
            team: Team::Spectator,
            username: j.auth_packet.player_name,
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
            is_bot: false,
            god_mode: false,
            is_admin: false,
            admin_tries: 0,
            bot_deadline: 0,
            bot_mission: 0,
            bot: BotBrain::default(),
        }
    }

    /// create_player: an empty player with 50000 money, no account and a random look (head, skin, hair, eyes).
    pub fn new_bot(player_id: PlayerId, customization: CharacterCustomization, menu: MenuType) -> Self {
        // TODO: the aim and reaction floats create_player sets (player +0x140 and +0x36ec..+0x3718) for the bot AI
        Self {
            player_id,
            steam_id: 0,
            actions: ActionQueue::default(),
            account_id: u32::MAX,
            menu,
            menu_tab: 0,
            items_bought: 0,
            menu_buttons: Vec::new(),
            button_extras: Vec::new(),
            manager_tab: false,
            ghost_human: false,
            ghost: None,
            team_switch_timer: 0,
            vehicles_bought: 0,
            bills_withdrawn: 0,
            saved_inventory: Default::default(),
            saved_body: None,
            corp_money: 0,
            corp_credit: 0,
            input: InputFlags::empty(),
            team: Team::Spectator,
            username: String::new(),
            money: BOT_MONEY,
            stocks: 0,
            corp_rating: 0,
            crim_rating: 0,
            spawn_timer: 0,
            phone_number: 0,
            is_ready: false,
            voice: PlayerVoice::new(),
            customization,
            camera_pos: Default::default(),
            view_yaw: 0.0,
            view_pitch: 0.0,
            human: None,
            controls: [0.0; 16],
            controls_extra: [0.0; 4],
            input_bits: 0,
            control_mode: 0,
            zoom_level: 0,
            is_bot: true,
            god_mode: false,
            is_admin: false,
            admin_tries: 0,
            bot_deadline: 0,
            bot_mission: 0,
            bot: BotBrain::default(),
        }
    }

    /// Adds a menu button; without an extra value it shows whatever its slot last held.
    pub fn push_button(&mut self, id: i32, text: &str, extra: Option<i32>) {
        let k = self.menu_buttons.len();
        if self.button_extras.len() <= k {
            self.button_extras.resize(k + 1, 0);
        }
        if let Some(e) = extra {
            self.button_extras[k] = e;
        }
        let extra = self.button_extras[k];
        self.menu_buttons.push(MenuButton { id, text: text.to_string(), extra });
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
                is_bot: self.is_bot,
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

/// The player records (players, 0x3834 each): a new player takes the lowest free slot, as create_player does.
#[derive(Default)]
pub struct PlayerTable {
    slots: Vec<Option<Player>>,
    count: usize,
}

impl PlayerTable {
    pub fn next_key(&self) -> usize {
        self.slots.iter().position(Option::is_none).unwrap_or(self.slots.len())
    }

    pub fn insert(&mut self, p: Player) -> usize {
        let k = self.next_key();
        if k == self.slots.len() {
            self.slots.push(Some(p));
        } else {
            self.slots[k] = Some(p);
        }
        self.count += 1;
        k
    }

    pub fn remove(&mut self, k: usize) -> Player {
        let p = self.slots.get_mut(k).and_then(Option::take).expect("no player in that slot");
        self.count -= 1;
        p
    }

    pub fn get(&self, k: usize) -> Option<&Player> {
        self.slots.get(k)?.as_ref()
    }

    pub fn get_mut(&mut self, k: usize) -> Option<&mut Player> {
        self.slots.get_mut(k)?.as_mut()
    }

    pub fn len(&self) -> usize {
        self.count
    }

    pub fn is_empty(&self) -> bool {
        self.count == 0
    }

    pub fn iter(&self) -> impl Iterator<Item = (usize, &Player)> {
        self.slots.iter().enumerate().filter_map(|(k, p)| Some((k, p.as_ref()?)))
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = (usize, &mut Player)> {
        self.slots.iter_mut().enumerate().filter_map(|(k, p)| Some((k, p.as_mut()?)))
    }
}
