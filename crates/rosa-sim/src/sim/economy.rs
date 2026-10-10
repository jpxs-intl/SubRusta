use rosa_map::file_types::srk::SrkPlayerData;
use rosa_protocol::{
    GameMode, Team,
    clientbound::game::events::{Event, ServerEvent, chat::ChatType, update_stock::EventUpdateStock},
};

use super::Sim;
use crate::{PlayerId, player::Player};

pub const CORPORATIONS: usize = 6;
const MAX_STOCKS: i32 = 9999;
const TAX_FREE: i32 = 10000;
const TAX_RATE: f32 = 0.05;
const STOCK_TAX_FREE: f32 = 10000.0;
const SELL_RATE: f32 = 0.95;
const ROUND_SELL_RATE: f32 = 0.98;

/// A corporation's share market (0x5ac0bc00 + corp * 0x18).
#[derive(Clone, Copy, Debug)]
pub struct Corporation {
    /// Shares held by players.
    pub shares: i32,
    pub price: f32,
    // TODO: name once its readers are ported (the price a reset starts from)
    pub base_price: f32,
    // TODO: name once its readers are ported
    pub unk_10: i32,
    /// Subtracted from each running project's spend when a share's sale price is worked out.
    pub unk_14: i32,
}

impl Default for Corporation {
    fn default() -> Self {
        Self { shares: 100, price: 100.0, base_price: 100.0, unk_10: 100, unk_14: 0 }
    }
}

/// The corporation a player's team trades in, if it is one.
pub fn corporation_of(team: Team) -> Option<usize> {
    let t = team as usize;
    (t < CORPORATIONS).then_some(t)
}

/// purchase_stocks: buys up to `n` shares (never past 9999 held) if the player has more money than they cost.
pub fn purchase_stocks(player: &mut Player, corps: &mut [Corporation; CORPORATIONS], n: i32) {
    let Some(c) = corporation_of(player.team) else { return };
    let held = player.stocks;
    let (n, count) = if held > MAX_STOCKS {
        (0, 0.0)
    } else {
        let n = if held + n > MAX_STOCKS { (MAX_STOCKS + 1) - held } else { n };
        (n, n as f32)
    };
    let money = player.money as f32;
    let cost = count * corps[c].price;
    if money > cost {
        corps[c].shares += n;
        player.stocks = held + n;
        player.money = (money - cost) as i32;
    }
}

/// player_sell_stocks: sells `n` of the player's shares at 95% (98% in round mode) of the price less what the
/// corporation's running projects have spent per share.
pub fn sell_stocks(player: &mut Player, corps: &mut [Corporation; CORPORATIONS], n: i32, mode: GameMode) {
    let Some(c) = corporation_of(player.team) else { return };
    let held = player.stocks;
    if held < n {
        return;
    }
    let shares = corps[c].shares;
    let price = corps[c].price;
    // TODO: each of the corporation's 16 active projects (team + 0x45c * k, spend at +0x4c) with positive spend
    // lowers the price by max(spend - unk_14, 0) / shares
    let rate = if mode == GameMode::Round { ROUND_SELL_RATE } else { SELL_RATE };
    player.stocks = held - n;
    player.money = ((price * rate) * n as f32 + player.money as f32) as i32;
    corps[c].shares = shares - n;
}

/// wealth_tax: takes 5% of the money over 10000 and sells off a twentieth of the shares worth more than 10000.
pub fn wealth_tax(player: &mut Player, corps: &mut [Corporation; CORPORATIONS]) {
    let money = player.money;
    let excess = money - TAX_FREE;
    if excess > 0 {
        player.money = money - (excess as f32 * TAX_RATE) as i32;
    }
    let Some(c) = corporation_of(player.team) else { return };
    let price = corps[c].price;
    let over = ((player.stocks as f32 * price - STOCK_TAX_FREE) / price) as i32;
    if over > 0 {
        let sold = over / 20;
        corps[c].shares -= sold;
        player.stocks -= sold;
    }
}

