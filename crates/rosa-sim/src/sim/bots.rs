use std::f64;

use glam::Vec3;
use rosa_protocol::{
    clientbound::game::GameState,
    serverbound::game::actions::{GameAction, InventoryAction},
};

use super::Sim;
use crate::{PlayerId, player::BotTarget, world::trace::line_intersect_level};

/// How far the view may turn from the body, either way.
const MAX_LOOK: f64 = f64::from_bits(0x4005fdbbe9bba90d);
const LOOK_LIMIT: f32 = f32::from_bits(0x402feddf);
/// A bot remembers up to 16 enemies within 128 of its chest, sure of them up to 16; every other tick it looks again
/// and its memory fades (x 31/32) until it forgets below 0.875.
const MAX_TARGETS: usize = 16;
const SIGHT_RANGE: f32 = 128.0;
const MAX_AWARENESS: f32 = 16.0;
const FADE: f32 = 0.96875;
const FORGET: f32 = 0.875;
const TARGET_TIMER: i32 = 600;
/// Each sighting pulls the remembered place an eighth of the way to where the enemy is going (4 ticks ahead), give or
/// take a spread that shrinks as the bot grows sure.
const SIGHTING_KEEP: f32 = 0.875;
const SIGHTING_STEP: f32 = 0.125;
const LEAD_TICKS: f32 = 4.0;
const SPREAD_SCALE: f32 = 0.25;
/// Thresholds on awareness: below 2 a bot goes to look, above 8 it aims, above 12 it attacks on foot.
const INVESTIGATE: f32 = 2.0;
const AIM: f32 = 8.0;
const ATTACK: f32 = 12.0;
/// On foot a bot stops walking within 32 of its target and fires within 64; in a vehicle it fires within 32.
const STOP_RANGE: f32 = 32.0;
const FIRE_RANGE: f32 = 64.0;
const TEAMMATE_RANGE_FOOT: f32 = 128.0;
const TEAMMATE_RANGE_SEATED: f32 = 64.0;
const SHOT_LENGTH: f32 = -64.0;
const SHOT_RADIUS: f32 = 0.25;
/// Aim is good when within 0.125 of the target in yaw and pitch, and a seated bot's gun must point within 0.9975.
const AIM_TOLERANCE: f32 = 0.125;
const GUN_ALIGNMENT: f32 = -0.9975;
const EYE_DROP: f32 = 0.25;
const TURN_RATE: f32 = 0.125;
const HALF: f32 = 0.5;
/// A waypoint counts as reached within 2.
const WAYPOINT_RADIUS: f32 = 2.0;
/// A bot whose pelvis tips past this lies down and crouches half the time.
const UPRIGHT: f32 = 0.9;
/// The seated bots look out at these headings: front passenger, then the back seats left and right.
const SEAT_YAW: [f32; 3] = [f32::from_bits(0x3f490fdb), f32::from_bits(0xc016cbe4), f32::from_bits(0x4016cbe4)];
const WRECK_PATIENCE: i32 = 179;
/// The driver's gear stick: second gear, forward; reversing out of a jam after 180 and 280 stuck ticks.
const DRIVE_GEAR_X: f32 = 2.0;
const DRIVE_GEAR_Y: f32 = -1.0;
const STUCK_FORWARD: i32 = 0xb3;
const STUCK_REVERSE: i32 = 0x117;
const THROTTLE_SCALE: f32 = 8.0;
const MIN_TARGET_SPEED: f32 = 0.05;
/// Input bits a bot presses: fire, crouch, get out, chamber a round.
const INPUT_FIRE: u32 = 0x1;
const INPUT_CROUCH: u32 = 0x8;
const INPUT_EXIT: u32 = 0x800;
const INPUT_CHAMBER: u32 = 0x1000;

/// The difference from `a` to `b` taken the short way, the way player_ai wraps it: `b` is moved up a turn when `a`
/// is half a turn past it, then the difference has `a` moved up a turn when still half a turn out.
fn wrap(a: f32, b: f32) -> f32 {
    let t = if ((a - b) as f64) >= std::f64::consts::PI { ((b as f64) + (std::f64::consts::PI * 2.0)) as f32 } else { b };
    let d = t - a;
    if (d as f64) >= std::f64::consts::PI { t - ((a as f64 + (std::f64::consts::PI * 2.0)) as f32) } else { d }
}

