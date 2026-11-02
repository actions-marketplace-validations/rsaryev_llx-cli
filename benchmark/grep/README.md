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
- `hybrid` and `hybrid-words`: `llx grep -i -e PATTERN` with the question, the pattern `rg` is given
  (below), with the decision model and without;
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
| judged | 98% | 100% | 80% | 0.89 | 4/4 | 2.09 s | $0.0053 | 4.3 |
| words | 80% | 93% | 43% | 0.68 | 0/4 | 0.19 s | 0 | 11.3 |
| rg | 46% | 67% | 35% | 0.25 | 2/4 | 0.03 s | 0 | 19.2 |

A file that does the thing with none of the question's words (`concept-1`'s `doctor.rs`, which
runs a shell through `tools::shell::query`) is found through the module it shares with a file
that answers: the second round, which reads again the files the first doubted or left below its
margin where they name the module, and shows what it keeps after the first round's. Every file
an answer must name is now in the first ten; `extras.rs`, found that way, comes sixth. What it
still misses is the precision of a vague question (`flow`), where the decision model leans to yes
on every file near the subject. Two runs of the same build differ by about 0.02 in MRR and a
second in time: the decision model's answers and the network both vary.

With the pattern an agent would type first (`rg` in each question) given as `-e` beside the
question, on the same snapshot:

| Mode | R@5 | R@10 | P@5 | MRR | none | time | cost | KB read |
| --- | --- | --- | --- | --- | --- | --- | --- | --- |
| hybrid | 92% | 92% | 77% | 0.88 | 4/4 | 2.31 s | $0.0020 | 4.0 |
| hybrid-words | 63% | 81% | 49% | 0.62 | 2/4 | 0.21 s | 0 | 13.9 |

The pattern ranks rg's own matches far better than rg does (MRR 0.62 against 0.25 without a
model), and with the model costs about a third of the question alone; but it finds no file it
does not match, so where the first pattern misses (`flow`, `cross`), the question alone finds more.
Give a pattern where a string the answer must have is known; else ask the question alone.

## How deep the decision model reads

`bun benchmark/grep/run.ts --ranks` asks no model: it prints where each file an answer must name
stands in the ranking by words, the order the decision model reads files in, so the depth of
`--judge` can be chosen without paying for a run. At this commit the deepest one is at 66
(`concept-1`'s `extras.rs`; its `doctor.rs` has none of the question's words and is reached only
through links), every other within 21: `--judge 80` reads past each, and 40 would leave
`extras.rs` to the second round.
