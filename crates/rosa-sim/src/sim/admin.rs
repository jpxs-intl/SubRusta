use std::net::IpAddr;

use glam::Vec3;
use rosa_physics::rotation::IDENTITY;
use rosa_protocol::{
    GameMode, Team,
    clientbound::{
        admin_list::{ADMIN_LIST_ROWS, AdminList},
        game::{ItemKind, events::chat::ChatType},
    },
    frame_packet,
};

use super::Sim;
use super::item_state::PhoneStatus;
use crate::{ConnId, PlayerId};

/// handle_player_message keeps the first 63 bytes of a message.
const MESSAGE_LEN: usize = 63;
/// Wrong /admin passwords stop working after the fourth try.
const ADMIN_TRIES: i32 = 4;
/// /hlist lists humans 0 to 9; /list1 to /list4 list connections ten at a time.
const HLIST_HUMANS: usize = 10;
const LIST_PAGE: usize = 10;
/// The vehicles the spawn commands put 4 units in front of the admin, in colour 1.
const SPAWN_AHEAD: f32 = -4.0;
const SPAWN_COLOR: i32 = 1;
const HELI: usize = 12;
const TOWN_CAR: usize = 0;
const BEAMER: usize = 6;
const PICKUP: usize = 17;
const TURBO: usize = 5;
const VAN: usize = 7;
/// The admin phone's number, and the pocket it and /arm's bandages go in.
const ADMIN_PHONE: i32 = 9999;
const ADMIN_POCKET: usize = 6;
/// /arm's guns by number (1 to 5, anything else the MP5), with four magazines.
const ARM_GUNS: [ItemKind; 5] = [ItemKind::Ak47, ItemKind::M16, ItemKind::Mp5, ItemKind::Uzi, ItemKind::Pistol];
const ARM_DEFAULT: usize = 2;
const ARM_MAGAZINES: i32 = 4;
const ARM_BANDAGES: usize = 2;
const CASH_GIFT: i32 = 100000;
/// /setmap picks one of 32 versus maps.
const VERSUS_MAPS: i32 = 32;
/// A connection is dropped 1800 ticks after its last packet; a kick leaves it 600 of them.
const TIMEOUT: i32 = 1800;
const KICK_TIMEOUT: i32 = 1200;
/// Admin connections get the admin list every 128 ticks, staggered by connection.
const ADMIN_LIST_PERIOD: u32 = 0x7f;

/// The admin side of the server: the password and admin phones (serveradmin.txt), the kicked addresses, a ban
/// waiting to be confirmed, a game mode change asked for with /resetgame and the versus map picked with /setmap.
#[derive(Debug, Default)]
pub struct AdminState {
    pub password: String,
    pub phones: Vec<u32>,
    pub kicked: Vec<IpAddr>,
    pub pending_ban: Option<usize>,
    pub reset_requested: bool,
    pub next_mode: Option<GameMode>,
    pub versus_map: i32,
}

impl AdminState {
    /// load_admin_data: every `adminphone=` number in serveradmin.txt.
    pub fn load(path: &std::path::Path) -> Self {
        let phones = std::fs::read_to_string(path)
            .map(|text| text.split("adminphone=").skip(1).filter_map(|rest| leading_int(rest).map(|n| n as u32)).collect())
            .unwrap_or_default();
        AdminState { phones, ..Default::default() }
    }
}

/// sscanf("%d"): Err for nothing but blanks (EOF), Ok(None) when no number starts the text.
fn scan_int(s: &str) -> Result<Option<i32>, ()> {
    let t = s.trim_start();
    if t.is_empty() {
        return Err(());
    }
    Ok(leading_int(t))
}

fn leading_int(s: &str) -> Option<i32> {
    let t = s.trim_start();
    let end = t.char_indices().find(|&(i, c)| !(c.is_ascii_digit() || (i == 0 && (c == '-' || c == '+')))).map_or(t.len(), |(i, _)| i);
    t[..end].parse::<i64>().ok().map(|v| v as i32)
}

