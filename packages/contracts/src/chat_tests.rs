use super::*;

#[test]
fn usages_add_up_and_a_nameless_answer_keeps_the_last_model() {
    let usage = |model: &str, input| Usage {
        model: model.into(),
        input_tokens: input,
        output_tokens: 10,
        cost: 0.001,
        millis: 5,
        ..Usage::default()
    };
    let mut spend = Spend::default();
    spend.add(&usage("m", 1000));
    spend.add(&usage("", 2000));
    assert_eq!(
        (
            spend.calls,
            spend.input_tokens,
            spend.output_tokens,
            spend.millis
        ),
        (2, 3000, 20, 10)
    );
    assert!((spend.cost - 0.002).abs() < 1e-12);
    assert_eq!(spend.model, "m");
}

#[test]
fn another_runs_spend_is_added_and_the_model_kept() {
    let mut mine = Spend {
        model: "m".into(),
        calls: 2,
        cost: 0.5,
        ..Spend::default()
    };
    let theirs = Spend {
        model: "other".into(),
        calls: 3,
        cost: 0.25,
        input_tokens: 10,
        cached_tokens: 4,
        output_tokens: 6,
        millis: 9,
    };
    mine.absorb(&theirs);
    assert_eq!((mine.model.as_str(), mine.calls, mine.cost), ("m", 5, 0.75));
    assert_eq!(
        (
            mine.input_tokens,
            mine.cached_tokens,
            mine.output_tokens,
            mine.millis
        ),
        (10, 4, 6, 9)
    );
    let mut empty = Spend::default();
    empty.absorb(&theirs);
    assert_eq!(empty.model, "other");
}
