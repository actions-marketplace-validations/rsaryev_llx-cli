//! What a refreshed model catalog costs a run as it starts. models.dev lists hundreds of
//! providers and thousands of models, and a run uses one: `llx decide` reads none of them, and
//! `llx exec` decodes only what it uses. What is counted is memory, the peak a run reaches with a
//! large catalog against the same run without one: unlike time, it is the same on a loaded CI
//! machine as on a laptop.

use std::io::Read;
use std::process::{Command, Stdio};

use crate::common::home::{Home, detach, feed};
use crate::common::model::{Model, Reply};

/// One model as a refresh keeps it: what llx reads of models.dev's v2 API.
fn model(provider: usize, at: usize) -> String {
    format!(
        r#""m{at}": {{"name": "Model {at}", "type": "chat", "release_date": "2026-01-{day:02}",
        "modalities": {{"output": ["text"]}},
        "capabilities": {{"tools": {{"supported": true}},
            "reasoning": {{"supported": true, "effort": ["low", "medium", "high"]}},
            "cache": {{"implicit": true}}, "system_updates": true, "effort_updates": false}},
        "limit": {{"context": 200000, "output": 64000}},
        "cost": {{"text": {{"input": {{"price": 1.0, "cache": {{"read": 0.1}}}},
            "output": {{"price": 4.0}}}}}},
        "api": {{"chat-completions": {{"base_url": "https://api.p{provider}.dev/v1",
            "compatibility": {{"reasoning_field": "reasoning_content"}}}}}}}}"#,
        day = at % 28 + 1
    )
}

/// A refreshed catalog of at least `bytes`: providers of their own, 30 models each, none of them
/// the one a run thinks with (OpenRouter stays the built-in one).
fn catalog(bytes: usize) -> String {
    let mut providers = vec![];
    let mut size = 0;
    while size < bytes {
        let at = providers.len();
        let models: Vec<String> = (0..30).map(|m| model(at, m)).collect();
        let provider = format!(
            r#""p{at}": {{"name": "Provider {at}", "env": ["P{at}_API_KEY"], "models": {{{}}}}}"#,
            models.join(",")
        );
        size += provider.len();
        providers.push(provider);
    }
    format!("{{{}}}", providers.join(",\n"))
}

/// A home whose last refresh left `catalog`.
fn refreshed(catalog: &str) -> Home {
    let home = Home::new();
    let dir = home.path().join("config").join("llx");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("models.json"), catalog).unwrap();
    home
}

/// `command` run to its end with no terminal (a session of its own), `stdin` piped in: its exit
/// code, its stdout and stderr, and the most memory it held, in bytes. `wait4` reaps the child, for
/// the usage `Child::wait` does not give.
fn measured(mut command: Command, stdin: &str) -> (Option<i32>, String, String, u64) {
    detach(&mut command);
    let mut child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    feed(&mut child, stdin);
    let pid = libc::pid_t::try_from(child.id()).unwrap();
    let mut status = 0;
    // SAFETY: rusage is plain data that wait4 fills; zeroed is a valid value of it.
    let mut usage: libc::rusage = unsafe { std::mem::zeroed() };
    // SAFETY: wait4 reaps this test's own child and writes only to the two places given.
    let waited = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
    assert_eq!(waited, pid);
    let (stdout, stderr) = (read(child.stdout.take()), read(child.stderr.take()));
    let code = libc::WIFEXITED(status).then(|| libc::WEXITSTATUS(status));
    // macOS counts the peak in bytes, Linux in kilobytes.
    let unit = if cfg!(target_os = "macos") { 1 } else { 1024 };
    let peak = u64::try_from(usage.ru_maxrss).unwrap() * unit;
    (code, stdout, stderr, peak)
}

/// What a pipe of the child said, once it ended.
fn read(pipe: Option<impl Read>) -> String {
    let mut said = String::new();
    pipe.unwrap().read_to_string(&mut said).unwrap();
    said
}

const MB: u64 = 1 << 20;

/// The extra memory `run` takes in a home with a 4 MB refreshed catalog over one without.
fn cost(run: impl Fn(&Home) -> (Option<i32>, String, String, u64)) -> (u64, u64) {
    let large = catalog(4 << 20);
    let (plain, refreshed) = (Home::new(), refreshed(&large));
    let (code, stdout, stderr, without) = run(&plain);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    let (code, stdout, stderr, with) = run(&refreshed);
    assert_eq!(code, Some(0), "{stdout}{stderr}");
    (without / MB, with.saturating_sub(without))
}

#[test]
fn decide_never_reads_the_model_catalog() {
    let run = |home: &Home| {
        let model = Model::start(vec![Reply::Decided("yes".into(), vec![])]);
        measured(home.with_model(&model, &["decide", "Is it?"]), "x\n")
    };
    let (base, extra) = cost(run);
    // Reading the file alone would take its 4 MB: decide needs OpenRouter's key, no catalog.
    assert!(
        extra < 2 * MB,
        "decide took {} MB more than its {base} MB with a 4 MB catalog",
        extra / MB
    );
}

#[test]
fn exec_decodes_only_the_provider_it_thinks_with() {
    let run = |home: &Home| {
        let model = Model::start(vec![Reply::done("Done.")]);
        let mut command = home.with_model(&model, &["exec", "hi"]);
        command.current_dir(home.path());
        let (code, stdout, stderr, peak) = measured(command, "");
        assert_eq!(stdout, "Done.\n", "{stderr}");
        (code, stdout, stderr, peak)
    };
    let (base, extra) = cost(run);
    // The file read and indexed is about twice its size (9 MB on macOS); decoding every model of
    // it, as a catalog read whole does, is about twelve times (48 MB). The bound sits between,
    // with room for another allocator.
    assert!(
        extra < 16 * MB,
        "exec took {} MB more than its {base} MB with a 4 MB catalog",
        extra / MB
    );
}