/// The numbers in the text in turn, skipping whatever lies between them (get_actual_number and
/// opened_file_parse_next_int).
fn numbers(s: &str) -> impl Iterator<Item = i32> + '_ {
    let mut rest = s;
    std::iter::from_fn(move || {
        let start = rest.find(|c: char| c.is_ascii_digit() || c == '-')?;
        rest = &rest[start..];
        let n = leading_int(rest);
        let len = rest.char_indices().skip(1).find(|&(_, c)| !c.is_ascii_digit()).map_or(rest.len(), |(i, _)| i);
        rest = &rest[len..];
        n
    })
}

fn account_name(a: &rosa_map::file_types::srk::SrkPlayerData) -> String {
    String::from_utf8_lossy(a.player_name.split(|&b| b == 0).next().unwrap_or(&[])).into_owned()
}

impl Sim {
    pub fn set_admin_password(&mut self, password: String) {
        self.admin.password = password;
    }

    /// Whether this account's phone is listed in serveradmin.txt.
    pub(crate) fn is_admin_phone(&self, phone: u32) -> bool {
        self.admin.phones.contains(&phone)
    }

    fn admin_say(&mut self, line: &str) {
        self.send_chat(line, ChatType::AdminChat, -1, 0);
    }

    /// handle_player_message for a message starting with '/': /admin <password> makes the player an admin, and an
    /// admin's commands run in the game's order. Every such message is kept from the chat.
    pub(crate) fn admin_message(&mut self, pid: PlayerId, raw: &str) {
        let msg = &raw[..raw.floor_char_boundary(MESSAGE_LEN)];
        let Some(player) = self.players.get_mut(pid.idx()) else { return };
        if let Some(password) = msg.strip_prefix("/admin ") {
            player.admin_tries += 1;
            if player.admin_tries <= ADMIN_TRIES && !self.admin.password.is_empty() && password == self.admin.password {
                player.is_admin = true;
                let name = player.username.clone();
                if let Some(c) = self.clients.values_mut().find(|c| c.player_id == pid) {
                    c.admin_visible = true;
                }
                self.admin_say(&format!("{name} admin"));
            }
        }
        if !self.players.get(pid.idx()).is_some_and(|p| p.is_admin) {
            return;
        }
        if msg.starts_with("/hlist") {
            for i in 0..HLIST_HUMANS {
                let Some(h) = self.humans.get(i) else { continue };
                let line = match h.player.and_then(|p| self.players.get(p.idx()).map(|pl| (p, pl))) {
                    Some((p, pl)) => format!("{i}:{} {}", p.0, pl.username),
                    None => format!("{i}:-1"),
                };
                self.admin_say(&line);
            }
        }
        for (page, cmd) in ["/list1", "/list2", "/list3", "/list4"].into_iter().enumerate() {
            if !msg.starts_with(cmd) {
                continue;
            }
            let conns = self.connection_order();
            let end = if page == 3 { conns.len() } else { (page + 1) * LIST_PAGE };
            for &conn in conns.iter().take(end).skip(page * LIST_PAGE) {
                let Some(p) = self.clients.get(&conn).and_then(|c| self.players.get(c.player_id.idx())) else { continue };
                let line = format!("{}  {}", p.username, p.phone_number as i32);
                self.admin_say(&line);
            }
        }
        // TODO: /savereplay sets the replay save count once replays are ported
        if msg.starts_with("/godmode") {
            self.godmode_command(pid);
        }
        for (cmd, kind) in [("/heli", HELI), ("/car", TOWN_CAR), ("/beamer", BEAMER), ("/pickup", PICKUP), ("/turbo", TURBO), ("/van", VAN)] {
            if msg.starts_with(cmd) {
                self.admin_spawn_vehicle(pid, kind);
            }
        }
        if msg.starts_with("/phone") {
            self.admin_phone(pid);
        }
        if msg.starts_with("/arm") {
            self.admin_arm(pid, msg.get(5..).unwrap_or(""));
        }
        if msg.starts_with("/cash")
            && let Some(p) = self.players.get_mut(pid.idx())
        {
            p.money = p.money.wrapping_add(CASH_GIFT);
        }
        if msg.starts_with("/kill")
            && let Some(h) = self.players.get(pid.idx()).and_then(|p| p.human).and_then(|h| self.humans.get_mut(h))
        {
            h.old_health = 0;
        }
        if let Some(rest) = msg.strip_prefix("/setmap ") {
            let mut n = self.admin.versus_map + 1;
            if !rest.is_empty() {
                if let Ok(v) = scan_int(rest) {
                    n = v.unwrap_or(0);
                }
                if (0..VERSUS_MAPS).contains(&(n - 1)) {
                    self.admin.versus_map = n - 1;
                } else {
                    self.admin.versus_map = 0;
                    n = 1;
                }
            }
            self.admin_say(&format!("mapnum: {n}"));
        }
        if msg.starts_with("/resetgame") {
            self.admin.reset_requested = true;
            if msg.as_bytes().get(10) == Some(&b' ') {
                match msg.as_bytes().get(11) {
                    Some(b'r') => {
                        self.admin.next_mode = Some(GameMode::Round);
                        self.weekday = super::round::WEEK_DAYS;
                    }
                    Some(b'e') => self.admin.next_mode = Some(GameMode::Eliminator),
                    Some(b'v') => {
                        self.admin.next_mode = Some(GameMode::Versus);
                        self.weekday = super::round::WEEK_DAYS;
                    }
                    _ => {}
                }
            }
        }
        if let Some(rest) = msg.strip_prefix("/ban ")
            && !rest.is_empty()
        {
            let phone = leading_int(rest).unwrap_or(0);
            if let Some(i) = self.saved_accounts.players.iter().position(|a| a.phone_number as i32 == phone) {
                self.admin.pending_ban = Some(i);
                let name = account_name(&self.saved_accounts.players[i]);
                self.admin_say(&format!("{name}({phone}) ban?"));
            }
        }
        if let Some(rest) = msg.strip_prefix("/give ")
            && !rest.is_empty()
        {
            let mut nums = numbers(rest);
            let phone = nums.next().unwrap_or(0);
            let amount = nums.next().unwrap_or(0);
            if let Some(i) = self.saved_accounts.players.iter().position(|a| a.phone_number as i32 == phone) {
                let a = &mut self.saved_accounts.players[i];
                a.money = (a.money as i32).wrapping_add(amount) as u32;
                let (id, name, phone) = (a.account_id, account_name(a), a.phone_number as i32);
                for (_, p) in self.players.iter_mut().filter(|(_, p)| p.account_id == id) {
                    p.money = p.money.wrapping_add(amount);
                }
                self.admin_say(&format!("{name}({phone}) {amount} cash"));
            }
        }
        if let Some(rest) = msg.strip_prefix("/setrating ")
            && !rest.is_empty()
        {
            let mut nums = numbers(rest);
            let phone = nums.next().unwrap_or(0);
            let rating = nums.next().unwrap_or(0);
            if let Some(i) = self.saved_accounts.players.iter().position(|a| a.phone_number as i32 == phone) {
                let a = &mut self.saved_accounts.players[i];
                a.crim_rating = rating as u32;
                let (id, name, phone) = (a.account_id, account_name(a), a.phone_number as i32);
                for (_, p) in self.players.iter_mut().filter(|(_, p)| p.account_id == id) {
                    p.crim_rating = rating;
                }
                self.admin_say(&format!("{name}({phone}) {rating} rating"));
            }
        }
        if let Some(rest) = msg.strip_prefix("/bany ")
            && !rest.is_empty()
            && let Ok(time) = scan_int(rest)
        {
            let time = time.unwrap_or(0);
            if let Some(i) = self.admin.pending_ban.filter(|&i| i < self.saved_accounts.players.len()) {
                let a = &mut self.saved_accounts.players[i];
                a.ban_time = time as u32;
                let line = format!("{}({}) banned {time}", account_name(a), a.phone_number as i32);
                self.admin_say(&line);
                self.admin.pending_ban = None;
            }
        }
        if msg.starts_with("/bann") {
            self.admin_say("ban canceled");
            self.admin.pending_ban = None;
        }
        if let Some(name) = msg.strip_prefix("/kick ") {
            let target = self.connection_order().into_iter().filter(|c| self.clients.get(c).and_then(|c| self.players.get(c.player_id.idx())).is_some_and(|p| p.username == name)).last();
            if let Some(c) = target.and_then(|t| self.clients.get_mut(&t)) {
                c.timeout = KICK_TIMEOUT;
                let ip = c.addr.ip();
                self.admin.kicked.push(ip);
            }
        }
        if let Some(text) = msg.strip_prefix("/message ") {
            self.send_chat(text, ChatType::Announce, -1, 0);
        }
        if msg.starts_with("/clearkick") {
            self.admin.kicked.clear();
        }
    }

