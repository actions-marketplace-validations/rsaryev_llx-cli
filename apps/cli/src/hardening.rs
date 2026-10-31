//! The binary's (D7): what the process does once, at its start, to keep what it holds from the
//! other processes of the same user: the provider's key is in its environment, and a command the
//! agent runs is such a process.

/// On Linux, the process becomes non-dumpable: `/proc/<pid>/environ` and `/proc/<pid>/mem` are
/// then readable by root alone and `ptrace` is refused, so a command cannot read the key from
/// llx. A program the process starts is its own process: exec makes it dumpable again, and it
/// reads its own environment as ever. Elsewhere nothing is set: there is no such flag (on macOS
/// `ps -E` shows a process's environment to the same user whatever it does).
#[cfg(target_os = "linux")]
pub(crate) fn private() {
    // SAFETY: prctl(PR_SET_DUMPABLE, 0) takes plain integers (the three unused arguments zero)
    // and only clears a flag of this process. It fails only where a sandbox forbids the call,
    // which leaves the process as it was.
    unsafe { libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0) };
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn private() {}
