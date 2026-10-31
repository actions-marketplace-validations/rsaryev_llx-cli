use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use super::*;
use crate::picker::{Item, Lines, Picker};

/// A person who types lines from a script.
struct Typed(VecDeque<&'static str>);

impl Picker for Typed {
    fn pick(&mut self, _: &str, _: &[Item]) -> Option<usize> {
        None
    }
}

impl Lines for Typed {
    fn secret(&mut self, _: &str) -> Option<String> {
        self.0.pop_front().map(str::to_string)
    }
}

/// `come_back` with lines typed, the callback coming back on the `back`-th wait.
fn signed(lines: &[&'static str], back: usize) -> (Result<Option<String>, String>, String) {
    let mut terminal = Typed(lines.iter().copied().collect());
    let said = Mutex::new(String::new());
    let waits = AtomicUsize::new(0);
    let wait = |_| {
        let waited = waits.fetch_add(1, Ordering::SeqCst) + 1;
        (waited == back).then(|| Ok("from-browser".to_string()))
    };
    let pasted = |text: &str| Ok(format!("pasted:{text}"));
    let say = |text: &str| lock(&said).push_str(text);
    let outcome = come_back(&mut terminal, "Login › X", &say, &wait, &pasted);
    (outcome, said.into_inner().unwrap())
}

#[test]
fn enter_waits_for_the_browser_and_says_when_it_has_not_come_back() {
    let (outcome, said) = signed(&["", ""], 2);
    assert_eq!(outcome, Ok(Some("from-browser".into())));
    assert!(said.contains("has not come back yet"), "{said}");
}

#[test]
fn a_pasted_address_is_read_and_q_or_the_end_of_input_goes_back() {
    assert_eq!(
        signed(&["http://x/?code=c"], 0).0,
        Ok(Some("pasted:http://x/?code=c".into()))
    );
    assert_eq!(signed(&["q"], 0).0, Ok(None));
    assert_eq!(signed(&[], 0).0, Ok(None));
}

/// An interface whose line stays open, untouched, until `done` closes it.
struct Open;

impl Picker for Open {
    fn pick(&mut self, _: &str, _: &[Item]) -> Option<usize> {
        None
    }
}

impl Lines for Open {
    fn secret(&mut self, _: &str) -> Option<String> {
        panic!("the line for the address is one that closes")
    }

    fn line_until(&mut self, _: &str, done: &dyn Fn() -> bool) -> Option<String> {
        while !done() {}
        None
    }
}

#[test]
fn the_browser_coming_back_closes_the_line_with_nothing_pressed() {
    let waits = AtomicUsize::new(0);
    let wait =
        |_| (waits.fetch_add(1, Ordering::SeqCst) + 1 == 3).then(|| Ok("from-browser".to_string()));
    let outcome = come_back(&mut Open, "Login › X", &|_| {}, &wait, &|_| {
        Err("not pasted".into())
    });
    assert_eq!(outcome, Ok(Some("from-browser".into())));
    assert_eq!(
        waits.load(Ordering::SeqCst),
        3,
        "the callback is not asked again once it came back"
    );
}
