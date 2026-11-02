# The search benchmark

`llx grep` against questions about this repository whose answers are known, by group, with `rg`
beside it as an agent would first type it. Run it after a change to `llx grep`'s words, ranking,
parts or judging, and keep a change only when the table says so.

```bash
cargo build --release
bun benchmark/grep/run.ts                        # every mode: words, judged, rg
bun benchmark/grep/run.ts --mode words           # no network, no cost
bun benchmark/grep/run.ts --only concept --label "after the stems"
bun benchmark/grep/run.ts --record               # and a line in HISTORY.md
```

## The questions

[queries.json](queries.json) holds 24 questions about the repository at one commit (`commit`),
four in each group, each with the files an answer must name (`must`) and those it may name
without being wrong (`ok`, globs), every one checked against the code by hand:

| Group | What it asks | Example |
| --- | --- | --- |
| `symbol` | Where a name is defined, or a thing with a name the question half gives | "where is READY_LINE defined" |
| `concept` | Something done under another name, or behind a wrapper | "which files start a child process" (some through `tools::shell::query`) |
| `flow` | How a behaviour happens | "how does Ctrl-C stop the running task but not the whole process" |
| `cross` | Every place that does something | "files created with owner-only 0600 permissions" (`0o600` in the code) |
| `absent` | Something the code does not have: the right answer is no file | "where is the GraphQL schema defined" |
| `config` | Scripts, workflows, configuration | "where the linters are pinned to a version and checked against a checksum" |

The repository at `commit` is put in a fresh git repository in the temporary directory, so git
lists what it did then whatever has changed since. 1,200 files, a catalog of several hundred
model descriptions among them: a search must not drown in them.

## The modes

- `words`: `llx grep --lexical`, the ranking by the question's words alone;
- `judged`: `llx grep`, the decision model reading the best of them (an OpenRouter key; about
  $0.11 a run);
- `rg`: `rg -c -i PATTERN`, the files by their count of matches, `PATTERN` the one an agent would
  type first (`rg` in each question).

## What a run is judged by

For each question with an answer: **R@5** and **R@10**, the share of its `must` files among the
first 5 and 10 named; **P@5**, the share of the first 5 that are `must` or `ok`; **MRR**, the
reciprocal of the place of the first `must` file. For a question with none: **none**, whether
nothing was named. And the time, the cost, and **KB read**: what an agent reads of the answer
(`llx grep`'s paths and lines; `rg -n`'s output).

`results/<date>_<time>[-label].json` keeps every answer, with each file's score: enough to try a
threshold again without asking the model again. [HISTORY.md](HISTORY.md) has a line per recorded
run.

## Where it stands

At `f19ba1cb`, on an idle macOS arm64:

| Mode | R@5 | R@10 | P@5 | MRR | none | time | cost | KB read |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| judged | 98% | 98% | 80% | 0.89 | 4/4 | 3.89 s | $0.0053 | 4.1 |
| words | 80% | 93% | 43% | 0.68 | 0/4 | 0.19 s | 0 | 11.3 |
| rg | 46% | 67% | 35% | 0.25 | 2/4 | 0.03 s | 0 | 19.2 |

A file that does the thing with none of the question's words (`concept-1`'s `doctor.rs`, which
runs a shell through `tools::shell::query`) is found through the module it shares with a file
that answers: the second round, which costs about a second. What it still misses: a file so
found that the model still finds less likely than the best by more than 0.25 (`extras.rs`, at
0.70), and the precision of a vague question (`flow`), where the decision model leans to yes on
every file near the subject.
