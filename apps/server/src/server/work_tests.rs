use protocol::{Event, JobRow, Running, Update};

use super::Work;

fn job(name: &str, monitor: bool) -> JobRow {
    JobRow {
        id: format!("job-{name}"),
        name: name.into(),
        kind: match monitor {
            true => protocol::JobKind::Monitor,
            false => protocol::JobKind::Schedule,
        },
        detail: "gh run watch".into(),
        until_at: monitor.then_some(1),
        next: (!monitor).then(|| "14:05".to_string()),
        events: 0,
    }
}

fn running(kind: &str, name: &str) -> Running {
    Running {
        kind: kind.into(),
        name: name.into(),
    }
}

fn started(work: &Work, id: &str, name: &str) {
    let (id, name) = (id.to_string(), name.to_string());
    work.saw(None, &Event::LaneStarted { id, name });
}

#[test]
fn a_lane_working_is_named_by_its_title_and_its_jobs_by_theirs() {
    let work = Work::default();
    started(&work, "A", "first");
    started(&work, "B", "second");
    assert_eq!(work.running(), [], "lanes at their prompt stop nothing");
    work.saw(Some("A"), &Event::TaskStarted("fix it".into()));
    let title = Some("Fix the flaky test".to_string());
    work.saw(Some("A"), &Event::Updated(Update { title }));
    let jobs = vec![job("watch-ci", true), job("nightly", false)];
    work.saw(Some("B"), &Event::Jobs(jobs));
    assert_eq!(
        work.running(),
        [
            running("session", "Fix the flaky test"),
            running("monitor", "watch-ci"),
            running("schedule", "nightly"),
        ]
    );
    let ended = Event::TaskEnded {
        status: "done".into(),
        millis: 1,
        turns: 1,
        cost: 0.0,
    };
    work.saw(Some("A"), &ended);
    work.ended("B");
    assert_eq!(work.running(), []);
}

#[test]
fn an_agents_task_is_not_a_lanes() {
    let work = Work::default();
    started(&work, "A", "first");
    work.saw(Some("agent-1"), &Event::TaskStarted("look".into()));
    assert_eq!(work.running(), []);
}
