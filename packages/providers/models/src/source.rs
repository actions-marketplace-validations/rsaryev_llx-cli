//! Where the catalog comes from: the files built in, and over them models.dev's published v2 API
//! as last fetched (`<config>/models.json`, cut down to what llx reads). A refresh asks
//! models.dev at most once an hour, with the ETag it last had, so an unchanged catalog costs no
//! body (a changed one is about 800 KB gzipped); a failed one changes nothing. A day was too
//! long: a model released in the morning was missing until the next. Nothing waits on the
//! network: the app refreshes in the background, and what it fetches is read at the next [`load`].

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use provider_kit::http::{Reply, Web};
use provider_kit::{App, store};

use crate::Catalog;

/// The v2 API: resolved, with every model's APIs and reasoning defaults (MODELS.md).
const URL: &str = "https://models.dev/experimental/v2.0/api.json";
/// How old the fetched catalog may be before it is asked for again.
const STALE: Duration = Duration::from_secs(3600);
const TIMEOUT: Duration = Duration::from_secs(30);

fn paths(app: App) -> Option<(PathBuf, PathBuf)> {
    Some((
        store::path(app, "models.json")?,
        store::path(app, "models.etag")?,
    ))
}

/// The catalog: the built-in one, under the last fetched one when there is one (and read only
/// for a provider that one lacks).
pub fn load(app: App) -> Catalog {
    let fetched = paths(app)
        .and_then(|(cache, _)| store::read(&cache))
        .and_then(|text| Catalog::parse(&text).ok());
    match fetched {
        Some(fetched) => fetched.over_built_in(),
        None => Catalog::built_in(),
    }
}

/// When the fetched catalog was last written (or found unchanged); `None` when there is none.
pub fn written(app: App) -> Option<SystemTime> {
    modified(&paths(app)?.0)
}

/// When `path` was last written; `None` when it cannot be told.
fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
}

/// Ask models.dev for its catalog when the one fetched last is an hour old; whether a new one
/// was kept.
pub fn refresh(app: App) -> Result<bool, String> {
    let (cache, etag_path) = paths(app).ok_or("no config directory")?;
    settle(&cache, &etag_path, |etag| {
        let headers: Vec<_> = etag
            .map(|etag| ("If-None-Match", etag))
            .into_iter()
            .collect();
        let web = Web::new(app, TIMEOUT);
        web.get(URL, &headers).map_err(|error| error.to_string())
    })
}

/// A refresh of `cache`, with what `fetch` answers when asked (with the ETag to send, if any).
fn settle(
    cache: &Path,
    etag_path: &Path,
    fetch: impl FnOnce(Option<&str>) -> Result<Reply, String>,
) -> Result<bool, String> {
    let fresh = modified(cache)
        .and_then(|at| SystemTime::now().duration_since(at).ok())
        .is_some_and(|age| age < STALE);
    if fresh {
        return Ok(false);
    }
    // The ETag stands for the copy kept: one that does not read (cut short) is asked for whole.
    let whole = store::read(cache).is_some_and(|text| Catalog::parse(&text).is_ok());
    let etag = store::read(etag_path).filter(|_| whole);
    let reply = fetch(etag.as_deref())?;
    match reply.status {
        // Unchanged: the copy is as new as models.dev's, for another hour.
        304 => {
            let file = std::fs::File::options().append(true).open(cache);
            let renewed = file.and_then(|file| file.set_modified(SystemTime::now()));
            renewed.map_err(|error| error.to_string())?;
            Ok(false)
        }
        200 => {
            let catalog = Catalog::parse(&reply.body)?;
            store::save(cache, &catalog.kept()).map_err(|error| error.to_string())?;
            let tag = reply.header("etag").unwrap_or("");
            store::save(etag_path, tag).map_err(|error| error.to_string())?;
            Ok(true)
        }
        status => Err(format!("models.dev answered {status}")),
    }
}

#[cfg(test)]
#[path = "source_tests.rs"]
mod tests;
