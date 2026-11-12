//! `llx doctor`: is everything a run needs in place? The sign-in to each provider, both models
//! (each asked one tiny question), and the shell `bash` runs in. A missing decision model only
//! leaves out `llx decide` and the advice, so it warns without failing.

use std::collections::BTreeMap;
use std::time::Duration;

use contracts::chat::{Calling, Chat, Message, Request, TaskId};
use contracts::model::{Decider, Question};
use serde_json::json;

use crate::choice;
use crate::cli::DoctorArgs;
use crate::error::{self, Failure};
use crate::exit;
use crate::providers::{self, Provider};

/// One check: passed, failed, or a warning that does not fail the run.
#[derive(Clone, Copy)]
enum Mark {
    Pass,
    Fail,
    Warn,
}

fn line(mark: Mark, text: &str) -> Result<(), Failure> {
    let mark = match mark {
        Mark::Pass => "✓",
        Mark::Fail => "✗",
        Mark::Warn => "!",
    };
    error::print(&format!("{mark} {text}\n"))
}

pub(crate) fn doctor(args: &DoctorArgs) -> Result<i32, Failure> {
    let mut ok = report(shell())?;
    let choice = choice::resolve(&args.models, choice::saved(), None)?;
    // The provider in use, and every other one signed in to: the catalog lists hundreds.
    for provider in Provider::all() {
        match provider.signed_in() {
            Some(from) => line(
                Mark::Pass,
                &format!("{}: signed in with {from}", provider.name()),
            )?,
            None if provider == choice.provider => line(
                Mark::Fail,
                &format!(
                    "{}: not signed in (type /login in a session)",
                    provider.name()
                ),
            )?,
            None => {}
        }
    }
    // The servers on this machine that answer, and the one in use when it does not.
    for (provider, base, models) in Provider::local_report() {
        match models {
            Some(models) => line(
                Mark::Pass,
                &format!("{}: answers at {base}, models: {models}", provider.name()),
            )?,
            None if provider == choice.provider => {
                line(Mark::Fail, &provider.how_to_sign_in())?;
            }
            None => {}
        }
    }
    match choice.provider.chat(&choice.selection) {
        Some(chat) => ok &= report(agent_model(chat.as_ref(), &choice.label()))?,
        None => ok = false,
    }
    match providers::decider(&args.decision_model) {
        Some(decider) => ok &= report(decision_model(decider.as_ref(), &args.decision_model))?,
        None => line(
            Mark::Warn,
            &format!("decision model: {}", providers::how_to_decide()),
        )?,
    }
    Ok(if ok { exit::OK } else { exit::ENVIRONMENT })
}

/// A check's mark and its line, printed; whether it passed (a warning does not fail the run).
fn report((mark, text): (Mark, String)) -> Result<bool, Failure> {
    line(mark, &text)?;
    Ok(!matches!(mark, Mark::Fail))
}

/// How long `echo ok` may take (R10): it ends at once where the shell works.
const SHELL_WAIT: Duration = Duration::from_secs(5);

/// The shell `bash` runs commands with, as the tool names it.
fn shell() -> (Mark, String) {
    let name = tools::shell::SHELL;
    match tools::shell::query(tools::shell::shell("echo ok"), SHELL_WAIT) {
        Ok(_) => (Mark::Pass, format!("shell: {name}")),
        Err(why) => (Mark::Fail, format!("shell: {name} {why}")),
    }
}

fn agent_model(chat: &dyn Chat, model: &str) -> (Mark, String) {
    let hello = [Message::User("Reply with the word ready.".into())];
    let request = Request {
        conversation: &hello,
        tools: &[],
        calling: Calling::Nothing,
        deadline: None,
        effort: None,
        task: TaskId::NONE,
    };
    match chat.complete(request) {
        Ok(completion) => (
            Mark::Pass,
            format!(
                "agent model {} answers in {} ms",
                completion.usage.model, completion.usage.millis
            ),
        ),
        Err(error) => (Mark::Fail, format!("agent model {model}: {error}")),
    }
}

fn decision_model(decider: &dyn Decider, model: &str) -> (Mark, String) {
    let question = Question::Noul {
        instructions: json!("Is the sky in the state blue?"),
    };
    let questions = BTreeMap::from([("q".to_string(), question)]);
    match decider.decide(&json!({"sky": "blue"}), &questions) {
        // A reply with no answer in it is no working model, whatever its status.
        Ok(decision) if !decision.answers.contains_key("q") => {
            (Mark::Fail, format!("decision model {model}: no answer"))
        }
        Ok(decision) => (
            Mark::Pass,
            format!(
                "decision model {} answers in {} ms",
                decision.model, decision.millis
            ),
        ),
        Err(error) => (Mark::Fail, format!("decision model {model}: {error}")),
    }
}
