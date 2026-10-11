use glam::Vec3;
use subrusta_protocol::GameMode;

use self::rules::ModeRules;
use crate::{PlayerId, sim::Sim};

pub mod eliminator;
pub mod round;
pub mod rules;
pub mod sandbox;
pub mod unported;
pub mod world;

/// A game mode: its hooks. Modes are compared by name.
pub type Mode = &'static dyn ModeHooks;

/// Every game mode the server can run. A new mode is a type implementing ModeHooks and an entry here.
pub static MODES: [Mode; 9] = [
    &round::Hooks,
    &eliminator::Hooks,
    &world::Hooks,
    &sandbox::Hooks,
    &unported::DRIVING,
    &unported::RACING,
    &unported::COOP,
    &unported::VERSUS,
    &unported::NONE,
];

/// The mode with this name or alias, in any case (config.toml's gamemode, /resetgame's argument).
pub fn by_name(name: &str) -> Option<Mode> {
    MODES.iter().copied().find(|m| m.name().eq_ignore_ascii_case(name) || m.aliases().iter().any(|a| a.eq_ignore_ascii_case(name)))
}

/// Each game mode's own state and config. All of them are kept across mode switches, as the binary keeps its globals.
#[derive(Clone, Debug)]
pub struct Modes {
    pub round: round::RoundMode,
    pub world: world::WorldMode,
    pub eliminator: eliminator::EliminatorState,
}

impl Modes {
    /// The modes' configs read from config_round.txt, config_versus.txt and config_world.txt.
    pub(crate) fn load() -> Self {
        Self { round: round::RoundMode::load(), world: world::WorldMode::load(), eliminator: Default::default() }
    }
}

/// What the initial sync tells clients about the mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ModeSync {
    pub weekly_enabled: bool,
    pub weekday: u8,
    pub versus_movedelay: Option<u8>,
}

/// What a game mode supplies. The server asks the current mode rather than naming modes. Clients only know the
/// binary's eight modes, so each mode says which of those it looks like (`client_mode`) and works within that mode's
/// menus and HUD.
pub trait ModeHooks: Sync {
    /// The name config.toml and /resetgame use.
    fn name(&self) -> &'static str;

    /// Other names it answers to.
    fn aliases(&self) -> &'static [&'static str] {
        &[]
    }

    /// The mode clients and the server list are told.
    fn client_mode(&self) -> GameMode;

    /// The settings shared code reads, taken at every reset_game.
    fn rules(&self, sim: &Sim) -> ModeRules;

    /// The mode's part of reset_game, after the corporations' missions are cleared.
    fn reset(&self, _sim: &mut Sim) {}

    /// The mode's logic each tick, before the traffic and NPCs move.
    fn tick(&self, _sim: &mut Sim) {}

    /// The mode's logic each tick, after the traffic and NPCs move.
    fn tick_after_traffic(&self, _sim: &mut Sim) {}

    /// A player's menus each tick, once the shop menus are done: `outside` is where their human stands when it is in
    /// no shop or bank. By default, as in every mode but world, a corporation's base offers the buy menus where the
    /// rules allow them.
    fn base_menus(&self, sim: &mut Sim, pid: PlayerId, outside: Option<Vec3>) {
        sim.round_base_menus(pid, outside);
    }

    /// Once every player's menus are done for the tick.
    fn menus_done(&self, _sim: &mut Sim) {}

    /// A menu button pressed on `menu`; true if the mode took it, otherwise the shared menus (shops, bank, buy menus
    /// and the plain lobby) get it.
    fn on_menu(&self, _sim: &mut Sim, _pid: PlayerId, _menu: u8, _button: u32) -> bool {
        false
    }

    /// A player has joined, their account restored and the initial sync sent.
    fn on_join(&self, _sim: &mut Sim, _pid: PlayerId) {}

    /// A player is leaving, before their shares and account are settled.
    fn on_leave(&self, _sim: &mut Sim, _pid: PlayerId) {}

    /// A real player's human has died; the shared bookkeeping has taxed them and sent their money.
    fn on_death(&self, _sim: &mut Sim, _pid: PlayerId) {}

    /// The money a joining player starts with, given what their account has.
    fn joining_money(&self, _sim: &Sim, money: i32) -> i32 {
        money
    }

    /// What the initial sync says about the mode.
    fn sync(&self, sim: &Sim) -> ModeSync {
        ModeSync { weekly_enabled: false, weekday: sim.world.weekday as u8, versus_movedelay: None }
    }

    /// Whether /resetgame may switch to this mode.
    fn switchable(&self) -> bool {
        false
    }

    /// /resetgame has named this mode (whether or not it switches to it).
    fn on_admin_select(&self, _sim: &mut Sim) {}
}

impl std::fmt::Debug for dyn ModeHooks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}
