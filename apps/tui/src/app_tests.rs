use std::time::{Duration, Instant};

use crossterm::event::KeyCode;

use protocol::{Approval, Command, Reply, ToClient};

use super::*;
use crate::style;
use crate::test_support::{
    approve as approve_asked, call, commands, ctrl, done, failed, history, in_session as from, key,
    live_area, modes, queue, ran, render, replies, result, sent, sent_texts, typed,
};
use protocol::Event;

fn app() -> App {
    let commands = [
        ("help", "the commands"),
        ("mode", "show or switch the mode"),
        ("new", "start over"),
    ];
    let mut app = App::new(
        60,
        20,
        commands
            .iter()
            .map(|(n, w)| (n.to_string(), w.to_string()))
            .collect(),
    );
    app.modes = modes();
    app
}

/// When a question was opened that takes keys at `now`: an approval ignores them at first.
fn opened(now: Instant) -> Instant {
    now.checked_sub(Duration::from_millis(300)).unwrap_or(now)
}

#[test]
fn a_task_leaves_its_blocks_in_order() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("list the files".into()), now);
    let ls = ran("ls");
    app.event(call("c1", "bash", &ls), now);
    assert!(!app.shown().running.is_empty());
    let outcome = done("Exit code: 0\nWall time: 0.0 seconds\nOutput:\nREADME.md");
    app.event(result("c1", "bash", &ls, outcome), now);
    app.event(Event::Answer("One file: `README.md`.".into()), now);
    app.event(
        Event::TaskEnded {
            status: "done".into(),
            millis: 3_000,
            turns: 2,
            cost: 0.001,
        },
        now,
    );
    insta::assert_snapshot!(history(&mut app), @r"
    ❯ list the files

    • Ran ls
      └ README.md

    • One file: README.md.

    ─ Worked for 3.0s · 2 turns · $0.0010 ──────────────────────
    ");
    assert!(app.busy().is_none() && app.shown().running.is_empty());
}

#[test]
fn ctrl_c_on_a_draft_keeps_it_in_history_and_a_second_quits() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "a draft", now);
    assert_eq!(app.input(ctrl('c'), now), None);
    let screen = render(&mut app, now);
    assert!(!screen.contains("a draft"), "{screen}");
    assert!(
        screen.contains("↑ brings it back · ctrl-c again to quit"),
        "{screen}"
    );
    // ↑ brings it back, and a key other than Ctrl-C disarms the quit.
    app.input(key(KeyCode::Up), now);
    assert_eq!(app.composer.text(), "a draft");
    assert_eq!(app.input(ctrl('c'), now), None, "cleared again, not quit");
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Quit));
}

#[test]
fn ctrl_c_at_an_empty_prompt_quits_only_when_pressed_twice_in_time() {
    let now = Instant::now();
    let mut app = app();
    assert_eq!(app.input(ctrl('c'), now), None);
    assert!(render(&mut app, now).contains("ctrl-c again to quit"));
    assert_eq!(
        app.input(ctrl('c'), now + Duration::from_secs(3)),
        None,
        "too late: armed again"
    );
    assert_eq!(
        app.input(ctrl('c'), now + Duration::from_secs(4)),
        Some(Action::Quit)
    );
}

#[test]
fn ctrl_c_while_a_task_works_stops_it_and_keeps_the_draft() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("x".into()), now);
    typed(&mut app, "next idea", now);
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Stop(String::new())));
    assert_eq!(app.composer.text(), "next idea");
    assert_eq!(app.input(key(KeyCode::Esc), now), None, "stopping already");
}

#[test]
fn ctrl_d_quits_from_an_empty_prompt_and_asks_while_a_task_works() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "x", now);
    assert_ne!(app.input(ctrl('d'), now), Some(Action::Quit), "a draft");
    app.composer.clear();
    assert_eq!(app.input(ctrl('d'), now), Some(Action::Quit));
    app.event(Event::TaskStarted("x".into()), now);
    // The server says the task runs.
    app.work = Box::new(|| {
        let (kind, name) = ("session".into(), "x".into());
        vec![protocol::Running { kind, name }]
    });
    assert_eq!(app.input(ctrl('d'), now), None);
    assert!(render(&mut app, now).contains("Background work is running"));
}

#[test]
fn esc_twice_on_an_empty_prompt_says_what_the_second_does() {
    let now = Instant::now();
    let mut app = app();
    app.input(key(KeyCode::Esc), now);
    assert!(render(&mut app, now).contains("esc again to rewind"));
}

