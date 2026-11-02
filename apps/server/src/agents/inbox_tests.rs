use std::sync::Arc;

use contracts::gate::Open;
use contracts::inbox::Inbox;
use tools::workspace::Workspace;

use super::*;
use crate::jobs::{Jobs, Kit};
use crate::testing::{Captured, Scratch, quiet};

/// The main agent's inbox on `output`, with exec's jobs, in a task.
fn with_inbox(output: &Output, work: impl FnOnce(&MainInbox, &Registry, &Jobs)) {
    let scratch = Scratch::new();
    let workspace = Workspace::new(scratch.path()).unwrap();
    let jobs_output = quiet();
    let queue = Arc::new(crate::arrivals::queue(None));
    queue.begin();
    let kit = Kit {
        workspace: &workspace,
        gate: &Open,
        hidden: &Vec::new,
        output: &jobs_output,
        session: false,
        queue: &queue,
    };
    let (registry, jobs) = (Registry::new(Arc::clone(&queue)), Jobs::new("s", kit));
    let inbox = MainInbox {
        registry: &registry,
        jobs: &jobs,
        output,
        stopped: &|| false,
    };
    work(&inbox, &registry, &jobs);
}

fn answer(registry: &Registry, text: &str) {
    (registry.queue).post(crate::arrivals::report("a", "s1", "done", text));
}

#[test]
fn the_person_comes_first_then_the_queue_in_arrival_order() {
    let (ui, peer, _frames) = crate::testing::fake_peer();
    let ui = ui.for_lane("lane");
    // The person said it to the task running.
    let input = protocol::Input {
        session: "lane".into(),
        text: "stop, use the other file".into(),
        ..protocol::Input::default()
    };
    peer.inputs.input(input, true);
    let output = quiet().in_ui(Some(ui));
    with_inbox(&output, |inbox, registry, jobs| {
        jobs.queue().post(crate::arrivals::monitor("m", "tick"));
        answer(registry, "found");
        let came = inbox.take();
        assert!(came[0].starts_with("stop, use"), "{came:?}");
        // Then the queue, in the order things came: the monitor first here.
        assert!(came[1].contains("<monitor job=\"m\">"), "{came:?}");
        assert!(came[2].starts_with("<agent_result"), "{came:?}");
    });
}

#[test]
fn the_waiting_line_is_said_once_until_the_task_ends() {
    let said = Captured::default();
    let output = Output::with(
        false,
        false,
        Box::new(Captured::default()),
        Box::new(said.clone()),
    );
    with_inbox(&output, |inbox, registry, _| {
        let rendered = |text: &str| crate::arrivals::report("a", "s1", "done", text).render();
        answer(registry, "one");
        assert_eq!(inbox.wait(), [rendered("one")]);
        answer(registry, "two");
        assert_eq!(inbox.wait(), [rendered("two")]);
        assert_eq!(said.text().matches("Waiting").count(), 1, "{}", said.text());
        // The task ends however it ends (stopped, out of turns, done): the next one says it again.
        registry.settle();
        answer(registry, "three");
        assert_eq!(inbox.wait(), [rendered("three")]);
        assert_eq!(said.text().matches("Waiting").count(), 2, "{}", said.text());
    });
}
