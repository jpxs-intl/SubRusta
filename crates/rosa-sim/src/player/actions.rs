use rosa_protocol::serverbound::game::actions::GameAction;

pub struct ActionQueue {
    ring: [Option<GameAction>; 64],
    pub write: u8,
    applied: u8
}

impl Default for ActionQueue {
    fn default() -> Self {
        Self { ring: [const { None }; 64], write: Default::default(), applied: Default::default() }
    }
}

impl ActionQueue {
    /// A client game packet: its count (player +0x1b4) becomes the base, and its actions go in from there.
    pub fn ingest(&mut self, start: u8, actions: Vec<GameAction>) {
        self.write = start % 64;
        for action in actions {
            self.ring[self.write as usize] = Some(action);
            self.write = (self.write + 1) % 64;
        }
    }

    /// reset_game: both counts back to 0.
    pub fn reset(&mut self) {
        self.write = 0;
        self.applied = 0;
    }

    /// An action the server makes for a bot, queued after the others.
    pub fn push(&mut self, action: GameAction) {
        self.ring[self.write as usize] = Some(action);
        self.write = (self.write + 1) % 64;
    }

    pub fn get(&self, i: u8) -> Option<&GameAction> {
        self.ring[(i % 64) as usize].as_ref()
    }

    /// logic_playerinteractions: every slot from the last handled one (+0x1b8) up to the count.
    pub fn drain(&mut self) -> impl Iterator<Item = GameAction> + '_ {
        std::iter::from_fn(|| {
            while self.applied != self.write {
                let a = self.ring[self.applied as usize].take();
                self.applied = (self.applied + 1) % 64;
                if a.is_some() {
                    return a;
                }
            }
            None
        })
    }
}