#[test]
fn an_approval_is_answered_with_a_key() {
    let now = Instant::now();
    let mut app = app();
    // Approvals come while a task works.
    app.event(Event::TaskStarted("a task".into()), now);
    app.asked(1, "", approve_asked("bash", "run cargo test"), opened(now));
    app.input(key(KeyCode::Char('3')), now);
    typed(&mut app, "use make", now);
    app.input(key(KeyCode::Enter), now);
    let no = Reply::Approval(Approval::No(Some("use make".into())));
    assert_eq!(replies(&mut app), [(1, no)]);
    assert!(app.questions.is_empty());
    app.asked(2, "", approve_asked("bash", "rm -rf build"), opened(now));
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Stop(String::new())));
    assert_eq!(
        replies(&mut app),
        [(2, Reply::Approval(Approval::No(None)))]
    );
}

#[test]
fn keys_typed_ahead_as_an_approval_opens_do_not_answer_it() {
    let now = Instant::now();
    let mut app = app();
    // Approvals come while a task works.
    app.event(Event::TaskStarted("a task".into()), now);
    app.asked(1, "", approve_asked("bash", "rm -rf build"), now);
    // Typed for the composer just as the question opened: j, k, digits and Enter go nowhere.
    let soon = now + Duration::from_millis(120);
    typed(&mut app, "jk2", soon);
    app.input(key(KeyCode::Enter), soon);
    assert_eq!(replies(&mut app), []);
    assert!(!app.questions.is_empty(), "the approval is still open");
    let later = now + Duration::from_millis(300);
    app.input(key(KeyCode::Char('3')), later);
    typed(&mut app, "use make", later);
    app.input(key(KeyCode::Enter), later);
    let no = Reply::Approval(Approval::No(Some("use make".into())));
    assert_eq!(replies(&mut app), [(1, no)]);
    app.asked(2, "", approve_asked("bash", "rm -rf build"), now);
    // Ctrl-C still stops at once.
    assert_eq!(
        app.input(ctrl('c'), soon),
        Some(Action::Stop(String::new()))
    );
    assert_eq!(
        replies(&mut app),
        [(2, Reply::Approval(Approval::No(None)))]
    );
}

