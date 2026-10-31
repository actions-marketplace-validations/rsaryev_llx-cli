use std::time::{Duration, SystemTime};

use super::*;

/// An answer of `status`, with the ETag `tag` when there is one, and `body`.
fn answer(status: u16, tag: Option<&str>, body: &str) -> Reply {
    let headers = tag.map(|tag| ("etag".to_string(), tag.to_string()));
    Reply {
        status,
        headers: headers.into_iter().collect(),
        body: body.into(),
    }
}

/// A config directory with a cached catalog of `text` last fetched two days ago, and its ETag.
fn kept(name: &str, text: &str) -> (PathBuf, PathBuf) {
    let dir = std::env::temp_dir().join(format!("llx-source-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let (cache, etag) = (dir.join("models.json"), dir.join("models.etag"));
    store::save(&cache, text).unwrap();
    store::save(&etag, "\"v1\"").unwrap();
    age(&cache, Duration::from_secs(48 * 3600));
    (cache, etag)
}

fn age(path: &Path, by: Duration) {
    let file = std::fs::File::options().append(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

fn modified(path: &Path) -> SystemTime {
    std::fs::metadata(path).unwrap().modified().unwrap()
}

#[test]
fn a_catalog_fetched_within_the_hour_is_not_asked_for_again() {
    let (cache, etag) = kept("fresh", "{}");
    age(&cache, Duration::from_secs(60));
    let asked = settle(&cache, &etag, |_| panic!("asked")).unwrap();
    assert!(!asked);
}

#[test]
fn a_catalog_fetched_hours_ago_is_asked_for_again() {
    // A model released this morning must not wait for tomorrow: an unchanged catalog costs a 304.
    let (cache, etag) = kept("hours", "{}");
    age(&cache, Duration::from_secs(2 * 3600));
    let mut asked = false;
    let settled = settle(&cache, &etag, |_| {
        asked = true;
        Ok(answer(304, None, ""))
    });
    assert_eq!(settled, Ok(false));
    assert!(asked, "a two-hour-old catalog is asked for");
}

#[test]
fn an_unchanged_catalog_is_kept_another_hour() {
    let (cache, etag) = kept("unchanged", "{}");
    let mut sent = None;
    let settled = settle(&cache, &etag, |tag| {
        sent = tag.map(str::to_string);
        Ok(answer(304, None, ""))
    });
    assert_eq!(settled, Ok(false));
    assert_eq!(sent.as_deref(), Some("\"v1\""));
    assert_eq!(store::read(&cache).as_deref(), Some("{}"));
    let age = SystemTime::now().duration_since(modified(&cache)).unwrap();
    assert!(age < Duration::from_secs(60), "fresh for another hour");
}

#[test]
fn a_cache_cut_short_is_asked_for_whole() {
    // Its ETag stands for a whole catalog: sent, it would be answered 304 and the cut copy kept.
    let (cache, etag) = kept("cut", "{\"openai\": {\"name\": ");
    let mut sent = Some("unset".to_string());
    let settled = settle(&cache, &etag, |tag| {
        sent = tag.map(str::to_string);
        Ok(answer(200, Some("\"v2\""), "{}"))
    });
    assert_eq!(settled, Ok(true));
    assert_eq!(sent, None, "no If-None-Match");
    assert_eq!(store::read(&cache).as_deref(), Some("{}"));
    assert_eq!(store::read(&etag).as_deref(), Some("\"v2\""));
}

#[test]
fn a_failed_refresh_changes_nothing() {
    let (cache, etag) = kept("failed", "{}");
    let before = modified(&cache);
    let settled = settle(&cache, &etag, |_| Ok(answer(500, None, "")));
    assert_eq!(settled, Err("models.dev answered 500".into()));
    assert_eq!(modified(&cache), before);
}
