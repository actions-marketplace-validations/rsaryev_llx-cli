//! The sandbox on Linux: Landlock (its ABI 3: Linux 6.2 and later) keeps the command from writing
//! anywhere but `/dev/null`, its scratch directory and, in the workspace's scope, the workspace
//! where `sandbox::writable` lets it (no `.git` at its root, not the home nor above it), and from
//! tracing a process outside its domain; a seccomp filter refuses it every socket (`socket`, and
//! `io_uring`, which can open one). Both are set between fork and exec, after `no_new_privs` (so
//! no setuid program lifts them), and everything the command starts inherits them.

use std::io;
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd, RawFd};
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::Command;

// From <linux/landlock.h>.
/// The oldest ABI that can keep a command from changing any file.
const MIN_ABI: i64 = 3;
const CREATE_RULESET_VERSION: u32 = 1;
const RULE_PATH_BENEATH: u32 = 1;
const WRITE_FILE: u64 = 1 << 1;
/// Every right that changes the file system, as ABI 1 has them: writing a file, removing one or a
/// directory, making any kind of file.
const CHANGES: u64 = WRITE_FILE | (0x1FF << 4);
/// ABI 2: linking or renaming a file into another directory.
const REFER: u64 = 1 << 13;
/// ABI 3: truncating a file.
const TRUNCATE: u64 = 1 << 14;
/// ABI 4: binding and connecting TCP sockets (the seccomp filter refuses every socket already).
const TCP: u64 = 0b11;
/// ABI 6: abstract unix sockets and signals stay inside the domain.
const SCOPED: u64 = 0b11;

#[repr(C)]
struct RulesetAttr {
    handled_access_fs: u64,
    handled_access_net: u64,
    scoped: u64,
}

#[repr(C, packed)]
struct PathBeneath {
    allowed_access: u64,
    parent_fd: RawFd,
}

/// This kernel's Landlock ABI; negative when it has none.
fn abi() -> i64 {
    let none = std::ptr::null::<RulesetAttr>();
    // SAFETY: with the version flag the call reads no attribute; it returns the ABI's version.
    unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            none,
            0usize,
            CREATE_RULESET_VERSION,
        )
    }
}

pub(super) fn available() -> bool {
    supported(abi()).is_ok()
}

/// The rules for a command whose writable places are `scratch` and `writes`, as a Landlock ruleset
/// the child takes on before it starts. An error when the kernel has no Landlock.
pub(super) fn rules(scratch: &Path, writes: Option<&Path>) -> io::Result<OwnedFd> {
    let abi = abi();
    supported(abi)?;
    let since = |version: i64, rights: u64| if abi >= version { rights } else { 0 };
    // ABI 3 is required, so renames across directories and truncation are always handled.
    let changes = CHANGES | REFER | TRUNCATE;
    let attr = RulesetAttr {
        handled_access_fs: changes,
        handled_access_net: since(4, TCP),
        scoped: since(6, SCOPED),
    };
    // The attribute as long as this kernel knows it: an older one refuses a longer one.
    let size: usize = match abi {
        ..=3 => 8,
        4 | 5 => 16,
        _ => 24,
    };
    // SAFETY: `attr` lives through the call and is at least `size` bytes long.
    let made = unsafe {
        libc::syscall(
            libc::SYS_landlock_create_ruleset,
            &raw const attr,
            size,
            0u32,
        )
    };
    let fd = RawFd::try_from(made)
        .ok()
        .filter(|fd| *fd >= 0)
        .ok_or_else(io::Error::last_os_error)?;
    // SAFETY: a descriptor the call just opened, owned here alone.
    let rules = unsafe { OwnedFd::from_raw_fd(fd) };
    grant(&rules, Path::new("/dev/null"), WRITE_FILE | TRUNCATE)?;
    grant(&rules, scratch, changes)?;
    if let Some(root) = writes {
        grant(&rules, root, changes)?;
    }
    Ok(rules)
}

/// Whether a kernel at Landlock ABI `abi` (negative: none) can confine a command to reading. Below
/// 3 a file could still be truncated, and a rename could move one across directories, so the
/// command would not be read-only: it is refused with the reason, never run open (B3).
fn supported(abi: i64) -> io::Result<()> {
    if abi >= MIN_ABI {
        return Ok(());
    }
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "this kernel's Landlock is missing or too old (ABI 3, Linux 6.2 or later, is needed to keep \
         a command from truncating files): the command could not be confined to reading",
    ))
}

