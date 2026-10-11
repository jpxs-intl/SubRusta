use subrusta_protocol::GameMode;

use super::{ModeHooks, ModeSync, rules::ModeRules};
use crate::{
    sim::{Sim, crime::VERSUS_TEAM_DAMAGE},
    time::TICKS_PER_SECOND,
};

/// The binary's modes this server does not run (driving, racing, co-op, versus and none), with what the shared code
/// still does differently for them.
pub struct Unported(GameMode, &'static str);

pub static DRIVING: Unported = Unported(GameMode::Driving, "driving");
pub static RACING: Unported = Unported(GameMode::Racing, "racing");
pub static COOP: Unported = Unported(GameMode::CoOp, "coop");
pub static VERSUS: Unported = Unported(GameMode::Versus, "versus");
pub static NONE: Unported = Unported(GameMode::None, "none");

/// A versus player joins with 250.
const VERSUS_MONEY: i32 = 250;

impl ModeHooks for Unported {
    fn name(&self) -> &'static str {
        self.1
    }

    fn aliases(&self) -> &'static [&'static str] {
        if self.0 == GameMode::Versus { &["v"] } else { &[] }
    }

    fn client_mode(&self) -> GameMode {
        self.0
    }

    fn map(&self) -> Option<&'static str> {
        match self.0 {
            GameMode::CoOp => Some("round"),
            GameMode::None => Some("test2"),
            _ => None,
        }
    }

    fn rules(&self, sim: &Sim) -> ModeRules {
        let base = ModeRules::default();
        match self.0 {
            GameMode::Racing => ModeRules { reset_traffic: None, intermission_lobby: true, ..base },
            GameMode::CoOp => ModeRules { coop_traffic: true, intermission_lobby: true, ..base },
            GameMode::Versus => ModeRules {
                team_damage: VERSUS_TEAM_DAMAGE,
                corp_buy_menus: true,
                free_team_switch: true,
                intermission_lobby: true,
                move_delay: Some(sim.modes.round.versus.movedelay * TICKS_PER_SECOND),
                ..base
            },
            _ => base,
        }
    }

    fn joining_money(&self, _sim: &Sim, money: i32) -> i32 {
        if self.0 == GameMode::Versus { VERSUS_MONEY } else { money }
    }

    fn sync(&self, sim: &Sim) -> ModeSync {
        let movedelay = (self.0 == GameMode::Versus).then_some(sim.modes.round.versus.movedelay as u8);
        ModeSync { weekly_enabled: false, weekday: sim.world.weekday as u8, versus_movedelay: movedelay }
    }

    fn on_admin_select(&self, sim: &mut Sim) {
        if self.0 == GameMode::Versus {
            sim.modes.round.weekday = super::round::WEEK_DAYS;
        }
    }
}
