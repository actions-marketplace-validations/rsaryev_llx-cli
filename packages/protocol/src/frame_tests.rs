use serde_json::json;

use super::*;
use crate::{
    Adjustment, Answer, Approval, Group, Input, JobRow, Label, Outcome, Place, Question, Running,
    Saved, Target, Turn,
};

fn through_json<T: Serialize + for<'de> Deserialize<'de>>(frame: &T) -> T {
    let line = serde_json::to_string(frame).unwrap();
    assert!(!line.contains('\n'), "a frame is one line: {line}");
    serde_json::from_str(&line).unwrap()
}

fn event(event: Event) -> ToClient {
    ToClient::Event {
        session: Some("s1".into()),
        event,
        seq: Some(3),
    }
}

#[test]
fn every_frame_to_the_interface_comes_back_the_same_through_json() {
    let frames = [
        event(Event::Header(vec!["llx 0.1".into(), "gpt".into()])),
        event(Event::TaskStarted("hi".into())),
        event(Event::LaneEnded),
        event(Event::Copy("the answer".into())),
        event(Event::Call {
            call: "c1".into(),
            name: "bash".into(),
            label: Label {
                doing: "Running".into(),
                done: "Ran".into(),
                subject: "ls".into(),
            },
        }),
        event(Event::Result {
            call: "c1".into(),
            name: "apply_patch".into(),
            label: Label::default(),
            outcome: Outcome::Failed {
                why: "no".into(),
                declined: true,
            },
        }),
        event(Event::TaskEnded {
            status: "done".into(),
            millis: 1200,
            turns: 2,
            cost: 0.01,
        }),
        event(Event::Total(crate::Total {
            input_tokens: 24_100,
            cached_tokens: 21_000,
            output_tokens: 1_200,
            cost: 0.042,
        })),
        event(Event::Earlier(vec![Turn {
            goal: "a".into(),
            answer: "b".into(),
            status: "done".into(),
        }])),
        event(Event::Updated(crate::Update {
            title: Some("Fix".into()),
        })),
        event(Event::Saved(vec![Saved {
            id: "x".into(),
            title: "t".into(),
            meta: "2 h ago".into(),
        }])),
        event(Event::Jobs(vec![JobRow {
            id: "j1".into(),
            name: "ticks".into(),
            detail: "date".into(),
            until_at: Some(1_700_000_000_000),
            next: Some("14:05".into()),
            events: 3,
        }])),
        event(Event::Queue {
            ready: false,
            steered: vec!["faster".into()],
            queued: vec![],
            background: vec!["≋ ticks · tick 1".into()],
        }),
        ToClient::Event {
            session: None,
            event: Event::Quit,
            seq: None,
        },
        ToClient::Gap,
    ];
    for frame in &frames {
        assert_eq!(&through_json(frame), frame);
    }
}

#[test]
fn every_question_to_the_person_comes_back_the_same_through_json() {
    let frames = [
        ToClient::Ask {
            id: 7,
            session: "s1".into(),
            asked: Asked::Question {
                question: Question {
                    id: "colour".into(),
                    header: "Colour".into(),
                    text: "Which?".into(),
                    choices: vec![crate::Choice {
                        label: "Red".into(),
                        description: String::new(),
                        preview: "#f00\n  red".into(),
                    }],
                    multiple: false,
                },
                at: 0,
                of: 1,
                headers: vec!["Colour".into()],
            },
        },
        ToClient::Ask {
            id: 8,
            session: "s1".into(),
            asked: Asked::ChooseAdjustable {
                title: "Model".into(),
                options: vec![("a".into(), "b".into())],
                marked: Some(0),
                adjustments: vec![Adjustment {
                    values: vec!["low".into()],
                    at: 0,
                }],
                groups: vec![Group {
                    name: "OpenAI".into(),
                    rows: 1,
                }],
            },
        },
        ToClient::Resolved { id: 7 },
        ToClient::Done {
            id: 3,
            done: Done::Status(crate::Status {
                build: "0.4.0+1".into(),
                pid: 42,
                attached: 2,
                lanes: 3,
                working: 1,
                kept: 1,
                jobs: 2,
            }),
        },
        ToClient::Done {
            id: 1,
            done: Done::Initialized {
                version: crate::VERSION,
                max_lanes: 8,
                commands: vec![("help".into(), "the commands".into())],
                modes: vec![("plan".into(), "plan first".into())],
                background: true,
            },
        },
        ToClient::Done {
            id: 2,
            done: Done::Work {
                running: vec![Running {
                    kind: "monitor".into(),
                    name: "cargo build".into(),
                }],
            },
        },
    ];
    for frame in &frames {
        assert_eq!(&through_json(frame), frame);
    }
}

