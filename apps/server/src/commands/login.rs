//! `/login` in a session, as a few steps, each a pick: the provider (every one of the catalog,
//! searched by typing, with how it is signed in to), signing in when it is not (OAuth in the
//! browser, or a pasted key: see `signin`), one of its models, and a reasoning level that model
//! takes (see `model`). The choice is put in at once and saved as the default for later sessions.

use crate::choice::Choice;
use crate::commands::model::{adopted, pick_among};
use crate::commands::signin;
use crate::compose::Rig;
use crate::picker::{Item, Terminal};
use crate::providers::{Method, Provider};
use crate::server::Ui;
use protocol::Event;

/// What the steps ask of the world beyond the terminal: where each provider's sign-in comes
/// from, and signing in (what was saved, in words).
pub(crate) struct World<'a> {
    pub signed_in: &'a dyn Fn(&Provider) -> Option<String>,
    pub sign_in: &'a SignIn<'a>,
    pub say: &'a dyn Fn(&str),
}

/// Sign in to a provider in a way, asking at the terminal under the step's path; what was
/// saved, in words, or `None` when the person cancelled.
pub(crate) type SignIn<'a> =
    dyn Fn(Provider, Method, &str, &mut dyn Terminal) -> Result<Option<String>, String> + 'a;

/// How the steps ended.
#[derive(Debug, PartialEq)]
pub(crate) enum Ended {
    Chose(Choice),
    /// Nothing was done.
    Nothing,
    /// A sign-in was saved, then the person gave up before choosing a model.
    SignedIn(Provider),
}

/// One step of the stack: what is asked now.
#[derive(Clone)]
enum Step {
    Provider,
    Keep(Provider, String),
    Method(Provider),
    SignIn(Provider, Method),
    Model(Provider),
}

/// What a step came to.
enum Move {
    Next(Step),
    Back,
    Signed(Provider),
    Chose(Choice),
}

/// What each step shows as marked when it is asked again: what was chosen there.
struct Marks {
    provider: Provider,
    keep: usize,
    method: usize,
}

/// The steps from `current`, a stack: a cancel goes back one step (as it was), and ends the
/// flow at the first.
pub(crate) fn choose(
    current: &Choice,
    terminal: &mut dyn Terminal,
    world: &World,
) -> Result<Ended, String> {
    let mut marks = Marks {
        provider: current.provider.clone(),
        keep: 0,
        method: 0,
    };
    let mut stack = vec![Step::Provider];
    let mut signed = None;
    while let Some(step) = stack.last().cloned() {
        match advance(&step, current, terminal, world, &mut marks)? {
            Move::Next(next) => stack.push(next),
            Move::Back => {
                stack.pop();
            }
            // The sign-in happened: back from its model is the providers, not a second sign-in.
            Move::Signed(provider) => {
                stack.clear();
                stack.push(Step::Provider);
                stack.push(Step::Model(provider.clone()));
                signed = Some(provider);
            }
            Move::Chose(choice) => return Ok(Ended::Chose(choice)),
        }
    }
    Ok(signed.map_or(Ended::Nothing, Ended::SignedIn))
}

fn advance(
    step: &Step,
    current: &Choice,
    terminal: &mut dyn Terminal,
    world: &World,
    marks: &mut Marks,
) -> Result<Move, String> {
    Ok(match step {
        Step::Provider => match pick_provider(current, terminal, world, marks) {
            Some(provider) => Move::Next(match (world.signed_in)(&provider) {
                Some(from) => Step::Keep(provider, from),
                None => after_keep(provider),
            }),
            None => Move::Back,
        },
        Step::Keep(provider, from) => {
            let items = [
                Item::new("Keep it", "", marks.keep == 0),
                Item::new("Sign in again", "", marks.keep == 1),
            ];
            let title = format!("{} › Signed in with {from}", path(provider));
            match terminal.pick(&title, &items) {
                None => Move::Back,
                Some(at) => {
                    marks.keep = at;
                    Move::Next(if at == 0 {
                        Step::Model(provider.clone())
                    } else {
                        after_keep(provider.clone())
                    })
                }
            }
        }
        Step::Method(provider) => {
            let methods = provider.methods();
            let items: Vec<Item> = methods
                .iter()
                .enumerate()
                .map(|(at, method)| Item::new(describe(*method), "", at == marks.method))
                .collect();
            let title = format!("{} › Sign in with", path(provider));
            match terminal.pick(&title, &items).and_then(|at| {
                marks.method = at;
                methods.get(at)
            }) {
                Some(method) => Move::Next(Step::SignIn(provider.clone(), *method)),
                None => Move::Back,
            }
        }
        Step::SignIn(provider, method) => {
            let signing = (world.sign_in)(provider.clone(), *method, &path(provider), terminal)?;
            match signing {
                Some(said) => {
                    (world.say)(&format!("{said}\n"));
                    Move::Signed(provider.clone())
                }
                None => Move::Back,
            }
        }
        Step::Model(provider) => {
            let title = format!("{} › Model", path(provider));
            match pick_among(current, terminal, std::slice::from_ref(provider), &title) {
                Some(choice) => Move::Chose(choice),
                None => Move::Back,
            }
        }
    })
}