#[test]
fn slash_offers_the_commands_and_enter_takes_the_marked_one() {
    let now = Instant::now();
    let mut app = app();
    app.event(queue(true, &[], &[], &[]), now);
    typed(&mut app, "/", now);
    insta::assert_snapshot!(render(&mut app, now), @"
    ────────────────────────────────────────────────────────────
    ❯ /
    ────────────────────────────────────────────────────────────
      /help       the commands
      /mode       show or switch the mode
      /new        start over
      default mode · shift+tab to cycle
    ");
    typed(&mut app, "n", now);
    insta::assert_snapshot!(render(&mut app, now), @"
    ────────────────────────────────────────────────────────────
    ❯ /n
    ────────────────────────────────────────────────────────────
      /new        start over
      default mode · shift+tab to cycle
    ");
    assert_eq!(app.input(key(KeyCode::Enter), now), None);
    assert_eq!(
        commands(&mut app),
        vec![Command::Open {
            target: protocol::Target::New
        }],
        "the marked one"
    );
    assert!(
        sent_texts(&mut app).is_empty(),
        "the desk opens it, not the lane"
    );
    typed(&mut app, "/m", now);
    app.input(key(KeyCode::Tab), now);
    typed(&mut app, "plan", now);
    insta::assert_snapshot!(render(&mut app, now), @"
    ────────────────────────────────────────────────────────────
    ❯ /mode plan
    ────────────────────────────────────────────────────────────
      mode  ›  current: default · ↑↓ choose · tab complete · en…
      plan         Read-only: file changes are refused
      default mode · shift+tab to cycle
    ");
    app.input(key(KeyCode::Enter), now);
    assert_eq!(sent_texts(&mut app), ["/mode plan"]);
}

#[test]
fn command_drill_down_shows_modes_and_preserves_argument_entry() {
    let now = Instant::now();
    let mut app = app();
    app.event(queue(true, &[], &[], &[]), now);
    typed(&mut app, "/mo", now);
    app.input(key(KeyCode::Enter), now);
    assert_eq!(
        sent_texts(&mut app),
        ["/mode"],
        "a part runs the marked one"
    );
    let sent = |app: &mut App, text: &str| {
        typed(app, text, now);
        app.input(key(KeyCode::Enter), now);
        sent_texts(app).pop()
    };
    assert_eq!(sent(&mut app, "/mode").as_deref(), Some("/mode"));
    assert_eq!(sent(&mut app, "/plan").as_deref(), Some("/plan"));
    typed(&mut app, "/mode ", now);
    insta::assert_snapshot!(render(&mut app, now), @"
    ────────────────────────────────────────────────────────────
    ❯ /mode
    ────────────────────────────────────────────────────────────
      mode  ›  current: default · ↑↓ choose · tab complete · en…
      default      Works in the workspace without asking
      plan         Read-only: file changes are refused
      bypass       Everything runs without asking (-y)
      review       The person's own mode.
      default mode · shift+tab to cycle
    ");
    typed(&mut app, "rev", now);
    app.input(key(KeyCode::Tab), now);
    assert_eq!(sent(&mut app, "").as_deref(), Some("/mode review"));
    assert_eq!(
        sent(&mut app, "/permissions by").as_deref(),
        Some("/permissions bypass"),
        "a part runs the mode marked"
    );
}

#[test]
fn freeform_plan_task_and_unknown_mode_are_not_replaced_by_suggestions() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "/plan fix the tests", now);
    assert!(app.offers().is_empty());
    app.input(key(KeyCode::Enter), now);
    typed(&mut app, "/mode custom", now);
    assert!(app.offers().is_empty());
    app.input(key(KeyCode::Enter), now);
    assert_eq!(
        sent_texts(&mut app),
        ["/plan fix the tests", "/mode custom"]
    );
}

#[test]
fn leaving_ends_every_wait() {
    let mut app = app();
    app.leave();
    assert_eq!(commands(&mut app), [Command::Leave]);
}

#[test]
fn a_call_the_person_refused_says_what_they_said_not_an_error() {
    let now = Instant::now();
    let mut app = app();
    let git_log = ran("git log");
    app.event(call("c1", "bash", &git_log), now);
    app.asked(1, "", approve_asked("bash", "git log"), opened(now));
    app.input(key(KeyCode::Char('3')), now);
    typed(&mut app, "not this repository", now);
    app.input(key(KeyCode::Enter), now);
    let outcome = failed("not allowed: the person declined it", true);
    app.event(result("c1", "bash", &git_log, outcome), now);
    let lines = crate::test_support::rows(app.take_history());
    assert_eq!(lines[0].spans[0].style, style::waiting());
    insta::assert_snapshot!(lines.iter().map(style::plain).collect::<Vec<_>>().join("\n"), @r"
    • Skipped git log
      └ You said no: not this repository
    ");
}

#[test]
fn at_offers_files_and_enter_puts_the_path_in_without_sending() {
    let now = Instant::now();
    let mut app = app();
    app.files = Files::with(vec!["src/main.rs".into(), "README.md".into()]);
    typed(&mut app, "explain @mai", now);
    assert_eq!(
        app.offers(),
        [crate::keys::Offer::File("src/main.rs".into())]
    );
    app.input(key(KeyCode::Enter), now);
    assert_eq!(app.composer.text(), "explain src/main.rs ");
    assert!(sent(&mut app).is_empty(), "nothing was sent");
}

#[test]
fn pasted_text_is_shown_as_a_mark_but_sent_as_text() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "review ", now);
    app.input(Input::Paste("first\r\nsecond".into()), now);
    assert_eq!(app.composer.text(), "review [Paste] ");
    app.input(key(KeyCode::Enter), now);
    assert_eq!(sent_texts(&mut app), ["review first\nsecond "]);
}

#[test]
fn dropped_files_become_marks_and_go_with_the_message() {
    let now = Instant::now();
    let dir = crate::test_support::Scratch::new();
    let picture = dir.path().join("Screenshot at 2.26.05\u{202f}AM.png");
    std::fs::write(&picture, b"\x89PNG\r\n\x1a\n").unwrap();
    let notes = dir.path().join("notes.md");
    std::fs::write(&notes, "# notes").unwrap();
    let mut app = app();
    app.pasted = dir.path().join("kept");
    typed(&mut app, "compare ", now);
    let dropped = format!(
        "{} {}",
        picture.display().to_string().replace(' ', "\\ "),
        notes.display()
    );
    app.input(Input::Paste(dropped), now);
    assert_eq!(
        app.composer.text(),
        "compare [image: Screenshot at 2.26.05\u{202f}AM.png] [file: notes.md]"
    );
    app.input(key(KeyCode::Enter), now);
    let sent = sent(&mut app).remove(0);
    assert_eq!(
        sent.text,
        format!(
            "compare [image: Screenshot at 2.26.05\u{202f}AM.png] {}",
            notes.display()
        )
    );
    assert_eq!(sent.images.len(), 1);
    assert!(
        sent.images[0].starts_with(dir.path().join("kept")),
        "a kept copy is sent"
    );
}

#[test]
fn a_picture_path_typed_and_kept_for_after_with_tab_goes_as_a_picture() {
    let now = Instant::now();
    let dir = crate::test_support::Scratch::new();
    let picture = dir.path().join("shot.png");
    std::fs::write(&picture, b"\x89PNG\r\n\x1a\n").unwrap();
    let mut app = app();
    app.pasted = dir.path().join("kept");
    app.event(Event::TaskStarted("go".into()), now);
    typed(&mut app, &format!("look at {}", picture.display()), now);
    app.input(key(KeyCode::Tab), now);
    let sent = sent(&mut app).remove(0);
    assert!(sent.later, "kept for after the task");
    assert_eq!(sent.text, "look at [image: shot.png]");
    assert_eq!(sent.images.len(), 1, "{sent:?}");
}

#[test]
fn slash_then_arrows_then_enter_runs_the_command_chosen() {
    // The bug: "/" alone, Down to a command, Enter sent "/" and the session said it was unknown.
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "/", now);
    app.input(key(KeyCode::Down), now);
    app.input(key(KeyCode::Enter), now);
    assert_eq!(sent_texts(&mut app), ["/mode"]);
    assert!(app.composer.is_empty());
}

