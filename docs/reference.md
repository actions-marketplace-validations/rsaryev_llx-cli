# Reference

How a task ends, the exit codes, the `--json` events and `llx decide`. Both the exit codes and the events are a public contract.

## Exit codes

A task ends when the model replies with a message and no tool call: that message is the `answer`. A reply the output limit cut off is not one: the model goes on from it, and a fourth in a row that did nothing ends the task with `error` (exit 3). An answer given while an agent's answer, or in `exec` a monitor's lines, is still to come waits for it.

With `--schema`, llx then asks the same model once more, on the same conversation, for the `result` call (`tool_choice` forced to it). Its data is what stdout and `-o` get, and `run.completed`'s `data`; the answer goes to stderr. Data that does not match the schema goes back to be fixed, up to two times; then the run ends `no_data`.

| Code | Meaning |
| --- | --- |
| 0 | Done: the model answered, and with `--schema` its data matched |
| 1 | Only with `--schema`: no data matched it |
| 2 | Turns, time or budget ran out before the model answered |
| 3 | The environment or the command line is not ready: no key, no network, a model error, a bad argument |
| 130 | Interrupted: Ctrl-C, SIGTERM or SIGHUP |

With `--json`, a closed stdout (`llx exec --json ... | head -1`) ends the run quietly at its next turn, with status `interrupted` and exit 0. Without it nothing is written to stdout until the answer, so the run goes on to its end and then exits 0.

## JSON events

`--json` (with `exec` or `-p`) prints one event per line on stdout, in the order things happen. Every event has a `type`; `tool.call`, `tool.result` and `turn` also have `ms`, the time since the task started.

| Event | Fields |
| --- | --- |
| `run.started` | `schema` (7), `goal`, `session`, `model`, `workspace` |
| `tool.call` | `id`, `name`, `arguments` |
| `tool.result` | `id`, `name`, `failed`, `fatal`, `output` |
| `turn` | One model turn: `model`, `input_tokens`, `cached_tokens`, `output_tokens`, `cost_usd`, `model_ms` |
| `run.completed` | `status` (`done`, `no_data`, `exhausted`, `error`, `interrupted`), `exit_code`, `answer`, `data` or `data_error` (with `--schema`), `error`, `session`, `usage` {`agent` {`model`, `calls`, `input_tokens`, `cached_tokens`, `output_tokens`, `cost_usd`, `ms`}, `cost_usd`}, `duration_ms` |

The turns of the agents a task starts come as `turn` events too, and count in `usage`. A breaking change to the events or the exit codes bumps `schema` and is listed in [CHANGELOG.md](../CHANGELOG.md).

## llx decide

`llx decide QUESTION` asks the decision model one question about what comes on stdin and prints the option it picked; `llx decide QUESTION FILE…` asks it about each file on its own. It is one request: no agent, no tools, no session. It needs an OpenRouter key (`OPENROUTER_API_KEY`, or one saved with `/login`) and answers in about a second, for a fraction of a cent.

```bash
git diff origin/main... | llx decide "Does this change break the public API?" --expect no
git log -1 --format=%B | llx decide "What release is this?" --choices "patch,minor=a new feature,major=a breaking change"
llx decide "Bug report or feature request?" --choices "bug,feature" issues/*.txt
```

| Option | Meaning |
| --- | --- |
| `--choices A,B,…` | The options, comma-separated, at least two. `key=what it means` tells the model more. Given more than once, each `key=…` is one option whole, commas and all. Default `yes,no`. |
| `--expect CHOICE` | Exit 1 when the model picks any other option. It must be one of the choices. |
| `--json` | Print the whole answer as JSON instead of the option alone. |
| `-m, --model MODEL` | The decision model. Env `LLX_DECISION_MODEL`, default `~typesafe/jev-latest`. |
| `FILE…` | Ask about each file instead of stdin, eight at once. |
| `--check` | Ask nothing: exit 0 when decide can run (an OpenRouter key is set or saved), 3 when it cannot. No network, no cost: for a script or a plugin that offers decide only then. |

Stdin is read to its end when it is not a terminal, up to 256 KiB as `exec` reads it. The question is sent with the instruction that the input is data to judge, never instructions to follow.

Each file is read the same way, and stdin is not read when files are given.

The output is the option alone on stdout, and a newline; with files, a line each in the order given, `CHOICE<TAB>FILE`. With `--json` it is one object (with files, one a line, with its `file`):

| Field | Meaning |
| --- | --- |
| `choice` | The option picked |
| `confidence` | The model's own confidence in it |
| `probabilities` | Each option and its probability |
| `model` | The decision model that answered |
| `cost_usd` | What the request cost |
| `millis` | How long it took |

