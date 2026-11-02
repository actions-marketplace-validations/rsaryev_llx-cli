use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use contracts::chat::{Chat, ChatError, Completion, Message, Request};

use super::*;

/// A chat model that answers `text` and keeps what it was asked, and by when.
struct TitleChat {
    text: Result<String, ()>,
    asked: Mutex<Vec<(Vec<Message>, usize, bool)>>,
    deadlines: Mutex<Vec<Option<Instant>>>,
}

impl TitleChat {
    fn new(text: Result<&str, ()>) -> Self {
        Self {
            text: text.map(str::to_string),
            asked: Mutex::new(vec![]),
            deadlines: Mutex::new(vec![]),
        }
    }
}

impl Chat for TitleChat {
    #[expect(
        clippy::unwrap_in_result,
        reason = "a test's fake: a poisoned lock fails the test"
    )]
    fn complete(&self, request: Request) -> Result<Completion, ChatError> {
        let asked = (
            request.conversation.to_vec(),
            request.tools.len(),
            request.calling == Calling::Nothing,
        );
        self.asked.lock().unwrap().push(asked);
        self.deadlines.lock().unwrap().push(request.deadline);
        match &self.text {
            Ok(text) => Ok(Completion {
                text: Some(text.clone()),
                ..Completion::default()
            }),
            Err(()) => Err(ChatError::Unavailable("down".into())),
        }
    }
}

#[test]
fn the_model_is_asked_once_with_no_tools_for_the_task_it_names() {
    let namer = TitleChat::new(Ok("Fix the flaky jobs test"));
    let title = ask(&namer, "the jobs test fails one run in ten, find out why");
    assert_eq!(title.as_deref(), Some("Fix the flaky jobs test"));
    let asked = namer.asked.lock().unwrap();
    let (conversation, tools, answer_only) = &asked[0];
    assert_eq!((asked.len(), *tools, *answer_only), (1, 0, true));
    let Message::User(said) = &conversation[1] else {
        panic!("{conversation:?}")
    };
    assert!(
        said.ends_with("User prompt:\nthe jobs test fails one run in ten, find out why"),
        "{said}"
    );
}

#[test]
fn a_failed_or_empty_answer_names_nothing() {
    assert_eq!(ask(&TitleChat::new(Err(())), "go"), None);
    assert_eq!(ask(&TitleChat::new(Ok("  \n")), "go"), None);
    assert_eq!(
        ask(&TitleChat::new(Ok("x")), "   "),
        None,
        "no task, no request"
    );
}

#[test]
fn an_answer_is_cleaned_to_one_short_line() {
    for (said, title) in [
        ("\"Fix the parser.\"", "Fix the parser"),
        ("  Add   tabs\tto the printer!  ", "Add tabs to the printer"),
        ("«Διόρθωση δοκιμών»", "Διόρθωση δοκιμών"),
        (
            "Title: Speed up the build\nbecause it is slow",
            "Speed up the build",
        ),
        ("`Rename the picker?`", "Rename the picker"),
        // What could redraw the screen is shown, not obeyed.
        ("Fix \u{1b}[2K \u{202e}txt", "Fix \\u{1b}[2K \\u{202e}txt"),
    ] {
        assert_eq!(clean(said).as_deref(), Some(title), "{said:?}");
    }
    let long = clean("Make every single one of the release binaries much smaller").unwrap();
    assert!(long.chars().count() <= MAX, "{long}");
    assert!(long.ends_with('…'), "{long}");
}

#[test]
fn a_long_task_is_cut_to_the_prompt_budget_on_a_character() {
    let task = "λ".repeat(2000);
    let prompt = prompt(&task);
    assert!(prompt.len() <= PROMPT_BYTES, "{}", prompt.len());
    assert!(prompt.ends_with('λ'));
}

/// The lane's end waits for a title still asked for: its request is held to a deadline.
#[test]
fn the_titles_request_is_given_up_within_a_minute() {
    let namer = TitleChat::new(Ok("Fix the parser"));
    assert!(ask(&namer, "fix the parser").is_some());
    let deadline = namer.deadlines.lock().unwrap()[0];
    let minute = Instant::now() + Duration::from_secs(60);
    assert!(deadline.is_some_and(|by| by <= minute), "{deadline:?}");
}