#[test]
fn a_refusal_is_drawn_on_the_call_refused_not_on_another_that_failed_meanwhile() {
    let now = Instant::now();
    let mut app = app();
    let (refused, other) = (ran("git log"), ran("false"));
    app.event(call("refused", "bash", &refused), now);
    app.event(call("other", "bash", &other), now);
    app.asked(1, "", approve_asked("bash", "git log"), opened(now));
    app.input(key(KeyCode::Esc), now);
    assert_eq!(app.lane().declined.as_deref(), Some(""));
    let declined = "not allowed: the person declined it; do not try the same thing another way";
    for (id, label, outcome) in [
        ("other", other, failed("exit 1", false)),
        ("refused", refused, failed(declined, true)),
    ] {
        app.event(result(id, "bash", &label, outcome), now);
    }
    let shown = history(&mut app);
    assert!(!shown.contains("Skipped false"), "{shown}");
    assert!(shown.contains("Skipped git log"), "{shown}");
}

/// Two lanes, A (shown, started first) and B, each with its name.
fn lanes(now: Instant) -> App {
    let mut app = app();
    for (id, name) in [("A", "fix the flaky jobs test"), ("B", "add tabs")] {
        let (id, name) = (id.into(), name.into());
        app.event(Event::LaneStarted { id, name }, now);
    }
    app
}

#[test]
fn typing_goes_to_the_shown_lane() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.show("B");
    typed(&mut app, "for b", now);
    app.input(key(KeyCode::Enter), now);
    let sent: Vec<(String, String)> = (sent(&mut app).into_iter())
        .map(|input| (input.session, input.text))
        .collect();
    assert_eq!(sent, [("B".to_string(), "for b".to_string())]);
}

#[test]
fn steered_is_per_lane() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.frame(from("A", Event::TaskStarted("a's task".into())), now);
    app.frame(from("B", Event::TaskStarted("b's task".into())), now);
    app.show("B");
    typed(&mut app, "steer b", now);
    app.input(key(KeyCode::Enter), now);
    let sessions: Vec<String> = sent(&mut app).into_iter().map(|i| i.session).collect();
    assert_eq!(sessions, ["B"]);
    app.frame(from("B", queue(false, &["steer b"], &[], &[])), now);
    assert!(live_area(&app, now).contains("↳ steer b"));
    app.show("A");
    assert!(!live_area(&app, now).contains("steer b"));
}

#[test]
fn leave_answers_every_lane() {
    let now = Instant::now();
    let mut app = lanes(now);
    let picked = choose(&mut app, "B", now);
    app.leave();
    assert_eq!(commands(&mut app), [Command::Leave]);
    assert_eq!(replies(&mut app), [(picked, Reply::Choice(None))]);
}

#[test]
fn esc_from_agent_view_shows_its_parent_lane() {
    let now = Instant::now();
    let mut app = lanes(now);
    let started = Event::Started {
        id: "scout".into(),
        name: "scout".into(),
    };
    app.frame(from("B", started), now);
    app.show("scout");
    app.input(key(KeyCode::Esc), now);
    assert_eq!(app.views.shown, "B");
}

#[test]
fn exit_typed_in_any_lane_quits_the_interface() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.show("B");
    typed(&mut app, "/exit", now);
    assert_eq!(app.input(key(KeyCode::Enter), now), Some(Action::Quit));
    assert!(
        sent(&mut app).is_empty(),
        "the lane is not told: every lane is"
    );
}

/// An approval of `summary` from lane `id`; the id its reply goes under.
fn approve(app: &mut App, id: &str, summary: &str, at: Instant) -> u64 {
    let asked = next_id();
    app.asked(asked, id, approve_asked("bash", summary), at);
    asked
}

