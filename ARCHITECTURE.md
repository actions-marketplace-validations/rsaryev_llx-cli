# ARCHITECTURE.md

The architecture of llx, as binding rules. Every change follows them, strictly; AGENTS.md says
what each file is, this says where a thing goes and what may know what. When a change does not
fit, the change is wrong, not the rules: fix the design, or change this file first, in its own
commit that says why. An exception is never made in code.

Each rule has an id. Cite it in a review or a commit when a change touches it. A rule marked
_checked_ fails the checks when broken: a test (`apps/cli/tests/suite/repo/`, or the tests of the
behaviour the rule names) or a workspace lint (`Cargo.toml`, `clippy.toml`); a rule is checked as
soon as a test or a lint can state it, and the check comes with the rule.

Sources: matklad's ARCHITECTURE.md and "Large Rust Workspaces", rust-analyzer's architecture and
style guides, the Rust Design Patterns' anti-patterns, corrode.dev, BurntSushi on unwrap,
sans-IO (quinn-proto, Firezone), ports and adapters, the Rust API Guidelines, LSP and JSON-RPC
2.0, the clippy configurations of ruff, uv, zed, tikv and rust-analyzer, an agent server's protocol crates, app-server client, TUI boundary check and workspace lints,
and Localix (rule ids, ratchets, poke-then-pull sync).

## Bird's-eye view

llx is a model in a loop over five tools: `bash`, through which it reads, searches and runs the checks, `apply_patch`, through which it changes files (B1-B5), `ask`, `agents` and `jobs`. The loop decides; it knows no technology. Tools, stores and providers are technology behind ports. One composition root wires
them for a session. A session runs in a server, and an interface shows it through a protocol,
in the same process: the server on its threads, the interface on its own, frames over channels
between them. Nothing a person sees is known below the interface; nothing a model or a tool
does is known above the server.

## The layers

```text
                         apps/cli        the binary: the process, the terminal
                        ╱        ╲
           apps/tui             apps/server             composition root: the command line,
           keys, drawing,       sessions, lanes, agents, jobs, modes, the gate's and the person's answers
           its client             │
                │                 │        packages/agent                  the core
                │                 │        packages/tools, session, providers/*   adapters
                │                 │
          packages/protocol                 packages/contracts, queue, providers/kit
          the seam's types only             vocabulary: ports, primitives
```

The server's package is `server`, its library `llx`: the log's targets (`llx::server::serve`) and
`RUST_LOG=llx=debug` are its modules' paths.

| Layer | Crates | Knows |
|---|---|---|
| Vocabulary | `contracts` (ports, no logic), `queue`, `providers/kit` | no workspace crate |
| Seam | `protocol` | no crate at all |
| Core | `agent` | `contracts` |
| Adapters | `tools`, `session` | `contracts` |
| Adapters | `providers/{models,openai,openrouter}` | `contracts`, `kit` |
| Server | `apps/server` (library `llx`) | every package |
| Interface | `apps/tui` (its client to the server a module, `client.rs`) | `protocol` |
| Binary | `apps/cli` | `server`, `tui`, `protocol` (the frames and the offer it hands over, P7) |

## Dependency rules

- **L1** _checked_: `contracts` depends on no workspace crate.
- **L2** _checked_: `agent` depends only on `contracts`.
- **L3** _checked_: tools and stores depend only on `contracts`; primitives (`queue`) and the
  provider kit on nothing; providers on `contracts` and the kit. A tool knows nothing of the
  agent, the models or the other tools; a provider nothing of tools or other providers.
- **L4** _checked_: only apps compose packages. Nothing depends on an app, but the binary on the
  server and the interface; the server and the interface depend on no app in any section (the
  binary's tests list the two again, with their `testing` features). A test that needs both a
  screen and a server lives in the binary's crate, the one that knows both.
- **L5** _checked_: `contracts` and `agent` name no tool or provider (no browser, page, URL,
  OpenRouter, OpenAI, model name). The agent learns a tool's world from `ToolSpec::description`,
  the run's place from the app's context.
- **L6** _checked_: `protocol` depends on no crate. Its types are the seam's own, never the
  domain's: the server maps its values onto them (`server/wire.rs`), and the protocol never
  re-exports one.
- **L7** _checked_ on `apps/tui`'s manifest and sources: the interface depends only on
  `protocol`: never on
  the agent, tools, providers, the session store or the server, nor on the app's other modules.
  What it needs to show, the protocol carries.
- **L8**: the server is the only crate that serves the protocol and the only one that knows
  every package; the interface's client (`apps/tui`'s `client.rs`) is the only code that calls
  it.