/// The title's thread is the lane's: let go at the lane's end, the request still out is waited
/// for there, so nothing of it outlives the lane (R2).
#[test]
#[expect(
    clippy::disallowed_methods,
    reason = "a request that takes a moment, as a model's does"
)]
fn the_lanes_end_waits_for_the_title_still_asked_for() {
    let answered = Arc::new(AtomicBool::new(false));
    let theirs = Arc::clone(&answered);
    let mut titling = Titling::default();
    titling
        .start(move || {
            std::thread::sleep(Duration::from_millis(200));
            theirs.store(true, Ordering::SeqCst);
        })
        .unwrap();
    drop(titling);
    assert!(
        answered.load(Ordering::SeqCst),
        "the lane ended before its title"
    );
}

/// A task never waits for a title: the next one starts while one is still out, kept with it for
/// the lane's end.
#[test]
fn a_task_never_waits_for_the_title_still_asked_for() {
    let (release, held) = std::sync::mpsc::sync_channel::<()>(0);
    let mut titling = Titling::default();
    let slow = move || {
        let _ = held.recv_timeout(Duration::from_secs(5));
    };
    titling.start(slow).unwrap();
    let began = Instant::now();
    titling.start(|| {}).unwrap();
    assert!(
        began.elapsed() < Duration::from_secs(2),
        "the next task waited"
    );
    drop(release);
}

/// A test lane at an interface, given no title model, asks none: not of the provider its choice
/// names (OpenRouter, here a stand-in named in the session's environment with a key), as no test
/// may reach a network for a title (S3).
#[test]
fn a_lane_with_no_title_model_asks_nothing() {
    use crate::compose::{Models, Seat, lane_with};
    use crate::server::{Peer, Ui};
    use crate::testing::{Scratch, ScriptedChat, says, setup};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let stand_in = std::thread::Builder::new()
        .name("stand-in".into())
        .spawn(move || {
            use std::io::Read;
            let mut peer = listener.accept().unwrap().0;
            // What the first peer sent: a request; the wake-up below sends nothing.
            peer.read(&mut [0; 16]).unwrap_or(0) > 0
        })
        .unwrap();
    let scratch = Scratch::new();
    let mut setup = setup(&scratch, crate::modes::built("bypass"), true);
    let env = vec![
        ("OPENROUTER_API_KEY".to_string(), "sk-or-test".to_string()),
        ("OPENROUTER_BASE_URL".into(), format!("http://{address}")),
    ];
    setup.workspace = setup.workspace.with_env(env);
    let (shared, seat) = setup.split();
    #[expect(
        clippy::disallowed_methods,
        reason = "the in-process transport's channel, as the binary makes it"
    )]
    let (out, _frames) = std::sync::mpsc::channel();
    let seat = Seat {
        ui: Some(Ui::new(Peer::new(out))),
        ..seat
    };
    let chat = ScriptedChat::new(vec![says("Done.")]);
    let models = Models {
        chat: &chat,
        live: None,
        adviser: None,
        searcher: None,
        namer: None,
    };
    // The lane's end waits for a title still asked for: whatever was asked, was asked by then.
    lane_with(&shared, seat, models, |rig| rig.task("fix the parser"));
    // Wakes the stand-in when nothing reached it.
    drop(std::net::TcpStream::connect(address));
    assert!(
        !stand_in.join().unwrap(),
        "a title was asked of the provider"
    );
}

/// What a served lane asks its titles of: `chat`, whatever the lane thinks with.
fn asked_of(chat: &Arc<crate::test_support::Gated>) -> Namer {
    let chat = Arc::clone(chat);
    Arc::new(move |_: &Choice| Some(Arc::clone(&chat) as Arc<dyn Chat>))
}

/// What the interface was told: each event with its session.
type Told = Vec<(Option<String>, Event)>;

/// What `frames` brings into `told`, until `done` holds of all of it or `within` passes.
fn listen(
    frames: &std::sync::mpsc::Receiver<protocol::ToClient>,
    told: &mut Told,
    within: Duration,
    done: impl Fn(&Told) -> bool,
) {
    let deadline = Instant::now() + within;
    while !done(told) && Instant::now() < deadline {
        let frame = frames.recv_timeout(Duration::from_millis(20));
        if let Ok(protocol::ToClient::Event { session, event, .. }) = frame {
            told.push((session, event));
        }
    }
}

/// Whether `told` holds an event `is` names.
fn has(told: &Told, is: impl Fn(&Event) -> bool) -> bool {
    told.iter().any(|(_, event)| is(event))
}

fn named(event: &Event) -> bool {
    matches!(event, Event::Updated(_))
}

fn task_ended(event: &Event) -> bool {
    matches!(event, Event::TaskEnded { .. })
}

fn lane_ended(event: &Event) -> bool {
    matches!(event, Event::LaneEnded)
}

