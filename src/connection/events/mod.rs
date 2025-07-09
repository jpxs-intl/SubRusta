use dashmap::DashMap;

use crate::connection::events::event_types::Event;

pub mod event_types;

#[derive(Clone, Default)] 
pub struct EventManager {
    pub players: DashMap<u32, PlayerEventManager>,
    // TODO: this never gets cleared until the server restarts. soooooooo...
    // Also, if we issue a SHIT LOAD events, it doesnt get cleared.
    pub global_events: DashMap<u32, Event>
}

#[derive(Clone, Debug)]
pub struct PlayerEventManager {
    pub player_id: u32,
    pub recieved_events: u32
}

impl EventManager {
    pub fn new() -> Self {
        EventManager { 
            players: DashMap::new(),
            global_events: DashMap::new()
        }
    }

    pub fn num_global_events(&self) -> u32 {
        self.global_events.len() as u32
    }

    pub fn emit_globally(&self, event: Event) {
        self.global_events.insert(self.global_events.len() as u32, event);
    }

    pub fn emit_globally_mult(&self, events: Vec<Event>) {
        for event in events {
            self.global_events.insert(self.global_events.len() as u32, event);
        }
    }

    pub fn get_client_missing_events(&self, client_id: u32) -> Vec<(u32, Event)> {
        if let Some(client) = self.players.get(&client_id) {
            let event_count = self.global_events.len() as u32;

            let missing_count = event_count.saturating_sub(client.recieved_events).min(63);

            let mut events = vec![];
            let mut index = client.recieved_events;

            for _ in 0..missing_count {
                if let Some(event) = self.global_events.get(&index) {
                    events.push((index, event.clone()))
                }

                index += 1;
            }

            return events;
        }

        vec![]
    }
}