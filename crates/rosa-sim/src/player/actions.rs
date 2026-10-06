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
    pub fn ingest(&mut self, start: u8, actions: Vec<GameAction>) {
        let mut idx = start % 64;
        for action in actions {
            self.ring[idx as usize] = Some(action);
            idx = (idx + 1) % 64;
        }

        if (idx.wrapping_sub(self.applied) % 64) > (self.write.wrapping_sub(self.applied) % 64) {
            self.write = idx;
        }
    }

    pub fn drain(&mut self) -> impl Iterator<Item = GameAction> + '_ {
        std::iter::from_fn(|| {
            if self.applied == self.write { return None; }
            let a = self.ring[self.applied as usize].take();
            self.applied = self.applied.wrapping_add(1) % 64;
            a
        })
    }
}