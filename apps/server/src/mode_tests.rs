use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::gate::{Asker, TerminalGate};
use crate::modes::{Modes, built};
use crate::testing::Typed;
use protocol::Approval;

/// Asking at a terminal that reads `typed`, when there is one.
fn asking<'a>(
    mode: &'a ModeCell,
    typed: Option<&'a Typed>,
    waited: &'a (dyn Fn(Duration) + Sync),
) -> ModeGate<'a> {
    let asker = typed.map(|typed| Box::new(TerminalGate(typed)) as Box<dyn Asker + 'a>);
    ModeGate::new(mode, asker, waited)
}

/// A built-in mode with the command line's `rules` beside its own.
fn ruled(name: &str, rules: &Rules) -> Mode {
    Modes::built_in().with(rules).named(name).unwrap()
}

fn effect(tool: &str) -> Effect<'_> {
    Effect {
        tool,
        subject: "",
        summary: "the summary",
        needs: Needs::Sandbox,
    }
}

fn command(line: &str) -> Effect<'_> {
    Effect {
        tool: "bash",
        subject: line,
        summary: line,
        needs: Needs::Sandbox,
    }
}

/// An effect that must reach beyond its sandbox: a command the model escalated.
fn open(tool: &str) -> Effect<'_> {
    Effect {
        needs: Needs::Open,
        ..effect(tool)
    }
}

fn denied(verdict: Verdict) -> String {
    match verdict {
        Verdict::Deny(why) => why,
        Verdict::Allow | Verdict::Workspace | Verdict::ReadOnly => panic!("allowed"),
    }
}

#[test]
fn default_works_in_the_workspace_without_asking() {
    let mode = ModeCell::new(built("default"));
    let typed = Typed::new(&[]);
    let waited = |_: Duration| panic!("nobody waited");
    let gate = asking(&mode, Some(&typed), &waited);
    for tool in ["bash", "apply_patch", "jobs"] {
        assert_eq!(gate.check(effect(tool)), Verdict::Workspace, "{tool}");
    }
    let gate = asking(&mode, None, &waited);
    assert_eq!(gate.check(command("cargo test")), Verdict::Workspace);
    assert!(typed.asked.lock().unwrap().is_empty(), "nobody was asked");
}

#[test]
fn default_asks_what_reaches_beyond_the_sandbox_and_refuses_it_without_anyone() {
    let mode = ModeCell::new(built("default"));
    let typed = Typed::new(&["y\n"]);
    let waited = |_: Duration| {};
    let gate = asking(&mode, Some(&typed), &waited);
    assert_eq!(gate.check(open("bash")), Verdict::Allow);
    assert_eq!(typed.asked.lock().unwrap().len(), 1);
    let gate = asking(&mode, None, &waited);
    assert_eq!(denied(gate.check(open("bash"))), NO_ONE);
    assert_eq!(denied(gate.check(open("apply_patch"))), NO_ONE);
}

#[test]
fn a_read_only_mode_refuses_what_must_leave_its_sandbox() {
    let more = Rules::new(&["bash".into()], &[]).unwrap();
    let mode = ModeCell::new(ruled("plan", &more));
    let typed = Typed::new(&["y\n"]);
    let waited = |_: Duration| panic!("nobody waited");
    let gate = asking(&mode, Some(&typed), &waited);
    let why = denied(gate.check(open("bash")));
    assert!(why.contains("read-only") && why.contains("plan"), "{why}");
    assert!(typed.asked.lock().unwrap().is_empty(), "nobody was asked");
}

#[test]
fn plan_runs_commands_read_only_without_asking_and_its_deny_rules_still_refuse() {
    let mode = ModeCell::new(built("plan"));
    let typed = Typed::new(&["y\n"]);
    let waited = |_: Duration| {};
    let gate = asking(&mode, Some(&typed), &waited);
    assert_eq!(gate.check(command("rm -rf src")), Verdict::ReadOnly);
    assert_eq!(gate.check(effect("jobs")), Verdict::ReadOnly);
    assert!(typed.asked.lock().unwrap().is_empty(), "nobody was asked");
    let more = Rules::new(&["bash".into()], &["bash(curl *)".into()]).unwrap();
    mode.set(ruled("plan", &more));
    let gate = asking(&mode, None, &waited);
    assert_eq!(
        gate.check(command("cargo test")),
        Verdict::ReadOnly,
        "an allow does not open it"
    );
    assert!(denied(gate.check(command("curl x"))).contains("plan"));
}

#[test]
fn the_command_lines_rules_join_the_modes() {
    let rules = Rules::new(&["bash(cargo test *)".into()], &["bash(git push *)".into()]).unwrap();
    let mode = ModeCell::new(ruled("default", &rules));
    let waited = |_: Duration| {};
    let gate = asking(&mode, None, &waited);
    assert_eq!(
        gate.check(command("cargo test -q")),
        Verdict::Allow,
        "as it is"
    );
    assert_eq!(gate.check(command("cargo build")), Verdict::Workspace);
    assert!(denied(gate.check(command("git push x"))).contains("bash(git push *)"));
    mode.set(ruled("bypass", &rules));
    assert!(denied(gate.check(command("git push"))).contains("bash(git push *)"));
    assert_eq!(gate.check(command("git status")), Verdict::Allow);
}

