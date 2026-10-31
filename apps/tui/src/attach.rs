//! What the person attaches to a message, on the interface's side: a file dragged in or pasted
//! as its path, or the clipboard's image (Ctrl+V), kept where it outlives its source. A picture
//! goes to the server as its path, which reads it for the model; any other file is named by its
//! path (`@`), and the agent reads it.

use std::path::{Path, PathBuf};

/// A file named in what the person typed or dropped in.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Dropped {
    /// A picture: sent to the model as one.
    Image(PathBuf),
    /// Any other file or folder: named by its path, for the agent to read.
    File(PathBuf),
}

/// What `token` names, when it is a path that exists: as a terminal writes a dropped file
/// (quoted, its spaces escaped, or as a `file://` address) or as typed (`~/x`).
pub(crate) fn resolve(token: &str) -> Option<Dropped> {
    let token = token.trim();
    let unquoted = token
        .strip_prefix('\'')
        .and_then(|t| t.strip_suffix('\''))
        .or_else(|| token.strip_prefix('"').and_then(|t| t.strip_suffix('"')));
    let path = unquoted.map_or_else(|| unescape(token), str::to_string);
    let path = match path.strip_prefix("file://") {
        Some(p) => p.replace("%20", " "),
        None => path,
    };
    let path = match path.strip_prefix("~/") {
        Some(rest) => PathBuf::from(std::env::var_os("HOME")?).join(rest),
        None => PathBuf::from(&path),
    };
    if !path.is_absolute() || !path.exists() {
        return None;
    }
    let lower = path.to_string_lossy().to_ascii_lowercase();
    let picture = [".png", ".jpg", ".jpeg", ".gif", ".webp"]
        .iter()
        .any(|ext| lower.ends_with(ext));
    Some(if picture && path.is_file() {
        Dropped::Image(path)
    } else {
        Dropped::File(path)
    })
}

/// A shell word's backslash escapes undone: `a\ b` is `a b`.
fn unescape(word: &str) -> String {
    let mut out = String::new();
    let mut chars = word.chars();
    while let Some(c) = chars.next() {
        match c {
            '\\' => out.extend(chars.next()),
            c => out.push(c),
        }
    }
    out
}

/// A copy of the picture at `path` under `dir`, so a file the system removes soon (a screenshot's
/// temporary file) is still there when the message is sent; the original when it cannot be made.
pub(crate) fn keep(path: &Path, dir: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let stamp = super::time::now_ms();
    let copy = dir.join(format!("{stamp}-{name}"));
    match std::fs::create_dir_all(dir).and_then(|()| std::fs::copy(path, &copy)) {
        Ok(_) => copy,
        Err(_) => path.to_path_buf(),
    }
}

/// Where pictures the person pasted or dropped in are kept: beside the sessions, so they outlive
/// the files they came from.
pub(crate) fn pasted_dir() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".llx").join("pasted")
}

/// The clipboard's picture, saved as a PNG under `dir`; why not when there is none.
#[cfg(target_os = "macos")]
pub(crate) fn clipboard(dir: &Path) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|error| format!("{}: {error}", dir.display()))?;
    let stamp = super::time::now_ms();
    let path = dir.join(format!("pasted-{stamp}.png"));
    // AppleScript writes the clipboard's PNG data to the file; it fails when there is none.
    let script = format!(
        "set f to open for access POSIX file \"{}\" with write permission\n\
         write (the clipboard as «class PNGf») to f\nclose access f",
        path.display()
    );
    let mut osascript = std::process::Command::new("osascript");
    osascript.args(["-e", &script]);
    let ran = super::within::within(osascript, std::time::Duration::from_secs(5));
    match ran {
        Some(_) if path.metadata().is_ok_and(|m| m.len() > 0) => Ok(path),
        _ => {
            let _ = std::fs::remove_file(&path);
            Err("the clipboard holds no picture".into())
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub(crate) fn clipboard(_: &Path) -> Result<PathBuf, String> {
    Err("pasting a picture is not supported here yet: drag the file in instead".into())
}

#[cfg(test)]
#[path = "attach_tests.rs"]
mod tests;
