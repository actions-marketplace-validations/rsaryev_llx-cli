use super::*;

fn waited(ended: Ended, checks: usize, how: &str) -> Waited {
    Waited {
        ended,
        checks,
        last: Some(Check {
            how: how.into(),
            output: String::new(),
        }),
    }
}

#[test]
fn how_a_wait_ended_says_what_and_after_how_long_and_only_met_is_no_failure() {
    let end = |ended, checks, by, seconds| {
        let end = how(
            &waited(ended, checks, "Exit code: 127"),
            by,
            Duration::from_secs(seconds),
        );
        (end.how, end.failed)
    };
    assert_eq!(
        end(Ended::Met, 4, None, 32),
        ("met after 4 checks (32 s)".into(), false)
    );
    assert_eq!(
        end(Ended::TimedOut, 30, None, 300),
        ("timed out after 30 checks (5 min)".into(), true)
    );
    assert_eq!(
        end(Ended::Unrunnable, 1, None, 0),
        ("could not run: Exit code: 127".into(), true)
    );
    assert_eq!(
        end(Ended::Stopped, 1, Some(By::Person), 9),
        ("stopped by the person after 1 check".into(), true)
    );
    assert_eq!(
        end(Ended::Stopped, 2, Some(By::Run), 9),
        ("stopped by the agent after 2 checks".into(), true)
    );
}

#[test]
fn the_model_reads_how_the_last_check_ended_and_what_it_printed() {
    let check = |how: &str, output: &str| Check {
        how: how.into(),
        output: output.into(),
    };
    assert_eq!(said(&check("Exit code: 0", "up")), "Exit code: 0\nup");
    assert_eq!(
        said(&check("Exit code: 7", "")),
        "Exit code: 7\n(no output)"
    );
    assert_eq!(said(&Check::default()), "(no check ran)");
}
