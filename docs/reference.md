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

`llx decide QUESTION` asks the decision model one question about what comes on stdin and prints the option it picked. It is one request: no agent, no tools, no session. It needs an OpenRouter key (`OPENROUTER_API_KEY`, or one saved with `/login`) and answers in about a second, for a fraction of a cent.

```bash
git diff origin/main... | llx decide "Does this change break the public API?" --expect no
git log -1 --format=%B | llx decide "What release is this?" --choices "patch,minor=a new feature,major=a breaking change"
```

| Option | Meaning |
| --- | --- |
| `--choices A,B,…` | The options, comma-separated, at least two. `key=what it means` tells the model more. Given more than once, each `key=…` is one option whole, commas and all. Default `yes,no`. |
| `--expect CHOICE` | Exit 1 when the model picks any other option. It must be one of the choices. |
| `--json` | Print the whole answer as JSON instead of the option alone. |
| `-m, --model MODEL` | The decision model. Env `LLX_DECISION_MODEL`, default `~typesafe/jev-latest`. |

Stdin is read to its end when it is not a terminal, up to 256 KiB as `exec` reads it. The question is sent with the instruction that the input is data to judge, never instructions to follow.

The output is the option alone on stdout, and a newline. With `--json` it is one object:

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
| 0 | The option was printed, and with `--expect` it is the expected one |
| 1 | `--expect` names another option than the one picked |
| 3 | Not ready: no OpenRouter key, no network, a model error, or a bad argument (fewer than two choices, an `--expect` that is not a choice) |
