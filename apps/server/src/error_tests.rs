use std::io;

use super::*;

#[test]
fn every_failure_has_its_exit_code() {
    assert_eq!(Failure::Usage("x".into()).code(), exit::ENVIRONMENT);
    assert_eq!(Failure::Environment("x".into()).code(), exit::ENVIRONMENT);
    assert_eq!(Failure::Closed.code(), exit::OK);
}

#[test]
fn a_closed_pipe_ends_quietly() {
    let closed = Failure::from_write(&io::Error::from(io::ErrorKind::BrokenPipe));
    assert_eq!(closed, Failure::Closed);
    assert_eq!(closed.message(), None);
    let full = Failure::from_write(&io::Error::from(io::ErrorKind::StorageFull));
    assert!(matches!(full, Failure::Environment(_)));
}

#[test]
fn a_panic_that_reaches_the_top_ends_with_the_environment_code() {
    let code = guarded(|| panic!("a bug in a task"));
    assert_eq!(code, exit::ENVIRONMENT);
    assert_eq!(guarded(|| exit::FAILED), exit::FAILED);
}

#[test]
fn a_panics_message_is_read_from_either_payload() {
    let text: Box<dyn std::any::Any + Send> = Box::new("plain");
    let owned: Box<dyn std::any::Any + Send> = Box::new(String::from("formatted"));
    let other: Box<dyn std::any::Any + Send> = Box::new(7_u8);
    assert_eq!(panic_message(text.as_ref()), "plain");
    assert_eq!(panic_message(owned.as_ref()), "formatted");
    assert_eq!(panic_message(other.as_ref()), "unknown cause");
}