/// some_cash_penalty: the wealth tax on the money of an account whose player has left.
pub fn account_wealth_tax(account: &mut SrkPlayerData) {
    let money = account.money as i32;
    let excess = money - TAX_FREE;
    if excess > 0 {
        account.money = (money - (excess as f32 * TAX_RATE) as i32) as u32;
    }
}

/// create_event_update_stock: the first five share prices in tenths.
pub fn stock_event(corps: &[Corporation; CORPORATIONS], tick: u32) -> Event {
    let tenths: Vec<i32> = corps.iter().map(|c| (c.price * 10.0) as i32).collect();
    Event {
        tick_created: tick,
        kind: ServerEvent::UpdateStock(EventUpdateStock { prices: [(tenths[1] << 16) + tenths[0], (tenths[3] << 16) + tenths[2], tenths[4]] }),
    }
}

/// The money a player has on joining: world mode tops it up to 500 and versus sets it to 250.
pub fn joining_money(mode: GameMode, money: i32) -> i32 {
    // TODO: round mode with the setting at 0x452cacd0 set gives the 0x453855d0 setting (250)
    match mode {
        GameMode::World if money < 500 => 500,
        GameMode::Versus => 250,
        _ => money,
    }
}

const SUIT_MODEL: u8 = 1;
const CASUAL_MODEL: u8 = 0;
const SUIT_COLORS: [u8; CORPORATIONS] = [6, 10, 11, 2, 1, 2];
const TIE_COLORS: [u8; CORPORATIONS] = [2, 9, 8, 3, 7, 1];

/// Suit colours a team change leaves alone.
fn keeps_suit(suit: u8) -> bool {
    matches!(suit, 3 | 7 | 8)
}

pub const WORLD_TIME_START: i32 = 1728010;
const WORLD_TIME_STEP: i32 = 10;
const WORLD_MISSIONS_TIME: i32 = 1728060;
const WORLD_SAVE_PERIOD: i32 = 18000;
const WORLD_RESPAWN_BASE: i32 = 1200;
const WORLD_RESPAWN_PER_CRIME: i32 = 30;
/// An eliminator death adds 18000 ticks split between a third of the starting players, up to 50400.
const DEATH_CUT_TIME: i32 = 18000;
const DEATH_CUT_CAP: i32 = 50400;
const PLAY_TIME_STEP: u32 = 5;

impl Sim {
    /// The economy part of logic_world: the clock, and every 18000 clock units the accounts are saved and the share
    /// prices sent.
    pub(crate) fn logic_world(&mut self) {
        // TODO: the per-player checks before the clock (spawn timers, vehicle ownership)
        // TODO: the clock advances by the rate at 0xe3b21e4
        self.world_time += WORLD_TIME_STEP;
        if self.world_time == WORLD_MISSIONS_TIME {
            // TODO: the day's world missions (create_world_mode_mission, world_distribute_document_missions)
        }
        self.stock_burger_shops();
        // TODO: the day's end (reset_game past 0x3e9f3f), traffic and trains, and the 54000 checks
        if self.world_time % WORLD_SAVE_PERIOD != 0 {
            return;
        }
        // TODO: count down the human +0x14 timers
        self.restock_dealerships();
        self.stock_gun_stores();
        self.save_accounts();
        for (_, p) in self.players.iter() {
            if let Some(a) = self.saved_accounts.get_player_data(p.account_id) {
                a.play_time += PLAY_TIME_STEP;
            }
        }
        // TODO: the account countdown at record +0x38 (not saved)
        self.save_stats();
        self.events.push(stock_event(&self.corporations, self.tick));
    }

    /// update_suitcolor_model: moves the player to another team and dresses them for it (the corporation's suit and
    /// tie, or plain clothes with no team), then their human.
    pub(crate) fn set_player_team(&mut self, pid: PlayerId, team: Team) {
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if player.team == team {
            return;
        }
        let old = player.team;
        self.leave_corp_management(pid, old);
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        player.team = team;
        let look = &mut player.customization;
        if let Some(c) = corporation_of(team) {
            look.model = SUIT_MODEL;
            if !keeps_suit(look.suit_color) {
                look.suit_color = SUIT_COLORS[c];
            }
            look.tie_color = TIE_COLORS[c];
        } else if team == Team::Spectator {
            look.model = CASUAL_MODEL;
            if !keeps_suit(look.suit_color) {
                look.suit_color = 0;
            }
            look.tie_color = 0;
        }
        let look = player.customization;
        if let Some(h) = player.human.and_then(|id| self.humans.get_mut(id)) {
            h.customization.model = look.model;
            h.customization.suit_color = look.suit_color;
            h.customization.tie_color = look.tie_color;
        }
    }

