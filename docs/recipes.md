# Recipes

llx has no GitHub, GitLab or review feature, and needs none: `llx decide` and `llx exec` read
stdin and write stdout, so the CLIs you already have read the input and post the answer. Each
recipe is a line or a few you can run as is; the outputs below are from real runs.

- `llx decide`: one of a few options, about a second, a fraction of a cent. No tools.
- `llx exec --mode plan`: the agent reads the code and answers. Its commands run in a
  read-only sandbox: they write no file and open no network (what they print still goes to the
  model's provider). Its patches are refused.
- `llx exec`: the `default` mode. Its commands run in a sandbox that writes only inside the
  workspace, never its `.git`, with no network (on Linux they only
  read); its patches change the workspace's files. What must reach further is
  asked at the terminal, and refused where there is none (cron, CI).
- `llx exec --allow bash`: every command runs as it is, outside the sandbox and with the
  network, without asking. The agent explores, runs the tests and confirms what it says.
- `llx exec -y`: everything runs without asking, every command as it is and every patch in the
  workspace, its `.git` included. Only on input you trust.

`exec` ends with an exit code ([the contract](reference.md#exit-codes)), and a run that did not
answer writes nothing on stdout. The commands of a pipe run at once, so the one after llx acts
whatever llx's code turns out to be: where it posts, write the answer to a file and post it
after `&&`, as below.

`decide` needs an OpenRouter key (`OPENROUTER_API_KEY`, or one saved with `/login`); `exec` runs
on any provider llx has a key or a sign-in for ([docs/ci.md](ci.md#keys-and-models)).

These recipes run at a terminal, with your own `gh` or `glab` login. In CI the job that runs
the model must not hold the token that posts: read [docs/ci.md](ci.md#the-two-job-pattern) for the
GitHub Action and the two-job pattern, and [In a workflow](#in-a-workflow) below.

## Review

A pull request reviewed, the comment posted:

```bash
gh pr diff 42 | llx exec --mode plan "Review this diff: bugs and risks only, a short markdown list with file:line." > review.md && gh pr comment 42 -F review.md
```

```text
- `lib.py:4` — A hard-coded GitHub token is committed in source; revoke it.
- `lib.py:10` — `line.split(sep)` without a maximum split raises ValueError for `key=a=b`.
- `lib.py:17` — Removing the blank-line guard sends empty lines to `parse`.
```

The same on GitLab (`glab mr note create` reads the note from stdin):

```bash
glab mr diff 42 --raw | llx exec --mode plan "Review this diff: bugs and risks only." > review.md && glab mr note create 42 < review.md
```

A verdict as well as words, for `gh pr review`; the first line writes the schema:

```bash
echo '{"type":"object","properties":{"verdict":{"enum":["approve","comment","request-changes"]},"body":{"type":"string"}},"required":["verdict","body"]}' > verdict.json
gh pr diff 42 | llx exec --mode plan --schema @verdict.json "Review this diff." > review.out &&
  gh pr review 42 "--$(jq -r .verdict review.out)" -b "$(jq -r .body review.out)"
```

## Gates

A step that fails when the answer is not the one expected (`--expect`: exit 1):

```bash
git diff origin/main... | llx decide "Does this diff add a secret, token or password?" --expect no
git diff origin/main... | llx decide "Does this change break the public API?" --expect no
gh pr diff 42 --name-only | llx decide "Does this list include a changelog?" --expect yes
```

## Triage

```bash
gh pr edit 42 --add-label "$(gh pr diff 42 | llx decide "Which label fits?" --choices bug,feature,docs,chore)"
gh issue edit 7 --add-label "$(gh issue view 7 --json title,body | llx decide "What is this issue?" --choices bug,question,feature)"
```

## When CI fails

```bash
gh run view 1234 --log-failed | llx decide "Flaky (network, timing) or a real bug?" --choices flaky,real
gh run view 1234 --log-failed | llx exec --mode plan "Why did CI fail? One paragraph, with the fix."
```

```text
real
CI failed because `test_values_with_equals` passes a value containing `=`, so `line.split(sep)`
returns more than two pieces. Fix `lib.py:8`: `key, value = line.split(sep, 1)`.
```

## An incident in CI

What kind of failure it was, in about a second (a repeated `--choices` keeps the commas of its
description):

```bash
gh run view 1234 --log-failed | llx decide "What kind of CI failure is this?" \
  --choices "test=a test failed" --choices "build=the code does not compile" \
  --choices "dependency=a package could not be installed" \
  --choices "infra=the runner, network or a service was down" \
  --choices "permissions=a token or permission was refused" --choices "timeout=a time limit was hit"
```

```text
infra
```

The whole incident, to route it: rerun what a rerun fixes, hand the rest to its owner. The
first line writes the schema:

```bash
echo '{"type":"object","properties":{"kind":{"enum":["test","build","lint","dependency","infra","permissions","timeout"]},"severity":{"enum":["low","medium","high"]},"retry":{"type":"boolean","description":"running it again is likely to pass"},"owner":{"enum":["author","platform","security"]},"summary":{"type":"string"},"next":{"type":"string"}},"required":["kind","severity","retry","owner","summary","next"]}' > incident.json
gh run view 1234 --log-failed | llx exec --mode plan --schema @incident.json "Classify this CI failure." > incident.out &&
  jq -e .retry incident.out > /dev/null && gh run rerun 1234 --failed
```

```text
{"kind":"infra","severity":"medium","retry":true,"owner":"platform",
 "summary":"Checkout failed because the repository URL returned HTTP 503: a transient service failure, not the code.",
 "next":"Retry the job; if 503s persist, check GitHub's status."}
```

In a workflow it runs when another one fails, and the rerun is a second job with its own token:
[examples/github/ci-failure.yml](../examples/github/ci-failure.yml).

## Releases

```bash
git log v1.2.0..HEAD --format=%B | llx decide "What release is this?" --choices "patch,minor=a new feature,major=a breaking change"
git log v1.2.0..HEAD --oneline | llx exec --mode plan "Release notes in markdown, grouped by kind." > notes.md && gh release create v1.3.0 --notes-file notes.md
git diff --staged | llx exec --mode plan "Describe what this diff does as a one-line conventional commit message. The message alone." > msg.txt && git commit -F msg.txt
```

## On a schedule

A script for cron, run every 15 minutes (`*/15 * * * * sh /path/to/check-log.sh`). Cron gives it a
short `PATH` and none of your shell's variables, so it names llx by its path (the installer's
`~/.local/bin`), and the key is the one saved with `/login`, or `OPENROUTER_API_KEY` set in the
crontab.

```bash
#!/bin/sh
answer=$(tail -n 200 /var/log/app.log | "$HOME/.local/bin/llx" decide "Should someone be paged now?" --choices page,ignore)
if [ "$answer" = page ]; then your-pager "the app's log needs a look"; fi
```

`your-pager` is your own command. A run that fails (exit 3: no key, no network) prints nothing
and pages no one.

## In a workflow

A workflow is the same recipe with the work split in two jobs. The first runs the model with a
read-only token and answers as data; the second posts it, with the write token and no model key.
The action runs `llx exec` for the first:

```yaml
on: pull_request
jobs:
  review:
    if: github.event.pull_request.head.repo.full_name == github.repository
    runs-on: ubuntu-latest
    timeout-minutes: 15
    permissions: { contents: read, pull-requests: read }
    outputs:
      result: ${{ steps.llx.outputs.result }}
    steps:
      - uses: actions/checkout@v7
        with: { fetch-depth: 0, persist-credentials: false }
      - uses: rsaryev/llx-cli@v0
        id: llx
        env:
          OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
          GH_TOKEN: ${{ github.token }}
          PR: ${{ github.event.pull_request.number }}
        with:
          model: anthropic/claude-haiku-5.5
          allow: bash
          prompt: |
            Review pull request #$PR with gh and git. Bugs and risks only, as a short markdown list.
  post:
    needs: review
    runs-on: ubuntu-latest
    timeout-minutes: 5
    permissions: { pull-requests: write }
    steps:
      - run: gh pr comment "$PR" --body "$BODY"
        env:
          GH_TOKEN: ${{ github.token }}
          GH_REPO: ${{ github.repository }}
          PR: ${{ github.event.pull_request.number }}
          BODY: ${{ needs.review.outputs.result }}
```

Without the action, a script step installs llx and runs `llx exec` itself ([docs/ci.md](ci.md#without-the-action)).
GitLab CI has the same two jobs: [docs/ci.md](ci.md#gitlab).

## Untrusted input

A diff, an issue or a log is written by whoever opened it, and may carry instructions. llx
hands it to the model as data, not as yours, but keep the job safe on its own:

- run the model in a job with a read-only token, and post from a second job that has no model key
  and runs nothing from the pull request;
- review and triage with `--mode plan` or `decide`, never `-y`. `--allow bash` runs every
  command outside the sandbox, as `-y` does: where the input is from strangers, allow by name
  only what needs the network (`--allow 'bash(gh pr view *)'`), and the rest runs in the sandbox;
- never run `pull_request_target` with the pull request's code checked out, and let only people
  with write access start the agent.

The reasons are in [docs/ci.md](ci.md#security).
