//! The daemon as its host knows it (docs/daemon.md): a socket per config directory in the
//! person's own runtime directory (a test home gets its own daemon), a lock on the config
//! directory that a second daemon is refused, its build, and how it is started. A client's way to
//! it (its status, `reach`, `stop`) is the binary's (its `terminal`, in apps/cli: D7).

use std::fs::File;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// What `llx serve` says on stdout once clients may connect.
pub const READY_LINE: &str = "llx serve: ready";

/// The credential store's directory: the lock is there, and its path names the socket.
pub fn config() -> Result<PathBuf, String> {
    provider_kit::store::dir(crate::compose::APP)
        .ok_or_else(|| "no home directory for the daemon's lock".into())
}

/// This binary's build: its version, and its size and time, so that a daemon an older binary
/// started is told apart. `LLX_BUILD` stands in for it (a test's older daemon).
pub fn build() -> String {
    if let Some(build) = std::env::var_os("LLX_BUILD") {
        return build.to_string_lossy().into_owned();
    }
    let file = std::env::current_exe().and_then(std::fs::metadata);
    let made = file.as_ref().ok().and_then(|file| file.modified().ok());
    let made = made.and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok());
    let size = file.map_or(0, |file| file.len());
    let made = made.map_or(0, |at| at.as_secs());
    format!("{}+{size:x}.{made:x}", env!("CARGO_PKG_VERSION"))
}

/// `$TMPDIR`, else `/tmp`: where a user's sockets go.
fn runtime_dir() -> PathBuf {
    std::env::var_os("TMPDIR").map_or_else(|| PathBuf::from("/tmp"), PathBuf::from)
}

/// Where the daemon for `config` listens, as a client finds it: the socket its lock names while
/// it holds the lock (it was started where `$TMPDIR` may be another than this process's, as by
/// an app that sets its own), else where this process would put one.
pub fn socket_path(config: &Path) -> PathBuf {
    let named = std::fs::read_to_string(config.join("daemon.lock")).ok();
    let named = named.and_then(|text| text.lines().nth(1).map(PathBuf::from));
    named
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or_else(|| own_socket(config))
}

/// `$TMPDIR/llx-<uid>/<hash of the config directory>.sock`: short, as `sun_path` must be, and
/// the same for a config directory in every process and every build (FNV-1a, not std's hasher,
/// which may change). Where a daemon started by this process listens.
pub(crate) fn own_socket(config: &Path) -> PathBuf {
    let hash = config
        .as_os_str()
        .as_bytes()
        .iter()
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0100_0000_01b3)
        });
    // SAFETY: getuid has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    runtime_dir()
        .join(format!("llx-{uid}"))
        .join(format!("{hash:016x}.sock"))
}

/// The daemon's lock on its config directory, held while it lives; let go when it drops. The
/// file names the daemon's process and its socket while it holds it.
pub(crate) struct Lock {
    held: File,
}

impl Drop for Lock {
    fn drop(&mut self) {
        let _ = self.held.set_len(0);
    }
}

/// The lock on `<config>/daemon.lock`; `None` when another daemon holds it.
pub(crate) fn lock(config: &Path) -> std::io::Result<Option<Lock>> {
    std::fs::create_dir_all(config)?;
    let file = File::options()
        .create(true)
        .truncate(false)
        .write(true)
        .open(config.join("daemon.lock"))?;
    // SAFETY: the descriptor is the open file's, alive for the call; flock touches nothing else.
    let locked = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if locked == 0 {
        let mut held = file;
        held.set_len(0)?;
        write!(
            held,
            "{}\n{}",
            std::process::id(),
            own_socket(config).display()
        )?;
        return Ok(Some(Lock { held }));
    }
    let error = std::io::Error::last_os_error();
    if error.kind() == std::io::ErrorKind::WouldBlock {
        Ok(None)
    } else {
        Err(error)
    }
}

/// `llx serve` for `config`, in a session of its own, with no terminal: neither the starting
/// terminal's signals nor its closing reach it. Its stdin is nothing, its errors go to
/// `daemon.log` beside the lock; its stdout is the starter's to take (the ready line).
pub fn command(config: &Path) -> std::io::Result<Command> {
    std::fs::create_dir_all(config)?;
    let log = File::options()
        .create(true)
        .append(true)
        .open(config.join("daemon.log"))?;
    let mut command = Command::new(std::env::current_exe()?);
    // A session's options are its client's to say (`commands::serve`), never what the terminal
    // that happened to start the daemon had set.
    let options = <crate::cli::Cli as clap::CommandFactory>::command();
    for name in options.get_arguments().filter_map(clap::Arg::get_env) {
        command.env_remove(name);
    }
    command
        .arg("serve")
        .current_dir("/")
        .stdin(Stdio::null())
        .stderr(log);
    let lead = || {
        // SAFETY: setsid takes nothing, is async-signal-safe, and changes only the child.
        unsafe { libc::setsid() };
        Ok(())
    };
    // SAFETY: between fork and exec the closure calls only setsid (above).
    unsafe { std::os::unix::process::CommandExt::pre_exec(&mut command, lead) };
    // The daemon outlives the client that starts it: a descriptor of the client's it inherited
    // (one its host gave it, one another thread was making) would stay open as long.
    tools::shell::streams_only(&mut command);
    Ok(command)
}

/// What the daemon writes after its ready line goes where its errors go: nobody reads its stdout.
pub(crate) fn quiet() {
    // SAFETY: dup2 takes two descriptors this process has open, and replaces only the first.
    unsafe { libc::dup2(2, 1) };
}

#[cfg(test)]
#[path = "daemon_tests.rs"]
mod tests;
