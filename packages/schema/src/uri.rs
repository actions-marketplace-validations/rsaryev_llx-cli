//! A reference resolved against a base URI as RFC 3986 section 5.2 says, for `$id` and `$ref`:
//! no network, only the text. A document with no `$id` of its own has `BASE`.

/// The base of a document that names none, as jsonschema-rs gives one.
pub(crate) const BASE: &str = "json-schema:///";

/// A URI's parts, borrowed from its text.
struct Parts<'a> {
    scheme: Option<&'a str>,
    authority: Option<&'a str>,
    path: &'a str,
    query: Option<&'a str>,
    fragment: Option<&'a str>,
}

fn parts(uri: &str) -> Parts<'_> {
    let (rest, fragment) = match uri.split_once('#') {
        Some((rest, fragment)) => (rest, Some(fragment)),
        None => (uri, None),
    };
    let (rest, query) = match rest.split_once('?') {
        Some((rest, query)) => (rest, Some(query)),
        None => (rest, None),
    };
    let (scheme, rest) = match rest.split_once(':') {
        Some((scheme, after)) if is_scheme(scheme) => (Some(scheme), after),
        Some(_) | None => (None, rest),
    };
    let (authority, path) = match rest.strip_prefix("//") {
        Some(after) => match after.find('/') {
            Some(at) => (after.get(..at), after.get(at..).unwrap_or("")),
            None => (Some(after), ""),
        },
        None => (None, rest),
    };
    Parts {
        scheme,
        authority,
        path,
        query,
        fragment,
    }
}

fn is_scheme(text: &str) -> bool {
    let mut bytes = text.bytes();
    bytes.next().is_some_and(|b| b.is_ascii_alphabetic())
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

/// `reference` resolved against `base`.
pub(crate) fn resolve(base: &str, reference: &str) -> String {
    let (b, r) = (parts(base), parts(reference));
    let (scheme, authority, path, query) = if r.scheme.is_some() {
        (r.scheme, r.authority, dots(r.path), r.query)
    } else if r.authority.is_some() {
        (b.scheme, r.authority, dots(r.path), r.query)
    } else if r.path.is_empty() {
        (
            b.scheme,
            b.authority,
            b.path.to_string(),
            r.query.or(b.query),
        )
    } else if r.path.starts_with('/') {
        (b.scheme, b.authority, dots(r.path), r.query)
    } else {
        let merged = match (b.authority.is_some(), b.path.is_empty()) {
            (true, true) => format!("/{}", r.path),
            (true | false, false) | (false, true) => match b.path.rfind('/') {
                Some(at) => format!("{}{}", b.path.get(..=at).unwrap_or(""), r.path),
                None => r.path.to_string(),
            },
        };
        (b.scheme, b.authority, dots(&merged), r.query)
    };
    let mut out = String::new();
    if let Some(scheme) = scheme {
        out.push_str(scheme);
        out.push(':');
    }
    if let Some(authority) = authority {
        out.push_str("//");
        out.push_str(authority);
    }
    out.push_str(&path);
    if let Some(query) = query {
        out.push('?');
        out.push_str(query);
    }
    if let Some(fragment) = r.fragment {
        out.push('#');
        out.push_str(fragment);
    }
    out
}

/// `path` with its `.` and `..` segments removed.
fn dots(path: &str) -> String {
    let mut out: Vec<&str> = vec![];
    let segments: Vec<&str> = path.split('/').collect();
    let last = segments.len().saturating_sub(1);
    for (at, segment) in segments.iter().enumerate() {
        match *segment {
            "." if at == last => out.push(""),
            "." => {}
            ".." => {
                if out.len() > 1 {
                    out.pop();
                }
                if at == last {
                    out.push("");
                }
            }
            other => out.push(other),
        }
    }
    out.join("/")
}

/// `uri` without its fragment, and the fragment (empty when it has none).
pub(crate) fn split(uri: &str) -> (&str, &str) {
    uri.split_once('#').unwrap_or((uri, ""))
}

#[cfg(test)]
#[path = "uri_tests.rs"]
mod tests;