    /// The connections in the order they joined, standing in for the game's connection array.
    fn connection_order(&self) -> Vec<ConnId> {
        let mut conns: Vec<ConnId> = self.clients.keys().copied().collect();
        conns.sort_by_key(|c| c.0);
        conns
    }

    fn admin_spawn_vehicle(&mut self, pid: PlayerId, kind: usize) {
        let Some(h) = self.players.get(pid.idx()).and_then(|p| p.human).and_then(|h| self.humans.get(h)) else { return };
        let (p, r) = (h.bones[0].pos, h.bones[0].rot[2]);
        let pos = Vec3::new(r.x * SPAWN_AHEAD + p.x, r.y * SPAWN_AHEAD + p.y, SPAWN_AHEAD * r.z + p.z);
        self.spawn_vehicle(kind, SPAWN_COLOR, pos, IDENTITY);
    }

    /// /phone: every admin phone hangs up and despawns, then a new one goes in the admin's pocket.
    fn admin_phone(&mut self, pid: PlayerId) {
        for id in self.items.ids() {
            let Some(item) = self.items.get(id) else { continue };
            let Some(p) = item.state.phone().filter(|p| item.item_type == ItemKind::Phone && p.number == ADMIN_PHONE) else { continue };
            if let Some(other) = p.connected {
                if let Some(o) = self.items.get_mut(other).and_then(|i| i.state.phone_mut()) {
                    o.connected = None;
                    o.status = PhoneStatus::Idle;
                    o.display_number = 0;
                }
                self.phone_update(other);
            }
            if let Some(item) = self.items.get_mut(id) {
                item.despawn_time = 0;
            }
        }
        let Some(h) = self.players.get(pid.idx()).and_then(|p| p.human) else { return };
        let Some((pos, rot)) = self.humans.get(h).map(|hu| (hu.bones[0].pos, hu.bones[0].rot)) else { return };
        let Some(id) = self.create_item(ItemKind::Phone, pos, None, rot) else { return };
        if let Some(p) = self.items.get_mut(id).and_then(|i| i.state.phone_mut()) {
            p.number = ADMIN_PHONE;
        }
        self.link_to_slot(h, id, ADMIN_POCKET);
    }

