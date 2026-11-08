//! What a session spent, read from its steps: by task, by agent, by model; the prompt cache's
//! misses and their likely cause; the dearest requests. `llx spend` prints it, `/usage` ends with
//! it, and the run notes a miss as it happens ([`Streams`], fed by `Output`).
//!
//! A session's steps hold its agents' model turns too, each named by its agent (`Step::agent`):
//! an agent's own session records no steps, so nothing is counted twice. A session saved before
//! steps named their agent has its agents' turns under `main`.

mod miss;

use contracts::history::{Session, Spent, Step};

pub(crate) use miss::{Cause, Miss, Streams};

/// Rows of the misses and of the dearest requests.
const LISTED: usize = 10;
use std::fmt::Write as _;

/// The stream a step belongs to: an agent's name, `main` for the session's own model, or the step
/// itself for another model's (`decision`, the advisor's).
fn stream(step: &Step) -> &str {
    match (step.what.as_str(), step.agent.as_str()) {
        ("model" | "elided", "") => "main",
        ("model" | "elided", agent) => agent,
        (what, _) => what,
    }
}

/// Requests, tokens and money, summed.
#[derive(Debug, Clone, Default)]
struct Sum {
    requests: u64,
    input: u64,
    cached: u64,
    written: u64,
    output: u64,
    cost: f64,
}

impl Sum {
    fn add(&mut self, spent: &Spent) {
        self.requests += 1;
        self.input += spent.input_tokens;
        self.cached += spent.cached_tokens;
        self.written += spent.written_tokens;
        self.output += spent.output_tokens;
        self.cost += spent.cost;
    }

    fn row(&self, label: &str) -> String {
        columns(
            label,
            [
                self.requests.to_string(),
                short(self.input),
                percent(self.cached, self.input),
                short(self.written),
                short(self.output),
                dollars(self.cost),
            ],
        )
    }
}

fn columns(label: &str, [a, b, c, d, e, f]: [String; 6]) -> String {
    format!("  {label:<28} {a:>8} {b:>8} {c:>7} {d:>8} {e:>8} {f:>9}\n")
}

/// A miss where it happened.
struct Found {
    task: usize,
    stream: String,
    miss: Miss,
}

/// A session's spend, every step read once.
#[derive(Default)]
struct Read {
    all: Sum,
    tasks: Vec<(usize, String, Sum)>,
    streams: Vec<(String, Sum)>,
    models: Vec<(String, Sum)>,
    misses: Vec<Found>,
    dearest: Vec<(usize, String, Spent)>,
}

fn sum_in<'a>(sums: &'a mut Vec<(String, Sum)>, key: &str) -> &'a mut Sum {
    let at = match sums.iter().position(|(name, _)| name == key) {
        Some(at) => at,
        None => {
            sums.push((key.to_string(), Sum::default()));
            sums.len() - 1
        }
    };
    &mut sums[at].1
}

fn read(session: &Session) -> Read {
    let mut read = Read::default();
    let mut streams = Streams::default();
    for (n, turn) in session.turns.iter().enumerate() {
        let task = n + 1;
        // Unix milliseconds the task began at: it ended at `at`, its last step's end after it began.
        let took = turn.steps.iter().map(|step| step.at_ms + step.ms).max();
        let began = (turn.at * 1_000).saturating_sub(took.unwrap_or(0));
        let mut sum = Sum::default();
        for step in &turn.steps {
            if step.what == "elided" {
                streams.elided(stream(step));
                continue;
            }
            if step.spent.is_empty() {
                continue;
            }
            let name = stream(step);
            sum.add(&step.spent);
            read.all.add(&step.spent);
            sum_in(&mut read.streams, name).add(&step.spent);
            let model = match step.model.as_str() {
                "" => "(not recorded)",
                model => model,
            };
            sum_in(&mut read.models, model).add(&step.spent);
            if step.what == "model"
                && let Some(miss) = streams.request(
                    name,
                    &step.model,
                    (began + step.at_ms, step.ms),
                    &step.spent,
                )
            {
                read.misses.push(Found {
                    task,
                    stream: name.to_string(),
                    miss,
                });
            }
            read.dearest.push((task, name.to_string(), step.spent));
        }
        if sum.requests > 0 {
            read.tasks
                .push((task, crate::printable::one_line(&turn.goal), sum));
        }
    }
    read.dearest.sort_by(|a, b| b.2.cost.total_cmp(&a.2.cost));
    read.dearest.truncate(LISTED);
    read
}

