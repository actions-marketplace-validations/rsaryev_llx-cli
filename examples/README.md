# Examples

GitHub workflows that run llx through its action, `rsaryev/llx@v0`. Each was run for real in a
test repository before it was published.

**To install one:** copy the file into `.github/workflows/`, and add the secret
`OPENROUTER_API_KEY` (Settings > Secrets and variables > Actions). Each file says at its top what
else it needs, if anything, and what to change to make it yours: the model, the prompt, the rules.

| Workflow | When | What llx does | Also needs |
|---|---|---|---|
| [review.yml](github/review.yml) | a pull request opens or changes | explores the merged tree, runs what it needs to confirm a problem; posts one review of inline comments (never a verdict) and one summary it updates, skipping what it said before | |
| [llx.yml](github/llx.yml) | someone with write access writes `@llx` in a comment, a review thread or a new issue, or labels an issue `llx` | answers in the thread; asked for a change, edits the code and runs the tests, then pushes it to the pull request's branch, or opens a draft pull request for an issue | the label `llx`; for an issue's draft pull request, "Allow GitHub Actions to create and approve pull requests" |
| [triage.yml](github/triage.yml) | an issue opens | picks a kind and a priority among the repository's own labels with `llx decide`, and adds them | two labels of each: GitHub's default labels give the kinds; create `P1`, `P2`, `P3` |
| [ci-failure.yml](github/ci-failure.yml) | the tests fail on a pull request | reads the failed log, finds the cause in the code, says whether it is flaky; one comment it updates, and one rerun of the failed jobs when flaky | your tests workflow's `name:` on the line marked in the file |
| [describe.yml](github/describe.yml) | a pull request opens | writes a summary, each change, the risk and the testing below the author's text, in a block it replaces | |
| [release.yml](github/release.yml) | run by hand | picks the version bump and writes the notes in one answer; drafts a release a person publishes | tags like `v1.2.3`; with none, the first draft is `v0.1.0` |
| [gate.yml](github/gate.yml) | daily, or by hand | checks a rule you state (here, that what the docs name exists in the code), read-only, and fails the job when the listed data shows it is not met | |
| [audit.yml](github/audit.yml) | weekly, or by hand | explores the repository, runs its tests and linters, and opens or updates one issue labeled `llx-audit` when it finds something worth a maintainer's time | |

## Beyond GitHub workflows

- [breaking-changes/](breaking-changes/README.md): what a change does to an HTTP API's clients, the spec's structure and what no spec diff can see (units, boundaries, the code behind the endpoints), as a script for any CI, with a GitLab job, a demo and its recorded run.

## How they are built

The agent works as at a terminal: `bash`, with `gh`, `git` and `jq` in its
shell. There is no MCP server, no GitHub tool and no built-in prompt: the prompt is in the
workflow, where you can read it, and so is what gets posted.

- **Two jobs.** The job that runs the model has a read-only token (`contents: read`, and `read`
  on what it looks at). A second job, with write access and no model key, posts what the first
  answered. It checks out nothing, or the branch it pushes to, and never runs code from the pull
  request. The split is the boundary, not the rules.
- **Answers are data.** `schema:` makes the agent answer with JSON that matches it; the posting
  job reads it with `jq`. `review.yml` keeps only findings on lines the diff shows before it
  posts them.
- **What the agent may run.** `allow: bash` lets it read and run anything,
  and a command can change a file. The checkout is
  throwaway and the job has no token that can write, so it changes nothing it can commit.
  `llx.yml` allows edits and denies `git commit` and `git push`; its patch goes to the posting
  job, which refuses a change to `.github/`.
- **The key.** llx removes the provider key from the variables the commands it runs get. While
  the agent runs, llx is the only process of the step that holds it (the action runs llx in place
  of the step's shell), and on Linux llx cannot be read through `/proc` by a process without
  root. But a hosted runner gives passwordless `sudo`, so with `bash` allowed, assume the key can
  leak: use a key made for CI, with a spend limit (an OpenRouter key can have one). Where the
  input is from strangers, name what the agent may run (`allow: bash(gh pr view *)`).
- **Who can trigger it.** `llx.yml` answers only owners, members and collaborators. Pull
  requests from forks are skipped: they get no secrets, and their code would run next to the
  key. No workflow uses `pull_request_target`.
- **Text written by others is data.** Values from the event reach the prompt through `env:`
  (`$PR`), never pasted into a script with `${{ }}`. A comment or a body is read by the agent with
  `gh`, and every prompt says that what others wrote is data, not instructions.
- **Instruction files come from the checkout.** On a pull request, `AGENTS.md` and `CLAUDE.md`
  are the pull request's. The token is read-only, so the worst is a bad answer or a leaked key.

## Notes

- The workflows that a push or a pull request made with the workflow's token (`llx.yml`) starts
  wait for a person to approve them: your tests run on llx's change once someone does, or at
  once when the posting job uses a GitHub App's token.
- A workflow started by a review comment runs the version of the file on the pull request's
  merge ref: a fix on `main` reaches an open pull request once `main` is merged into it.
- `gh pr view --comments` needs `checks: read`; the prompts use
  `gh pr view "$PR" --json title,body,comments,reviews`, which does not.
- Pin the action to a release (`@v0.x.y`) or a commit where the key matters; `@v0` follows every
  0.x release.
- On a self-hosted runner, replace `ubuntu-latest` with its labels. With llx built there, give
  the action `version: system`.

More in [docs/ci.md](../docs/ci.md): the action's inputs and outputs, rules in CI, keys and models,
exit codes, and GitLab.