fn dist(v: Vec3) -> f32 {
    ((v.x * v.x + v.y * v.y) + v.z * v.z).sqrt()
}

impl Sim {
    /// The bots' part of server_main: player_ai for every bot while the round is on.
    pub(crate) fn run_bots(&mut self) {
        let bots: Vec<PlayerId> = self.players.iter().filter(|(_, p)| p.is_bot).map(|(_, p)| p.player_id).collect();
        for pid in bots {
            self.player_ai(pid);
        }
    }

    fn push_bot_action(&mut self, pid: PlayerId, a: u16) {
        if let Some(p) = self.players.get_mut(pid.idx()) {
            p.actions.push(GameAction::Inventory(InventoryAction { a, b: 1, c: 0 }));
        }
    }

    /// player_ai: a bot's controls for the tick. It reloads, then on foot walks its waypoints and turns towards and
    /// shoots at what it has seen; seated, it drives its traffic car's route or watches from its window and shoots.
    pub(crate) fn player_ai(&mut self, pid: PlayerId) {
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.controls[0] = 0.0;
        p.controls[1] = 0.0;
        p.controls[2] = 0.0;
        p.controls[3] = 0.0;
        p.input_bits = 0;
        if p.bot.delay > 0 {
            p.bot.delay -= 1;
            return;
        }
        let Some(hid) = p.human else { return };
        self.bot_reload(pid, hid);
        let Some(h) = self.humans.get(hid) else { return };
        match h.vehicle {
            None => self.bot_on_foot(pid, hid),
            Some(veh) => {
                let seat = h.seat;
                self.players.get_mut(pid.idx()).unwrap().control_mode = 2;
                if seat != 0 {
                    self.bot_seated(pid, hid, veh, seat);
                } else if self.gamestate != GameState::Intermission {
                    self.player_ai_control_vehicle(pid, veh);
                } else {
                    self.players.get_mut(pid.idx()).unwrap().controls[3] = -1.0;
                }
                self.bot_check_wreck(pid, veh);
            }
        }
    }

    /// A wrecked vehicle is left after 180 ticks.
    fn bot_check_wreck(&mut self, pid: PlayerId, veh: usize) {
        let wrecked = self.vehicles.get(veh).is_some_and(|v| v.health <= 0);
        let p = self.players.get_mut(pid.idx()).unwrap();
        if wrecked {
            p.bot.wrecked_ticks += 1;
            if p.bot.wrecked_ticks > WRECK_PATIENCE {
                p.input_bits |= INPUT_EXIT;
            }
        } else {
            p.bot.wrecked_ticks = 0;
        }
    }

    /// The gun in the right hand: an empty chamber is filled from the magazine, an empty magazine swapped for one in the
    /// left hand or a pocket, now and then.
    fn bot_reload(&mut self, pid: PlayerId, hid: usize) {
        let Some(h) = self.humans.get(hid) else { return };
        if h.action_type != -1 || h.inventory[0].count <= 0 {
            return;
        }
        let Some(gun) = self.items.get(h.inventory[0].items[0] as usize) else { return };
        if gun.state.left() != 0 {
            return;
        }
        let gun_type = gun.item_type as usize;
        if let Some(&mag) = gun.children.first() {
            let rounds = self.items.get(mag).map_or(0, |m| m.state.left());
            if rounds <= 0 {
                if crate::rng::rand() & 0x1f == 0 {
                    self.push_bot_action(pid, 1);
                }
            } else if crate::rng::rand() & 0x1f == 0 {
                self.players.get_mut(pid.idx()).unwrap().input_bits |= INPUT_CHAMBER;
            }
            return;
        }
        if h.inventory[1].count <= 0 {
            let pockets: Vec<(usize, Option<usize>)> = (3..7).map(|s| (s, (h.inventory[s].count > 0).then(|| h.inventory[s].items[0] as usize))).collect();
            for (s, first) in pockets {
                let Some(kind) = first.and_then(|i| self.items.get(i)).map(|i| i.item_type as usize) else { continue };
                if self.item_types[kind].can_mount_to[gun_type] != 0 && crate::rng::rand() & 1 == 0 {
                    self.push_bot_action(pid, s as u16 + 1);
                }
            }
            return;
        }
        let rounds = self.items.get(h.inventory[1].items[0] as usize).map_or(0, |m| m.state.left());
        if crate::rng::rand() & 0x1f == 0 {
            self.push_bot_action(pid, if rounds <= 0 { 2 } else { 1 });
        }
    }