#[test]
fn every_frame_to_the_server_comes_back_the_same_through_json() {
    let frames = [
        ToServer::Call {
            id: 1,
            command: Command::Input(Input {
                session: "s1".into(),
                text: "go".into(),
                images: vec!["/tmp/a.png".into()],
                later: true,
            }),
        },
        ToServer::Call {
            id: 2,
            command: Command::Open {
                target: Target::Saved("x".into()),
            },
        },
        ToServer::Call {
            id: 3,
            command: Command::Background,
        },
        ToServer::Reply {
            id: 7,
            reply: Reply::Approval(Approval::No(Some("not there".into()))),
        },
        ToServer::Reply {
            id: 8,
            reply: Reply::Question(Some(Answer {
                choices: vec![1],
                note: None,
            })),
        },
        ToServer::Reply {
            id: 9,
            reply: Reply::Adjusted(Some((0, 2))),
        },
        ToServer::Call {
            id: 10,
            command: Command::Initialize {
                version: crate::VERSION,
                place: Some(Place {
                    cwd: "/work/a".into(),
                    env: vec![("PATH".into(), "/venv/bin:/usr/bin".into())],
                    args: vec!["-y".into(), "fix it".into()],
                    dir: "/work".into(),
                }),
                build: "0.4.0+1".into(),
            },
        },
        ToServer::Call {
            id: 11,
            command: Command::Shutdown,
        },
    ];
    for frame in &frames {
        assert_eq!(&through_json(frame), frame);
    }
}

#[test]
fn a_frame_names_its_method_and_carries_its_params() {
    let frame = ToServer::Call {
        id: 3,
        command: Command::Halt { id: "s2".into() },
    };
    assert_eq!(
        serde_json::to_value(&frame).unwrap(),
        json!({"method": "command/call", "params": {"id": 3, "command": {"type": "halt", "id": "s2"}}})
    );
}

#[test]
fn what_a_reader_does_not_know_it_ignores() {
    let line = r#"{"method":"person/resolved","params":{"id":4,"by":"another terminal"}}"#;
    assert_eq!(
        serde_json::from_str::<ToClient>(line).unwrap(),
        ToClient::Resolved { id: 4 }
    );
}

#[test]
fn a_client_that_gives_no_place_still_initializes() {
    let line = r#"{"method":"command/call","params":{"id":1,"command":{"type":"initialize","version":1}}}"#;
    assert_eq!(
        serde_json::from_str::<ToServer>(line).unwrap(),
        ToServer::Call {
            id: 1,
            command: Command::Initialize {
                version: 1,
                place: None,
                build: String::new(),
            }
        }
    );
}

#[test]
fn an_event_from_a_server_that_numbers_nothing_still_reads() {
    let line = r#"{"method":"session/event","params":{"session":null,"event":{"type":"quit"}}}"#;
    let quit = ToClient::Event {
        session: None,
        event: Event::Quit,
        seq: None,
    };
    assert_eq!(serde_json::from_str::<ToClient>(line).unwrap(), quit);
}

#[test]
fn a_server_that_says_less_of_its_work_still_reads() {
    let said = r#"{"type":"work"}"#;
    let work = Done::Work { running: vec![] };
    assert_eq!(serde_json::from_str::<Done>(said).unwrap(), work);
    let said = r#"{"type":"initialized","version":1,"max_lanes":8,"commands":[],"modes":[]}"#;
    let Done::Initialized { background, .. } = serde_json::from_str(said).unwrap() else {
        panic!("{said}");
    };
    assert!(!background, "no background unless the server says so");
}

#[test]
fn a_choice_in_no_groups_still_reads() {
    let said = r#"{"type":"choose_adjustable","title":"Model","options":[],"marked":null,"adjustments":[]}"#;
    let Asked::ChooseAdjustable { groups, .. } = serde_json::from_str(said).unwrap() else {
        panic!("{said}");
    };
    assert!(groups.is_empty());
}