- **L9**: a new edge between crates is a design change, recorded here first. A new `pub` item is
  an API change: visibility is the tightest that compiles (`pub(super)`, `pub(crate)`), and
  `pub use` across crates is not used to reach past a layer. (The agent's conversation and history
  internals are `pub(crate)`.)
- **L10**: no `utils` or `common` crate, ever; a shared thing is named for what it is (`kit`,
  `queue`) and placed at the layer of what it knows.

## Core rules (the agent)

- **C1** _checked_ on the sources: the core decides, the shell acts. Effects leave the core
  through a port (`Chat`, `Tool`, `Gate`, `Person`, `Inbox`, `History`); the core opens
  no file, socket or process and reads no environment variable.
- **C2**: time and limits come in as values (a budget, a deadline, a strategy); a test sets them.
- **C3**: strategies, not special cases. What may be done another way is a trait with an
  implementation the app chooses: `history::Strategy` and its `Stage`s, the inbox, the gate, the
  person.
- **C4**: the prompt cache is a constraint. Nothing rewrites an earlier message unless a budget
  forces it, then in one pass with headroom; what was elided stays elided. Settings that change
  between tasks reach the model as a note at the end.
- **C5**: untrusted content stays data. Tool output, agents' answers, a monitor's output are
  tagged (`<agent_result>`, `<monitor>`) where the agent reads them, never instructions.
- **C6**: (retired.)
- **C7**: no task- or site-specific wording in the prompt or the tool descriptions. A rule earns its place by the evals and stays general.

- **C8**: callers decide, functions act: branches go up to the caller, loops down into
  functions that take the batch.
- **C9** _checked_: a decision model advises the loop only at a task's edges, through the
  `Advisor` port, where a few typed questions cost less than a model turn. As a conversation's
  first task starts: the level it thinks at (under `auto`; it stays the conversation's, so the
  prompt cache is never broken mid-way; a follow-up and a fork keep it, and are not asked). When
  the main session's model answers: whether the answer ends the task, else one line to go on
  with, at most twice a task and never on its last turn. It never writes a call, never changes
  the tools, the prompt or the mode; each question is bounded to a few seconds. With no advisor (no decision model reachable, `--no-advice`), or one that fails or
  is late, the loop runs exactly as it does without one: the requests a scripted model receives
  are the same.

## Port rules (contracts)

- **P1**: a port is a narrow trait, owned by the side that calls it, `Send + Sync`.
- **P2**: a port's error is its own enum, shaped by what the caller does with it
  (`ToolError { Failed, Fatal }`: tell the model, or end the run). No adapter's error type
  (`io`, HTTP, serde) crosses a port; the adapter translates it.
- **P3**: a value with an invariant keeps its fields private and checks them in its constructor
  (`Selection`); a value any field combination of which is valid has public fields. A struct
  never mixes a bare `pub` field with private ones (_checked_): another crate reads a private
  field through a method, and inside a crate a field says how far it reaches.
- **P4**: an option is an enum, not a `bool` or an `Option` argument, where a call site would
  otherwise read `f(true, None)`; a struct of several flags is a state enum (_checked_ for
  parameters and structs).