    /// The bot's human (`hid`) looks around every other tick: each living enemy within 128 in front of its chest and in
    /// plain sight is remembered or seen again, then its memories fade and it picks the nearest it is sure of.
    fn bot_find_targets(&mut self, pid: PlayerId, hid: usize) {
        if (self.tick as i32 ^ pid.0 as i32) & 1 != 0 {
            return;
        }
        let Some(me) = self.humans.get(hid) else { return };
        let (chest, front) = (me.bones[3].pos, me.bones[3].rot[2]);
        let Some(team) = self.players.get(pid.idx()).map(|p| p.team) else { return };
        let zombie = self.players.get(pid.idx()).is_some_and(|p| p.bot.is_zombie);
        let map = &self.world.map;
        for j in self.humans.ids() {
            let h = self.humans.get(j).unwrap();
            if h.old_health <= 0 {
                continue;
            }
            let Some(their) = h.player.and_then(|p| self.players.get(p.idx())) else { continue };
            if their.team == team {
                continue;
            }
            let d = chest - h.bones[3].pos;
            if !(SIGHT_RANGE > dist(d)) {
                continue;
            }
            let bot = &self.players.get(pid.idx()).unwrap().bot;
            let found = bot.targets.iter().rposition(|t| t.human == j as i32);
            let bone = if h.vehicle.is_some() { 3 } else { (self.tick & 15) as usize };
            let seen = h.bones[bone].pos;
            let dir = seen - chest;
            let range = ((dir.y * dir.y + dir.x * dir.x) + dir.z * dir.z).sqrt();
            let facing = dir.z * front.z + (dir.y * front.y + dir.x * front.x);
            if !(0.0 > facing) {
                continue;
            }
            // TODO: line_intersect_level's flag 1 also traces the dynamic city objects (0x7d04ea0), which round mode leaves empty
            if !zombie && line_intersect_level(&map.ground, &map.level.area, &map.level.meshes, chest, seen).is_some() {
                continue;
            }
            let vel = h.bones[0].vel;
            let p = self.players.get_mut(pid.idx()).unwrap();
            let k = match found {
                Some(k) => k,
                None => {
                    if p.bot.targets.len() >= MAX_TARGETS {
                        continue;
                    }
                    p.bot.targets.push(BotTarget { human: j as i32, pos: seen, awareness: 0.0, timer: TARGET_TIMER });
                    p.bot.targets.len() - 1
                }
            };
            let t = &mut p.bot.targets[k];
            let mut aw = 1.0 + t.awareness;
            if aw > MAX_AWARENESS {
                aw = MAX_AWARENESS;
            }
            t.awareness = aw;
            t.pos = Vec3::new(t.pos.x * SIGHTING_KEEP, t.pos.y * SIGHTING_KEEP, SIGHTING_KEEP * t.pos.z);
            let unsure = (MAX_AWARENESS - aw) * SIGHTING_STEP;
            let spread = unsure * SPREAD_SCALE + (SPREAD_SCALE * range) * SIGHTING_STEP;
            let out = crate::human::arms::calculate_spread_vector(&mut self.noise_seed, spread, 0.0);
            let ahead = Vec3::new((out.x + seen.x) + vel.x * LEAD_TICKS, (out.y + seen.y) + vel.y * LEAD_TICKS, LEAD_TICKS * vel.z + (seen.z + out.z));
            let t = &mut self.players.get_mut(pid.idx()).unwrap().bot.targets[k];
            t.pos = Vec3::new(ahead.x * SIGHTING_STEP + t.pos.x, ahead.y * SIGHTING_STEP + t.pos.y, ahead.z * SIGHTING_STEP + t.pos.z);
        }
        let my_pos = self.humans.get(hid).unwrap().pos;
        let humans = &self.humans;
        let p = self.players.get_mut(pid.idx()).unwrap();
        let targets = &mut p.bot.targets;
        let mut i = 0;
        while i < targets.len() {
            targets[i].awareness *= FADE;
            if FORGET > targets[i].awareness {
                let last = targets.len() - 1;
                targets[i] = targets[last];
                targets.truncate(last);
            } else {
                i += 1;
            }
        }
        p.bot.target = 0;
        let pos_of = |t: &BotTarget| humans.get(t.human as usize).map_or(Vec3::ZERO, |h| h.pos);
        let first = targets.first().map_or(Vec3::ZERO, pos_of);
        let d = my_pos - first;
        let mut best = (d.z * d.z + (d.x * d.x + d.y * d.y)).sqrt();
        for (k, t) in targets.iter().enumerate().skip(1) {
            if !(t.awareness > AIM) {
                continue;
            }
            let d = my_pos - pos_of(t);
            let r = ((d.x * d.x + d.y * d.y) + d.z * d.z).sqrt();
            if best > r {
                p.bot.target = k as i32;
                best = r;
            }
        }
    }

