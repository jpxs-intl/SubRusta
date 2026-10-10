use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use rosa_protocol::clientbound::game::events::chat::ChatType;

use super::Sim;

/// The ticks /tps averages over (about two seconds).
const WINDOW: usize = 128;

/// When the last ticks started and how long their work took.
#[derive(Default)]
pub struct TickStats {
    starts: VecDeque<Instant>,
    work: VecDeque<Duration>,
}

impl TickStats {
    pub fn record(&mut self, start: Instant, work: Duration) {
        if self.starts.len() == WINDOW {
            self.starts.pop_front();
            self.work.pop_front();
        }
        self.starts.push_back(start);
        self.work.push_back(work);
    }

    /// Ticks per second over the window, and the mean and longest tick's work in milliseconds.
    fn summary(&self) -> Option<(f64, f64, f64)> {
        let (first, last) = (self.starts.front()?, self.starts.back()?);
        let span = last.duration_since(*first).as_secs_f64();
        if self.starts.len() < 2 || span == 0.0 {
            return None;
        }
        let tps = (self.starts.len() - 1) as f64 / span;
        let ms = |d: &Duration| d.as_secs_f64() * 1000.0;
        let mean = self.work.iter().map(ms).sum::<f64>() / self.work.len() as f64;
        let max = self.work.iter().map(ms).fold(0.0, f64::max);
        Some((tps, mean, max))
    }
}

impl Sim {
    /// Test command (/tps): the measured tick rate against the 62.5 the 16 ms tick aims for, and how long ticks take.
    pub(crate) fn tps_command(&mut self) {
        let msg = match self.tick_stats.summary() {
            Some((tps, mean, max)) => format!("TPS {tps:.1} / {:.1}, tick {mean:.2} ms avg, {max:.2} ms max", 1000.0 / super::TICK_MS as f64),
            None => "TPS: not enough ticks yet".to_string(),
        };
        
        self.send_chat(&msg, ChatType::Announce, -1, 0);
    }
}
