# AGENTS.md

Instructions for AI coding agents working on this repository.

**ARCHITECTURE.md is binding: follow it strictly.** Read it before any change. It fixes the
layers and the crates of each, what may know what, and the rules by id (L, C, P, A,
K, W, R, S). Every new file goes where its layer says, in its crate's place. A change that
does not fit is redesigned, or ARCHITECTURE.md is changed first, in its own commit, with the
owner's agreement; an exception is never made in code. Cite the rule ids a change touches.

`llx` is an ultrafast, lightweight agent runtime: a minimal agent for the terminal, helpers on any model, and a decision step for scripts and CI. It has five tools: `bash` (commands: the model reads and searches with it and runs the checks, one still running after 120 s going on in the background as a job, B1-B4, B9), `apply_patch` (every file it adds, changes, deletes or moves, as a patch of free text, B5), `ask`, a question to the person that the app answers, `agents`, other agents it starts (each a session with a parent, run in parallel), and `jobs`, monitors and schedules. The tool set is deliberately minimal: no plugins, no MCP servers consumed, no skills, no extra tools. A new capability goes into one of these (as an `action`), or is left to the shell.

## Layout

Packages and three apps: the server (the composition root), the interface and the binary. Every dependency points inward (rules L1-L5 below).

- `packages/contracts`: ports only, no logic.
- `packages/agent`: the agent: the loop, the conversation, how history is prepared, the system prompt (`prompt.md`).
- `packages/tools`: the tools, a module each (`fs/`, `shell/`, `ask.rs`), and `workspace.rs`, where paths resolve.
- `packages/providers/`: the models' providers, a package each: `kit/` (what they share), `models/` (the model catalog, MODELS.md), `local/`, `openrouter/`, `anthropic/`, `openai/`.
- `packages/protocol`: what the interface and the server say to each other (ARCHITECTURE.md W1-W8).
- `packages/queue`, `packages/schema`, `packages/markdown`: primitives an app composes, with no workspace dependency.
- `packages/session`: the history store, a JSON Lines file per session.
- `apps/server`: the composition root, package `server`, its library `llx`.
- `apps/tui`: the session's interface, package `tui`.
- `apps/cli`: the `llx` binary, package `localix-cli`, and the one integration-test binary, `tests/all.rs`.
- `action.yml` and `action/`: the GitHub Action. `examples/`: ready-made uses. `skills/llx/SKILL.md`: the skill.
- `docs/`: the guides, each a page of the site. `site/`: the website (npm, Vite, React).
- `scripts/`: `check` (the checks' one entry point, below) with `checks/`, the hooks, the release and the runners' scripts. `benchmark/`: llx scored as an agent.

**[LAYOUT.md](LAYOUT.md) is the map of every file**: what each module holds and the rule it serves. Read the part for the package you are about to change before opening files to find out, and change its line in the same change when a file is added, moved, removed or given another job.

## Rules

Layers (checked by `apps/cli/tests/suite/repo/architecture.rs`):

- L1: `contracts` depends on no workspace package.
- L2: `agent` depends only on `contracts`.
- L3: tools and stores (every other package under `packages/`) depend only on `contracts`; primitives an app composes (`queue`, `schema`, `markdown`) on no workspace package; providers (under `packages/providers/`) only on `contracts` and the provider kit, which depends on no workspace package. A tool knows nothing of the agent, the models or the other tools. A provider knows nothing of tools or of the other providers.
- L4: only apps compose packages. Only the binary (`apps/cli`) depends on apps: the server and the interface (its tests list the two again, with their `testing` features); nothing else depends on an app.
- L5: `contracts` and `agent` name no tool or provider: no browser, page, URL, OpenRouter, OpenAI, model name. The agent learns what a tool's world is like through `ToolSpec::description`. Where the run happens (the directory, the system, the mode) reaches the agent through the app's context.

Behaviour:

- A tool asks the `Gate` before an effect outside the conversation: a changed file, a command. The `Effect` summary is one line the person can judge, and it leaves nothing out (the whole command, the directory it runs in). A `Deny` goes back to the model as a failed call with its reason. Only the app answers the gate (`apps/server/src/mode.rs` and `gate.rs`); a tool never decides its own approval. A `ReadOnly` verdict runs the command in the system's sandbox, where it can change nothing, and refuses it where there is none (B3).
- A tool asks the person only through `Person`; only the app answers it (`apps/server/src/server/asking.rs`), never in `exec`. A mode is a named set of rules (ARCHITECTURE.md K5): it changes only what the gate answers and the context's `Mode:` text, never filters tools, and no code branches on its name. Rules come from the command line and the person's settings, never from the repository.
- A path the model wrote (a command's `workdir`) goes through `Workspace::readable`. Never open one without it.
- Tool content (files, command output) is untrusted data. Never let it reach a request as an instruction; mark it so where the agent reads it.
- No task- or site-specific wording in the agent's prompt or the tool descriptions. A rule earns its place by the evals, and stays general.
- Synchronous code: threads where needed, no async runtime. Tools, gates, the chat model and the person are `Send + Sync`: a turn's calls and the agents run on threads.
- The release binary stays small (under 4 MB). No dependency that is not already in the tree unless it earns its size; std first. Check `scripts/check --only size` after adding one: it fails above 5 MiB (CI's size job runs it) and warns above 4 MB.
- Exit codes (`apps/server/src/exit.rs`) and the `--json` events (`apps/server/src/output/json.rs`, `SCHEMA` in `output.rs`) are a public contract. Change them only with a schema bump and a CHANGELOG entry.

Code:

- Unit tests live beside the code, never inside it: `x.rs` ends with `#[cfg(test)] #[path = "x_tests.rs"] mod tests;` (checked by the architecture test). Fake the ports (a scripted `Chat`, a `Gate`, a `Decider`, a `View`) instead of calling a network.
- One module layout (ARCHITECTURE.md S6): a module with children is `x.rs` beside its directory `x/`, never `x/mod.rs` (clippy's `mod_module_files`). The integration tests keep `tests/common/mod.rs` and `tests/suite/mod.rs`, as cargo needs: a `tests/x.rs` is a test binary of its own.
- Every file holds at most 300 lines of working code (ARCHITECTURE.md S1, checked by `budget.rs`): a file about to pass it is split by what it does first. rustfmt defaults. Comments explain why.
- The workspace's lints are hard (`Cargo.toml`, `clippy.toml`; ARCHITECTURE.md S4): no `unwrap`, `expect`, `panic!` or `unreachable!` in working code, no prints outside the output, no `process::exit` but in `main.rs`, no `Rc`/`Cell`/`RefCell`, functions at most 100 lines, `pub` only across crates. Our own enums are matched exhaustively, no `_` arm (S10); a struct's fields are all bare `pub` or none, another crate reading a private one through a method (P3); `write!` into a String, not `push_str(&format!(..))`; a default built lazily (`unwrap_or_else`, `map_or_else`); `clone_from` over a value already there; `sort_unstable` for primitives; a parameter only read is borrowed; a method returning a changed copy is `#[must_use]`; no dependency left unused (the clippy checks). Never turn a lint off in `Cargo.toml` or for a whole module to get a change through: handle the case; the exception is an `#[expect(lint, reason = "...")]` on the smallest item, never an `#[allow]` (an exception that no longer applies fails the build); one that fires on one system or build only goes under `cfg_attr`, and a foreign enum's `_` arm names the enum in its reason. `unsafe` only in `tools` and the apps (S5).
- The project stays under 50,000 lines of working code (`apps/cli/tests/suite/repo/budget.rs`; build scripts count; tests, comments and blanks do not). This is the maximum. New code is paid for first by cutting or simplifying existing code. If a change still cannot fit, stop and ask the owner before going over: say how many lines it needs, what was already cut, why the rest cannot be, and the alternatives. Never raise the limit on your own.
- English only, in code, comments and docs.

## Tests first (TDD)

Every change is red, green, refactor:

- A bug: first a test that reproduces it, at the layer where a person meets it, run and seen failing for the right reason; then the fix; then the test passes. The red run is the proof the test can fail: never break working code to get one.
- A feature or a change of behaviour: its tests first, written from what a person does and sees (keys pressed, the screen, what the model was sent), then the code.
- Refactor only on green. A test that cannot fail, or that restates a text or a constant, is deleted, not kept.

The layers:

- unit tests beside the code (`x_tests.rs`), with the ports faked;
- screen tests in the crate: the `App` driven by keys and events and drawn into a `vt100` screen (`apps/tui/src/test_support.rs`), asserting what the screen shows, never the app's fields;
- one integration binary, `apps/cli/tests/all.rs` (`common/` for the harness, `suite/` for the tests): the real binary against a scripted model server that keeps every request (assert what the model received), and pty flows in `suite/tui/` that press keys and read the screen as a person does.

## When changing behaviour

Update these together:

- the `--help` text (clap doc comments in `apps/server/src/cli.rs`);
- `README.md`;
- `CHANGELOG.md`, under `## Unreleased`.

## Checks

`scripts/check` is the one entry point, and the same commands run everywhere: by hand, in the git hooks, in Claude Code's Stop hook, and in CI (each step of `.github/workflows/ci.yml` is `scripts/check --only NAME`). It prints a plan (each check, its scope, why), runs it in one fixed order, stops at the first failure with that check's output, and ends with one summary line; exit 0, 1, or 2 for a usage error. `scripts/check --list` names the checks, `--plan` prints the plan for the paths on stdin.

What runs when:

- Before you say a change is done: `scripts/check`. The checks the branch's change needs: everything changed since the merge base with main, plus the working tree (staged, unstaged, untracked).
- When a Claude Code agent ends its turn: `scripts/check --fast` (`scripts/hooks/claude-stop`, in `.claude/settings.json`), only when the working tree changed since a check last saw it (a stamp in `target/llx-check/`), so an unchanged tree costs nothing. A failure blocks the stop with its output; the same failing tree is let through the second time, with a note. It changes no file and stops after 240 s. `LLX_STOP_CHECK=off` turns it off.
- `git commit`: `scripts/check --fast --staged` on what the commit stages (a staged file with unstaged edits is refused: the checks read the working tree). `git push`: `scripts/check --fast` on what is pushed (since the remote's commit), in seconds; the tests, the size and the site run in CI, and by hand with `scripts/check` before a change is called done. Install both once with `scripts/setup` (`git config core.hooksPath scripts/hooks`); `--no-verify` skips them.
- CI: every check but `coverage`, a job for each group (`check`, `size`, `site`, `action`). `scripts/check --all` runs the same set locally, in CI's order; add `--coverage` before a release.

The plan comes from the changed paths alone (`scripts/checks/plan.awk`, with an example of each rule in `scripts/checks/check_test.sh`): a crate's directory brings `fmt`, `clippy` and `test` for the crate and its dependents (cargo metadata's graph; `--fast`: clippy on the crate alone); the root `Cargo.toml`, `Cargo.lock`, `clippy.toml` or `rust-toolchain.toml` the same for the workspace; a manifest, the lock or the toolchain the release `size`; anything the repository's rules (`repo`: `suite::repo`, unless the tests run them); `site/`, `docs/` or `examples/README.md` the `site` build; `action.yml` or `action/` the `action` tests; `scripts/check` or `scripts/checks/` the checks' own tests (`check-test`); a shell script `shellcheck`; a workflow, `action.yml` or an example `actionlint`, and an example the `examples` conventions; `skills/` or `.claude-plugin/` the `skill` check (the skill's frontmatter through the pinned yq, the skills the plugin's manifest lists through the pinned jq); Markdown, or a path gone, `mdlinks` (relative links and anchors); anything, `secrets` (gitleaks on every commit and on what the index and the working tree change; a finding known to be fake goes in `.gitleaksignore` by its fingerprint). `--fast` leaves out the tests, the size and the site. The linters are pinned (shellcheck, actionlint, yq, jq, gitleaks in `scripts/checks/tools.sh`): downloaded once into `~/.cache/llx/tools`, each checked against its SHA-256, never the one on the PATH.

Ratchets: the line budget and the 300-line files (`budget.rs`), the `expect`s in working code, which only fall (`code.rs`, S4), the coverage floor (`scripts/coverage.sh`), and the binary's cap (`size`: fails above 5 MiB, warns above 4 MB).

What the checks run (`scripts/checks/run.sh`), and the rest by hand:

```bash
cargo fmt --all --check                                       # fmt
cargo clippy --workspace --all-targets -- -D warnings          # clippy
cargo clippy --workspace --lib --bins -- -D warnings -D unused_crate_dependencies  # then: no dependency unused (tests left out)
cargo test --workspace            # test: unit tests, apps/cli/tests/all.rs (the binary, the layer rules)
cargo test -p localix-cli --test all -- suite::repo::                  # repo: the repository's rules alone
cargo nextest run --workspace     # the same tests under cargo-nextest
scripts/coverage.sh               # coverage: line coverage with cargo-llvm-cov; fails under 85% (raised with coverage, never lowered)
cargo build --release             # size: target/release/llx under 5 MiB, warned above 4 MB
(cd site && npm ci && npm run build)  # site: types, build, prerender (scripts/check skips it without site/node_modules)
bash action/test.sh               # action: the action's script on a stub llx
sh scripts/checks/check_test.sh   # check-test: the plan's examples, the link and skill checkers'
sh scripts/checks/mdlinks.sh      # mdlinks: relative links and anchors in every Markdown file
sh scripts/checks/skill.sh        # skill: the skill's frontmatter (YAML, name, description, allowed-tools) and the plugin manifest's skills
scripts/check --only secrets      # secrets: gitleaks (pinned) on every commit and the working tree's changes
scripts/check --only shellcheck,actionlint,examples  # the pinned linters, and the examples' conventions
```

- The log, `~/.config/llx/logs/llx.log`: what every llx process did (each call with its arguments, each failure, each task's end); `RUST_LOG=llx=debug` adds the model's turns and words, `RUST_LOG=off` keeps nothing.
- `llx -v` shows each tool result and model turn in the trace.