/// A choice of llx's own from lane `id`; the id its pick goes under.
fn choose(app: &mut App, id: &str, now: Instant) -> u64 {
    let asked = next_id();
    let choose = protocol::Asked::Choose {
        title: "Login › Provider".into(),
        options: vec![("Yes".into(), String::new()), ("No".into(), String::new())],
        marked: None,
    };
    app.asked(asked, id, choose, now);
    asked
}

#[test]
fn hidden_lane_approval_opens_labelled() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.show("B");
    let _answer = approve(&mut app, "A", "cargo test -p jobs", now);
    let shown = live_area(&app, now);
    assert!(
        shown.contains("session fix the flaky jobs test (in the background)"),
        "{shown}"
    );
}

#[test]
fn hidden_lane_with_no_name_yet_asks_as_the_session_list_names_it() {
    let now = Instant::now();
    let mut app = lanes(now);
    let (id, name) = ("C".into(), String::new());
    app.event(Event::LaneStarted { id, name }, now);
    let _answer = approve(&mut app, "C", "cargo test -p jobs", now);
    let shown = render(&mut app, now);
    assert!(
        shown.contains("session New session (in the background)"),
        "{shown}"
    );
}

#[test]
fn own_lane_approval_has_no_label() {
    let now = Instant::now();
    let mut app = lanes(now);
    let _answer = approve(&mut app, "A", "cargo test -p jobs", now);
    let shown = live_area(&app, now);
    assert!(shown.contains("cargo test -p jobs"), "{shown}");
    assert!(!shown.contains("session "), "{shown}");
}

#[test]
fn second_question_waits() {
    let now = Instant::now();
    let mut app = lanes(now);
    let a = approve(&mut app, "A", "first command", opened(now));
    let b = approve(&mut app, "B", "second command", opened(now));
    let shown = live_area(&app, now);
    assert!(shown.contains("first command"), "{shown}");
    assert!(!shown.contains("second command"), "{shown}");
    assert!(shown.contains("1 more question waiting"), "{shown}");
    app.input(key(KeyCode::Char('1')), now);
    assert_eq!(replies(&mut app), [(a, Reply::Approval(Approval::Yes))]);
    let later = now + Duration::from_millis(300);
    let shown = live_area(&app, later);
    assert!(shown.contains("second command"), "{shown}");
    assert!(!shown.contains("more question"), "{shown}");
    app.input(key(KeyCode::Esc), later);
    assert_eq!(
        replies(&mut app),
        [(b, Reply::Approval(Approval::No(None)))]
    );
    assert!(app.questions.is_empty());
}

#[test]
fn held_choice_waits_until_shown() {
    let now = Instant::now();
    let mut app = lanes(now);
    let picked = choose(&mut app, "B", now);
    assert!(app.questions.is_empty(), "held until B is shown");
    let shown = live_area(&app, now);
    assert!(!shown.contains("Login › Provider"), "{shown}");
    app.show("B");
    let shown = live_area(&app, now);
    assert!(shown.contains("Login › Provider"), "{shown}");
    app.input(key(KeyCode::Enter), now);
    assert_eq!(replies(&mut app), [(picked, Reply::Choice(Some(0)))]);
}

#[test]
fn held_choice_does_not_block_other_lanes() {
    let now = Instant::now();
    let mut app = lanes(now);
    let _picked = choose(&mut app, "B", now);
    let a = approve(&mut app, "A", "cargo build", opened(now));
    app.input(key(KeyCode::Char('1')), now);
    assert_eq!(
        replies(&mut app),
        [(a, Reply::Approval(Approval::Yes))],
        "B's choice still waits for B"
    );
}

#[test]
fn keys_within_300ms_of_a_hidden_question_are_ignored() {
    let now = Instant::now();
    let mut app = lanes(now);
    let question = protocol::Question {
        id: "file".into(),
        text: "Which file?".into(),
        choices: vec![protocol::Choice {
            label: "a.rs".into(),
            ..protocol::Choice::default()
        }],
        multiple: false,
        ..protocol::Question::default()
    };
    let ask = protocol::Asked::Question {
        question,
        at: 0,
        of: 1,
        headers: vec![],
    };
    app.asked(7, "B", ask, now);
    app.input(key(KeyCode::Char('1')), now + Duration::from_millis(100));
    assert_eq!(replies(&mut app), [], "typed for the composer");
    app.input(key(KeyCode::Char('1')), now + Duration::from_millis(300));
    let replied = replies(&mut app);
    assert!(
        matches!(&replied[..], [(7, Reply::Question(Some(_)))]),
        "{replied:?}"
    );
}

