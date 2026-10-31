//! Where a client is, as the sessions it opens take it (ARCHITECTURE.md D2): the workspace at
//! its directory, whose commands run with its environment (its `PATH`, a virtualenv, its keys,
//! less the app's hidden ones). In process the place is the process's own; over a socket,
//! each client's.

use std::path::Path;

use protocol::Place;
use tools::workspace::Workspace;

use crate::compose::APP;

/// The workspace at `dir`, with llx's config (keys, tokens) and sessions off limits to the tools:
/// a prompt injection must not read them.
pub(crate) fn at(dir: &Path) -> Result<Workspace, String> {
    let workspace = Workspace::new(dir).map_err(|error| error.to_string())?;
    if !workspace.root().is_dir() {
        return Err("not a directory".into());
    }
    let private = provider_kit::store::dir(APP).into_iter();
    let sessions = crate::commands::sessions::store().dir;
    Ok(workspace.denying(private.chain([sessions])))
}

/// The workspace a client's sessions work in: its directory, its environment.
pub(crate) fn workspace(place: &Place) -> Result<Workspace, String> {
    let workspace = at(Path::new(&place.cwd)).map_err(|why| format!("{}: {why}", place.cwd))?;
    Ok(workspace.with_env(place.env.clone()))
}

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