- **P5**: a port never returns `Result<_, String>`: its error is its enum (P2).
- **P6**: `map_err` keeps the cause (a variant's field, or the source); rewording an error never
  drops it (_checked_: `map_err_ignore`).
- **P7**: a crate does not re-export another crate's types, and never a glob (`pub use x::*`).

## Adapter rules (tools, stores, providers)

- **A1**: a tool asks the `Gate` before an effect outside the conversation, with a one-line
  summary that leaves nothing out. Only the app answers the gate; a tool never approves itself.
- **A2**: a tool asks the person only through `Person`, answered only by the app.
- **A3**: a path the model wrote (a command's `workdir`) goes through `Workspace` (`readable`),
  never opened without it.
- **A4**: one source of truth for models: every model fact comes from `providers/models`. No
  provider keeps its own list; no app code hard-codes a model, window or level. A default is a
  policy the app states by a catalog id.
- **A5**: how to call a provider (wire, auth, retries) lives in its package; which provider a
  choice uses, in `apps/server/src/providers.rs` only.
- **A6**: a data file (JSON, TOML) lives in the package that owns its meaning.
- **A7**: a heavy dependency stays in the one adapter that needs it (TLS in the kit); std first, and the release binary under 5 MiB.
- **A8**, **A9**: (retired.)

## Composition rules (the apps)

- **K1**: the app composes, it does not know. No catalog data, wire format or provider rule in
  an app; if it needs a fact, a package exposes it.
- **K2**: sessions, approvals, the person, modes, agents and jobs are the server's; keys, the
  composer, drawing, `$EDITOR` and the clipboard the interface's.
- **K3**: one person, one question at a time: every approval and `ask` goes through
  `gate::one_question`, whichever thread asks, on the one lock of the person the run has
  (`Shared::asking`), whether the gate asks at the terminal or through the interface.
- **K4**: something that reaches a run from outside its calls is a kind in `arrivals.rs` (its
  policy and tag) posted to the lane's queue; the agent's inbox knows no source.
- **K5**: a mode is a named set of rules (`allow`, `deny`) and what its commands reach (`reach`:
  `anything`, `workspace` or `read-only`), and nothing else: the built-in ones (`default`, no rule,
  reaching the workspace: its commands run without asking in the sandbox of B3 that writes only
  inside the workspace, its patches of the workspace's files are made without asking, and what
  must reach further (B6) is asked; `plan`, read-only: its commands run without asking, in the
  read-only sandbox of B3, and its patches are refused (B5); `bypass`, everything allowed) and the
  person's, from `~/.config/llx/settings.json`, where one may replace a built-in. `--allow` and `--deny` add
  to the mode's. A deny refuses in every mode; an allow runs without asking, as it is (in a
  read-only mode, still in its sandbox); what no rule settles and the reach does not cover
  is asked of the person, or refused when no one is there. Rules come from the command line and
  the person's settings only, never from the repository (a pull request would allow itself). A
  mode changes only what the gate answers and the context's rules text; it never filters tools,
  and no code branches on a mode's name.

- **K6**: one place maps failures to the exit-code contract (`exit.rs`); the `--json` events and
  exit codes change only with a schema bump and a CHANGELOG entry.
- **K7**: (retired with `llx window`.)
- **K8**: a session has one writer. The store hands it out (`History::hold`, a `Record`) and
  keeps it the process's for as long as it lives, with a lock the operating system lets go when
  the process ends however it ends; a second, in this process or another, is refused
  (`HistoryError::Held`), and the session is not opened there. A session is written as it goes,
  each change appended (JSON Lines: its header, then each task and each fact), never rewritten
  whole; a line a crash cut short is left out when it is read.

## Protocol rules (the seam between interface and server)

- **W1**: frames are values on two channels, `ToServer` and `ToClient` (`protocol`'s
  `frame.rs`), never encoded: a call (`ToServer::Call`) carries an `id` and gets exactly one
  `ToClient::Done` with that id (a failed one too, `Failed` with why, which the interface
  shows the person, whether it waited for the call or only sent it); a question
  (`ToClient::Ask`) has an id its `ToServer::Reply` and `ToClient::Resolved` name; every other
  frame is a notification, with no id.
- **W2**: a frame holds protocol types only (`Command`, `Done`, `Event`, `Asked`, `Reply`); a
  frame's id is a number, a session's a string; times are `*_at` milliseconds since the epoch.
- **W3**: what the server offers (its commands, its modes, the lanes it opens at most: the
  protocol's `Offer`) is handed to the interface as it starts, with the channels, before any
  frame; no frame asks for it. The sessions work where the process does: the workspace the
  command line names, with the process's environment.
- **W4**: (retired with the daemon: one transport, the channels.)
- **W5**: a question the server asks a person belongs to the session: it waits under its id
  until the reply, and `Resolved` tells the interface it no longer waits (answered, or withdrawn
  when its task stops, its lane closes or the person leaves).
- **W6**: the server never waits on the interface: its frames go out on a channel that never
  blocks, read on the interface's own thread.
- **W7**: a request carries what the server needs to act on it (the session's id); the seam keeps
  no hidden state.
- **W8**: one request's panic or failure never ends the server: it becomes that request's error.
- **W9**: (retired.)

## Shell and patch rules (`bash`, `apply_patch`)

The model works as a person at a terminal does: it reads with `cat`, `sed -n` and `rg` and runs
the checks, chaining what does not need a result first into one command; it changes files with a
patch, as free text, with nothing to escape. A large file written through a command line, a
heredoc inside a JSON string, is where a model breaks: its quotes and newlines go wrong, or the
command is cut.

- **B1**: the model's tools for files and commands are `bash` (`tools/src/shell/`), for reading,
  searching and running, and `apply_patch` (`tools/src/fs/patch/`), for every file it adds,
  changes, deletes or moves. No other tool reads or writes a file for it; a capability is a
  command, or a program the shell has.
- **B2**: what the shell offers is the app's context to say, found on the process's `PATH`
  without running anything: the runtimes a script can be written in
  (`python3`, `node`, `bun`), named only when present. How to change a file well is the tools'
  descriptions' and the prompt's to say, in general terms (C7).
- **B3**: a confined command runs in the operating system's sandbox, never a check of its text:
  on macOS Seatbelt (`sandbox-exec`, closed by default), on Linux Landlock with a seccomp filter.
  It reads every file but the app's own (the workspace's denied paths: its keys and sessions; on
  macOS only, as Landlock cannot take a path out of what it lets read), opens no network or local
  socket and reaches no other process (no Apple events, no launchd). What it
  writes is the verdict's: a read-only command (a `read-only` mode's, K5; `Verdict::ReadOnly`)
  writes only `/dev/null` and a temporary directory of its own; a workspace command (a
  `workspace` mode's; `Verdict::Workspace`) writes there and inside the workspace too, but never
  a `.git` in it, the workspace's own, a nested repository's or a new one (a hook written there
  would run unconfined at the person's next commit there), nor the app's own paths. Where the
  sandbox cannot keep every `.git` (on Linux: Landlock cannot take a path out of what it lets
  write, and no tree is walked to find one), or the workspace is the home directory or holds it,
  a workspace command is confined to reading. Where no sandbox can be set up, a read-only command is refused with the
  reason, never run open; a workspace command is asked of the gate as one that must run as it is
  (B6).
- **B6**: a command or a patch that must reach beyond its sandbox says so in its `Effect`
  (`Needs::Open`): the model asks for it (`escalate` on `bash` and a monitor), a patch does for a
  path inside `.git`. The gate asks the person for it, as for any effect no rule settles; a
  read-only mode refuses it. A confined command that fails as a sandbox refuses (a write outside
  the workspace, the network) says so in its report, so the model can ask again with `escalate`;
  the model's word never widens the sandbox, it only decides whether the person is asked.
- **B4**: a changed file is the gate's to allow, as any command is (A1): the summary is the whole
  command, or the patch's files and what it does to each. Nothing records a command's or a
  patch's files: `/rewind` goes back in the conversation only.
- **B5**: `apply_patch` takes the patch's text, not JSON: where a provider takes a tool of free
  text (the Responses API's custom tools, for a model the catalog says takes a grammar) it is
  offered so, with its grammar; elsewhere as a function with one string argument. `ToolSpec`
  says which; the providers render it (L5: the agent knows neither). A read-only verdict refuses
  a patch: a file written cannot be sandboxed.
- **B7** _checked_: a reply the output limit cut off never runs its last call as it stands: a cut
  command may be missing its end. A tool that can tell what of a cut call is whole carries out
  that part and says where it stopped (`Tool::salvage`: `apply_patch` writes every file before
  the one the cut fell in, the one exception to a patch applied whole); any other cut call is
  answered unrun, with why. Replies are streamed where the host allows it (OpenRouter), so a cut
  call arrives with what was written of it; sent whole, it arrives empty.

- **B8**: (retired with `grep`: a search the model builds from the shell, `llx decide` and its
  helpers, as any other agent does.)
- **B9**: a command is waited for at most 120 s: a test that hangs costs minutes of nothing, and
  the model cannot see why while it waits. Where its session keeps jobs (the main agent's, in a
  session and in `exec`), a command still running then is moved to the background, not killed
  (a `sleep` is killed: waiting is the jobs' to do), and `run_in_background` starts one there at
  once. It is a job of `jobs`, listed and stopped as one (`jobs stop`, the person's `x`), with a
  limit of its own (R10: 30 minutes from the move, 10 in `exec`; `timeout_ms`, at most 2 hours,
  for one started there), its whole output in a file the result names, which the model may read.
  Its end comes back once, a kind of `arrivals.rs` (K4) carrying its report (the exit code and the
  end of its output, as `bash` reports a command), a task of its own when none runs; it ends
  with the session, and in `exec` the task waits for it. An agent's command, with no jobs, runs
  to its end or its `timeout_ms` (at most 600 s) and is killed there.
- **B10**: a monitor waits for one thing, as Localix's does: its command is a check, run every
  `every` seconds (5 at least; each run held to that interval, 30 s at most) until it exits 0 or
  its time is up (`timeout_ms`, 5 minutes by default, 30 at most, 10 in `exec`). It wakes the
  model once, with how it ended (met, timed out, or a check that cannot run: 126 or 127, at
  once) and what the check printed last. No job hands the model a stream of lines: a monitor
  that woke it on each line kept a task from ever ending (22 turns in 32 s). A command that runs
  long and ends by itself is `bash` with `run_in_background` (B9); work again and again is a
  schedule. A job the person or the model stops opens no task: the model asked for the stop, or
  reads it with the person's next task.

## Concurrency and errors

- **R1**: synchronous code: threads where needed, no async runtime.
- **R2**: state shared across threads is `Mutex` or atomics, never `Cell`/`RefCell`. A thread
  that borrows lives in a `thread::scope` a guard ends even on panic; a detached thread is owned
  by something that stops and joins it.
- **R3**: every queue and channel that a peer fills is bounded, or says what it dropped.
- **R4** _checked_ (S4): a panic is a bug, and one request's panic must not end the server
  (W8). Working code handles what can fail; where a value exists by construction, the smallest
  item expects the lint with the `reason` it cannot fail.
- **R5**: the app's edge has one error type (`Failure`); libraries return their port's enums.
- **R6**: a `MutexGuard` is never held across a send, a receive, a join, a sleep, I/O or a
  callback: copy out what is needed, drop the guard, then act (_checked_ where clippy sees it).
- **R7**: a check and the act that depends on it are one critical section or one atomic
  operation; two locks are always taken in one order, and a mutex is never locked twice on a
  thread.
- **R8**: every thread is named (_checked_) and started with `thread::Builder` (its failure an
  error, not a panic: _checked_), and is joined, or owned by what stops it.
- **R9**: wait on events, not on time: a `Condvar`, `recv_timeout`, a deadline. A `sleep` is a
  backoff, and says so where it is (_checked_).
- **R10** _checked_: a child process always has a deadline and a way to be stopped. The
  server's short local queries (`/diff`'s `git`, `doctor`'s shell) go through
  `tools::shell::query`, which kills one at its deadline with all it started; the interface's
  (the clipboard's tool, the pasted picture's, `@`'s `git`) through `apps/tui/src/within.rs`, as it may
  not reach `tools` (L7). (Debt: the provider kit's browser opener, which may not reach `tools`
  (L3), waits without one for the `sh` that hands the opener to the background and ends at once;
  the opener itself is never waited for.)

## Code rules

- **S1** _checked_: at most **300 lines of working code per file** (non-blank, non-comment;
  `budget.rs`), for every file of `src/` and every build script. A file that would go past it is
  split by what it does before the change lands, never let through. Test files do not count, and
  split by feature when they grow hard to read.
- **S2** _checked_: at most 50,000 lines of working code in all; new code is paid for by cuts
  first, and going over needs the owner's approval, asked with the numbers and the alternatives.
- **S3** _checked_: unit tests live beside the code: `x.rs` ends with
  `#[cfg(test)] #[path = "x_tests.rs"] mod tests;`. The ports are faked, never a network.
- **S4** _checked_: every crate takes the workspace's lints (`[lints] workspace = true`), and
  they are hard: denied, never turned off in `Cargo.toml`. An exception is an `expect` on the
  smallest item, with its `reason`, never an `allow`: an exception that no longer applies fails
  the build. In working code they forbid:
  - a panic: `unwrap`, `expect`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, a string
    sliced inside a character, a number cut by `as` (R4);
  - output and exits outside their place: `print!`/`eprint!`, `dbg!`, `process::exit` (K6);
  - shared state that is not `Send + Sync`: `Rc`, `Cell`, `RefCell`; setting the environment (R2);
  - functions past 100 lines or past a bounded complexity (S1's rule for functions);
  - a `pub` no other crate reaches (L9), qualifications not needed, wildcard imports, an
    implicit `Arc` clone;
  - a `_` arm over our own enums (S10); a struct whose fields mix a bare `pub` with private
    ones (P3);
  - work done for nothing: text formatted apart to be appended (`write!` into it), a default
    built when it is not needed, a clone assigned over a value (`clone_from`), a stable sort of
    primitives, a value taken that is only read; a method returning a changed copy that is not
    `#[must_use]`.

  Libraries and binaries leave no dependency unused, in either build (the clippy checks).

  Tests may unwrap, panic and print (`clippy.toml`); an integration-test crate says so once, at
  its root. The `expect`s in working code are counted, and the count only falls: a new one raises
  it in `code.rs` in the same commit, where a review sees it (_checked_).
- **S5** _checked_: `unsafe` lives only where the OS is called: `tools` and the apps. The
  vocabulary, the core, the stores and the providers have none.
- **S6**: one idea per file, named for it; a file's `//!` says what it is (_checked_: every
  working file opens with one). A directory module (`shell/`, `fs/`) groups the files of
  one tool or one subsystem, its root `x.rs` beside the directory `x/`, never `x/mod.rs`
  (_checked_; the integration tests' `tests/common/mod.rs` and `tests/suite/mod.rs` stay, as
  cargo needs).
- **S7**: comments say why; rustfmt defaults; English only (_checked_).
- **S8**: TDD: a bug's test fails first at the layer a person meets it; a feature's tests are
  written from what a person does and sees. A test that cannot fail is deleted.

- **S9**: a clone has a reason: an `Arc` crossing threads, a value kept while the original goes
  on. A clone that only quiets the borrow checker means the data's owner is wrong.
- **S10** _checked_: your own enums are matched exhaustively; a `_` arm is for foreign or
  `#[non_exhaustive]` enums, and says so in its `expect`.
- **S11**: an `unsafe` block says why it is sound (`// SAFETY:`), one unsafe operation each
  (_checked_).

## Adding things

- **A provider:** an entry in the catalog (`providers/models`, from models.dev), a wire in a
  provider package if none fits, one arm in `providers.rs`.
- **A model:** nothing in llx: it arrives with the catalog's refresh.
- **A tool capability:** an `action` of an existing tool, or the shell (B1). A new tool is a
  decision recorded here.
- **Something that reaches a run:** a kind in `arrivals.rs` and a post to the lane's queue (K4).
- **A CI integration:** the GitHub Action is `action.yml` at the repository's root, composite
  shell steps only (no Node, no MCP server, no GitHub tool): it installs the release its own tag
  names, runs `llx exec` once, and hands back the answer, the exit code and the spend. The
  prompt, the rules and what is posted stay in the person's workflow, where they can read them
  (C7, K5); the agent reaches GitHub, as anything else, with `gh` and `git` through `bash`. It
  reads the exit codes and `--json` events (K6), so a schema bump updates it in the same commit.
  Ready-made workflows live in `examples/`, each run for real before it is published.
- **A history stage:** a `Stage` in `agent/src/history.rs`, placed by its value in
  `Pipeline::default`, with a test, the long-dialogue simulation still passing.
- **A protocol method:** a request to the server is a `Command` variant, and what it comes to a
  `Done` one, in `protocol`'s `command.rs`; served in the server (`server/serve.rs`), called
  through the interface's client (`Client::call`); a test through the protocol's frames only. What the server says
  unasked is an `Event` variant (`event.rs`).
- **A rule:** here, with an id, and its test in the same commit when a test can state it.

## The seam's steps

- **D1** (done): the seam as the protocol, in process: `protocol` exists, and the interface
  speaks only through it, with its own client. L6 and L7 are checked.
- **D2**, **D3**, **D4**: (retired with the daemon, `llx serve`, by the owner's decision: a
  session runs in the process of the terminal that shows it, so it carries no place of its own,
  no socket serves it, and nothing attaches to it later or keeps it in the background. It cost
  more code and bugs than it was worth, and no measured speed.)
- **D5**, **D6**: (retired.)
- **D7** (done): the split into crates. `apps/tui`, the interface: its client, `protocol` and
  its terminal libraries; the exec trace too (`Trace`), drawn from the protocol's events. `apps/server`,
  the composition root: sessions, lanes, agents, jobs, modes, every package, and the
  command line's `Cli`; its run takes the terminal from the binary through its `Front` port.
  `apps/cli`, the binary: `main.rs`, the process's hardening, `terminal` (the `Front`: the
  interface in this process, each lane's trace), the tests that drive the interface's `App`
  against a desk, and the integration tests.

## Budgets

- 50,000 lines of working code (S2), 300 per file (S1).
- The release binary under 5 MiB (CI: `scripts/check --only size`, which warns past 4 MB; raised
  from 3 MiB by the owner for a release that unwinds a panic, W8); a bundled data
  file counts.
- Line coverage at least 85% (`scripts/coverage.sh`), raised with coverage, never lowered.
