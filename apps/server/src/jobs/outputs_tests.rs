use super::*;

#[test]
fn an_output_is_known_by_its_key_until_its_job_leaves_the_list() {
    let key = crate::interrupt::job_key("outputs-lane", "job-1");
    let shown = register(&key, || "tick 1\ntick 2".into());
    assert_eq!(of(&key).as_deref(), Some("tick 1\ntick 2"));
    assert_eq!(
        of(&crate::interrupt::job_key("outputs-lane", "job-2")),
        None
    );
    drop(shown);
    assert_eq!(of(&key), None);
}