    /// A bot on foot: the view follows the body's turns, a fallen bot only crouches, and otherwise it walks to its
    /// waypoints, goes to look at an enemy it barely saw, and turns to aim at and shoot one it is sure of.
    fn bot_on_foot(&mut self, pid: PlayerId, hid: usize) {
        self.bot_find_targets(pid, hid);
        let h = self.humans.get(hid).unwrap();
        let (body_yaw, pelvis_up, view_yaw, yaw_offset, pos) = (h.body_yaw, h.bones[0].rot[1].y, h.view_yaw, h.yaw_offset, h.pos);
        let tick = self.tick;
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.control_mode = 1;
        let turn = wrap(p.controls[8], body_yaw);
        let mut look = p.controls[4] - turn;
        p.controls[8] = body_yaw;
        if -MAX_LOOK > look as f64 {
            look = -LOOK_LIMIT;
        } else if look as f64 > MAX_LOOK {
            look = LOOK_LIMIT;
        }
        p.controls[4] = look;
        if UPRIGHT > pelvis_up {
            if (tick & 0x7f) > 0x40 {
                p.input_bits |= INPUT_CROUCH;
            }
            return;
        }
        let mut yaw_to = p.bot.idle_yaw;
        if p.bot.waypoint < p.bot.waypoint_count {
            p.controls[3] = 1.0;
            let wp = p.bot.waypoint_pos(p.bot.waypoint);
            let (dx, dz) = (wp.x - pos.x, wp.z - pos.z);
            let d = ((dx * dx + 0.0) + dz * dz).sqrt();
            yaw_to = if d == 0.0 {
                0.0f64.atan2(-0.0) as f32
            } else {
                let inv = 1.0 / d;
                ((dx * inv) as f64).atan2((-(inv * dz)) as f64) as f32
            };
            if WAYPOINT_RADIUS > d {
                p.bot.waypoint += 1;
                if !(p.bot.waypoint < p.bot.waypoint_count) {
                    p.bot.waypoint = 0;
                }
            }
        }
        let mut attack = false;
        if !p.bot.targets.is_empty() {
            let t = p.bot.targets[p.bot.target as usize];
            let mut aw = t.awareness;
            if INVESTIGATE > aw && INVESTIGATE > (t.pos.y - pos.y).abs() {
                if crate::rng::rand() & 0x1f == 0 {
                    let p = self.players.get_mut(pid.idx()).unwrap();
                    p.bot.waypoint = 0;
                    p.bot.set_waypoint(0, t.pos);
                    p.bot.waypoint_count = 1;
                }
                let p = self.players.get(pid.idx()).unwrap();
                aw = p.bot.targets.get(p.bot.target as usize).map_or(0.0, |t| t.awareness);
                attack = !p.bot.targets.is_empty() && aw > ATTACK;
            } else {
                attack = aw > ATTACK;
            }
        }
        let (yaw_to, pitch_to) = if attack { self.bot_attack_on_foot(pid, hid) } else { (yaw_to, 0.0) };
        let p = self.players.get_mut(pid.idx()).unwrap();
        let mut facing = (view_yaw + yaw_offset) + p.controls[4];
        if -std::f64::consts::PI > facing as f64 {
            facing = (facing as f64 + (std::f64::consts::PI * 2.0)) as f32;
        }
        if facing as f64 > std::f64::consts::PI {
            facing = (facing as f64 - (std::f64::consts::PI * 2.0)) as f32;
        }
        let d = wrap(facing, yaw_to);
        p.controls[4] += d * HALF;
        p.controls[5] = (TURN_RATE * (pitch_to - p.controls[5])) * HALF + p.controls[5];
        p.zoom_level = 1;
        if p.bot.targets.first().is_some_and(|t| t.awareness > AIM) {
            p.zoom_level = 0;
        }
    }