#[test]
fn ctrl_c_in_question_stops_the_asker() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.frame(from("A", Event::TaskStarted("a's task".into())), now);
    app.show("B");
    let a = approve(&mut app, "A", "rm -rf build", now);
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Stop("A".into())));
    assert_eq!(
        replies(&mut app),
        [(a, Reply::Approval(Approval::No(None)))]
    );
}

#[test]
fn ctrl_c_in_an_idle_lanes_choice_only_closes_it() {
    // Stopping a lane with no task quits llx, the other lanes' work with it.
    let now = Instant::now();
    let mut app = lanes(now);
    let picked = choose(&mut app, "A", now);
    assert_eq!(app.input(ctrl('c'), now), None);
    assert_eq!(replies(&mut app), [(picked, Reply::Choice(None))]);
}

#[test]
fn a_lanes_own_choice_after_the_person_left_gets_nothing() {
    // A hidden lane's held choice would never be shown, and the desk would wait for it.
    let now = Instant::now();
    let mut app = lanes(now);
    app.leave();
    let (a, b) = (choose(&mut app, "A", now), choose(&mut app, "B", now));
    let none = Reply::Choice(None);
    assert_eq!(replies(&mut app), [(a, none.clone()), (b, none)]);
    assert!(app.questions.is_empty());
}

#[test]
fn new_typed_while_a_task_works_opens_a_lane_at_once() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.frame(from("A", Event::TaskStarted("a's task".into())), now);
    typed(&mut app, "/new", now);
    assert_eq!(app.input(key(KeyCode::Enter), now), None);
    assert_eq!(
        commands(&mut app),
        vec![Command::Open {
            target: protocol::Target::New
        }]
    );
    assert!(sent(&mut app).is_empty(), "not kept for after the task");
}

#[test]
fn the_desk_shows_a_lane_it_opened() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.event(Event::Show("B".into()), now);
    assert_eq!(app.views.shown, "B");
}

#[test]
fn a_background_questions_label_goes_once_its_lane_is_shown() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.show("B");
    let _asked = approve(&mut app, "A", "cargo test -p jobs", now);
    app.show("A");
    let shown = live_area(&app, now);
    assert!(shown.contains("cargo test -p jobs"), "{shown}");
    assert!(!shown.contains("in the background"), "{shown}");
}

#[test]
fn at_the_most_lanes_an_idle_lane_closes_past_an_older_one_waiting_for_the_person() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.event(
        Event::LaneStarted {
            id: "C".into(),
            name: "write docs".into(),
        },
        now,
    );
    app.max_lanes = 3;
    for id in ["A", "B", "C"] {
        app.frame(from(id, queue(true, &[], &[], &[])), now);
    }
    app.show("C");
    let _held = choose(&mut app, "A", now);
    commands(&mut app);
    typed(&mut app, "/new", now);
    app.input(key(KeyCode::Enter), now);
    let target = protocol::Target::New;
    let session = "B".into();
    assert_eq!(
        commands(&mut app),
        [Command::Close { session }, Command::Open { target }]
    );
}

#[test]
fn a_background_question_with_tabs_keeps_its_text_once_its_lane_is_shown() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.show("B");
    let question = protocol::Question {
        text: "Which file?".into(),
        choices: vec![protocol::Choice {
            label: "a.rs".into(),
            ..protocol::Choice::default()
        }],
        ..protocol::Question::default()
    };
    let headers = vec!["File".into(), "Test".into()];
    let ask = protocol::Asked::Question {
        question,
        at: 0,
        of: 2,
        headers,
    };
    app.asked(next_id(), "A", ask, now);
    app.show("A");
    let shown = live_area(&app, now);
    assert!(shown.contains("Which file?"), "{shown}");
    assert!(!shown.contains("in the background"), "{shown}");
}

#[test]
fn a_long_history_shows_its_last_tasks_and_counts_the_rest() {
    let now = Instant::now();
    let mut app = app();
    let turns = (1..=25)
        .map(|n| protocol::Turn {
            goal: format!("task {n}"),
            answer: format!("answer {n}"),
            status: "done".into(),
        })
        .collect();
    app.event(Event::Earlier(turns), now);
    let shown = history(&mut app);
    assert!(
        shown.contains("25 earlier tasks · the last 20 shown"),
        "{shown}"
    );
    assert!(!shown.contains("❯ task 5\n"), "{shown}");
    assert!(
        shown.contains("❯ task 6") && shown.contains("• answer 25"),
        "{shown}"
    );
    assert_eq!(app.shown().tasks, 25);
}

