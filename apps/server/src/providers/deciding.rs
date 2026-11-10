//! The decision model: OpenRouter's (`/systemone`), the one provider that serves decision models,
//! so it needs an OpenRouter key whichever provider the agent thinks with. `llx decide` asks it
//! one question, about stdin or each file it is given; a session's tasks ask it for advice (C9),
//! each decision held to a few seconds.

use std::time::Duration;

use contracts::model::Decider;

use super::{DECISION_DEFAULT, Provider, Vars, key_from, openrouter_client};

/// How long a task waits on its advisor's decision: a few of its usual half second.
const ADVICE_PATIENCE: Duration = Duration::from_secs(4);

/// The decision model `model`, its key read from the process's environment.
pub(crate) fn decider(model: &str) -> Option<Box<dyn Decider>> {
    waiting(model, None, None)
}

/// The decision model that advises a session's tasks (C9): the default one, each decision waited
/// for `ADVICE_PATIENCE` at most, as the task waits on it.
pub(crate) fn adviser(vars: Vars) -> Option<Box<dyn Decider>> {
    waiting(DECISION_DEFAULT, vars, Some(ADVICE_PATIENCE))
}

/// The decision model `model`, each decision waited for `patience` at most, OpenRouter's key read
/// in `vars`: its variable as its package names it, not as the catalog does, so that `llx decide`
/// reads no catalog (A4: the fact is the package's).
fn waiting(model: &str, vars: Vars, patience: Option<Duration>) -> Option<Box<dyn Decider>> {
    let names = [openrouter::auth::KEY_VARIABLE.to_string()];
    let (key, _) = key_from(&names, vars, Provider::openrouter().key_path())?;
    let client = openrouter_client(key, vars).deciding();
    Some(Box::new(openrouter::system_one::SystemOne {
        client,
        model: model.to_string(),
        patience,
    }))
}
