# Reference

How a task ends, the exit codes, the `--json` events and `llx decide`. Both the exit codes and the events are a public contract.

## Exit codes

A task ends when the model replies with a message and no tool call: that message is the `answer`. A reply the output limit cut off is not one: the model goes on from it, and a fourth in a row that did nothing ends the task with `error` (exit 3). An answer given while an agent's answer, or in `exec` a monitor's lines, is still to come waits for it.

With `--schema`, the model is offered a `result` call with every request, and may end the task with it; when it answers in words instead, llx asks it once more, on the same conversation and with the same tools (so the prompt cache holds), for the `result` call: forced over OpenAI's APIs, asked for in words elsewhere (hosts refuse a forced call while their model thinks), and asked again when the model answers in words. Its data is what stdout and `-o` get, and `run.completed`'s `data`; the answer goes to stderr. Data that does not match the schema goes back to be fixed, up to two times; then the run ends `no_data`.

The data is checked against the schema as its draft says (`$schema`: 4, 6, 7, 2019-09 or 2020-12; 2020-12 when it names none): every keyword of validation and every applicator, `unevaluatedProperties` and `unevaluatedItems` among them, `$ref` by pointer, `$id` and `$anchor` (and into another document only where one is given), numbers equal by value (`1` is `1.0`) and bounds exact (`multipleOf` in decimal, integers past 2^53), lengths in characters. `format` is held to, not only noted: `date-time`, `date`, `time`, `duration`, `email`, `hostname`, `ipv4`, `ipv6`, `uri`, `uuid` and the rest of the draft's but `idn-hostname` and `idn-email`; `pattern` as ECMA-262 reads it. What is not checked (a lookaround or a back-reference in a pattern, those two formats, a `$ref` to a document not given) the model is still told, and llx names it on stderr as the run starts. The checker passes every test of the official JSON-Schema-Test-Suite it runs, and is faster than the `jsonschema` crate on the cases of `packages/schema/README.md`.

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