#[test]
fn a_call_that_took_a_while_says_how_long_and_a_quick_one_says_nothing() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("go".into()), now);
    history(&mut app);
    let run = |app: &mut App, command: &str, took: u64| {
        let label = ran(command);
        app.event(call(command, "bash", &label), now);
        let ended = now + Duration::from_millis(took);
        app.event(
            result(
                command,
                "bash",
                &label,
                done("Exit code: 0\nWall time: 0.0 seconds\nOutput:"),
            ),
            ended,
        );
        history(app).lines().next().unwrap_or("").to_string()
    };
    assert_eq!(
        run(&mut app, "cargo test", 3_240),
        "• Ran cargo test · 3.2s"
    );
    assert_eq!(run(&mut app, "ls", 40), "• Ran ls");
}

#[test]
fn an_entry_goes_to_the_lane_shown_tab_keeps_it_for_after_and_the_screen_shows_what_waits() {
    let now = Instant::now();
    let mut app = app();
    app.event(queue(true, &[], &[], &[]), now);
    typed(&mut app, "first", now);
    app.input(key(KeyCode::Enter), now);
    app.event(Event::TaskStarted("first".into()), now);
    typed(&mut app, "now", now);
    app.input(key(KeyCode::Enter), now);
    typed(&mut app, "/status", now);
    app.input(key(KeyCode::Enter), now);
    typed(&mut app, "second", now);
    app.input(key(KeyCode::Tab), now);
    let said: Vec<(String, bool)> = (sent(&mut app).into_iter())
        .map(|input| (input.text, input.later))
        .collect();
    let said: Vec<(&str, bool)> = said.iter().map(|(t, l)| (t.as_str(), *l)).collect();
    assert_eq!(
        said,
        [
            ("first", false),
            ("now", false),
            ("/status", false),
            ("second", true)
        ]
    );
    // The server keeps them, and says what waits.
    app.event(queue(false, &["now"], &["/status", "second"], &[]), now);
    let shown = live_area(&app, now);
    assert!(shown.contains("↳ now"), "{shown}");
    assert!(
        shown.contains("⏎ /status") && shown.contains("⏎ second"),
        "{shown}"
    );
    // The task read it: it is the person's now, in the transcript.
    history(&mut app);
    app.event(Event::Steered(vec!["now".into()]), now);
    app.event(queue(false, &[], &["/status", "second"], &[]), now);
    assert!(history(&mut app).contains("❯ now"));
    assert!(!live_area(&app, now).contains("↳ now"));
}

#[test]
fn a_withdrawn_question_closes_only_itself_and_nothing_is_replied() {
    let now = Instant::now();
    let mut app = lanes(now);
    let a = approve(&mut app, "A", "cargo test", opened(now));
    let b = choose(&mut app, "B", now);
    app.frame(ToClient::Resolved { id: b }, now);
    assert_eq!(app.questions.len(), 1, "A's question is still open");
    app.frame(ToClient::Resolved { id: a }, now);
    assert!(app.questions.is_empty());
    app.show("B");
    assert!(app.questions.is_empty(), "B's held choice went too");
    assert_eq!(replies(&mut app), []);
}

