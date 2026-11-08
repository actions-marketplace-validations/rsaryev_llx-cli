use std::time::Duration;

use super::*;

#[test]
fn a_failure_a_wait_may_mend_is_tried_three_times_then_said() {
    let mut tries = 0;
    let result: Result<(), u32> = retried(Duration::ZERO, None, Resend::Never, || {
        tries += 1;
        Attempt::Again(tries)
    });
    assert_eq!((result, tries), (Err(3), 3));
}

#[test]
fn an_answer_or_a_lasting_failure_ends_the_attempts() {
    let mut tries = 0;
    let result: Result<u32, ()> = retried(Duration::ZERO, None, Resend::Never, || {
        tries += 1;
        if tries == 2 {
            Attempt::Done(Ok(tries))
        } else {
            Attempt::Again(())
        }
    });
    assert_eq!(result, Ok(2));
    let mut tries = 0;
    let result: Result<(), &str> = retried(Duration::ZERO, None, Resend::Never, || {
        tries += 1;
        Attempt::Done(Err("refused"))
    });
    assert_eq!((result, tries), (Err("refused"), 1));
}

#[test]
fn no_pause_is_slept_past_the_deadline() {
    let mut tries = 0;
    let started = Instant::now();
    let until = started + Duration::from_millis(100);
    let result: Result<(), u32> =
        retried(Duration::from_secs(30), Some(until), Resend::Never, || {
            tries += 1;
            Attempt::Again(tries)
        });
    assert_eq!((result, tries), (Err(1), 1));
    assert!(started.elapsed() < Duration::from_secs(1));
}

#[test]
fn a_request_left_unanswered_is_not_sent_again_unless_the_caller_says_once() {
    // It may have been acted on and billed: sent again only where the caller takes that risk.
    let unanswered = |resend| {
        let mut tries = 0;
        let result: Result<(), u32> = retried(Duration::ZERO, None, resend, || {
            tries += 1;
            Attempt::Unanswered(tries)
        });
        (result, tries)
    };
    assert_eq!(unanswered(Resend::Never), (Err(1), 1));
    assert_eq!(unanswered(Resend::Once), (Err(2), 2));
}

#[test]
fn a_resend_counts_among_the_attempts() {
    let mut tries = 0;
    let result: Result<(), u32> = retried(Duration::ZERO, None, Resend::Once, || {
        tries += 1;
        match tries {
            1 => Attempt::Unanswered(tries),
            _ => Attempt::Again(tries),
        }
    });
    assert_eq!((result, tries), (Err(3), 3));
}

#[test]
fn the_wait_a_host_asks_for_is_kept_and_one_past_the_deadline_is_not_waited() {
    let started = Instant::now();
    let mut tries = 0;
    let result: Result<u32, u32> = retried(Duration::ZERO, None, Resend::Never, || {
        tries += 1;
        match tries {
            1 => Attempt::After(tries, Duration::from_millis(40)),
            _ => Attempt::Done(Ok(tries)),
        }
    });
    assert_eq!(result, Ok(2));
    assert!(
        started.elapsed() >= Duration::from_millis(40),
        "the host's wait was kept"
    );
    // A wait that would end past the deadline: the failure at once.
    let until = Instant::now() + Duration::from_millis(50);
    let started = Instant::now();
    let result: Result<(), u32> = retried(Duration::ZERO, Some(until), Resend::Never, || {
        Attempt::After(7, Duration::from_secs(30))
    });
    assert_eq!(result, Err(7));
    assert!(started.elapsed() < Duration::from_millis(50));
}

#[test]
fn a_retry_after_header_is_read_as_seconds() {
    assert_eq!(after(Some("2")), Some(Duration::from_secs(2)));
    assert_eq!(after(Some("1.5")), Some(Duration::from_millis(1500)));
    assert_eq!(after(Some("soon")), None);
    assert_eq!(after(None), None);
}

#[test]
fn a_wait_longer_than_a_minute_is_said_at_once_and_no_pause_grows_past_it() {
    let started = Instant::now();
    let result: Result<(), u32> = retried(Duration::ZERO, None, Resend::Never, || {
        Attempt::After(9, Duration::from_secs(3600))
    });
    assert_eq!(result, Err(9));
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "an hour's limit is not slept"
    );
    assert_eq!(grown(Duration::from_secs(50)), Duration::from_secs(60));
}