/// An asker whose person answers "always" each time, counted.
struct Always<'a>(&'a AtomicUsize);

impl Asker for Always<'_> {
    fn approve(&self, _: &Effect) -> Option<Approval> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Some(Approval::Always)
    }
}

#[test]
fn always_is_kept_by_mode() {
    let mode = ModeCell::new(built("default"));
    let asked = AtomicUsize::new(0);
    let count = || asked.load(Ordering::SeqCst);
    let gate = ModeGate::new(&mode, Some(Box::new(Always(&asked))), &|_| {});
    assert_eq!(gate.check(open("bash")), Verdict::Allow);
    assert_eq!(gate.check(open("bash")), Verdict::Allow);
    assert_eq!(count(), 1, "always holds across calls");
    mode.set(Mode::new("review", "", Rules::default(), Reach::Anything));
    assert_eq!(gate.check(open("bash")), Verdict::Allow);
    assert_eq!(gate.check(open("bash")), Verdict::Allow);
    assert_eq!(count(), 2, "review asks once for itself");
    mode.set(built("default"));
    assert_eq!(gate.check(open("bash")), Verdict::Allow);
    assert_eq!(gate.check(open("jobs")), Verdict::Allow);
    assert_eq!(count(), 3, "only bash was always");
}

#[test]
fn bypass_allows_without_asking() {
    let mode = ModeCell::new(built("bypass"));
    let typed = Typed::new(&[]);
    let waited = |_: Duration| panic!("nobody waited");
    let gate = asking(&mode, Some(&typed), &waited);
    for tool in ["bash", "jobs"] {
        assert_eq!(gate.check(effect(tool)), Verdict::Allow, "{tool}");
    }
    assert!(typed.asked.lock().unwrap().is_empty());
}

#[test]
fn the_time_the_person_takes_is_told() {
    let mode = ModeCell::new(built("default"));
    let typed = Typed::new(&["n\n"]);
    let told = Mutex::new(vec![]);
    let waited = |took: Duration| told.lock().unwrap().push(took);
    let gate = asking(&mode, Some(&typed), &waited);
    assert!(matches!(gate.check(open("bash")), Verdict::Deny(_)));
    assert_eq!(told.lock().unwrap().len(), 1);
}

#[test]
fn the_context_states_the_rules_and_what_becomes_of_the_rest() {
    let default = built("default");
    let more = Rules::new(&["bash(cargo test *)".into()], &[]).unwrap();
    let text = describe(&ruled("default", &more), false, false);
    assert!(
        text.starts_with("default:") && text.contains("bash(cargo test *)"),
        "{text}"
    );
    assert!(text.contains("is refused"), "{text}");
    assert!(describe(&default, true, false).contains("is asked"));
    assert!(describe(&default, true, true).contains("`ask`"));
    assert!(
        !describe(&default, true, false).contains("`ask`"),
        "in exec"
    );
    assert!(
        !describe(&default, false, true).contains("`ask`"),
        "with no terminal"
    );
    // A read-only mode is where a plan is made: its questions are the person's to settle too.
    let plan = built("plan");
    assert!(describe(&plan, true, true).contains("`ask`"), "in plan");
    assert!(
        !describe(&plan, false, true).contains("`ask`"),
        "plan, no terminal"
    );
}

#[test]
fn a_mode_that_allows_every_tool_refuses_nothing_in_its_text() {
    for terminal in [true, false] {
        let text = describe(&built("bypass"), terminal, false);
        assert!(
            text.starts_with("bypass:") && text.contains("whatever they do"),
            "{text}"
        );
        assert!(
            !text.contains("is asked")
                && !text.contains("is refused")
                && !text.contains("every part"),
            "{text}"
        );
    }
    assert!(describe(&built("bypass"), true, true).contains("`ask`"));
}

#[test]
fn a_read_only_mode_tells_the_model_its_commands_change_nothing() {
    for terminal in [true, false] {
        let text = describe(&built("plan"), terminal, false);
        assert!(
            text.starts_with("plan:") && text.contains("sandbox"),
            "{text}"
        );
        assert!(
            !text.contains("is asked") && !text.contains("is refused"),
            "{text}"
        );
    }
}

#[test]
fn the_default_mode_tells_the_model_where_its_commands_write_and_how_to_reach_further() {
    let text = describe(&built("default"), true, false);
    assert!(
        text.contains("sandbox") && text.contains("workspace") && text.contains("`escalate`"),
        "{text}"
    );
    assert!(text.contains("is asked"), "{text}");
    let text = describe(&built("default"), false, false);
    assert!(
        text.contains("`escalate`") && text.contains("is refused"),
        "{text}"
    );
}
