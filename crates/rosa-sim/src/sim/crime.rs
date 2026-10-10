use glam::Vec3;
use rosa_protocol::{GameMode, Team};

use super::Sim;
use crate::{PlayerId, human::damage::damage_human};

/// The crime settings (crimecivciv=, crimecivteam=, ... in config.txt) at their defaults: the criminal rating a
/// hundred points of damage earns, by who hurt whom.
const CRIME_CIV_CIV: i32 = 100;
const CRIME_CIV_TEAM: i32 = 200;
const CRIME_TEAM_CIV: i32 = 50;
const CRIME_TEAM_TEAM: i32 = 0;
const CRIME_TEAM_TEAM_IN_BASE: i32 = 100;
/// A rating this high kills the player's human (crimekick=).
const CRIME_KICK: i32 = 1000;
const MAX_CRIMINAL_RATING: i32 = 1023;
/// The share of team damage turned back on the attacker in round and versus modes (both settings default to 50).
/// The versus team damage share until its config is ported.
pub(crate) const VERSUS_TEAM_DAMAGE: i32 = 50;
const HEAD: usize = 3;
/// The damage a run-over kill scores.
const RUN_OVER_SCORE: i32 = 100;

impl Sim {
    /// handle_criminal_rating: in world mode, a player who hurts another gains criminal rating by their teams
    /// (civilian or corporation, and whether the victim was in their own base), less the more criminal the victim
    /// already is. A player who reaches crimekick has their human killed.
    pub(crate) fn handle_criminal_rating(&mut self, attacker: PlayerId, victim: PlayerId, damage: i32) {
        // TODO: bots are left out
        let (Some(a), Some(v)) = (self.players.get(attacker.idx()), self.players.get(victim.idx())) else { return };
        let (Some(attacker_human), Some(victim_human)) = (a.human, v.human) else { return };
        if self.gamemode != GameMode::World {
            return;
        }
        let Some(vh) = self.humans.get(victim_human) else { return };
        let (victim_pos, last_vehicle) = (vh.bones[0].pos, vh.last_vehicle);
        let owner = (last_vehicle != -1).then(|| self.vehicles.get(last_vehicle as usize)).flatten().map_or(-1, |v| v.owner);
        let (a_team, v_team) = (a.team as i32, v.team as i32);
        let rate = if a.team == Team::Spectator {
            if last_vehicle != -1 && attacker.idx() as i32 == owner && owner != -1 {
                return;
            }
            if v.team == Team::Spectator { CRIME_CIV_CIV } else { CRIME_CIV_TEAM }
        } else if !(0..=5).contains(&a_team) {
            CRIME_CIV_CIV
        } else {
            if self.in_base(a_team, victim_pos) {
                return;
            }
            if owner != -1 && self.players.get(owner as usize).is_some_and(|o| o.team as i32 == a_team) {
                return;
            }
            if v.team == Team::Spectator {
                CRIME_TEAM_CIV
            } else if (0..=5).contains(&v_team) && self.in_base(v_team, victim_pos) {
                CRIME_TEAM_TEAM_IN_BASE
            } else {
                CRIME_TEAM_TEAM
            }
        };
        let mut k = rate as f32 / 100.0;
        if v.crim_rating > 0 {
            let f = v.crim_rating as f32 / 100.0;
            if f > 1.0 {
                k *= 0.0;
            } else if !(0.0 > f) {
                k *= 1.0 - f;
            }
        }
        let gain = (damage as f32 * k) as i32;
        let Some(a) = self.players.get_mut(attacker.idx()) else { return };
        if gain > 0 {
            a.crim_rating += gain;
        }
        if a.crim_rating > MAX_CRIMINAL_RATING {
            a.crim_rating = MAX_CRIMINAL_RATING;
        }
        if CRIME_KICK <= a.crim_rating
            && let Some(h) = self.humans.get_mut(attacker_human)
        {
            h.old_health = 0;
        }
    }

    /// Whether `pos` is inside the interior of corporation `team`'s base.
    fn in_base(&self, team: i32, pos: Vec3) -> bool {
        usize::try_from(team).ok().and_then(|t| self.world.map.level.bases.get(t)).is_some_and(|b| b.contains(pos))
    }

    /// punish_team_kill: outside world and eliminator modes, a player who hurt a teammate takes a share of the damage
    /// to their own head.
    pub(crate) fn punish_team_kill(&mut self, player: PlayerId, damage: i32) {
        if self.players.get(player.idx()).is_none_or(|p| p.god_mode) || damage <= 0 || matches!(self.gamemode, GameMode::World | GameMode::Eliminator) {
            return;
        }
        self.handle_team_kill(player, damage);
    }

    /// The /godmode chat command: the player's god mode on or off, announced to admins.
    pub(crate) fn godmode_command(&mut self, pid: PlayerId) {
        // TODO: the binary takes this command from admins only, and sends chat type 4 only to admin connections
        let Some(p) = self.players.get_mut(pid.idx()) else { return };
        p.god_mode = !p.god_mode;
        let line = if p.god_mode { "godmode on" } else { "godmode off" };
        self.send_chat(line, rosa_protocol::clientbound::game::events::chat::ChatType::AdminChat, -1, 0);
    }

    /// handle_team_kill: the team damage is logged and dealt to the player's head, and the player's details resent.
    fn handle_team_kill(&mut self, player: PlayerId, damage: i32) {
        let Some(p) = self.players.get(player.idx()) else { return };
        println!("[Sim] {}({}) team damage: {damage}", p.username, p.phone_number);
        let share = self.team_damage;
        if let Some(h) = p.human.and_then(|h| self.humans.get_mut(h)) {
            damage_human(h, HEAD, damage * share / 100);
        }
        let (update, round) = (p.make_update_player_event(self.tick), p.make_update_round_event(self.tick));
        self.events.push(update);
        self.events.push(round);
    }

    /// The scoring of one player hurting another: a teammate is punished (not in eliminator) and the attacker's
    /// criminal rating goes up, as bullet_simulation does it.
    pub(crate) fn score_hurt(&mut self, attacker: Option<PlayerId>, victim: Option<PlayerId>, damage: i32) {
        let (Some(a), Some(v)) = (attacker, victim) else { return };
        if a == v {
            return;
        }
        self.handle_criminal_rating(a, v, damage);
        if self.gamemode != GameMode::Eliminator && self.same_team(a, v) {
            self.punish_team_kill(a, damage);
        }
    }

    pub(crate) fn same_team(&self, a: PlayerId, b: PlayerId) -> bool {
        matches!((self.players.get(a.idx()), self.players.get(b.idx())), (Some(p), Some(q)) if p.team == q.team)
    }

    /// human_collide_vehicle's scoring of a run-over: the driver's criminal rating goes up for another player's
    /// human, and a driver who ran over a teammate (or themselves) is punished.
    pub(crate) fn score_run_over(&mut self, driver: PlayerId, victim: Option<PlayerId>) {
        let Some(victim) = victim else { return };
        if driver != victim {
            self.handle_criminal_rating(driver, victim, RUN_OVER_SCORE);
        }
        if self.same_team(driver, victim) {
            self.punish_team_kill(driver, RUN_OVER_SCORE);
        }
    }
}
