# Changelog

## Unreleased

- An entry the session took as it was sent no longer stays shown as waiting under the composer: the queue reaches the interface in the order it changed.

## 0.2.0

The first public release.

- **`llx exec`**: one task, then exit, with an exit code a pipeline branches on (`0` done, `1` no data, `2` exhausted, `3` error, `130` interrupted), `--json` events ending with `run.completed`, and with `--schema` the answer as data matching a JSON Schema. A task is bounded by `--max-turns`, `--timeout` and `--budget`.
- **`llx`**: a minimal agent at the terminal, over five tools (`bash`, `apply_patch`, `ask`, `agents`, `jobs`). Every run is a session (`llx -c`, `llx sessions`, `llx resume`), several at once beside each other, kept by a daemon when the terminal leaves.
- **Modes**: `default` runs commands in the system's sandbox (Seatbelt on macOS, Landlock and seccomp on Linux), which writes only inside the workspace and opens no network, and asks before anything that reaches further; `plan` is read-only; `bypass` (`-y`) allows everything; `--allow` and `--deny` rules, and modes of your own in `settings.json`.
- **Models**: 1,000+ models from 200+ providers, from models.dev's catalog: a ChatGPT sign-in, OpenRouter, or a provider's key. With an OpenRouter key, a decision model picks each conversation's reasoning level and takes a second look at an answer that may leave work undone (`--no-advice` turns it off). `llx decide` asks it about stdin and prints the option it picked.
- **The GitHub Action** `rsaryev/llx@v0` (`llx agent`): installs the release, runs `llx exec` once and hands back the answer, the exit code and the spend. On a hosted Linux runner it drops `sudo` and the runner's groups for llx and its commands, redacts the step's secrets from what it hands back, and can read the instruction files of a trusted revision (`instructions-from`). Ready-made workflows in `examples/`: pull request review, `@llx` in a comment, triage, a failed run explained, a described pull request, a draft release, a daily gate, a weekly audit, a Jira work item made a draft pull request, and breaking changes to an HTTP API.
- **Install**: `curl -fsSL https://llx.sh/install | sh` on macOS (Apple silicon and Intel) and Linux (x86_64 and arm64), checked against the release's `SHA256SUMS`, each archive attested; also `mise use -g github:rsaryev/llx`, `cargo binstall --git https://github.com/rsaryev/llx llx`, `cargo install`, and the image `ghcr.io/rsaryev/llx:0.2.0`.
