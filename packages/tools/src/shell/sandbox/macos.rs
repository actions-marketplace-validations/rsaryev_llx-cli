//! The sandbox on macOS: Seatbelt, through `/usr/bin/sandbox-exec` (never one found on `PATH`,
//! which a command could replace) and a profile closed by default, after Chrome's and Codex's.

use std::fmt::Write as _;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";

/// What a confined command may do; everything else is refused. `SCRATCH` is its own directory.
/// No `network*`, `mach-lookup` beyond the user database, `appleevent-send` or launchd: nothing
/// that would ask another process to act for it.
const PROFILE: &str = r#"(version 1)
(deny default)
(allow process-exec process-fork)
(allow signal (target same-sandbox))
(allow process-info* (target same-sandbox))
(allow file-read*)
(allow file-write-data (require-all (path "/dev/null") (vnode-type CHARACTER-DEVICE)))
(allow file-ioctl (path "/dev/null"))
(allow file-write* (subpath (param "SCRATCH")))
(allow sysctl-read)
(allow ipc-posix-sem)
(allow mach-lookup (global-name "com.apple.system.opendirectoryd.libinfo"))
"#;

/// What a workspace command may write besides: `ROOT` but its `.git` (`GIT`, where it really is),
/// as a hook written there would run unconfined at the person's next commit.
const WORKSPACE: &str = r#"(allow file-write* (require-all (subpath (param "ROOT")) (require-not (subpath (param "GIT")))))
"#;

pub(super) fn available() -> bool {
    Path::new(SANDBOX_EXEC).is_file()
}

/// `sh -c command` under the profile, the scratch directory `scratch` and `writes` its writable
/// places and `kept` neither read nor written (a later rule wins over an earlier one).
pub(super) fn shell(
    command: &str,
    scratch: &Path,
    writes: Option<&Path>,
    kept: &[PathBuf],
) -> io::Result<Command> {
    if !available() {
        return Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!("{SANDBOX_EXEC} is missing: the command could not be confined to reading"),
        ));
    }
    let mut profile = PROFILE.to_string();
    let mut shell = Command::new(SANDBOX_EXEC);
    // As parameters, so a path is never read as the profile's syntax.
    shell.arg(format!("-DSCRATCH={}", scratch.display()));
    if let Some(root) = writes {
        profile.push_str(WORKSPACE);
        let git = crate::workspace::real(&root.join(".git"));
        shell.arg(format!("-DROOT={}", root.display()));
        shell.arg(format!("-DGIT={}", git.display()));
    }
    for (at, path) in kept.iter().enumerate() {
        let _ = writeln!(
            profile,
            "(deny file-read* file-write* (subpath (param \"KEPT{at}\")))"
        );
        shell.arg(format!("-DKEPT{at}={}", path.display()));
    }
    shell.arg("-p").arg(profile).args(["sh", "-c", command]);
    super::super::process::lead(&mut shell);
    Ok(shell)
}
