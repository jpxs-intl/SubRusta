use std::collections::{BTreeMap, HashMap};

use glam::Vec3;
use rosa_protocol::clientbound::game::{GameState, ItemKind};

use super::{Earshot, Sim, TickCtx};
use crate::{Client, PlayerId};

/// A speaker's reach by volume level: whisper, talk and shout.
const WHISPER_RANGE: f32 = 8.0;
const TALK_RANGE: f32 = 64.0;
const SHOUT_RANGE: f32 = 128.0;
/// Where everyone is heard from while the round restarts, and between players without humans.
const NEAR_DISTANCE: f32 = 4.0;
/// How far an earshot's volume moves towards the line of sight each tick.
const SMOOTHING: f32 = 0.125;
/// Keeps a silent earshot's distance / volume finite when an earshot is evicted.
const EVICT_EPSILON: f32 = 0.01;
/// Line of sight is only traced this far; past it, or through the level, a voice is at half volume.
const SIGHT_RANGE: f32 = 128.0;
const BLOCKED_VOLUME: f32 = 0.5;
/// The channels walkie-talkies can be on, and how many radios each lists.
pub(crate) const RADIO_CHANNELS: usize = 128;
const CHANNEL_RADIOS: usize = 256;

/// What calculate_voice reads of an item: its type and position, who holds it in which slot, the phone it is on a
/// call with and whether the call is connected, and a radio's channel and whether it is transmitting.
#[derive(Clone, Copy, Debug)]
pub(crate) struct VoiceItem {
    pub kind: ItemKind,
    pub pos: Vec3,
    pub parent_human: i32,
    pub parent_slot: i32,
    pub connected: Option<usize>,
    pub on_call: bool,
    pub transmitting: bool,
    pub channel: i32,
}

fn range(level: u8) -> f32 {
    match level {
        0 => WHISPER_RANGE,
        1 => TALK_RANGE,
        _ => SHOUT_RANGE,
    }
}

fn distance(a: Vec3, b: Vec3) -> f32 {
    let d = Vec3::new(a.x - b.x, a.y - b.y, a.z - b.z);
    ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt()
}

/// calc_earshot: a new earshot in the first free slot, else in place of slot 0 when that one is worse heard
/// (connection_find_earshot_slot only ever settles on slot 0).
fn add_earshot(earshots: &mut [Option<Earshot>; 8], e: Earshot) {
    if let Some(free) = earshots.iter_mut().find(|s| s.is_none()) {
        *free = Some(e);
        return;
    }
    if let Some(first) = earshots[0]
        && first.distance / (first.volume + EVICT_EPSILON) > e.distance / e.volume
    {
        earshots[0] = Some(e);
    }
}

impl Sim {
    /// The walkie-talkie channel lists logic_item rebuilds each tick: every radio, in item order, on its channel.
    pub(crate) fn build_radio_channels(&mut self) {
        self.radio_channels = vec![Vec::new(); RADIO_CHANNELS];
        for (id, item) in self.items.iter() {
            if let super::item_state::ItemState::Radio { channel, .. } = item.state {
                let list = &mut self.radio_channels[(channel & 0x7f) as usize];
                if list.len() < CHANNEL_RADIOS {
                    list.push(id);
                }
            }
        }
    }

    /// What calculate_voice needs of each item, in item order.
    pub(crate) fn voice_items(&self) -> BTreeMap<usize, VoiceItem> {
        self.items
            .iter()
            .map(|(id, i)| {
                let phone = i.state.phone();
                let (transmitting, channel) = match i.state {
                    super::item_state::ItemState::Radio { channel, transmitting } => (transmitting, channel),
                    _ => (false, 0),
                };
                let v = VoiceItem {
                    kind: i.item_type,
                    pos: i.pos2,
                    parent_human: i.parent_human,
                    parent_slot: i.parent_slot,
                    connected: phone.and_then(|p| p.connected),
                    on_call: phone.is_some_and(|p| p.status == super::item_state::PhoneStatus::Connected),
                    transmitting,
                    channel,
                };
                (id, v)
            })
            .collect()
    }

