use crate::sim::{economy::SELL_RATE, traffic::TRAFFIC_CARS};

/// What a game mode turns on and off, set by reset_game from the mode's hooks. Code outside the modes asks these rather
/// than which mode it is. The default is a plain mode; each mode changes what it needs from there.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeRules {
    /// The share of a teammate's damage that comes back on the attacker's head.
    pub team_damage: i32,
    /// The share of the price a sale of shares pays.
    pub stock_sell_rate: f32,
    /// The traffic cars reset_game asks for; modes without them make theirs when they start.
    pub reset_traffic: Option<u32>,
    /// Some traffic is parked in the third lane of flat three lane streets.
    pub parked_cars: bool,
    /// The traffic drives as in co-op.
    pub coop_traffic: bool,
    /// Dead humans stay on the ground.
    pub keep_bodies: bool,
    /// Loose items never despawn.
    pub keep_items: bool,
    /// Hurting people raises criminal ratings.
    pub crime: bool,
    /// Hurting a teammate hurts the attacker too.
    pub punish_team_kills: bool,
    /// Corporations run as in world mode: the manager's rating is the account's credit, ratings are settled when a
    /// manager leaves, and the base and tab menus (20 to 26) are offered.
    pub world_corporations: bool,
    /// The base buy menus (14 to 19) open inside a corporation's base.
    pub corp_buy_menus: bool,
    /// Corporation members count towards the player counts without a human.
    pub count_humanless_members: bool,
    /// The base computers and labs get their mission volumes.
    pub mission_spots: bool,
    /// Garage doors open and close when a teammate knocks on them.
    pub door_knocks: bool,
    /// Joining players get their saved money and ratings back.
    pub saved_accounts: bool,
    /// New accounts start with the world config's startcash.
    pub world_start_cash: bool,
    /// Shares are sold when the human dies and bought back when a returning player takes it over.
    pub shares_follow_human: bool,
    /// A saved key keeps its own vehicle for the next day, rather than a new vehicle of its type and colour.
    pub keys_keep_vehicles: bool,
    /// A death waits out a respawn timer that grows with criminal rating, spectating unless respawnteam is set.
    pub respawn_wait: bool,
    /// Teams can be picked in the lobby however late it is.
    pub free_team_switch: bool,
    /// A player without a human sees the lobby menu during intermission.
    pub intermission_lobby: bool,
    /// Bots join with the lobby menu open during intermission.
    pub bot_lobby: bool,
    /// The round starts once everyone is ready.
    pub ready_start: bool,
    /// Nobody can walk for this many ticks after the round starts.
    pub move_delay: Option<i32>,
    /// Buying a vehicle sends the round money update.
    pub round_money_updates: bool,
    /// Everyone may use the test commands (/human, /guns, ...).
    pub test_commands: bool,
}

impl Default for ModeRules {
    fn default() -> Self {
        Self {
            team_damage: 0,
            stock_sell_rate: SELL_RATE,
            reset_traffic: Some(TRAFFIC_CARS),
            parked_cars: false,
            coop_traffic: false,
            keep_bodies: false,
            keep_items: false,
            crime: false,
            punish_team_kills: true,
            world_corporations: false,
            corp_buy_menus: false,
            count_humanless_members: false,
            mission_spots: false,
            door_knocks: true,
            saved_accounts: false,
            world_start_cash: false,
            shares_follow_human: false,
            keys_keep_vehicles: false,
            respawn_wait: false,
            free_team_switch: false,
            intermission_lobby: false,
            bot_lobby: false,
            ready_start: true,
            move_delay: None,
            round_money_updates: false,
            test_commands: false,
        }
    }
}
