use std::time::Duration;

use contracts::chat::Completion;
use contracts::tool::Tool;
use serde_json::json;
use tools::shell::BashTool;
use tools::workspace::Workspace;

use super::*;
use crate::jobs::{JobsTool, Kit};
use crate::testing::{Scratch, composed, quiet, results, says, users};

fn bash(arguments: serde_json::Value) -> Completion {
    crate::testing::call("b1", "bash", arguments)
}

/// What came back from the background to the model, as it read it.
fn came_back(rig: &crate::compose::Rig) -> String {
    let came: Vec<String> = (users(rig).into_iter())
        .filter(|text| text.contains("<background job="))
        .collect();
    came.join("\n")
}

#[test]
fn in_exec_a_task_waits_for_a_command_it_started_in_the_background_and_reads_its_end() {
    let replies = vec![
        bash(
            json!({"command": "sleep 0.5; echo built", "run_in_background": true,
                    "description": "build"}),
        ),
        says("Started it."),
        says("It built."),
    ];
    composed(crate::modes::built("bypass"), false, replies, |rig| {
        rig.task("build it");
        let started = &results(rig)[0].1;
        assert!(
            started.starts_with("Running in the background: job `build` (job-1)"),
            "{started}"
        );
        let came = came_back(rig);
        assert!(
            came.contains("<background job=\"build\" ended=\"Exit code: 0\">"),
            "{came}"
        );
        assert!(came.contains("Output:\nbuilt"), "{came}");
        0
    });
}

#[test]
fn a_command_still_running_after_its_wait_goes_on_in_the_background_and_its_end_comes_back() {
    let replies = vec![
        bash(json!({"command": "echo first; sleep 1; echo second", "timeout_ms": 200})),
        says("It runs on."),
        says("It printed both."),
    ];
    composed(crate::modes::built("bypass"), false, replies, |rig| {
        rig.task("run it");
        let moved = &results(rig)[0].1;
        assert!(moved.starts_with("Still running after: "), "{moved}");
        assert!(moved.ends_with("Output so far:\nfirst"), "{moved}");
        let came = came_back(rig);
        assert!(came.contains("Output:\nfirst\nsecond"), "{came}");
        0
    });
}

#[test]
fn a_command_in_the_background_is_listed_and_stopped_as_a_job() {
    let scratch = Scratch::new();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let output = quiet();
    let queue = queue::Queue::new(16);
    let kit = Kit {
        workspace: &workspace,
        gate: &contracts::gate::Open,
        hidden: &Vec::new,
        output: &output,
        session: true,
        queue: &queue,
    };
    let jobs = Jobs::new("shells-lane", kit);
    std::thread::scope(|scope| {
        let shells = Shells { jobs: &jobs, scope };
        let bash = BashTool {
            workspace: &workspace,
            gate: &contracts::gate::Open,
            hidden: &Vec::new,
            stop: &|| false,
            background: Some(&shells),
        };
        let tool = JobsTool { jobs: &jobs, scope };
        let started = bash
            .call(&json!({"command": "sleep 30", "run_in_background": true,
                          "description": "nap"}))
            .unwrap()
            .report;
        assert!(started.contains("job `nap` (job-1)"), "{started}");
        let listed = tool.call(&json!({"action": "list"})).unwrap().report;
        assert!(
            listed.starts_with("- background command `nap` (job-1): `sleep 30`, running "),
            "{listed}"
        );
        let stopped = tool.call(&json!({"action": "stop", "job": "nap"})).unwrap();
        assert_eq!(stopped.report, "Stopped `nap`.");
        // The model asked for the stop: nothing more comes of it (B10), and it is listed as
        // ended, by the agent.
        assert!(until(|| (jobs.lock().jobs).is_empty()), "still running");
        assert!(
            queue.take_joining().is_empty(),
            "the model knows it stopped it"
        );
        let ended: Vec<JobEnd> = (jobs.lock().ended.iter())
            .filter_map(|ended| ended.row().ended)
            .collect();
        assert_eq!(ended[0].how, "Stopped: by the agent");
        // One the person stops comes back once, for the task they start next.
        bash.call(&json!({"command": "sleep 30", "run_in_background": true,
                          "description": "nap2"}))
            .unwrap();
        crate::interrupt::stop(&crate::interrupt::job_key("shells-lane", "job-2"));
        let mut came = vec![];
        for _ in 0..50 {
            came.extend(queue.take_joining());
            if !came.is_empty() {
                break;
            }
            queue.wait(Duration::from_millis(100));
        }
        let said: Vec<String> = came.iter().map(queue::Entry::render).collect();
        assert!(
            said.iter().any(
                |said| said.contains("<background job=\"nap2\" ended=\"Stopped: by the person")
            ),
            "{said:?}"
        );
        assert_eq!(came[0].policy.idle, queue::Idle::Waits);
        jobs.close();
    });
    assert!(jobs.lock().jobs.is_empty());
}