    /// calculate_voice: keeps each voice the client can hear in the same one of its 8 slots while it stays audible.
    /// The client listens from its human's head, else the human it spectates (else human 0). Two humans hear each
    /// other within a range set by the speaker's volume level, halved without line of sight; players without humans
    /// hear each other, and everyone hears everyone while the round restarts. A speaker on a connected phone held in
    /// the right hand is heard through the phone at the other end, and one transmitting on a radio in hand through
    /// every radio on its channel, by a listener within reach of that item (1, or 0.5 without line of sight).
    pub(super) fn calculate_earshot(client: &mut Client, player_id: PlayerId, ctx: &TickCtx) -> [Option<Earshot>; 8] {
        let restarting = ctx.gamestate == GameState::Restarting;
        let own = ctx.players.get(player_id.idx()).and_then(|p| p.human);
        let head = |h: usize| ctx.heads.get(&h).map(|&(pos, _)| pos);
        let at = own.and_then(head).or_else(|| client.spectating.and_then(head)).or_else(|| head(0)).unwrap_or(Vec3::ZERO);
        let map = &ctx.world.map;
        let line_of_sight = |from: Vec3, to: Vec3| -> f32 {
            if distance(to, from) <= SIGHT_RANGE && crate::world::trace::line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, from, to).is_none() { 1.0 } else { BLOCKED_VOLUME }
        };
        let items = &ctx.voice_items;
        let silenced = |p: PlayerId| ctx.players.get(p.idx()).is_none_or(|p| p.voice.is_silenced);
        let speaker_of = |it: &VoiceItem| usize::try_from(it.parent_human).ok().and_then(|h| ctx.human_players.get(&h).copied().flatten());
        for slot in client.earshots.iter_mut() {
            let Some(e) = slot else { continue };
            let Some(speaker) = ctx.players.get(e.player.idx()) else {
                *slot = None;
                continue;
            };
            let level = speaker.voice.volume_level;
            let mut keep = !speaker.voice.is_silenced;
            if let Some(item) = e.item {
                let Some(it) = items.get(&item) else {
                    *slot = None;
                    continue;
                };
                match it.kind {
                    ItemKind::Phone => {
                        keep &= it.connected.and_then(|j| items.get(&j)).is_some_and(|j| j.parent_human != -1 && j.parent_slot == 0 && j.connected.is_some() && j.on_call);
                    }
                    ItemKind::Radio => {
                        keep &= e.source.and_then(|k| items.get(&k)).is_some_and(|k| k.parent_human != -1 && k.parent_slot <= 1 && k.transmitting);
                    }
                    _ => {}
                }
                e.distance = distance(it.pos, at);
                e.volume = (line_of_sight(it.pos, at) - e.volume) * SMOOTHING + e.volume;
                if !keep || e.distance > e.volume {
                    *slot = None;
                }
                continue;
            }
            e.human = speaker.human;
            if restarting {
                if !keep {
                    *slot = None;
                }
                continue;
            }
            match (own, e.human) {
                (None, None) => {}
                (Some(_), None) => keep = false,
                (_, Some(h)) => {
                    if !ctx.heads.get(&0).is_some_and(|&(_, alive)| alive) {
                        keep = false;
                    }
                    let Some(pos) = head(h) else {
                        *slot = None;
                        continue;
                    };
                    e.distance = distance(pos, at);
                    e.volume = (line_of_sight(pos, at) - e.volume) * SMOOTHING + e.volume;
                }
            }
            if !keep || e.distance > e.volume * range(level) {
                *slot = None;
            }
        }
        for (_, p) in ctx.players.iter() {
            let id = p.player_id;
            if id == player_id || p.voice.is_silenced || client.earshots.iter().flatten().any(|e| e.player == id && e.human == p.human) {
                continue;
            }
            let (d, volume) = if restarting || (own.is_none() && p.human.is_none()) {
                (NEAR_DISTANCE, 1.0)
            } else {
                let Some(&(pos, alive)) = p.human.and_then(|h| ctx.heads.get(&h)) else { continue };
                if !alive {
                    continue;
                }
                (distance(pos, at), line_of_sight(pos, at))
            };
            if range(p.voice.volume_level) * volume > d {
                add_earshot(&mut client.earshots, Earshot { player: id, human: p.human, item: None, source: None, distance: d, volume });
            }
        }
        for (&a, it) in items.iter() {
            match it.kind {
                ItemKind::Phone => {
                    let Some(b) = it.connected else { continue };
                    if it.parent_human == -1 || it.parent_slot != 0 || !it.on_call {
                        continue;
                    }
                    let Some(speaker) = speaker_of(it).filter(|&s| !silenced(s)) else { continue };
                    if client.earshots.iter().flatten().any(|e| e.player == speaker && e.item == Some(b)) {
                        continue;
                    }
                    let Some(other) = items.get(&b).filter(|o| o.connected == Some(a)) else { continue };
                    let (d, volume) = (distance(other.pos, at), line_of_sight(other.pos, at));
                    if volume > d {
                        add_earshot(&mut client.earshots, Earshot { player: speaker, human: None, item: Some(b), source: other.connected, distance: d, volume });
                    }
                }
                ItemKind::Radio => {
                    if !it.transmitting || it.parent_human == -1 || it.parent_slot > 1 {
                        continue;
                    }
                    let Some(speaker) = speaker_of(it).filter(|&s| !silenced(s)) else { continue };
                    for &r in ctx.radio_channels.get((it.channel & 0x7f) as usize).map_or(&[][..], |l| &l[..]) {
                        if r == a || client.earshots.iter().flatten().any(|e| e.player == speaker && e.item == Some(r)) {
                            continue;
                        }
                        let Some(other) = items.get(&r) else { continue };
                        let (d, volume) = (distance(other.pos, at), line_of_sight(other.pos, at));
                        if volume > d {
                            add_earshot(&mut client.earshots, Earshot { player: speaker, human: None, item: Some(r), source: Some(a), distance: d, volume });
                        }
                    }
                }
                _ => {}
            }
        }
        client.earshots
    }
}

/// Which player each human belongs to, for the speakers holding phones and radios.
pub(crate) fn human_players(humans: &rosa_physics::Table<crate::human::Human>) -> HashMap<usize, Option<PlayerId>> {
    humans.iter().map(|(id, h)| (id, h.player)).collect()
}
