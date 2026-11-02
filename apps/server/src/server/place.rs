//! Where the sessions work: the workspace the command line names (`-C`, else the current
//! directory), with llx's own paths off limits to the tools.

use std::path::Path;

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

#[cfg(test)]
#[path = "place_tests.rs"]
mod tests;
