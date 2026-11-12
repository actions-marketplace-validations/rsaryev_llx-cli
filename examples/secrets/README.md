# Secrets kept out of an agent's context, a commit and a pull request

A secret an agent has read is a secret it can write: into the code, a commit message, a log. Two
levels keep one from getting that far, and each is a short shell script:

1. **Patterns**, on your machine: the formats tokens are issued in (`ghp_…`, `sk-…`, `AKIA…`, a
   private key's block) and your company's own. A match is replaced by `[REDACTED:NAME]`, so
   whoever reads the text sees that a key was there and never its value. No model, no network.
2. **A decision model**, for what no format gives away: a password in a config, a token in a URL.
   `llx decide` answers one question, "is there a secret here?", in about a second, $0.0003 for
   20 KB of text.

The second level sends the text to the decision model's provider, a secret no pattern knew with
it. A text a pattern matched is never sent, and in the agent's hook what the patterns masked is
not in what is asked. Where no text may leave at all, `LLX_SECRETS_LEVEL=patterns` keeps the first
level alone.

| Where | What runs | What happens to a secret |
| :-- | :-- | :-- |
| Before a command's output reaches an agent | [claude-hook.sh](claude-hook.sh), or `mask.sh` in your own tool | A known format is masked; anything else the model calls a secret withholds the whole output |
| Before a commit | [diff.sh](diff.sh) as git's `pre-commit` hook | The commit is refused, with the file and the line |
| In CI | [diff.sh](diff.sh) on what the pull request adds | The job fails |

## Files

| File | What it is |
| :-- | :-- |
| [patterns.txt](patterns.txt) | The formats: a name and a regular expression a line. Add your own. |
| [mask.sh](mask.sh) | A filter: stdin to stdout with every match replaced by `[REDACTED:NAME]`. The first level alone. |
| [check.sh](check.sh) | The check: `check.sh < TEXT` or `check.sh FILE…`. Prints a line per finding (never the value) and exits 0 (no secret found), 1 (a secret) or 2 (the patterns found none and the model could not be asked). |
| [diff.sh](diff.sh) | `check.sh` on the lines a change adds: what is staged, or `diff.sh origin/main...HEAD`. A removed line is not read, so the commit that takes a secret out goes through. |
| [claude-hook.sh](claude-hook.sh) | A Claude Code hook: masks, then checks, what a command printed before the model reads it. |
| [demo/](demo) | `run.sh`, the first two on a throwaway repository, and its recorded run, [output.txt](demo/output.txt). |

They need a POSIX shell, `git`, and `llx` signed in with an OpenRouter key for the second level
(`llx decide --check` says whether one is there); the hook also needs `jq` 1.6 or later. Copy this
folder into your repository as `ci/secrets/`.

## Before a commit

```bash
printf '#!/bin/sh\nsh ci/secrets/diff.sh || [ $? = 2 ]\n' > .git/hooks/pre-commit
chmod +x .git/hooks/pre-commit
```

A secret refuses the commit. `|| [ $? = 2 ]` lets one through when the model could not be asked
(no network, no key): the patterns were still checked, and the hook says so. Leave it out to
refuse then too. `git commit --no-verify` skips the hook.

```text
$ git commit -m "deploy script, database settings"
secret	GITHUB_TOKEN	deploy.sh:1
secret	model	config/app.yml:3-5
```

A pattern names the line; the model names the lines it read, here the three the commit adds to
`config/app.yml`. A finding of the model's is a judgment: when it is wrong, the line is usually
one a reader would stop at too (a long random value under a name like `secret`), and the commit
goes through with `--no-verify`.

## In CI

GitHub: a workflow in `.github/workflows/`, with the action installing llx and the provider key
as the secret `OPENROUTER_API_KEY`. It skips pull requests from forks, which get no secrets:

```yaml
name: secrets
on: pull_request
permissions: {}
jobs:
  check:
    if: github.event.pull_request.head.repo.full_name == github.repository
    runs-on: ubuntu-latest
    timeout-minutes: 5
    permissions:
      contents: read
    steps:
      - uses: actions/checkout@v7
        with: { fetch-depth: 0, persist-credentials: false }
      - uses: rsaryev/llx-cli@v0
        with: { install-only: 'true' }
      - env:
          OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
          BASE: ${{ github.base_ref }}
        run: |
          sh ci/secrets/diff.sh "origin/$BASE...HEAD" || {
            code=$?
            [ "$code" = 2 ] || exit "$code"
            echo "::warning::the model could not be asked: only the patterns were checked"
          }
```

Any other CI runs the same line after a fetch of the base branch, with llx installed
(`curl -fsSL https://llx.sh/install | sh`).

The scripts are the pull request's own, so its author can change them: the job catches a secret
committed by mistake, not one hidden on purpose. And a secret that reached a pull request is
already in the repository's history: rotate it, whatever the job says.

## Before a command's output reaches an agent

In Claude Code, [claude-hook.sh](claude-hook.sh) runs after each `Bash` call, in
`.claude/settings.json`:

```json
{"hooks": {"PostToolUse": [{"matcher": "Bash", "hooks": [
  {"type": "command", "command": "sh \"$CLAUDE_PROJECT_DIR\"/ci/secrets/claude-hook.sh"}]}]}}
```

What the patterns know is masked where it stands; when the decision model sees a secret in what
is left, the whole output is withheld and the model is told why:

```text
> Run `sh show-env.sh` and reply with what it printed.
USER=app
GH_TOKEN=[REDACTED:GITHUB_TOKEN]
PORT=8080

> Run `sh db-settings.sh` and reply with what it printed.
The command printed a secret, so its output was withheld and I can't reproduce it here.
```

The command has already run: the hook changes only what the model reads. It reads the output of
`Bash`, not a file opened with the `Read` tool: deny those by a permission rule (`Read(.env*)`).
Each call waits for the decision model, about a second; `LLX_SECRETS_LEVEL=patterns` in the
hook's environment keeps the masking alone, with no wait.

In an agent of your own, the same two lines go where a tool's output is handed to the model:

```bash
output=$(run_the_tool | sh ci/secrets/mask.sh)                         # known formats, masked
printf '%s\n' "$output" | sh ci/secrets/check.sh >/dev/null || [ $? = 2 ] || output='[withheld: a secret]'
```

## Results

**The demo** ([output.txt](demo/output.txt)): the token is masked in a command's output; a commit
that adds a token and a password is refused, the token by its pattern and the password by the
model; the same commit with the values taken from outside goes through.

**The question** was tried on eight short texts before it was kept: a password in YAML, a random
`SESSION_SECRET`, a bearer token in a `curl` line and a password in a database URL came back
`yes` (0.93 to 1.0); placeholders (`API_KEY=your-key-here`), code that reads a key from the
environment, plain code, and a page of commit hashes, checksums and UUIDs came back `no`. One
password planted in the middle of 20, 60 and 150 KB of source code was found each time, and the
same code without it passed.

**The hook** was run in Claude Code 2.1.322 (the transcript above), and the CI job in a test
repository: a pull request that added a token and a password failed it, and passed once they were
taken out.

## Adapt

- **Your formats**: a line in [patterns.txt](patterns.txt), `NAME` and a POSIX extended regular
  expression.
- **The question**: `question=` in [check.sh](check.sh). Say what counts as a secret for you (an
  internal host name, a customer's id) and what does not.
- **A model of your own**: `llx decide` asks OpenRouter's decision models (`LLX_DECISION_MODEL`).
  To keep every text on your machine, stay on the first level, or put a model you host behind the
  second: `check.sh` needs one command that answers yes or no.