/// The breadcrumb of a provider's steps: `Login › OpenAI`.
fn path(provider: &Provider) -> String {
    format!("Login › {}", provider.name())
}

/// The step that signs in: straight to the sign-in when there is one way, else which way.
fn after_keep(provider: Provider) -> Step {
    match provider.methods() {
        [only] => Step::SignIn(provider.clone(), *only),
        _ => Step::Method(provider),
    }
}

/// The provider chosen; `None` when the person cancelled.
fn pick_provider(
    current: &Choice,
    terminal: &mut dyn Terminal,
    world: &World,
    marks: &mut Marks,
) -> Option<Provider> {
    let mut offered = offered(Provider::all(), &current.provider, world.signed_in);
    let items: Vec<Item> = offered
        .iter()
        .map(|provider| {
            let id = provider.id();
            let (mark, detail) = match ((world.signed_in)(provider), provider.hint()) {
                (Some(from), _) => (" ✓", format!("{id} · signed in: {from}")),
                (None, Some(hint)) => ("", format!("{id} · not signed in · {hint}")),
                (None, None) => ("", format!("{id} · not signed in")),
            };
            let label = format!("{}{mark}", provider.name());
            Item::new(label, detail, *provider == marks.provider)
        })
        .collect();
    let at = terminal.pick("Login › Provider", &items)?;
    let provider = offered.swap_remove(at);
    if provider != marks.provider {
        marks.keep = 0;
        marks.method = 0;
    }
    marks.provider.clone_from(&provider);
    Some(provider)
}

/// Every provider of the catalog, in the order a person looks for one: the popular (in their
/// order), then the current one and those signed in to, then the rest, each by name.
fn offered(
    mut all: Vec<Provider>,
    current: &Provider,
    signed_in: &dyn Fn(&Provider) -> Option<String>,
) -> Vec<Provider> {
    all.sort_by_cached_key(|p| match p.popular() {
        Some(at) => (0, at, String::new()),
        None if p == current || signed_in(p).is_some() => (1, 0, p.name().to_lowercase()),
        None => (2, 0, p.name().to_lowercase()),
    });
    all
}

fn describe(method: Method) -> &'static str {
    match method {
        Method::Key => "Paste an API key",
        Method::Browser => "Sign in in the browser",
    }
}

/// `/login`: the steps, then the choice put in and saved.
pub(super) fn run(rig: &mut Rig, ui: &Ui) {
    let shown = &|text: &str| ui.send(Event::Note(text.to_string()));
    // The session's environment (its client's, in the daemon), kept while the rig changes.
    let env = rig.shared.workspace.env().map(<[_]>::to_vec);
    let vars = env.as_deref();
    let sign_in = |provider, method, path: &str, terminal: &mut dyn Terminal| {
        signin::sign_in(&provider, method, path, terminal, shown, vars)
    };
    let signed_in = |provider: &Provider| provider.signed_in_by(vars);
    let world = World {
        signed_in: &signed_in,
        sign_in: &sign_in,
        say: shown,
    };
    match choose(&rig.choice, &mut ui.clone(), &world) {
        Ok(Ended::Chose(choice)) => rig.output.note(&adopted(rig, choice)),
        Ok(Ended::Nothing) => rig.output.note("Nothing changed.\n"),
        Ok(Ended::SignedIn(provider)) => rig.output.note(&format!(
            "Signed in to {}; the model is still {}.\n",
            provider.name(),
            rig.choice.label()
        )),
        Err(why) => rig.output.note(&format!("Not signed in: {why}\n")),
    }
}

/// How the session stands with its provider, for `/status`.
pub(crate) fn signed(choice: &Choice, vars: crate::providers::Vars) -> String {
    match choice.provider.signed_in_by(vars) {
        Some(from) => format!("signed in: {from}"),
        None => format!("not signed in to {}: type /login", choice.provider.name()),
    }
}

#[cfg(test)]
#[path = "login_tests.rs"]
mod tests;
