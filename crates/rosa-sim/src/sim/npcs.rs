use glam::Vec3;
use rosa_protocol::{
    Team,
    clientbound::game::{
        ItemKind,
        events::{Event, ServerEvent, update_player::EventUpdatePlayer},
    },
};

use super::Sim;
use crate::{PlayerId, rng::rand};

pub const MAX_NPCS: usize = 0x400;
/// An NPC's bot comes in when a living player is within 96 of it, stays while one is within 104, and leaves when
/// none is; a killed one is back 18000 ticks later. Each NPC is looked at every 32 ticks, in turn.
const NEAR: f32 = 96.0;
const KEEP: f32 = 104.0;
const RESPAWN: i32 = 18000;
const CHECK_MASK: u32 = 0x1f;
const GUARD: i32 = 0;
const ZOMBIE: i32 = 1;
const NPC_TEAM: Team = Team::Mission;
const GUARD_MAGAZINES: i32 = 3;

/// NpcData (0x328 each): a guard or zombie waiting at its post, brought in as a bot near players.
#[derive(Clone, Debug)]
pub struct Npc {
    pub kind: i32,
    pub pos: Vec3,
    pub yaw: f32,
    pub waypoints: Vec<Vec3>,
    pub bot: Option<PlayerId>,
    pub human: Option<usize>,
    pub timer: i32,
}

impl Sim {
    /// create_npc: the first free NPC slot.
    pub(crate) fn create_npc(&mut self, kind: i32, pos: Vec3, yaw: f32) -> Option<usize> {
        let npc = Npc { kind, pos, yaw, waypoints: Vec::new(), bot: None, human: None, timer: 0 };
        if let Some(i) = self.npcs.iter().position(Option::is_none) {
            self.npcs[i] = Some(npc);
            return Some(i);
        }
        if self.npcs.len() >= MAX_NPCS {
            return None;
        }
        self.npcs.push(Some(npc));
        Some(self.npcs.len() - 1)
    }

    /// do_npc.
    pub(crate) fn do_npc(&mut self) {
        for idx in 0..self.npcs.len() {
            let Some(npc) = self.npcs[idx].as_mut() else { continue };
            if npc.timer > 0 {
                npc.timer -= 1;
            }
            if (self.tick ^ idx as u32) & CHECK_MASK != 0 {
                continue;
            }
            if npc.timer != 0 {
                self.despawn_npc_bot(idx);
                continue;
            }
            let centre = npc.human.and_then(|h| self.humans.get(h)).map_or(npc.pos, |h| h.pos);
            let mut state = 0;
            for (_, h) in self.humans.iter() {
                let Some(p) = h.player.and_then(|p| self.players.get(p.idx())) else { continue };
                if p.is_bot || h.old_health <= 0 {
                    continue;
                }
                if state == 2 {
                    continue;
                }
                let b = h.bones[0].pos;
                let (dx, dz) = (centre.x - b.x, centre.z - b.z);
                let d = ((dx * dx + 0.0) + dz * dz).sqrt();
                if NEAR > d {
                    state = 2;
                } else if KEEP > d {
                    state = 1;
                }
            }
            match state {
                2 => {
                    if self.npcs[idx].as_ref().is_some_and(|n| n.human.is_none()) {
                        self.spawn_bot(idx);
                    }
                }
                0 => {
                    self.despawn_npc_bot(idx);
                    continue;
                }
                _ => {}
            }
            self.check_npc_dead(idx);
        }
    }

    /// The end of do_npc's check: a killed NPC lets its bot go and waits 18000 ticks.
    fn check_npc_dead(&mut self, idx: usize) {
        let Some(h) = self.npcs[idx].as_ref().and_then(|n| n.human) else { return };
        if self.humans.get(h).is_some_and(|h| h.old_health > 0) {
            return;
        }
        let bot = self.npcs[idx].as_mut().and_then(|n| {
            n.human = None;
            n.timer = RESPAWN;
            n.bot.take()
        });
        if let Some(pid) = bot {
            self.delete_bot_player(pid);
        }
    }

    /// The NPC's bot leaves: its human's held items despawn and the human goes, then the bot player.
    fn despawn_npc_bot(&mut self, idx: usize) {
        let Some(n) = self.npcs[idx].as_mut() else { return };
        let (human, bot) = (n.human.take(), n.bot.take());
        if let Some(h) = human {
            self.clear_inventory(h);
            self.delete_human(h);
        }
        if let Some(pid) = bot {
            self.delete_bot_player(pid);
        }
    }

