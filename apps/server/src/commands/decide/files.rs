//! `llx decide QUESTION FILE…`: each file asked on its own, a few at once, the answers printed in
//! the order the files were given; a file that cannot be read or answered is named on stderr and
//! the rest are still answered, so one command covers a directory where a shell loop would not
//! fit an agent's allow rule.

use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Mutex, PoisonError};
use std::thread;

use contracts::model::Decider;

use super::{Picked, Questions, ask, shown, verdict};
use crate::cli::DecideArgs;
use crate::error::{self, Failure};
use crate::exit;
use crate::input;
use crate::sync::lock;

/// How many files are asked at once: each answer takes well under a second, so a few in flight
/// cover a directory quickly without a burst against the provider.
const AT_ONCE: usize = 8;

/// One answer per file.
type Answer = Result<Picked, Failure>;

/// Every file asked, then a line each in order; exit 3 when any failed, else `--expect`'s verdict.
pub(super) fn each(
    decider: &dyn Decider,
    questions: &Questions,
    args: &DecideArgs,
) -> Result<i32, Failure> {
    let answers = answered(decider, questions, &args.files)?;
    let (mut picks, mut failed) = (vec![], false);
    for (file, answer) in args.files.iter().zip(answers) {
        let name = file.display().to_string();
        match answer {
            Ok(picked) => {
                error::print(&shown(&picked, args.json, Some(&name)))?;
                picks.push(picked.choice);
            }
            Err(failure) => {
                failed = true;
                let why = failure.message().unwrap_or_default();
                error::note(&format!("llx decide: {name}: {why}\n"));
            }
        }
    }
    if failed {
        return Ok(exit::ENVIRONMENT);
    }
    Ok(verdict(
        args.expect.as_deref(),
        picks.iter().map(String::as_str),
    ))
}

/// Each file read and asked by [`AT_ONCE`] named threads taking the next file in turn; the
/// answers in the files' order.
fn answered(
    decider: &dyn Decider,
    questions: &Questions,
    files: &[PathBuf],
) -> Result<Vec<Answer>, Failure> {
    let slots: Vec<Mutex<Option<Answer>>> = files.iter().map(|_| Mutex::new(None)).collect();
    let next = AtomicUsize::new(0);
    let work = || {
        loop {
            let at = next.fetch_add(1, Ordering::Relaxed);
            let (Some(file), Some(slot)) = (files.get(at), slots.get(at)) else {
                break;
            };
            let answer = input::read_file(file).and_then(|text| ask(decider, questions, &text));
            *lock(slot) = Some(answer);
        }
    };
    let started = thread::scope(|scope| {
        (0..AT_ONCE.min(files.len()))
            .map(|n| {
                let work = &work;
                thread::Builder::new()
                    .name(format!("llx-decide-{n}"))
                    .spawn_scoped(scope, work)
                    .map(drop)
            })
            .collect::<Result<Vec<()>, _>>()
    });
    started.map_err(|error| Failure::Environment(format!("could not start a thread: {error}")))?;
    Ok(slots
        .into_iter()
        .map(|slot| {
            (slot.into_inner().unwrap_or_else(PoisonError::into_inner))
                .unwrap_or_else(|| Err(Failure::Environment("not asked".into())))
        })
        .collect())
}
