use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use subrusta_protocol::clientbound::game::events::chat::ChatType;

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
    pub(crate) fn record(&mut self, start: Instant, work: Duration) {
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

/// How many ticks the phase timings cover before they are printed (about ten seconds).
const REPORT_TICKS: u32 = 625;

/// Runs `$body`, adding how long it took to the phase `$name` of `$sim`'s timings.
macro_rules! timed {
    ($sim:expr, $name:expr, $body:expr) => {{
        let start = std::time::Instant::now();
        let result = $body;
        $sim.phase_times.add($name, start.elapsed());
        result
    }};
}
pub(crate) use timed;

/// How long each phase of the tick takes, printed every REPORT_TICKS ticks, slowest first, when the SUBRUSTA_TIMINGS
/// environment variable is set.
#[derive(Default)]
pub struct PhaseTimes {
    enabled: bool,
    ticks: u32,
    /// Each phase's name, total time and longest single tick.
    phases: Vec<(&'static str, Duration, Duration)>,
}

impl PhaseTimes {
    pub(crate) fn from_env() -> Self {
        Self { enabled: std::env::var_os("SUBRUSTA_TIMINGS").is_some(), ..Default::default() }
    }

    pub(crate) fn add(&mut self, name: &'static str, time: Duration) {
        if !self.enabled {
            return;
        }
        
        match self.phases.iter_mut().find(|p| p.0 == name) {
            Some(p) => {
                p.1 += time;
                p.2 = p.2.max(time);
            }
            None => self.phases.push((name, time, time)),
        }
    }

    pub(crate) fn end_tick(&mut self) {
        if !self.enabled {
            return;
        }
        self.ticks += 1;
        if self.ticks < REPORT_TICKS {
            return;
        }
        let ms = |d: Duration| d.as_secs_f64() * 1000.0;
        self.phases.sort_by_key(|p| std::cmp::Reverse(p.1));
        println!("[Timings] {} ticks, per tick:", self.ticks);
        for (name, total, max) in &self.phases {
            println!("[Timings]   {name:<28} {:>8.3} ms avg {:>8.3} ms max", ms(*total) / self.ticks as f64, ms(*max));
        }
        self.phases.clear();
        self.ticks = 0;
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