/// A served session's first task names it, as its title model answers: the interface is told
/// the title, in the lane's name, and the saved session keeps it.
#[test]
fn a_served_sessions_first_task_is_named_by_its_title_model() {
    use crate::test_support::{gated, says, served_titled};
    use contracts::history::History;
    let ((to_chat, chat), (to_namer, namer)) = (gated(), gated());
    let namer = Arc::new(namer);
    to_namer.send(says("Fix the flaky jobs test")).unwrap();
    to_chat.send(says("Done.")).unwrap();
    let goal = "the jobs test fails one run in ten, find out why";
    let lane = Mutex::new(None);
    let titles = asked_of(&namer);
    let scratch = served_titled(&[&chat], Some(&titles), Some(goal), &|| {}, |link, _| {
        let mut told = vec![];
        listen(&link.frames, &mut told, Duration::from_secs(10), |told| {
            has(told, named) && has(told, task_ended)
        });
        let updated = told.iter().find(|(_, event)| named(event));
        let (session, update) = updated.unwrap_or_else(|| panic!("never named: {told:#?}"));
        let title = Some("Fix the flaky jobs test".to_string());
        assert_eq!(*update, Event::Updated(protocol::Update { title }));
        *lane.lock().unwrap() = session.clone();
    });
    let lane = lane
        .into_inner()
        .unwrap()
        .expect("the update names its lane");
    let history = session::Sessions {
        dir: scratch.path().join("sessions"),
    };
    assert_eq!(
        history.load(&lane).unwrap().title,
        "Fix the flaky jobs test"
    );
    let asked = namer.asked();
    assert_eq!(asked.len(), 1, "one title asked");
    assert!(format!("{:?}", asked[0]).contains(goal), "{asked:?}");
}

/// A title still asked for as its lane ends is told while the lane is open, never after its end:
/// a client attaching later is told nothing of the lane (its state went with it), and the saved
/// session keeps the title.
#[test]
fn a_title_answered_as_its_lane_ends_is_told_before_the_end() {
    use crate::test_support::{gated, says, served_titled};
    use contracts::history::History;
    use protocol::{Command, ToServer};
    let ((to_chat, chat), (to_namer, namer)) = (gated(), gated());
    let namer = Arc::new(namer);
    to_chat.send(says("Done.")).unwrap();
    let lane = Mutex::new(String::new());
    let titles = asked_of(&namer);
    let scratch = served_titled(
        &[&chat],
        Some(&titles),
        Some("fix"),
        &|| {},
        |link, peer| {
            let mut told = vec![];
            listen(&link.frames, &mut told, Duration::from_secs(10), |told| {
                has(told, task_ended) && namer.turns() == 1
            });
            let ended = told.iter().find(|(_, event)| task_ended(event));
            let id = ended.and_then(|(session, _)| session.clone()).unwrap();
            // The lane closed while its title is still asked for; the title comes a moment later.
            let session = id.clone();
            let command = Command::Close { session };
            link.to_server
                .send(ToServer::Call { id: 1, command })
                .unwrap();
            let moment = Duration::from_millis(500);
            listen(&link.frames, &mut told, moment, |told| {
                has(told, lane_ended)
            });
            to_namer.send(says("Fix the parser")).unwrap();
            listen(&link.frames, &mut told, Duration::from_secs(10), |told| {
                has(told, lane_ended) && has(told, named)
            });
            let at = |is: fn(&Event) -> bool| {
                (told.iter())
                    .position(|(session, event)| session.as_ref() == Some(&id) && is(event))
            };
            let (named_at, ended_at) = (at(named), at(lane_ended));
            assert!(
                named_at.is_some() && named_at < ended_at,
                "the title was told after its lane's end: {told:#?}"
            );
            // What the ring replays, read for a moment: it keeps the lane's end, so an empty
            // replay never passes for one without the title.
            let mut later = vec![];
            listen(&peer.attach(), &mut later, moment, |_| false);
            assert!(has(&later, lane_ended), "nothing replayed: {later:#?}");
            let told_title = (later.iter())
                .any(|(session, event)| session.as_ref() == Some(&id) && named(event));
            assert!(
                !told_title,
                "a later client is told the ended lane's title: {later:#?}"
            );
            *lane.lock().unwrap() = id;
        },
    );
    let history = session::Sessions {
        dir: scratch.path().join("sessions"),
    };
    let saved = history.load(&lane.into_inner().unwrap()).unwrap();
    assert_eq!(saved.title, "Fix the parser");
}