    /// clear_inventory: every item in the human's slots (and the first item mounted on each) despawns.
    fn clear_inventory(&mut self, h: usize) {
        let Some(hu) = self.humans.get(h) else { return };
        let held: Vec<usize> = hu.inventory.iter().flat_map(|s| s.items[..s.count.max(0) as usize].iter().map(|&i| i as usize)).collect();
        for id in held {
            if let Some(&child) = self.items.get(id).and_then(|i| i.children.first()) {
                if let Some(c) = self.items.get_mut(child) {
                    c.despawn_time = 0;
                }
                super::items::remove_link(&mut self.items, child, id);
            }
            if let Some(i) = self.items.get_mut(id) {
                i.despawn_time = 0;
            }
        }
    }

    /// delete_player for a bot: its human (if still its own) is left without a player, and the player goes.
    fn delete_bot_player(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get(pid.idx()) else { return };
        let (human, account) = (p.human, p.account_id);
        if let Some(h) = human.and_then(|h| self.humans.get_mut(h))
            && h.player == Some(pid)
        {
            h.player = None;
            h.account = (account != u32::MAX).then_some(account);
        }
        let p = self.players.remove(pid.idx());
        let e = EventUpdatePlayer { active: false, client_id: p.player_id.0, is_bot: p.is_bot, human_id: p.human.map_or(-1, |h| h as i32), team: p.team, customization: p.customization, name: p.username };
        self.events.push(Event { tick_created: self.tick, kind: ServerEvent::UpdatePlayer(e) });
    }

    /// spawn_bot: a bot player (team 7, the NPC look) walking the NPC's route, its human at the NPC's post facing
    /// its way; a guard carries a pistol or an MP5 with three magazines, a zombie hunts bare-handed.
    fn spawn_bot(&mut self, idx: usize) {
        let Some(pid) = self.create_player() else { return };
        let Some(npc) = self.npcs[idx].clone() else { return };
        self.npcs[idx].as_mut().unwrap().bot = Some(pid);
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.team = NPC_TEAM;
        p.username = String::new();
        p.is_bot = true;
        p.customization.gender = 1;
        p.customization.model = 2;
        p.customization.suit_color = 1;
        p.bot.waypoint = 0;
        p.bot.waypoint_count = npc.waypoints.len() as i32;
        for (k, &w) in npc.waypoints.iter().enumerate() {
            p.bot.set_waypoint(k, w);
        }
        let e = p.make_update_player_event(self.tick);
        self.events.push(e);
        let mut rot = rosa_physics::rotation::IDENTITY;
        rosa_physics::rotation::rotate_orientation(&mut rot, Vec3::Y, npc.yaw);
        let human = self.spawn_human(npc.pos, &rot, Some(pid));
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.human = human;
        }
        let Some(h) = human else { return };
        self.npcs[idx].as_mut().unwrap().human = Some(h);
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.bot.idle_yaw = npc.yaw;
            p.controls[4] = 0.0;
        }
        if let Some(hu) = self.humans.get_mut(h) {
            hu.view_yaw = npc.yaw;
        }
        match npc.kind {
            ZOMBIE => {
                if let Some(p) = self.players.get_mut(pid.idx()) {
                    p.bot.is_zombie = true;
                }
            }
            GUARD => {
                let gun = if rand() & 1 != 0 { ItemKind::Pistol } else { ItemKind::Mp5 };
                self.give_weapon(h, gun, GUARD_MAGAZINES);
            }
            _ => {}
        }
    }
}

impl Sim {
    pub fn npc_list(&self) -> &[Option<Npc>] {
        &self.npcs
    }

    pub fn run_do_npc(&mut self) {
        self.do_npc();
    }

    /// A non-bot player from create_player on the given team, with a human at `pos`.
    pub fn npc_test_player(&mut self, team: Team, pos: Vec3) -> Option<(PlayerId, usize)> {
        let pid = self.create_player()?;
        let p = self.players.get_mut(pid.idx())?;
        p.team = team;
        p.is_bot = false;
        let h = self.spawn_human(pos, &rosa_physics::rotation::IDENTITY, Some(pid))?;
        self.players.get_mut(pid.idx())?.human = Some(h);
        Some((pid, h))
    }
}