    /// /arm [1-5]: a gun with four magazines and two bandages.
    fn admin_arm(&mut self, pid: PlayerId, rest: &str) {
        let Some(h) = self.players.get(pid.idx()).and_then(|p| p.human) else { return };
        let n = if rest.is_empty() { None } else { scan_int(rest).ok().map(|n| n.unwrap_or(0)) };
        let gun = n.and_then(|n| usize::try_from(n - 1).ok()).filter(|&i| i < ARM_GUNS.len()).map_or(ARM_GUNS[ARM_DEFAULT], |i| ARM_GUNS[i]);
        self.give_weapon(h, gun, ARM_MAGAZINES);
        let Some((pos, rot)) = self.humans.get(h).map(|hu| (hu.bones[0].pos, hu.bones[0].rot)) else { return };
        for _ in 0..ARM_BANDAGES {
            if let Some(id) = self.create_item(ItemKind::Bandage, pos, None, rot) {
                self.link_to_slot(h, id, ADMIN_POCKET);
            }
        }
    }

    /// Admin actions (logic_playerinteractions types 4 and 5): an account's ban time or criminal rating.
    pub(crate) fn admin_action(&mut self, pid: PlayerId, kind: u8, account: u32, value: u32) {
        if !self.players.get(pid.idx()).is_some_and(|p| p.is_admin) {
            return;
        }
        let (i, value) = (account as i32, value as i32);
        if i < 0 || i as usize >= self.saved_accounts.players.len() || value < 0 {
            return;
        }
        let a = &mut self.saved_accounts.players[i as usize];
        let line = match kind {
            4 => {
                a.ban_time = value as u32;
                format!("{} {i} banned {value}", account_name(a))
            }
            5 => {
                a.crim_rating = value as u32;
                let id = a.account_id;
                let line = format!("{} {i} rating {value}", account_name(a));
                for (_, p) in self.players.iter_mut().filter(|(_, p)| p.account_id == id) {
                    p.crim_rating = value;
                }
                line
            }
            _ => return,
        };
        self.admin_say(&line);
    }