/// The next question's id.
fn next_id() -> u64 {
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

#[test]
fn a_choice_after_a_withdrawn_approval_takes_keys_at_once() {
    let now = Instant::now();
    let mut app = lanes(now);
    let asked = approve(&mut app, "A", "cargo build", now);
    app.frame(ToClient::Resolved { id: asked }, now);
    let picked = choose(&mut app, "A", now);
    app.input(key(KeyCode::Enter), now + Duration::from_millis(10));
    assert_eq!(
        replies(&mut app),
        [(picked, Reply::Choice(Some(0)))],
        "not swallowed by the approval's settling"
    );
}

#[test]
fn a_sessions_copy_is_the_interfaces_to_do() {
    let (mut app, now) = (app(), Instant::now());
    let copied = app.frame(from("", Event::Copy("the answer".into())), now);
    assert_eq!(copied, Some(Action::Copy("the answer".into())));
}

#[test]
fn a_gap_clears_what_is_shown_and_draws_again_from_what_follows() {
    let now = Instant::now();
    let mut app = app();
    let mut term = crate::test_support::Term::new(&app);
    app.event(Event::TaskStarted("the old task".into()), now);
    let _asked = approve(&mut app, "", "rm -rf build", now);
    let before = term.draw(&mut app, now);
    assert!(
        before.contains("the old task") && before.contains("rm -rf build"),
        "{before}"
    );
    app.frame(ToClient::Gap, now);
    app.event(Event::Note("what came after".into()), now);
    let after = term.draw(&mut app, now);
    assert!(after.contains("what came after"), "{after}");
    assert!(!after.contains("the old task"), "{after}");
    assert!(!after.contains("rm -rf build"), "{after}");
}

#[test]
fn a_lane_that_ended_leaves_the_interface_with_its_agents_and_jobs() {
    let now = Instant::now();
    let mut app = lanes(now);
    let (id, name) = ("a1".to_string(), "scout".to_string());
    app.frame(from("B", Event::Started { id, name }), now);
    app.show("B");
    app.frame(from("B", Event::LaneEnded), now);
    let ids: Vec<&str> = app.views.sessions.iter().map(|v| v.id.as_str()).collect();
    assert_eq!(ids, ["A"]);
    assert_eq!(app.lane_id(), "A", "the lane left shows another");
    typed(&mut app, "for a", now);
    app.input(key(KeyCode::Enter), now);
    let sessions: Vec<String> = sent(&mut app).into_iter().map(|i| i.session).collect();
    assert_eq!(sessions, ["A"]);
    app.frame(from("A", Event::LaneEnded), now);
    assert_eq!(
        app.views.sessions.len(),
        1,
        "the last lane stays until the end"
    );
}

/// What the server replays to an interface back to a lane whose start the ring no longer holds
/// (`server::hub`'s `a_ring_that_wrapped_still_gives_each_lane_whole_then_what_it_kept`): a gap,
/// the lane's state, then the events it kept.
#[test]
fn a_ring_that_wrapped_rebuilds_the_lanes_and_the_person_can_type() {
    let now = Instant::now();
    let event = |session: Option<&str>, event| ToClient::Event {
        session: session.map(str::to_string),
        event,
        seq: None,
    };
    let (id, name) = ("bg".to_string(), "moved to the background".to_string());
    let mut replay = vec![
        ToClient::Gap,
        event(None, Event::LaneStarted { id, name }),
        event(Some("bg"), Event::Header(vec!["llx 0.1".into()])),
        event(Some("bg"), Event::Mode("plan".into())),
    ];
    replay.extend((16..20).map(|at| event(Some("bg"), Event::Note(format!("tick {at}")))));
    let mut app = app();
    for frame in replay {
        app.frame(frame, now);
    }
    assert_eq!(app.lane_id(), "bg");
    assert_eq!(app.lane().mode, "plan");
    let shown = history(&mut app);
    assert!(
        shown.contains("llx 0.1") && shown.contains("tick 19"),
        "{shown}"
    );
    typed(&mut app, "go on", now);
    app.input(key(KeyCode::Enter), now);
    let sent: Vec<(String, String)> = (sent(&mut app).into_iter())
        .map(|input| (input.session, input.text))
        .collect();
    assert_eq!(sent, [("bg".to_string(), "go on".to_string())]);
}

#[test]
fn lanes_ending_as_the_person_quits_leave_the_screen_as_it_is() {
    let now = Instant::now();
    let mut app = lanes(now);
    app.leave();
    app.frame(from("A", Event::LaneEnded), now);
    assert_eq!(app.lane_id(), "A", "no other lane drawn on the way out");
    assert!(!app.take_replay());
}

#[test]
fn ctrl_c_on_a_line_typed_in_a_question_goes_back_to_its_options_then_stops() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("a task".into()), now);
    app.asked(1, "", approve_asked("bash", "rm -rf build"), opened(now));
    app.input(key(KeyCode::Char('3')), now);
    typed(&mut app, "use make", now);
    assert_eq!(app.input(ctrl('c'), now), None, "one step back");
    assert_eq!(replies(&mut app), []);
    let shown = render(&mut app, now);
    assert!(!shown.contains("use make"), "{shown}");
    assert!(shown.contains("❯ 3. No"), "back on the options: {shown}");
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Stop(String::new())));
    assert_eq!(
        replies(&mut app),
        [(1, Reply::Approval(Approval::No(None)))]
    );
}

#[test]
fn an_approval_says_esc_answers_no_and_ctrl_c_stops() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("a task".into()), now);
    app.asked(1, "", approve_asked("bash", "rm -rf build"), opened(now));
    let shown = render(&mut app, now);
    assert!(
        shown.contains("enter select · esc no · ctrl-c stop"),
        "{shown}"
    );
}

#[test]
fn a_stopping_task_says_a_second_ctrl_c_quits_at_once() {
    let now = Instant::now();
    let mut app = app();
    app.event(Event::TaskStarted("x".into()), now);
    assert_eq!(app.input(ctrl('c'), now), Some(Action::Stop(String::new())));
    let shown = render(&mut app, now);
    assert!(shown.contains("ctrl-c again to quit now"), "{shown}");
}

#[test]
fn ctrl_z_suspends_and_keeps_the_draft() {
    let now = Instant::now();
    let mut app = app();
    typed(&mut app, "half a thought", now);
    assert_eq!(app.input(ctrl('z'), now), Some(Action::Suspend));
    assert_eq!(app.composer.text(), "half a thought");
    assert!(render(&mut app, now).contains("half a thought"));
}
