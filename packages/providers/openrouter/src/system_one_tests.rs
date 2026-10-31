use super::*;

#[test]
fn reads_the_systemone_answer_shapes() {
    let answer = json!({"type": "choice", "choice": "A", "probabilities": {"A": 1, "B": 0}, "confidence": 1});
    assert_eq!(
        read_answer(&answer),
        Some(Answer::Choice {
            choice: "A".into(),
            probabilities: [("A".to_string(), 1.0), ("B".to_string(), 0.0)].into(),
            confidence: 1.0,
        })
    );
    assert_eq!(
        read_answer(&json!({"type": "noul", "noul": 0.99})),
        Some(Answer::Noul(0.99))
    );
    assert_eq!(read_answer(&json!({"type": "score"})), None);
}
