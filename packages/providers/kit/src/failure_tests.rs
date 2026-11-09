use serde_json::json;

use super::*;

#[test]
fn a_null_error_is_no_error_and_a_failed_answer_says_its_message_else_its_body() {
    assert_eq!(reported(&json!({"error": null, "choices": []})), None);
    let failed = json!({"error": {"message": "slow down"}});
    assert_eq!(reported(&failed), Some(&json!({"message": "slow down"})));
    assert_eq!(said(Some(&failed), "ignored"), "slow down");
    assert_eq!(said(Some(&json!({"error": {}})), "ignored"), "no message");
    let page = "<html>".repeat(100);
    assert_eq!(said(None, &page).chars().count(), 300);
    assert_eq!(
        said(Some(&json!({"error": null})), "Bad Gateway"),
        "Bad Gateway"
    );
}

#[test]
fn a_rate_limit_and_the_statuses_every_host_means_so_may_be_mended_by_a_wait() {
    assert!(transient(429) && transient(503) && transient(529));
    assert!(!transient(400) && !transient(401) && !transient(402));
}