    /// save_accounts: writes server.srk.
    pub(crate) fn save_accounts(&self) {
        if let Err(e) = self.saved_accounts.save(std::path::Path::new("server.srk")) {
            println!("[Sim] Could not save accounts: {e:?}");
        }
    }

    /// save_stats: writes stats.txt, the deals made and bullets fired since the server started, the accounts and their
    /// play time (in tens), and a line per account.
    pub(crate) fn save_stats(&self) {
        let accounts = &self.saved_accounts.players;
        let total = accounts.iter().fold(0u32, |t, a| t.wrapping_add(a.play_time / 10));
        let average = if accounts.is_empty() { 0 } else { total / accounts.len() as u32 };
        let mut out = format!(
            "numofmissions={}\r\nnumofbullets={}\r\nusers={}\r\ntotaltime={}\r\naveragetime={}\r\n\r\n",
            self.stats.missions as i32,
            self.stats.bullets as i32,
            accounts.len(),
            total as i32,
            average as i32
        );
        for (i, a) in accounts.iter().enumerate() {
            let name = String::from_utf8_lossy(a.player_name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned();
            let line = format!("{i}:{name}:{} ({})", (a.play_time / 10) as i32, a.account_id as i32);
            out += &line;
            if a.ban_time != 0 {
                out += &format!(" BANNED {}", a.ban_time as i32);
            }
            out += "\r\n";
        }
        if let Err(e) = std::fs::write("stats.txt", out) {
            println!("[Sim] Could not save stats: {e:?}");
        }
    }

    /// The death bookkeeping of human_simulation for a dead human's player: world mode sells their shares and sets
    /// the respawn wait, every mode taxes their wealth, then the player loses the human.
    pub(crate) fn settle_death(&mut self, pid: PlayerId) {
        let mode = self.gamemode;
        let tick = self.tick;
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        let real = pid.idx() <= 0xff;
        if mode == GameMode::World && real {
            let held = player.stocks;
            sell_stocks(player, &mut self.corporations, held, mode);
        }
        if real {
            wealth_tax(player, &mut self.corporations);
            self.events.push(player.make_update_round_event(tick));
        }
        if mode == GameMode::Eliminator && real {
            self.broadcast_death_cut_time(pid);
        }
        // TODO: versus clears the player's seven saved slots (+0x17f4, 0x64 apart)
        if mode == GameMode::World {
            // TODO: the setting at 0x45385614 (off by default) keeps the dead player in their corporation
            self.set_player_team(pid, Team::Spectator);
            let Some(player) = self.players.get_mut(pid.idx()) else { return };
            player.spawn_timer = player.crim_rating * WORLD_RESPAWN_PER_CRIME + WORLD_RESPAWN_BASE;
        }
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        player.human = None;
        self.events.push(player.make_update_player_event(tick));
    }

    /// broadcast_death_cut_time: everyone hears of the death, and before the last 50400 ticks the clock gains 18000
    /// ticks shared between a third of the players who started.
    fn broadcast_death_cut_time(&mut self, pid: PlayerId) {
        let Some(name) = self.players.get(pid.idx()).map(|p| p.username.clone()) else { return };
        self.send_chat(&format!("{name} has died"), ChatType::EliminatorAnnouncement, -1, 0);
        let timer = self.game_timer;
        let thirds = self.eliminator.ready / 3;
        if timer <= DEATH_CUT_CAP && thirds > 0 {
            self.game_timer = (DEATH_CUT_TIME / thirds + timer).min(DEATH_CUT_CAP);
        }
    }

    /// The account part of server_recv_and_dispatch's join and restore_account_data: round and world mode players
    /// get their saved money and ratings back, then the mode's starting money applies.
    pub(crate) fn restore_account(&mut self, pid: PlayerId) {
        let mode = self.gamemode;
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if matches!(mode, GameMode::Round | GameMode::World)
            && let Some(a) = self.saved_accounts.get_player_data(player.account_id)
        {
            player.money = a.money as i32;
            if (a.corp_rating as i32) < 0 {
                a.corp_rating = 0;
            }
            player.corp_rating = a.corp_rating as i32;
            player.crim_rating = a.crim_rating as i32;
            player.spawn_timer = self.account_spawn_timers.get(&player.account_id).copied().unwrap_or(0);
        }
        self.reclaim_humans(pid);
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        player.money = joining_money(mode, player.money);
    }

    /// A returning player takes back every human still holding their account: its team becomes theirs, and in world
    /// mode they buy back the shares it remembers.
    fn reclaim_humans(&mut self, pid: PlayerId) {
        let mode = self.gamemode;
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        let account = player.account_id;
        for id in self.humans.ids() {
            let Some(h) = self.humans.get_mut(id) else { continue };
            if h.account != Some(account) {
                continue;
            }
            h.player = Some(pid);
            player.human = Some(id);
            player.ghost_human = false;
            player.team = h.team.unwrap_or(Team::Spectator);
            if mode == GameMode::World {
                let stocks = h.stocks;
                purchase_stocks(player, &mut self.corporations, stocks);
            } else {
                player.menu = rosa_protocol::clientbound::game::MenuType::Empty;
                player.customization.model = SUIT_MODEL;
                player.customization.suit_color = player.team as u8;
            }
        }
    }

    /// The economy part of delete_player: the leaving player's shares are sold and their account updated, and their
    /// human remembers the account.
    pub(crate) fn settle_leave(&mut self, pid: PlayerId) {
        let mode = self.gamemode;
        self.remove_player_from_corps(pid);
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        // TODO: bots skip all of this
        let held = player.stocks;
        sell_stocks(player, &mut self.corporations, held, mode);
        if pid.idx() <= 0xff {
            let held = player.stocks;
            sell_stocks(player, &mut self.corporations, held, mode);
        }
        let team = player.team;
        self.leave_corp_management(pid, team);
        // TODO: delete_player also clears 4 bytes of each corporation the player managed (near corporation +0x328)
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if let Some(a) = self.saved_accounts.get_player_data(player.account_id) {
            a.money = player.money as u32;
            a.corp_rating = player.corp_rating as u32;
            a.crim_rating = player.crim_rating as u32;
        }
        self.account_spawn_timers.insert(player.account_id, player.spawn_timer);
        let account = player.account_id;
        if let Some(h) = player.human.and_then(|id| self.humans.get_mut(id))
            && h.player == Some(pid)
        {
            h.player = None;
            h.account = Some(account);
        }
    }

    /// handle_buy_menu_selection for the share menu: options 0 to 3 buy 1, 10, 100 or 1000 shares, 4 to 7 sell them.
    pub(crate) fn stock_menu_selection(&mut self, pid: PlayerId, button: u32) {
        let mode = self.gamemode;
        let tick = self.tick;
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if player.human.is_none() {
            return;
        }
        let amounts = [1, 10, 100, 1000];
        let b = (button & 0xff) as usize;
        match b {
            0..4 => purchase_stocks(player, &mut self.corporations, amounts[b]),
            4..8 => sell_stocks(player, &mut self.corporations, amounts[b - 4], mode),
            _ => {}
        }
        self.events.push(player.make_update_round_event(tick));
    }
}

/// Hooks for checking stats.txt against the original server.
impl Sim {
    pub fn set_stats(&mut self, missions: u32, bullets: u32) {
        self.stats.missions = missions;
        self.stats.bullets = bullets;
    }

    pub fn set_accounts(&mut self, accounts: Vec<rosa_map::file_types::srk::SrkPlayerData>) {
        self.saved_accounts.players = accounts;
    }

    pub fn write_stats(&self) {
        self.save_stats();
    }
}
