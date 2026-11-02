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
        // Its end comes back once, saying it was stopped, and it is no longer listed.
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
            said.iter()
                .any(|said| said.contains("ended=\"Stopped: by the person")),
            "{said:?}"
        );
        jobs.close();
    });
    assert!(jobs.lock().jobs.is_empty());
}
