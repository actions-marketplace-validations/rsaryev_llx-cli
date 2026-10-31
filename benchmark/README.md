# Benchmark

A deterministic check of llx as an agent, with Codex beside it to compare: the agent implements
a small TypeScript project for Bun against acceptance tests written in advance, which it may not
change. The tests score the work, so a run is judged by what passes, not by what the agent says.
Run it often, after a change to the loop, the prompt, the tools or a provider and in between, and
read the rows of [HISTORY.md](HISTORY.md).

## The tasks

- `tasks/evalkit` (238 tests, the default): four libraries specified exactly in its README: an
  expression language, a SQL-like query over arrays, unified diffs and patches, and cron
  schedules. Hard enough that runs differ.
- `tasks/shopkit` (69 tests): six small modules (money, a cart, an LRU cache, CSV, semver, a
  router). Both agents solve it; a quick check that nothing broke.

Each task has `project/`, what the agent is given (the README, the tests, stubs that throw), and
`ref/`, a reference implementation that passes every test. `ref/` is never given to the agent.
`bun benchmark/run.ts --task <task> --check` checks a task after an edit: the reference passes
everything, and the stubs next to nothing.

## A run

```bash
cargo build --release
bun benchmark/run.ts --note "after the prompt change"      # evalkit, llx, one run, Jev on
bun benchmark/run.ts --task shopkit
bun benchmark/run.ts --agent codex                          # Codex on the same task
bun benchmark/run.ts --agent llx --agent codex              # both, at the same time
bun benchmark/run.ts --agent base=llx:/path/to/old/llx --agent llx   # two builds of llx
```

`--agent [label=]kind[:path]` names an agent: `llx` (this repository's `target/release/llx`) or
`codex` (the `codex` on the `PATH`, signed in to ChatGPT), with a label where two of a kind run
together. Each run copies `project/` into a fresh git repository in a temporary directory (outside
this one) and starts the agent there, every effect allowed:

- llx: `llx exec -y`, with `apply_patch` denied on the tests, `package.json` and `tsconfig.json`,
  and with its advisor (Jev, ARCHITECTURE.md C9: when the model says it is done, it judges whether
  the task is, and if not gives it a line to go on with, twice a task at most; it needs an
  OpenRouter key, `--no-advice` runs without it);
- Codex: `codex exec --dangerously-bypass-approvals-and-sandbox --ignore-user-config` (the
  person's `config.toml` left out, the sign-in kept). It has no deny rule; the prompt says the
  same to both.

What the agents think with, and how a run is bounded:

- `--provider`, `--model` and `--reasoning`: default `openai`, `gpt-6-luna`, `low`; Codex takes
  the model and the level alone;
- `--timeout S`: each run's limit, 1200 s by default (llx's `--timeout`; both agents are killed
  60 s past it, which is Codex's only bound);
- `--parallel N`: at most N runs at once (4);
- `--repeat N`: N runs of each agent (1);
- `--no-advice`: llx without Jev.

After a run, the fixed files are put back from the pristine copy, so an edit through the shell
cannot help, and a change to them is noted as tampered; then `bun test` is the score. It needs
`bun` and `git` on the `PATH`, and `codex` for Codex.

## What a run is judged by

The task as a whole, from the prompt to the agent's last word:

- **solved**: every test passed (and how many did, of how many);
- **wall s**: the seconds the task took, split for llx into the model's (`model s`) and the rest
  (`the rest s`: llx and the tools, the commands, `bun test`);
- **turns**: the model's requests;
- **Jev**: how often llx's advisor sent the model back to work;
- **cost $**: what the run would cost at the model's price in the catalog (a ChatGPT sign-in pays
  nothing per run, but the price is how runs compare);
- **cache %**: the share of input tokens read from the prompt cache;
- the tokens in and out, the calls, the commands that failed, the patches and those refused, and
  whether the agent changed the fixed files (`tampered`).

## The results

Everything a run leaves is kept, in git, to read again later:

```text
results/<date>_<time>-<task>/
  run.json           what ran (the agents, their versions, the machine, its load) and each run's measures
  summary.md         the same as tables
  <agent>-<n>/       one run's logs, compressed:
    events.jsonl.gz    the agent's own events (llx's --json, Codex's --json)
    session.jsonl.gz   Codex's session file: each model request's tokens
    stderr.txt.gz      what it wrote on stderr (llx's advisor's words among them)
    test.txt.gz        `bun test` after the fixed files were put back
    src.tar.gz         the code it wrote
    answer.md          its last words
```

The cost is worked out the same way for every agent, from each model request's tokens at the
model's price in llx's catalog (`packages/providers/models`).

`HISTORY.md` is made again from every `run.json` after each run (`bun benchmark/run.ts --history`
makes it alone): a row per agent per run, with whether it solved the task and its medians. A run
is one by default (`--repeat N` for more); the history over time is what to read.

Record on the Mac with nothing heavy running beside it (a build, Docker); the load is in each
summary.
