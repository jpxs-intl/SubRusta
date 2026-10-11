use rosa_map::file_types::srk::SrkPlayerData;
use rosa_protocol::{
    Team,
    clientbound::game::events::{Event, ServerEvent, update_stock::EventUpdateStock},
};

use super::Sim;
use crate::time::TICKS_PER_SECOND;
use crate::{PlayerId, player::Player};

pub const CORPORATIONS: usize = 6;
const MAX_STOCKS: i32 = 9999;
/// The share of the price a sale pays: 95%, or 98% in round mode.
pub(crate) const SELL_RATE: f32 = 0.95;
pub(crate) const ROUND_SELL_RATE: f32 = 0.98;
const TAX_FREE: i32 = 10000;
const TAX_RATE: f32 = 0.05;
const STOCK_TAX_FREE: f32 = 10000.0;

/// A corporation's share market (0x5ac0bc00 + corp * 0x18).
#[derive(Clone, Copy, Debug)]
pub struct Corporation {
    /// Shares held by players.
    pub shares: i32,
    pub price: f32,
}

impl Default for Corporation {
    fn default() -> Self {
        Self { shares: 100, price: 100.0 }
    }
}

impl Sim {
    /// sell_all_stocks: sells all of a players stocks
    pub(crate) fn sell_all_stocks(&mut self, pid: PlayerId) {
        let held = self.players.get(pid.idx()).map_or(0, |p| p.stocks);
        self.sell_stocks(pid, held);
    }

    /// player_sell_stocks: sells `n` of the player's shares at `stock_sell_rate` of the price less what the
    /// corporation's running projects have spent per share.
    pub(crate) fn sell_stocks(&mut self, pid: PlayerId, n: i32) {
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        let Some(c) = corporation_of(player.team) else { return };

        let held = player.stocks;
        if held < n {
            return;
        }

        let corp = &mut self.corporations[c];

        let shares = corp.shares;
        let mut price = corp.price;

        for m in self.corp_state.get(c).map_or(&[][..], |p| &p.missions[..]) {
            if m.active && m.provided_cash > 0 && shares > 0 {
                price -= m.provided_cash.max(0) as f32 / shares as f32;
            }
        }
        player.stocks = held - n;
        player.money = ((price * self.rules.stock_sell_rate) * n as f32 + player.money as f32) as i32;
        corp.shares = shares - n;
    }

    /// purchase_stocks: buys up to `n` shares (never past 9999 held) if the player has more money than they cost.
    pub(crate) fn purchase_stocks(&mut self, pid: PlayerId, n: i32) {
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        let Some(c) = corporation_of(player.team) else { return };

        let held = player.stocks;
        let (n, count) = if held > MAX_STOCKS {
            (0, 0.0)
        } else {
            let n = if held + n > MAX_STOCKS { (MAX_STOCKS + 1) - held } else { n };
            (n, n as f32)
        };

        let money = player.money as f32;
        let cost = count * self.corporations[c].price;
        if money > cost {
            self.corporations[c].shares += n;
            player.stocks = held + n;
            player.money = (money - cost) as i32;
        }
    }
}