    /// Whether packets from this address are ignored (a /kick until /clearkick).
    pub(crate) fn is_kicked(&self, ip: IpAddr) -> bool {
        self.admin.kicked.contains(&ip)
    }

    /// The connection pass of server_recv_and_dispatch: every connection's silence counts up, and those of banned
    /// accounts or quiet for 1800 ticks are dropped.
    pub(crate) fn connection_timeouts(&mut self) {
        for c in self.clients.values_mut() {
            c.timeout += 1;
        }
        for conn in self.connection_order() {
            let Some(c) = self.clients.get(&conn) else { continue };
            let banned = self.players.get(c.player_id.idx()).and_then(|p| self.saved_accounts.players.iter().find(|a| a.account_id == p.account_id)).is_some_and(|a| a.ban_time > 0);
            if banned || c.timeout >= TIMEOUT || self.players.get(c.player_id.idx()).is_none() {
                self.on_leave(conn);
            }
        }
    }

    /// /resetgame's request, taken up by logic_simulation: round or eliminator mode starts over as the other with
    /// everyone spectating, anything else just resets.
    pub(crate) fn admin_reset(&mut self) {
        if !self.admin.reset_requested {
            return;
        }
        if let Some(mode) = self.admin.next_mode.filter(|m| matches!(m, GameMode::Round | GameMode::Eliminator) && *m != self.gamemode) {
            // TODO: reset_game loads the round map for these modes; switching from another map needs map reloading
            if self.world.map.map_name == "round" {
                for (_, p) in self.players.iter_mut() {
                    p.team = Team::Spectator;
                }
                self.gamemode = mode;
            }
        }
        self.reset_game();
        self.admin.reset_requested = false;
    }

    /// send_adminpacket for each admin connection whose turn it is.
    pub(crate) fn send_admin_lists(&mut self) {
        // TODO: the rows are filled by the replay recorder (replay_writepacket); without it every row is empty
        let packet = AdminList { rows: vec![None; ADMIN_LIST_ROWS] };
        let tick = self.tick;
        for (i, conn) in self.connection_order().into_iter().enumerate() {
            let Some(c) = self.clients.get(&conn) else { continue };
            if c.admin_visible && (tick ^ i as u32) & ADMIN_LIST_PERIOD == 0 {
                let _ = self.out_tx.send((frame_packet(packet.clone()), c.addr));
            }
        }
    }
}
