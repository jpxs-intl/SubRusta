use subrusta_protocol::{GameMode, clientbound::game::GameState};

use super::{ModeHooks, rules::ModeRules};
use crate::sim::Sim;

/// Our own mode: no game mode logic, always in game, with the test commands open to everyone. Clients are told it is
/// round mode.
pub struct Hooks;

impl ModeHooks for Hooks {
    fn name(&self) -> &'static str {
        "sandbox"
    }

    fn client_mode(&self) -> GameMode {
        GameMode::Round
    }

    fn rules(&self, _sim: &Sim) -> ModeRules {
        ModeRules { test_commands: true, ..Default::default() }
    }

    fn reset(&self, sim: &mut Sim) {
        sim.gamestate = GameState::InGame;
    }
}
