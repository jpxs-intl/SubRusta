use glam::Vec3;
use rosa_physics::rotation::{IDENTITY, rotate_orientation};
use rosa_protocol::clientbound::game::ItemKind;

use super::{Sim, corporations::TEAM_NAMES, item_state::ItemState};
use crate::computer::links::{ITEM_SLOTS, KIND_MEMO, Link};

/// A memo holds up to 1023 characters (item +0x368), sent in 64 character blocks.
pub const MEMO_LEN: usize = 0x400;
const BLOCK: usize = 64;
/// The newspaper is made of readme.txt's lines (124 characters each, up to 1021) and the 10 richest accounts.
const LINE_LEN: usize = 0x7c;
const MAX_LINES: usize = 0x3fd;
const NEWSPAPER_LEN: usize = 0x3fd;
const TOP: usize = 10;
/// The world clock: an hour is 216000, a minute 3600; noon is 0x278d00.
const HOUR: i32 = 0x34bc0;
const MINUTE: i32 = 0xe10;
const NOON: i32 = 0x278cff;
const MEMO_BACK: f32 = -0.5;
const KEEP: i32 = 0x10000;

/// sprintf_comma: the amount with thousands separated by commas.
pub fn with_commas(n: i32) -> String {
    if n > 999_999_999 {
        format!("{},{:03},{:03},{:03}", n / 1_000_000_000, (n / 1_000_000) % 1000, (n / 1000) % 1000, n % 1000)
    } else if n > 999_999 {
        format!("{},{:03},{:03}", n / 1_000_000, (n / 1000) % 1000, n % 1000)
    } else if n > 999 {
        format!("{},{:03}", n / 1000, n % 1000)
    } else {
        format!("{n}")
    }
}

/// The time of day the clock shows, as hour (1 to 12), minute and whether it is afternoon.
pub fn clock_time(t: i32) -> (i32, i32, bool) {
    let h = (t / HOUR) % 12;
    (if h == 0 { 12 } else { h }, (t / MINUTE) % 60, t > NOON)
}

impl Sim {
    /// write_memo_contents: the memo's text replaced, its old blocks dropped and each 64 character block linked.
    pub(crate) fn write_memo(&mut self, id: usize, text: &[u8]) {
        let Some(item) = self.items.get(id) else { return };
        let (mask, links) = (item.link_mask, item.links);
        for k in 0..ITEM_SLOTS {
            if mask & (1 << k) != 0 {
                self.links.free(links[k]);
            }
        }
        let n = (text.iter().position(|&c| c == 0).unwrap_or(text.len()) + 1).min(MEMO_LEN - 1);
        let Some(item) = self.items.get_mut(id) else { return };
        item.link_mask = 0;
        let ItemState::Memo(buf) = &mut item.state else { return };
        for (k, b) in buf.iter_mut().enumerate().take(n) {
            *b = text.get(k).copied().unwrap_or(0);
        }
        buf[MEMO_LEN - 1] = 0;
        for block in 0..=(n >> 6) {
            self.memo_block(id, block);
        }
    }

    /// enqueue_item_customdata_block: a link carrying 64 characters of the memo, in its first free slot.
    fn memo_block(&mut self, id: usize, block: usize) {
        let Some(free) = self.links.links.iter().position(|k| !k.active) else { return };
        let Some(item) = self.items.get(id) else { return };
        let ItemState::Memo(buf) = &item.state else { return };
        let text = buf[block * BLOCK..(block * BLOCK + BLOCK).min(MEMO_LEN)].to_vec();
        self.links.links[free] = Link { active: true, kind: KIND_MEMO, tick: self.tick as i32, item: id as i32, line: block as i32, text, colors: Vec::new() };
        let Some(slot) = (0..ITEM_SLOTS).find(|&k| item.link_mask & (1 << k) == 0) else { return };
        if let Some(item) = self.items.get_mut(id) {
            item.links[slot] = free as i32;
            item.link_mask |= 1 << slot;
        }
        for client in self.clients.values_mut() {
            if let Some(m) = client.link_sent.get_mut(&id) {
                *m &= !(1u64 << slot);
            }
        }
    }