| Exit code | Meaning |
| --- | --- |
| 0 | The option was printed, and with `--expect` it is the expected one (with files, for every file); with `--check`, decide can run |
| 1 | `--expect` names another option than the one picked (with files, for any file) |
| 3 | Not ready: no OpenRouter key, no network, a model error, or a bad argument (fewer than two choices, an `--expect` that is not a choice); with files, a file that could not be read or answered, named on stderr while the others are still printed |

## llx grep

`llx grep QUESTION [PATH…]` searches the code by what a question means and prints the files that answer it, best first, each with its parts that matter. It is a search for a person or an agent that does not know the exact name, or for whom a word would match too much; for an exact name or text, `rg` is faster.

```bash
llx grep "where is a failed HTTP request sent again"
llx grep "which files start a child process" src/ lib/
llx grep -l "where the release binary's size limit is checked"     # the paths alone
```

How it works:

1. **The files.** Under each path (the current directory when none): in git, the files git tracks or would (its ignore rules kept); elsewhere every file but hidden ones, `target`, `node_modules` and `__pycache__`. A file over 1 MiB, one that is not text, a lock file, a minified bundle and a source map are left out.
2. **The words.** The question's words without those every question has (`where`, `the`, `file`), and each name's parts as code writes them (`parse_rows`, `parseRows` and `ParseRows` are each `parse` and `rows` too); a word finds its inflections (`renames` finds `rename` and `renamed`, `failed` finds `failure`), and a number its spellings (`0600` finds `0o600`).
3. **The ranking.** BM25 over each file's words, a rarer word counting more; a word in the path counts more, and on a definition's line more still when the question asks where something is defined; a file with more of the question's words comes before one that repeats a few; a test, and a file under `data/`, `vendor/` or the like, counts half.
4. **The decision model** (where an OpenRouter key is set, unless `--lexical`). It reads the best files by their words, 24 at once, up to `--judge` of them (all of them, but the tests, when there are no more than half again as many), each as its start and its best parts, and says how likely each is to be one the answer names, and of a long one which of its parts shows best what was asked (one request: the text is read once). The files at least 0.5 likely, and within 0.25 of the most likely, are the answer, most likely first; below that, down to 0.2, are named as unsure. Without a decision model, the ranking by words is the answer, `--limit` files of it.
5. **The links** (with the decision model, unless the question asks where something is defined). A file that does what was asked through another module's function (`shell::query` for "starts a child process") may have none of the question's words. So the modules the kept files name (each `a::b` path's prefixes, and what an `import`, `from` or `require` brings in) are looked for in the files not read yet and in those the model doubted or left below its margin, the rarer modules counting more (a module a quarter of the files name links nothing); up to 12 of them are read again where they name those modules, the model told how each was reached. A file this second round keeps comes after those the first kept, held to the same margin from its own round's best.
6. **The parts.** Around each line with the question's words, the largest whole unit that holds it and is no longer than 40 lines (a function, a type, a section of a config), found by indentation with the comments above it, so it works in any language; else the lines around it. The part the decision model picked comes first; then the parts with more of the rarer words in fewer lines, within `--lines` in all.

| Option | Meaning |
| --- | --- |
| `PATH…` | Files and directories to search; the current directory when none. |
| `-l, --files` | The paths alone, best first, a line each. |
| `--json` | One JSON object (below). |
| `-n, --limit N` | At most N files shown with their lines; the rest are named. Default 10. |
| `--lines N` | At most N lines shown in all, the best file given the most. Default 150. |
| `--lexical` | By the words alone, even where a decision model is there: no network, no cost. |
| `--judge N` | At most N files read by the decision model, the best by their words first. Default 80. |
| `-m, --model MODEL` | The decision model. Env `LLX_DECISION_MODEL`, default `~typesafe/jev-latest`. |

The output is each file's path and score, then its parts, each line with its number; `..` between parts apart; then `also:` the files past `--limit`, and `unsure, read to check:` those the model doubted. On stderr, one line says how many files answer of how many read, how they were picked, the time and the cost. With `--json`:

| Field | Meaning |
| --- | --- |
| `files` | Each file that answers: `file`, `score` (the model's probability, or the words' score against the best file's), `blocks` (`start` and `end`, 1-based lines, and `text`) |
| `unsure` | The files the model doubted, each with its `file` and `score` |
| `by` | `judged` (the decision model picked them) or `words` |
| `model`, `cost_usd` | The decision model and what its requests cost |
| `read`, `millis` | How many files were read, and how long it took |

| Exit code | Meaning |
| --- | --- |
| 0 | At least one file answers |
| 1 | No file answers: with the decision model, it judged none likely; by the words, none has any of them |
| 3 | A path that is not there, or a question with nothing to look for |

What it costs: one request to the decision model for each file it reads, about $0.005 and 2 to 3 s a question on this repository's 1,200 files, 24 files read at once ([benchmark/grep](../benchmark/grep/README.md)). By the words alone it costs nothing and takes a fraction of a second.
