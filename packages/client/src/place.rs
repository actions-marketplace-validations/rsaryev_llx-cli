//! Where an interface is, as it tells the server at its handshake: both interfaces (the
//! terminal's and its tests') read it the same way from their own process.

use std::path::Path;

use protocol::Place;

/// This process's place: its `cwd` (where the sessions work), its environment (a variable that
/// is not text is left out), its working directory (`dir`, which the relative paths of `args`
/// are of), and the command line `args` it is to be served as.
#[must_use]
pub fn here(cwd: &Path, args: Vec<String>) -> Place {
    let env = std::env::vars_os()
        .filter_map(|(name, value)| Some((name.into_string().ok()?, value.into_string().ok()?)));
    let dir = std::env::current_dir().unwrap_or_default();
    Place {
        cwd: cwd.display().to_string(),
        env: env.collect(),
        args,
        dir: dir.display().to_string(),
    }
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
