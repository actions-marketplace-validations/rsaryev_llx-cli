//! llx: an ultrafast, lightweight agent runtime. It reads, edits and runs commands in a
//! workspace. This binary hardens the process, hands the server (`llx`, apps/server) this
//! terminal's side of a run, and exits here, with the code the run returns. Nothing else exits.

// The binary's own (ARCHITECTURE.md D7): the process's hardening, and the terminal's side of a
// run, in this process.
mod hardening;
mod terminal;

#[expect(
    clippy::disallowed_methods,
    reason = "the one place the process ends, with the exit-code contract (exit.rs)"
)]
fn main() {
    hardening::private();
    std::process::exit(llx::error::guarded(|| llx::commands::main(&terminal::Here)));
}