    /// An on-foot bot sure of its target: it walks at it (a zombie keeps walking), stops within 32 and, aimed within 64
    /// with no teammate in the way, fires now and then. The heading and pitch to turn to.
    fn bot_attack_on_foot(&mut self, pid: PlayerId, hid: usize) -> (f32, f32) {
        let p = self.players.get(pid.idx()).unwrap();
        let t = p.bot.targets[p.bot.target as usize];
        let (zombie, pitch_now, team) = (p.bot.is_zombie, p.controls[5], p.team);
        let h = self.humans.get(hid).unwrap();
        let eye = h.bones[7].pos;
        let (dx, dz) = (t.pos.x - eye.x, t.pos.z - eye.z);
        let dy = (t.pos.y - eye.y) - EYE_DROP;
        let d = ((dy * dy + dx * dx) + dz * dz).sqrt();
        let yaw = (dx as f64).atan2((-dz) as f64) as f32;
        let horiz = ((dx * dx + dz * dz) as f64).sqrt();
        let pitch = ((-dy) as f64).atan2(horiz) as f32;
        let p = self.players.get_mut(pid.idx()).unwrap();
        if zombie {
            p.controls[3] = 1.0;
        } else if STOP_RANGE > d {
            p.controls[3] = 0.0;
        }
        if !(FIRE_RANGE > d) {
            return (yaw, pitch);
        }
        if !(AIM_TOLERANCE > wrap(h.view_yaw, yaw).abs()) || !(AIM_TOLERANCE > wrap(pitch_now, pitch).abs()) {
            return (yaw, pitch);
        }
        let head = &h.bones[9];
        let (start, up) = (head.pos, head.rot[1]);
        let end = Vec3::new(up.x * SHOT_LENGTH + start.x, up.y * SHOT_LENGTH + start.y, SHOT_LENGTH * up.z + start.z);
        let chest = h.bones[3].pos;
        if self.teammate_in_line(hid, team, chest, start, end, TEAMMATE_RANGE_FOOT) {
            return (yaw, pitch);
        }
        if self.bot_gun_loaded(hid) && crate::rng::rand() & 3 == 0 {
            self.players.get_mut(pid.idx()).unwrap().input_bits |= INPUT_FIRE;
        }
        (yaw, pitch)
    }

    /// Whether a teammate's human within `range` of `chest` is in the line of fire `start..end`.
    fn teammate_in_line(&self, hid: usize, team: rosa_protocol::Team, chest: Vec3, start: Vec3, end: Vec3, range: f32) -> bool {
        self.humans.ids().into_iter().filter(|&j| j != hid).any(|j| {
            let h = self.humans.get(j).unwrap();
            let Some(p) = h.player.and_then(|p| self.players.get(p.idx())) else { return false };
            if p.team != team {
                return false;
            }
            let d = chest - h.bones[3].pos;
            range > dist(d) && crate::human::damage::trace_ray_human(h, start, end, SHOT_RADIUS).is_some()
        })
    }

    fn bot_gun_loaded(&self, hid: usize) -> bool {
        let h = self.humans.get(hid).unwrap();
        h.inventory[0].count > 0 && self.items.get(h.inventory[0].items[0] as usize).is_some_and(|g| g.state.left() > 0)
    }

