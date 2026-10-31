//! What each command does. Every command returns its exit code, or a [`Failure`] that [`main`]
//! reports.

pub(crate) mod decide;
pub(crate) mod doctor;
pub(crate) mod entry;
pub(crate) mod extras;
pub(crate) mod login;
pub(crate) mod model;
pub(crate) mod repl;
pub(crate) mod rewind;
pub mod run;
pub(crate) mod serve;
pub(crate) mod sessions;
pub(crate) mod signin;
use clap::Parser;

use crate::cli::{Cli, Command, ExecCommand, joined};
use crate::error::{self, Failure};
use crate::{compose, exit, logs};
use run::{At, Front, Kind};

/// The process's command line parsed and its command run, a session shown at this terminal
/// through `front`, the binary's side; the process's exit code.
pub fn main(front: &'static dyn Front) -> i32 {
    if let Some(config) = provider_kit::store::dir(compose::APP) {
        logs::start(&config);
    }
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            // `--help` and `--version` come here too, and are no error; a closed pipe while they
            // print is no error either.
            let _ = error.print();
            return if error.use_stderr() {
                exit::ENVIRONMENT
            } else {
                exit::OK
            };
        }
    };
    match dispatch(cli, At::Terminal(front)) {
        Ok(code) => code,
        Err(failure) => {
            // The binary's own line, as it has always been logged.
            log::warn!(target: "llx", "ended with exit {}: {failure}", failure.code());
            if let Some(message) = failure.message() {
                error::note(&format!("{message}\n"));
            }
            failure.code()
        }
    }
}

/// Run what `cli` asks for, a session's shown `at`.
pub(crate) fn dispatch(cli: Cli, at: At) -> Result<i32, Failure> {
    let kind = match cli.print {
        true => Kind::Exec,
        false => Kind::Session,
    };
    match cli.command {
        None if cli.resume_last => {
            sessions::resume(None, joined(&cli.prompt).as_deref(), cli.run, kind, at)
        }
        None => run::run(joined(&cli.prompt).as_deref(), cli.run, None, kind, at),
        Some(Command::Exec(exec)) => match exec.command {
            None => run::run(
                joined(&exec.prompt).as_deref(),
                exec.run,
                None,
                Kind::Exec,
                at,
            ),
            Some(ExecCommand::Resume(args)) => resumed(args, Kind::Exec, at),
        },
        Some(Command::Decide(args)) => decide::decide(&args),
        Some(Command::Resume(args)) => resumed(*args, Kind::Session, at),
        Some(Command::Sessions) => sessions::list(),
        Some(Command::Logout) => signin::logout(),
        Some(Command::Doctor(args)) => doctor::doctor(&args),
        Some(Command::Serve(serve)) => serve::serve(serve.action.as_ref(), at.front()),
    }
}

fn resumed(args: crate::cli::ResumeArgs, kind: Kind, at: At) -> Result<i32, Failure> {
    let (id, prompt) = args.target();
    let id = id.map(str::to_string);
    sessions::resume(id.as_deref(), prompt.as_deref(), args.run, kind, at)
}