/// The corporation a player's team trades in, if it is one.
pub fn corporation_of(team: Team) -> Option<usize> {
    let t = team as usize;
    (t < CORPORATIONS).then_some(t)
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

/// A new account's money and corporate rating (create_account_from_join_ticket), before world mode's startcash.
const NEW_ACCOUNT_MONEY: u32 = 1000;
const NEW_ACCOUNT_RATING: u32 = 1000;
/// The longest name an account or player keeps.
const NAME_LEN: usize = 31;

/// The name as an account stores it: at most 31 bytes.
/// An account on the Steam ban list is banned for this long.
const STEAM_BAN_TIME: u32 = 100000000;
/// A name taken at join is kept for this many world saves.
const NAME_LOCK: i32 = 5;
/// A banned account dropped from the server keeps its name for this many world saves.
pub(crate) const BANNED_NAME_LOCK: i32 = 2;

fn account_name_bytes(name: &str) -> [u8; 32] {
    let mut buf = [0u8; 32];
    let src = name.as_bytes();
    let n = src.len().min(NAME_LEN);
    buf[..n].copy_from_slice(&src[..n]);
    buf
}

/// sanitize_name: slashes become spaces, the spaces at either end go, and an empty name is "Noname".
pub fn sanitize_name(raw: &[u8]) -> String {
    let end = raw.iter().take(NAME_LEN).position(|&b| b == 0).unwrap_or(raw.len().min(NAME_LEN));
    let bytes: Vec<u8> = raw[..end].iter().map(|&b| if b == b'/' { b' ' } else { b }).collect();
    let start = bytes.iter().position(|&b| b != b' ').unwrap_or(bytes.len());
    let stop = bytes.iter().rposition(|&b| b != b' ').map_or(start, |i| i + 1);
    let name = String::from_utf8_lossy(&bytes[start..stop.max(start)]).into_owned();
    if name.is_empty() { "Noname".into() } else { name }
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
const WORLD_RESPAWN_BASE: i32 = 20 * TICKS_PER_SECOND;
const WORLD_RESPAWN_PER_CRIME: i32 = 30;

impl Sim {
    /// update_suitcolor_model: moves the player to another team and dresses them for it (the corporation's suit and
    /// tie, or plain clothes with no team), then their human.
    pub(crate) fn set_player_team(&mut self, pid: PlayerId, team: Team) {
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        let old = player.team;
        if old == team {
            return;
        }
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
        let tick = self.tick;
        let real = pid.idx() <= 255;

        if self.rules.shares_follow_human && real {
            self.sell_all_stocks(pid);
        }

        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if real {
            wealth_tax(player, &mut self.corporations);
            self.events.push(player.make_update_round_event(tick));
        }

        if real {
            self.mode.on_death(self, pid);
        }

        if self.rules.respawn_wait {
            if !self.modes.world.cfg.respawnteam {
                self.set_player_team(pid, Team::Spectator);
            }
            let Some(player) = self.players.get_mut(pid.idx()) else { return };
            player.spawn_timer = player.crim_rating * WORLD_RESPAWN_PER_CRIME + WORLD_RESPAWN_BASE;
        }

        let Some(player) = self.players.get_mut(pid.idx()) else { return };

        player.human = None;
        self.events.push(player.make_update_player_event(tick));
    }

    /// create_account_from_join_ticket: the joining player's account, made new (1000 money, startcash in world mode,
    /// 1000 corporate rating and a random look) if this server has not seen them, and brought up to date with what the
    /// master server says about them.
    pub(crate) fn join_account(&mut self, auth: &rosa_protocol::masterserver::AuthPacket) -> SrkPlayerData {
        let accounts = &mut self.saved_accounts;
        if !accounts.players.iter().any(|a| a.account_id == auth.account_id) {
            let mut a = SrkPlayerData::new(auth.account_id, &auth.player_name, auth.phone_number, auth.steam_id);
            a.money = if self.rules.world_start_cash { self.modes.world.cfg.startcash as u32 } else { NEW_ACCOUNT_MONEY };
            a.corp_rating = NEW_ACCOUNT_RATING;
            a.eye_color = rand::random_range(0..3) as u32;
            a.hair_color = rand::random_range(0..8) as u32;
            a.skin_color = rand::random_range(0..4) as u32;
            accounts.players.push(a);
            accounts.player_count = accounts.players.len() as u32;
        }
        let a = accounts.players.iter_mut().find(|a| a.account_id == auth.account_id).unwrap();
        a.phone_number = auth.phone_number;
        a.steam_id = auth.steam_id;
        if self.admin.steam_bans.contains(&a.steam_id) {
            a.ban_time = STEAM_BAN_TIME;
        }
        let lock = self.account_name_locks.entry(auth.account_id).or_insert(0);
        if *lock == 0 {
            a.player_name = account_name_bytes(&auth.player_name);
            *lock = NAME_LOCK;
        }
        a.clone()
    }

    /// The account part of server_recv_and_dispatch's join and restore_account_data: round and world mode players
    /// get their saved money and ratings back, then the mode's starting money applies.
    pub(crate) fn restore_account(&mut self, pid: PlayerId) {
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if self.rules.saved_accounts
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
        let Some(money) = self.players.get(pid.idx()).map(|p| p.money) else { return };
        let money = self.mode.joining_money(self, money);
        if let Some(player) = self.players.get_mut(pid.idx()) {
            player.money = money;
        }
    }

    /// A returning player takes back every human still holding their account: its team becomes theirs, and in world
    /// mode they buy back the shares it remembers.
    fn reclaim_humans(&mut self, pid: PlayerId) {
        let Some(account) = self.players.get(pid.idx()).map(|p| p.account_id) else { return };

        for id in self.humans.ids() {
            let Some(h) = self.humans.get_mut(id).filter(|h| h.account == Some(account)) else { continue };

            h.player = Some(pid);

            let (team, stocks) = (h.team.unwrap_or(Team::Spectator), h.stocks);

            let Some(player) = self.players.get_mut(pid.idx()) else { return };
            player.human = Some(id);
            player.ghost_human = false;
            player.team = team;

            if self.rules.shares_follow_human {
                self.purchase_stocks(pid, stocks);
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
        if let Some(p) = self.players.get(pid.idx()).filter(|p| p.is_bot) {
            let (human, account) = (p.human, p.account_id);
            if let Some(h) = human.and_then(|h| self.humans.get_mut(h)).filter(|h| h.player == Some(pid)) {
                h.player = None;
                h.account = Some(account);
            }
            return;
        }
        self.remove_player_from_corps(pid);

        self.sell_all_stocks(pid);

        for k in 0..self.corp_state.len() {
            if self.corp_state[k].manager != Some(pid) {
                continue;
            }
            if self.rules.world_corporations {
                self.update_corp_player_ratings(k);
            }
            self.corp_state[k].manager = None;
            self.corp_state[k].fired.clear();
        }
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
        let tick = self.tick;

        if self.players.get(pid.idx()).is_none_or(|p| p.human.is_none()) {
            return;
        }

        let amounts = [1, 10, 100, 1000];
        let b = (button & 0xff) as usize;

        match b {
            0..4 => self.purchase_stocks(pid, amounts[b]),
            4..8 => self.sell_stocks(pid, amounts[b - 4]),
            _ => {}
        }

        let Some(player) = self.players.get(pid.idx()) else { return };
        self.events.push(player.make_update_round_event(tick));
    }
}