/// Let the ruleset `rules` grant `access` beneath `path`.
fn grant(rules: &OwnedFd, path: &Path, access: u64) -> io::Result<()> {
    let place = std::fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)?;
    let beneath = PathBeneath {
        allowed_access: access,
        parent_fd: place.as_raw_fd(),
    };
    // SAFETY: `beneath` lives through the call; both descriptors are open.
    let added = unsafe {
        libc::syscall(
            libc::SYS_landlock_add_rule,
            rules.as_raw_fd(),
            RULE_PATH_BENEATH,
            &raw const beneath,
            0u32,
        )
    };
    match added {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    }
}

/// `sh -c command`, a session's leader, taking on `rules` and the socket filter before it starts.
/// `rules` must stay open until the command has started.
pub(super) fn shell(command: &str, rules: &OwnedFd) -> Command {
    let mut shell = super::super::process::shell(command);
    let rules = rules.as_raw_fd();
    // SAFETY: `confine` makes only system calls that are async-signal-safe (prctl, Landlock's,
    // seccomp's), takes no lock and allocates nothing.
    unsafe { shell.pre_exec(move || confine(rules)) };
    shell
}

/// In the child: no new privileges, the ruleset, the filter.
fn confine(rules: RawFd) -> io::Result<()> {
    let failed = |done: libc::c_long| match done {
        0 => Ok(()),
        _ => Err(io::Error::last_os_error()),
    };
    // SAFETY: sets a flag of this process alone.
    failed(unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) }.into())?;
    // SAFETY: `rules` is the ruleset the parent holds open until the command has started.
    failed(unsafe { libc::syscall(libc::SYS_landlock_restrict_self, rules, 0u32) })?;
    let program = libc::sock_fprog {
        len: FILTER_LEN,
        filter: FILTER.as_ptr().cast_mut(),
    };
    // SAFETY: `program` points into the static filter, which the kernel copies.
    let filtered = unsafe {
        libc::prctl(
            libc::PR_SET_SECCOMP,
            libc::SECCOMP_MODE_FILTER,
            &raw const program,
        )
    };
    failed(filtered.into())
}

#[cfg(target_arch = "x86_64")]
const ARCH: u32 = 0xC000_003E;
#[cfg(target_arch = "x86_64")]
const SOCKET: u32 = 41;
#[cfg(target_arch = "aarch64")]
const ARCH: u32 = 0xC000_00B7;
#[cfg(target_arch = "aarch64")]
const SOCKET: u32 = 198;
const IO_URING_SETUP: u32 = 425;
/// x86_64's x32 calls carry this bit: refused as a whole.
const X32: u32 = 0x4000_0000;

const LOAD: u16 = 0x20; // BPF_LD | BPF_W | BPF_ABS
const EQUALS: u16 = 0x15; // BPF_JMP | BPF_JEQ | BPF_K
const AT_LEAST: u16 = 0x35; // BPF_JMP | BPF_JGE | BPF_K
const RETURN: u16 = 0x06; // BPF_RET | BPF_K
const REFUSED: u32 = libc::SECCOMP_RET_ERRNO | 1; // EPERM

const fn op(code: u16, k: u32, jt: u8, jf: u8) -> libc::sock_filter {
    libc::sock_filter { code, jt, jf, k }
}

const FILTER_LEN: u16 = 9;
/// Another architecture's call kills the command; `socket`, `io_uring_setup` and x32's calls are
/// refused (EPERM); every other call goes on. A jump skips that many instructions.
static FILTER: [libc::sock_filter; FILTER_LEN as usize] = [
    op(LOAD, 4, 0, 0), // seccomp_data.arch
    op(EQUALS, ARCH, 1, 0),
    op(RETURN, libc::SECCOMP_RET_KILL_PROCESS, 0, 0),
    op(LOAD, 0, 0, 0), // seccomp_data.nr
    op(AT_LEAST, X32, 3, 0),
    op(EQUALS, SOCKET, 2, 0),
    op(EQUALS, IO_URING_SETUP, 1, 0),
    op(RETURN, libc::SECCOMP_RET_ALLOW, 0, 0),
    op(RETURN, REFUSED, 0, 0),
];

#[cfg(test)]
#[path = "linux_tests.rs"]
mod tests;