    /// A passenger bot looks out of its window, or turns towards an enemy it is sure of in its vehicle's frame and,
    /// aimed within 32 with neither a teammate nor its own vehicle's body in the way, fires.
    fn bot_seated(&mut self, pid: PlayerId, hid: usize, veh: usize, seat: usize) {
        self.bot_find_targets(pid, hid);
        let mut yaw = match seat {
            1 => SEAT_YAW[0],
            2 | 4 => SEAT_YAW[1],
            3 | 5 => SEAT_YAW[2],
            _ => 0.0,
        };
        let mut pitch = 0.0;
        let p = self.players.get(pid.idx()).unwrap();
        let team = p.team;
        let aimed_at = p.bot.targets.get(p.bot.target as usize).filter(|t| !p.bot.targets.is_empty() && t.awareness > AIM).copied();
        if let Some(t) = aimed_at {
            let h = self.humans.get(hid).unwrap();
            let eye = h.bones[9].pos;
            let (dx, dz) = (t.pos.x - eye.x, t.pos.z - eye.z);
            let dy = (t.pos.y - eye.y) - EYE_DROP;
            let d = ((dx * dx + dy * dy) + dz * dz).sqrt();
            let n = if d == 0.0 {
                Vec3::ZERO
            } else {
                let inv = 1.0 / d;
                Vec3::new(dx * inv, dy * inv, inv * dz)
            };
            let [r0, r1, r2] = self.vehicles.get(veh).map_or(rosa_physics::rotation::IDENTITY, |v| v.rot);
            let lx = (r0.x * n.x + r0.y * n.y) + r0.z * n.z;
            let ly = (r1.x * n.x + r1.y * n.y) + r1.z * n.z;
            let lz = (n.x * r2.x + n.y * r2.y) + n.z * r2.z;
            yaw = (lx as f64).atan2((-lz) as f64) as f32;
            let horiz = ((lx * lx + lz * lz) as f64).sqrt();
            pitch = ((-ly) as f64).atan2(horiz) as f32;
            let p = self.players.get(pid.idx()).unwrap();
            if STOP_RANGE > d && AIM_TOLERANCE > wrap(p.controls[4], yaw).abs() && AIM_TOLERANCE > wrap(p.controls[5], pitch).abs() {
                self.bot_seated_fire(pid, hid, veh, team, n);
            }
        }
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.controls[4] = ((yaw - p.controls[4]) * TURN_RATE) * HALF + p.controls[4];
        p.controls[5] = ((pitch - p.controls[5]) * TURN_RATE) * HALF + p.controls[5];
    }

    /// The seated bot's shot along its gun's magazine, when the gun points at the enemy (`n`, a unit vector).
    fn bot_seated_fire(&mut self, pid: PlayerId, hid: usize, veh: usize, team: rosa_protocol::Team, n: Vec3) {
        let h = self.humans.get(hid).unwrap();
        let Some(mag) = self.items.get(h.inventory[0].items[0] as usize).and_then(|g| self.items.get(g.first_child)) else { return };
        let Some(body) = self.bodies.get(mag.body) else { return };
        let r2 = body.rot[2];
        let along = (n.x * r2.x + n.y * r2.y) + n.z * r2.z;
        if !(GUN_ALIGNMENT > along) {
            return;
        }
        let start = mag.pos2;
        let end = Vec3::new(r2.x * SHOT_LENGTH + start.x, r2.y * SHOT_LENGTH + start.y, start.z + r2.z * SHOT_LENGTH);
        if self.teammate_in_line(hid, team, h.bones[3].pos, start, end, TEAMMATE_RANGE_SEATED) {
            return;
        }
        let own_body = self.vehicles.get(veh).and_then(|v| crate::vehicle::physics::trace_vehicle_parts(v, &self.vehicle_types[v.kind], start, end, false));
        if own_body.is_some_and(|hit| matches!(hit.part, crate::vehicle::physics::VehiclePart::Body(_))) {
            return;
        }
        if self.bot_gun_loaded(hid) {
            self.players.get_mut(pid.idx()).unwrap().input_bits |= INPUT_FIRE;
        }
    }

