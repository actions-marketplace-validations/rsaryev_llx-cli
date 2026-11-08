use serde_json::json;

use super::*;

#[test]
fn each_event_is_a_line_of_the_sessions_file_named_by_its_agent() {
    let dir = std::env::temp_dir().join(format!("llx-record-{}", std::process::id()));
    write(&dir, "s1", "", json!({"kind": "task", "goal": "fix it"}));
    write(
        &dir,
        "s1",
        "policy",
        json!({"kind": "said", "text": "on it"}),
    );
    write(&dir, "", "", json!({"kind": "lost"}));
    let text = std::fs::read_to_string(dir.join("s1.jsonl")).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "{text}");
    assert_eq!(lines[0]["goal"], "fix it");
    assert!(lines[0].get("agent").is_none());
    assert_eq!(lines[1]["agent"], "policy");
    assert!(lines[1]["at_ms"].as_u64().unwrap() > 0);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join("s1.jsonl"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }
    let _ = std::fs::remove_dir_all(&dir);
}
