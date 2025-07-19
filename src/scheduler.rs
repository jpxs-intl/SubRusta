use std::sync::{Arc, Mutex};

use dashmap::DashMap;

use crate::app_state::AppState;

pub type ScheduledFunction = Box<dyn Fn(Arc<AppState>) + Send + Send>;
type InternalFunction = Arc<Mutex<ScheduledFunction>>;

#[derive(Default)]
pub struct TaskScheduler {
    scheduled_tasks: DashMap<i32, Vec<ScheduledTask>>
}

#[derive(Clone)]
pub struct ScheduledTask {
    pub task: InternalFunction,
    pub last_ran: i32,
    pub time_between: Option<i32>
}

impl TaskScheduler {
    pub fn new() -> Self {
        Self {
            scheduled_tasks: DashMap::new()
        }
    }

    pub fn task_count(&self) -> u32 {
        self.scheduled_tasks.len() as u32
    }

    pub fn schedule_task(&self, tick: i32, time_between: Option<i32>, function: ScheduledFunction) {
        let task = ScheduledTask {
            task: Arc::new(Mutex::new(function)),
            time_between,
            last_ran: 0
        };

        self.insert_task(tick, task);
    }

    fn insert_task(&self, tick: i32, task: ScheduledTask) {
        if let Some(mut task_list) = self.scheduled_tasks.get_mut(&tick) {
            task_list.push(task);
        } else {
            self.scheduled_tasks.insert(tick, vec![task]);
        }
    }

    pub fn run_tasks(&self, state: &Arc<AppState>) {
        let tick = state.network_tick();

        if let Some(tasks) = self.scheduled_tasks.get(&tick) {
            let mut to_readd = vec![];

            for task in tasks.clone().iter_mut() {
                let taskfn = task.task.lock().unwrap();
                (taskfn)(state.clone());

                task.last_ran = tick;

                if let Some(time) = task.time_between {
                    to_readd.push(((time + tick), task.clone()));
                }
            }

            drop(tasks);
            self.scheduled_tasks.remove(&tick);

            for (tick, task) in to_readd {
                self.insert_task(tick, task);
            }
        }
    }
}