    /// player_ai_control_vehicle: the driver lets its traffic car's AI (made aggressive) steer and sets the gas from how
    /// far the car is below the speed the AI wants, slowing for sharp turns; a car stuck for 180 ticks drives on and
    /// after 280 reverses.
    fn player_ai_control_vehicle(&mut self, pid: PlayerId, veh: usize) {
        let Some(car) = self.vehicles.get(veh).and_then(|v| usize::try_from(v.traffic_car).ok()) else { return };
        self.traffic.cars[car].is_aggressive = 1;
        crate::traffic::ai::ai_traffic_car(&mut self.traffic, &self.world.map.streets, car);
        let v = self.vehicles.get(veh).unwrap();
        let Some(car) = usize::try_from(v.traffic_car).ok() else { return };
        let c = &self.traffic.cars[car];
        let (steer, target, stuck) = (c.steer, c.target_speed, c.stuck);
        let (vel, r2) = (v.vel, v.rot[2]);
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.controls[0] = DRIVE_GEAR_X;
        p.controls[2] = DRIVE_GEAR_Y;
        p.controls[1] = steer;
        let gas = (((vel.x * r2.x + vel.y * r2.y) + vel.z * r2.z) + target) * THROTTLE_SCALE;
        let gas = if gas > 1.0 { 1.0 } else { gas };
        p.controls[3] = if MIN_TARGET_SPEED > target { -1.0 } else { (gas as f64 / ((steer.abs() as f64) + 1.0)) as f32 };
        p.input_bits = 0;
        if stuck > STUCK_FORWARD {
            p.controls[3] = 1.0;
            p.controls[1] = 0.0;
            p.controls[0] = DRIVE_GEAR_X;
            p.controls[2] = 1.0;
            if stuck > STUCK_REVERSE {
                p.controls[3] = -1.0;
            }
        }
    }
}

/// Hooks for checking the bot AI against the original server.
impl Sim {
    /// create_player and create_human: a player of `team` (a bot or not) with a human at `pos`.
    pub fn make_test_player(&mut self, team: rosa_protocol::Team, bot: bool, pos: Vec3) -> (PlayerId, usize) {
        let pid = self.create_player().unwrap();
        let p = self.players.get_mut(pid.idx()).unwrap();
        p.team = team;
        p.is_bot = bot;
        let h = self.spawn_human(pos, &rosa_physics::rotation::IDENTITY, Some(pid)).unwrap();
        self.players.get_mut(pid.idx()).unwrap().human = Some(h);
        self.humans.get_mut(h).unwrap().pos = pos;
        (pid, h)
    }

    pub fn run_player_ai(&mut self, pid: PlayerId) {
        self.player_ai(pid);
    }

    pub fn set_tick(&mut self, t: u32) {
        self.tick = t;
    }

    pub fn give_pistol(&mut self, h: usize) {
        self.give_weapon(h, rosa_protocol::clientbound::game::ItemKind::Pistol, 4);
    }

    /// Moves a human's position and every bone by `d`.
    pub fn shift_human(&mut self, h: usize, d: Vec3) {
        let Some(h) = self.humans.get_mut(h) else { return };
        h.pos += d;
        for b in &mut h.bones {
            b.pos += d;
        }
    }

    /// Empties the chamber of the gun in the human's right hand.
    pub fn empty_gun(&mut self, h: usize) {
        let Some(id) = self.humans.get(h).map(|h| h.inventory[0].items[0] as usize) else { return };
        if let Some(i) = self.items.get_mut(id) {
            i.state.set_left(0);
        }
    }

    /// Puts `rounds` in the chamber of the gun in the human's right hand.
    pub fn load_gun(&mut self, h: usize, rounds: i32) {
        let Some(h) = self.humans.get(h).filter(|h| h.inventory[0].count > 0).map(|h| h.inventory[0].items[0] as usize) else { return };
        if let Some(i) = self.items.get_mut(h) {
            i.state.set_left(rounds);
        }
    }

    /// Sets how upright the human's pelvis is (bone 0's rotation row 1, y).
    pub fn tilt_human(&mut self, h: usize, up: f32) {
        if let Some(h) = self.humans.get_mut(h) {
            h.bones[0].rot[1].y = up;
        }
    }

    pub fn spawn_chase(&mut self, disk: rosa_protocol::clientbound::game::ItemKind, deadline: i32) {
        self.create_round_traffic_car(disk, deadline);
    }

    pub fn bot_ids(&self) -> Vec<PlayerId> {
        let mut ids: Vec<PlayerId> = self.players.iter().filter(|(_, p)| p.is_bot).map(|(i, _)| PlayerId(i as u32)).collect();
        ids.sort_by_key(|p| p.0);
        ids
    }
}
