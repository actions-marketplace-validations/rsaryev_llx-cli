//! What runs in the lanes, as their events say it: for the question asked before quitting, which
//! names what would stop. Only lanes are known here, by the desk's `LaneStarted`: an agent's
//! task is its lane's work.

use std::sync::{Mutex, MutexGuard};

use protocol::{Event, JobKind, JobRow, Running};

#[derive(Default)]
struct Lane {
    id: String,
    title: String,
    busy: bool,
    jobs: Vec<JobRow>,
}

/// Every lane's state, the first opened first.
#[derive(Default)]
pub(crate) struct Work {
    lanes: Mutex<Vec<Lane>>,
}

impl Work {
    /// `event`, sent in `lane`'s name (`None` for the server's own).
    pub(crate) fn saw(&self, lane: Option<&str>, event: &Event) {
        // Only these can change a lane, whoever sends them (an agent's find none): any other
        // event (a step, a call, a note) takes no lock.
        let change: &dyn Fn(&mut Lane) = match event {
            Event::LaneStarted { id, name } => {
                let mut lanes = self.lock();
                lanes.retain(|known| known.id != *id);
                let (id, title) = (id.clone(), name.clone());
                return lanes.push(Lane {
                    id,
                    title,
                    ..Lane::default()
                });
            }
            Event::TaskStarted(_) => &|state| state.busy = true,
            Event::TaskEnded { .. } => &|state| state.busy = false,
            Event::Updated(update) => match &update.title {
                Some(title) => &|state| state.title.clone_from(title),
                None => return,
            },
            Event::Jobs(rows) => &|state| state.jobs.clone_from(rows),
            Event::Header(_)
            | Event::JobOutput { .. }
            | Event::Note(_)
            | Event::Error(_)
            | Event::Mode(_)
            | Event::Call { .. }
            | Event::Result { .. }
            | Event::Step(_)
            | Event::Answer(_)
            | Event::Earlier(_)
            | Event::LaneEnded
            | Event::Saved(_)
            | Event::Show(_)
            | Event::Started { .. }
            | Event::Closed
            | Event::Spent(_)
            | Event::Total(_)
            | Event::AgentsCleared
            | Event::Prefill(_)
            | Event::Context(_)
            | Event::Queue { .. }
            | Event::Steered(_)
            | Event::Copy(_)
            | Event::Quit => return,
        };
        let mut lanes = self.lock();
        if let Some(state) = lanes.iter_mut().find(|known| Some(&*known.id) == lane) {
            change(state);
        }
    }

    /// Whether a task of lane `lane` runs (words the person sends are for it); `None` when the
    /// lane is not open: never started, or ended.
    pub(crate) fn busy(&self, lane: &str) -> Option<bool> {
        let lanes = self.lock();
        lanes
            .iter()
            .find(|known| known.id == lane)
            .map(|known| known.busy)
    }

    /// The lane ended: nothing of it runs.
    pub(crate) fn ended(&self, lane: &str) {
        self.lock().retain(|known| known.id != lane);
    }

    /// The lanes working, by title, then the lanes' monitors and schedules, by name.
    pub(crate) fn running(&self) -> Vec<Running> {
        let lanes = self.lock();
        let working = lanes.iter().filter(|lane| lane.busy);
        let working = working.map(|lane| ("session", lane.title.clone()));
        // A command that ended is listed for the person to look at: nothing of it runs.
        let jobs = lanes.iter().flat_map(|lane| &lane.jobs);
        let jobs = jobs.filter(|job| job.ended.is_none()).map(|job| {
            let kind = match job.kind {
                JobKind::Monitor => "monitor",
                JobKind::Schedule => "schedule",
                JobKind::Shell => "command",
            };
            (kind, job.name.clone())
        });
        let all = working.chain(jobs);
        all.map(|(kind, name)| Running {
            kind: kind.into(),
            name,
        })
        .collect()
    }

    fn lock(&self) -> MutexGuard<'_, Vec<Lane>> {
        crate::sync::lock(&self.lanes)
    }
}

#[cfg(test)]
#[path = "work_tests.rs"]
mod tests;
