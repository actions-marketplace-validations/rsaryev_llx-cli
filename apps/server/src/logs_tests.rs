use log::{Level, LevelFilter, Record};

use super::*;

#[test]
fn rust_log_is_read_as_env_logger_reads_it() {
    let filter = Filter::parse("warn, llx=debug ,llx::server=trace,nonsense=loud");
    assert_eq!(filter.level("ureq::pool"), LevelFilter::Warn);
    assert_eq!(filter.level("llx"), LevelFilter::Debug);
    assert_eq!(filter.level("llx::output"), LevelFilter::Debug);
    assert_eq!(filter.level("llx::server::hub"), LevelFilter::Trace);
    // A target is a path: `llxx` is not under `llx`.
    assert_eq!(filter.level("llxx"), LevelFilter::Warn);
    assert_eq!(filter.max(), LevelFilter::Trace);
    // A bare target is all of its levels; nothing else is kept.
    let filter = Filter::parse("llx");
    assert_eq!(filter.level("llx::session"), LevelFilter::Trace);
    assert_eq!(filter.level("ureq"), LevelFilter::Off);
    assert_eq!(Filter::parse("off").max(), LevelFilter::Off);
    assert_eq!(Filter::parse("").max(), LevelFilter::Off);
}

#[test]
fn a_line_is_one_event_with_its_time_level_target_and_process() {
    let at = UNIX_EPOCH + std::time::Duration::from_millis(1_793_112_659_042);
    let said = format_args!("two\nlines");
    let record = Record::builder()
        .level(Level::Warn)
        .target("llx::session")
        .args(said)
        .build();
    let line = line(at, 7, &record);
    assert!(
        line.starts_with("[2026-10-27T14:50:59.042Z WARN  llx::session] pid=7 "),
        "{line}"
    );
    assert!(line.ends_with(": two\\nlines\n"), "{line}");
    assert_eq!(line.matches('\n').count(), 1);
}

#[test]
fn a_long_message_is_cut_at_a_character() {
    let long = "é".repeat(LONGEST);
    let said = format_args!("{long}");
    let record = Record::builder().level(Level::Info).args(said).build();
    let line = line(UNIX_EPOCH, 1, &record);
    assert!(
        line.len() < LONGEST + 200 && line.ends_with(" …\n"),
        "{}",
        line.len()
    );
}

#[test]
fn past_its_limit_the_log_starts_again_and_keeps_the_one_before() {
    let dir = std::env::temp_dir().join(format!("llx-logs-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut kept = Kept::open(&dir).unwrap();
    kept.write("first\n");
    kept.size = LIMIT + 1;
    kept.write("second\n");
    assert_eq!(std::fs::read_to_string(dir.join(FILE)).unwrap(), "second\n");
    let before = std::fs::read_to_string(dir.join(format!("{FILE}.1"))).unwrap();
    assert_eq!(before, "first\n");
    let _ = std::fs::remove_dir_all(&dir);
}