/// The whole report, as `llx spend` prints it.
pub(crate) fn report(session: &Session) -> String {
    let read = read(session);
    let title = match session.title.as_str() {
        "" => String::new(),
        title => format!(" · {}", crate::printable::printable(title)),
    };
    let mut out = format!("{}{title}\n", session.id);
    if read.all.requests == 0 {
        out.push_str("Nothing spent yet.\n");
        return out;
    }
    let _ = writeln!(
        out,
        "total      {} requests · {} · {} in · {} cached · {} written to the cache · {} out",
        read.all.requests,
        dollars(read.all.cost),
        short(read.all.input),
        percent(read.all.cached, read.all.input),
        short(read.all.written),
        short(read.all.output),
    );
    let header = columns(
        "",
        ["requests", "in", "cached", "written", "out", "cost"].map(String::from),
    );
    let _ = write!(out, "\nby task\n{header}");
    for (task, goal, sum) in &read.tasks {
        out.push_str(&sum.row(&format!("{task:>3} {}", clip(goal, 24))));
    }
    let _ = write!(out, "\nby agent\n{header}");
    for (name, sum) in &read.streams {
        out.push_str(&sum.row(&clip(name, 28)));
    }
    let _ = write!(out, "\nby model\n{header}");
    for (name, sum) in &read.models {
        out.push_str(&sum.row(&clip(name, 28)));
    }
    out.push_str(&misses(&read));
    out.push_str("\ndearest requests\n");
    for (task, name, spent) in &read.dearest {
        let _ = writeln!(
            out,
            "  task {task:<3} {:<16} {:>8} in · {:>4} cached · {:>6} out · {}",
            clip(name, 16),
            short(spent.input_tokens),
            percent(spent.cached_tokens, spent.input_tokens),
            short(spent.output_tokens),
            dollars(spent.cost),
        );
    }
    out
}

/// The misses: how many, what they cost, by cause, the dearest of them.
fn misses(read: &Read) -> String {
    if read.misses.is_empty() {
        return "\ncache misses: none\n".to_string();
    }
    let cost: f64 = read.misses.iter().map(|found| found.miss.cost).sum();
    let tokens: u64 = read.misses.iter().map(|found| found.miss.tokens).sum();
    let mut out = format!(
        "\ncache misses: {} · {} sent uncached · {} of {}\n",
        read.misses.len(),
        short(tokens),
        dollars(cost),
        dollars(read.all.cost),
    );
    for cause in Cause::ALL {
        let of: Vec<&Found> = read
            .misses
            .iter()
            .filter(|found| found.miss.cause == cause)
            .collect();
        if of.is_empty() {
            continue;
        }
        let cost: f64 = of.iter().map(|found| found.miss.cost).sum();
        let _ = writeln!(
            out,
            "  {:>3} × {:>8}  {}",
            of.len(),
            dollars(cost),
            cause.words()
        );
    }
    let mut dearest: Vec<&Found> = read.misses.iter().collect();
    dearest.sort_by(|a, b| b.miss.cost.total_cmp(&a.miss.cost));
    for found in dearest.into_iter().take(LISTED) {
        let _ = writeln!(
            out,
            "  task {:<3} {:<16} {:>8} uncached · {:>8} · {}",
            found.task,
            clip(&found.stream, 16),
            short(found.miss.tokens),
            dollars(found.miss.cost),
            found.miss.cause.words(),
        );
    }
    out
}

/// A miss as the run notes it.
pub(crate) fn noted(agent: &str, miss: &Miss) -> String {
    let whose = match agent {
        "" => String::new(),
        agent => format!(" ({agent})"),
    };
    format!(
        "✻ The prompt cache missed{whose}: {} tokens sent uncached, {}, {}\n",
        short(miss.tokens),
        dollars(miss.cost),
        miss.cause.words()
    )
}

/// `245k`, `1.2M`.
fn short(n: u64) -> String {
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", n as f64 / 1e3).replace(".0k", "k"),
        _ => format!("{:.1}M", n as f64 / 1e6),
    }
}

fn percent(part: u64, of: u64) -> String {
    match of {
        0 => "-".into(),
        of => format!("{}%", (part.saturating_mul(100) / of).min(100)),
    }
}

fn dollars(cost: f64) -> String {
    match cost {
        c if c < 1.0 => format!("${c:.3}"),
        c => format!("${c:.2}"),
    }
}

fn clip(text: &str, chars: usize) -> String {
    match text.chars().count() > chars {
        true => format!("{}…", text.chars().take(chars - 1).collect::<String>()),
        false => text.to_string(),
    }
}

#[cfg(test)]
#[path = "spend_tests.rs"]
mod tests;