/// Waits up to 5 s for `done`.
fn until(done: impl Fn() -> bool) -> bool {
    for _ in 0..100 {
        if done() {
            return true;
        }
        #[expect(clippy::disallowed_methods, reason = "a test waits on another thread")]
        std::thread::sleep(Duration::from_millis(50));
    }
    done()
}

#[test]
#[expect(
    clippy::wildcard_enum_match_arm,
    reason = "a test picks out the variant it checks"
)]
fn a_command_the_person_stops_stays_listed_as_failed_with_its_output_until_let_go() {
    let scratch = Scratch::new();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let (ui, frames) = crate::testing::fake_ui();
    let output = quiet().in_ui(Some(ui.for_lane("shells-ended")));
    let queue = queue::Queue::new(16);
    let kit = Kit {
        workspace: &workspace,
        gate: &contracts::gate::Open,
        hidden: &Vec::new,
        output: &output,
        session: true,
        queue: &queue,
    };
    let jobs = Jobs::new("shells-ended", kit);
    let key = crate::interrupt::job_key("shells-ended", "job-1");
    std::thread::scope(|scope| {
        let _closing = Closing(&jobs);
        let ticking = std::thread::Builder::new().name("llx-test-tick".into());
        ticking.spawn_scoped(scope, || jobs.tick()).unwrap();
        let shells = Shells { jobs: &jobs, scope };
        let bash = BashTool {
            workspace: &workspace,
            gate: &contracts::gate::Open,
            hidden: &Vec::new,
            stop: &|| false,
            background: Some(&shells),
        };
        let started = (bash.call(&json!({"command": "echo one; sleep 30",
            "run_in_background": true, "description": "nap"})))
        .unwrap();
        assert!(started.report.contains("(job-1)"), "{}", started.report);
        // The person looks at what it printed while it runs, then stops it from its row.
        assert!(until(|| outputs::of(&key).as_deref() == Some("one")));
        crate::interrupt::stop(&key);
        assert!(until(|| !jobs.lock().ended.is_empty()));
        // The model hears it was stopped; the row says so, and what it printed stays.
        let said: Vec<String> = (queue.take_joining().iter())
            .map(queue::Entry::render)
            .collect();
        assert!(
            said.iter()
                .any(|said| said.contains("ended=\"Stopped: by the person")),
            "{said:?}"
        );
        let rows = (crate::testing::events(&frames).into_iter().rev())
            .find_map(|(_, event)| match event {
                protocol::Event::Jobs(rows) => Some(rows),
                _ => None,
            })
            .unwrap();
        let end = rows
            .iter()
            .find(|row| row.id == "job-1")
            .and_then(|row| row.ended.clone());
        assert_eq!(
            end,
            Some(JobEnd {
                how: "Stopped: by the person".into(),
                failed: true,
            })
        );
        // With where the whole of it is, as the model read it.
        let kept = outputs::of(&key).unwrap_or_default();
        assert!(kept.starts_with("one\n... the whole output"), "{kept}");
        // `x` again lets it go.
        crate::interrupt::stop(&key);
        assert!(until(|| jobs.lock().ended.is_empty()));
        assert_eq!(outputs::of(&key), None);
    });
}

/// Closes the jobs when dropped, so a failed assertion does not leave the scheduler running and
/// the scope waiting for it.
struct Closing<'a>(&'a Jobs<'a>);

impl Drop for Closing<'_> {
    fn drop(&mut self) {
        self.0.close();
    }
}