    /// newspaper_load: readme.txt, a blank line, then the 10 accounts with the most money, joined by newlines.
    pub(crate) fn load_newspaper(&mut self) {
        let mut lines: Vec<Vec<u8>> = Vec::new();
        if let Ok(data) = std::fs::read("readme.txt") {
            let mut k = 0;
            while k < data.len() && lines.len() <= MAX_LINES {
                let mut line = Vec::new();
                while k < data.len() && data[k] != b'\n' && data[k] != b'\r' && line.len() < LINE_LEN - 1 {
                    line.push(data[k]);
                    k += 1;
                }
                while k < data.len() && (data[k] == b'\n' || data[k] == b'\r') {
                    k += 1;
                }
                lines.push(line);
            }
        }
        lines.push(Vec::new());
        lines.push(b"Top 10 list".to_vec());
        let mut order: Vec<usize> = (0..self.saved_accounts.players.len()).collect();
        order.sort_by_key(|&i| (0x8fff_ffffu32.wrapping_sub(self.saved_accounts.players[i].money)) as i64);
        for (rank, &i) in order.iter().take(TOP).enumerate() {
            let a = &self.saved_accounts.players[i];
            let name: String = a.player_name.iter().take_while(|&&c| c != 0).map(|&c| c as char).collect();
            lines.push(format!("{}. {name}   ${}", rank + 1, with_commas(a.money as i32)).into_bytes());
        }
        let mut paper: Vec<u8> = Vec::new();
        for line in &lines {
            if line.len() + paper.len() <= NEWSPAPER_LEN {
                paper.extend_from_slice(line);
            }
            if paper.len() <= NEWSPAPER_LEN {
                paper.push(b'\n');
            }
        }
        self.world_state.newspaper = paper;
    }

    /// place_corporation_mission_memo: a memo headed with the corporation and the time, on the corporation's table.
    pub(crate) fn place_corporation_memo(&mut self, k: usize, text: &[u8]) {
        let (h, m, pm) = clock_time(self.world_time);
        let mut memo = format!("\n{} Internal Memo\n{}:{:02} {}\n\n", TEAM_NAMES[k], h, m, if pm { "PM" } else { "AM" }).into_bytes();
        memo.extend_from_slice(&text[..text.iter().position(|&c| c == 0).unwrap_or(text.len())]);
        memo.truncate(0x7ff);
        let base = &self.world.map.level.bases[k];
        let mut rot = IDENTITY;
        let axis = rot[1];
        rotate_orientation(&mut rot, axis, (base.table_orientation as f64 - 90.0_f64.to_radians()) as f32);
        let t = base.table;
        let r0 = rot[0];
        let pos = Vec3::new(r0.x * MEMO_BACK + t.x, r0.y * MEMO_BACK + t.y, MEMO_BACK * r0.z + t.z);
        let Some(id) = self.create_item(ItemKind::Paper, pos, None, rot) else { return };
        self.write_memo(id, &memo);
        if let Some(i) = self.items.get_mut(id) {
            i.despawn_time = KEEP;
        }
    }

    /// Ctrl-P in the decrypter: a memo of the screen half a step in front of the computer, 4 per corporation.
    pub(crate) fn print_to_memo(&mut self, id: usize, c: &crate::computer::Computer) {
        const PRINTS: i32 = 3;
        let Some(k) = usize::try_from(c.team).ok().filter(|&k| k < self.corp_state.len()) else { return };
        if self.corp_state[k].prints > PRINTS {
            return;
        }
        let Some(body) = self.items.get(id).and_then(|i| self.bodies.get(i.body)) else { return };
        let (p, rot) = (body.pos, body.rot);
        let pos = Vec3::new(rot[0].x * 0.5 + p.x, rot[0].y * 0.5 + p.y, 0.5 * rot[0].z + p.z);
        let Some(memo) = self.create_item(ItemKind::Paper, pos, None, rot) else { return };
        self.print_screen(memo, c);
        self.corp_state[k].prints += 1;
        if let Some(i) = self.items.get_mut(memo) {
            i.physics_settled = true;
        }
    }

    /// computer_print_screen_to_memo: the 23 lines from the top of the screen, each ended by a newline.
    pub(crate) fn print_screen(&mut self, memo: usize, c: &crate::computer::Computer) {
        let mut text = Vec::new();
        let mut l = c.top_line;
        for _ in 0..23 {
            let line = &c.lines[l as usize & 31];
            let n = crate::computer::text_len(line);
            if text.len() + n + 1 < MEMO_LEN {
                text.extend_from_slice(&line[..n]);
                text.push(b'\n');
            }
            l = (l + 1) & 31;
        }
        self.write_memo(memo, &text);
    }
}

impl Sim {
    pub fn memo_text(&self, id: usize) -> Option<&[u8; MEMO_LEN]> {
        match &self.items.get(id)?.state {
            ItemState::Memo(b) => Some(b),
            _ => None,
        }
    }
}
