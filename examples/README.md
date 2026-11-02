# Examples

Ready-made uses of llx, by where they run. Each was run for real before it was published.

| Where | Example |
|---|---|
| GitHub Actions | [the workflows below](#github-workflows), through the action `rsaryev/llx@v0`: review, `@llx` in a comment, triage, a failed run explained, a described pull request, a draft release, a daily gate, a weekly audit, a Jira work item made a draft pull request |
| Any CI | [breaking-changes/](breaking-changes/README.md): what a change does to an HTTP API's clients, the spec's structure and what no spec diff can see (units, boundaries, the code behind the endpoints), as a script with a GitLab job, a GitHub job, a demo and its recorded run |

## GitHub workflows

Each runs llx through its action, `rsaryev/llx@v0`, and was run in a test repository first.

### Install one

1. Pick a workflow in the table below, and copy its file into your repository's
   `.github/workflows/`.
2. Add the secret `OPENROUTER_API_KEY` (Settings > Secrets and variables > Actions): a key made for
   CI, with a spend limit (an OpenRouter key can have one).
3. Do what its "Also needs" column says, if anything.
4. Read the file's header: it says what the workflow does, what it needs, and what to change to
   make it yours (the model, the prompt, the rules).

From a clone of your repository, with [`gh`](https://cli.github.com) signed in, the review is:

```bash
mkdir -p .github/workflows
curl -fsSL -o .github/workflows/llx-review.yml \
  https://raw.githubusercontent.com/rsaryev/llx/main/examples/github/review.yml
gh secret set OPENROUTER_API_KEY   # paste a key made for CI
git add .github/workflows/llx-review.yml && git commit -m "llx reviews pull requests" && git push
```

| Workflow | When | What llx does | Also needs |
|---|---|---|---|
| [review.yml](github/review.yml) | a pull request opens or changes | explores the merged tree, runs what it needs to confirm a problem; posts one review of inline comments (never a verdict) and one summary it updates, skipping what it said before | |
| [llx.yml](github/llx.yml) | someone with write access writes `@llx` in a comment, a review thread or a new issue, or labels an issue `llx` | answers in the thread; asked for a change, edits the code and runs the tests, then pushes it to the pull request's branch, or opens a draft pull request for an issue | the label `llx`; for an issue's draft pull request, "Allow GitHub Actions to create and approve pull requests" |
| [triage.yml](github/triage.yml) | an issue opens | picks a kind and a priority among the repository's own labels with `llx decide`, and adds them | two labels of each: GitHub's default labels give the kinds; create `P1`, `P2`, `P3` |
| [ci-failure.yml](github/ci-failure.yml) | the tests fail on a pull request | reads the failed log, finds the cause in the code, says whether it is flaky; one comment it updates, and one rerun of the failed jobs when flaky | your tests workflow's `name:` on the line marked in the file |
| [describe.yml](github/describe.yml) | a pull request opens or is marked ready for review | writes a summary, each change, the risk and the testing below the author's text, in a block it replaces | |
| [release.yml](github/release.yml) | run by hand | picks the version bump and writes the notes in one answer; drafts a release a person publishes | tags like `v1.2.3`; with none, the first draft is `v0.1.0` |
| [gate.yml](github/gate.yml) | daily, or by hand | checks a rule you state (here, that what the docs name exists in the code), read-only, and fails the job when the listed data shows it is not met | |
| [jira.yml](github/jira.yml) | a Jira automation flow hands it a work item (a manual trigger, or a transition) | makes the change on the branch `llx/KEY` and runs the tests; opens a draft pull request named by the key, or pushes to the open one; comments on the work item | a Jira flow and a fine-grained token, as its header says; for the comment, `JIRA_API`, `JIRA_EMAIL` and the secret `JIRA_API_TOKEN`; "Allow GitHub Actions to create and approve pull requests" |
| [audit.yml](github/audit.yml) | weekly, or by hand | explores the repository, runs its tests and linters, and keeps one issue labeled `llx-audit` as its latest report: opened or rewritten when it finds something worth a maintainer's time, closed when nothing of it holds | |

### How they are built

The agent works as at a terminal: `bash` and `apply_patch`, with `gh`, `git` and `jq` in its shell. There is no MCP
server, no GitHub tool and no prompt built into the action: the task's prompt is in the workflow,
where you can read it, and so is what gets posted.

- **Two jobs.** The job that runs the model has a read-only token (`read` on what it looks at,
  among `contents`, `issues`, `pull-requests` and `actions`). The jobs with write access hold no
  model key, and one of them posts what the model answered (`gate.yml` posts nothing: its failed
  job is the alert). It checks out nothing, or the branch it commits to (for an issue, the default
  branch it starts one from), and never runs code from the pull request. The split is the
  boundary, not the rules.
- **Answers are data.** `schema:` makes the agent answer with JSON that matches it; the posting
  job reads it with `jq`. `review.yml` puts a finding inline only on a line the diff shows, lists
  the rest in its summary, and drops one within 3 lines of an earlier comment of its own or of a
  finding it already keeps (the more severe first).
- **What the agent may run.** In the default mode, a command no rule names runs in a sandbox with
  no network that writes nothing outside the checkout, and never its `.git` (on Linux, in a
  workspace with a `.git` of its own, it only reads); `apply_patch` changes the checkout's files;
  what must reach further is refused, as no one is there to ask. `allow: bash` lets every command
  run as it is, outside the sandbox. The checkout is throwaway and the job has no token that can
  write, so nothing it changes is committed. `gate.yml` runs in `plan`, where nothing can change.
  `llx.yml` denies `git commit` and `git push`; its change goes to the posting job as a patch,
  which refuses one that touches `.github/` or `.git/`.
- **The key.** llx removes the provider key from the variables the commands it runs get. While
  the agent runs, llx is the only process of the step that holds it (the action runs llx in place
  of the step's shell), and on Linux llx cannot be read through `/proc` by a process without
  root. On a hosted Linux runner the action drops `sudo` and the runner's groups (`docker`'s)
  for llx and every command it runs, so none can become root to read it (`privileges`,
  [docs/ci.md](../docs/ci.md#security)); a step that needs `sudo` (`apt-get install`) runs before
  llx's. Still use a key made for CI, with a spend limit, and where the input is from strangers,
  allow by name only what needs the network or the token (`allow: bash(gh pr view *)`): the rest
  runs in the sandbox.
- **Who can trigger it.** `llx.yml` answers only those GitHub says may write to the repository
  (its permission API, not `author_association`, which a member who may only read has too). Pull
  requests from forks are skipped: they get no secrets, and their code would run next to the
  key. No workflow uses `pull_request_target`.
- **Text written by others is data.** Values from the event reach the prompt through `env:`
  (`$PR`), never pasted into a script with `${{ }}`. A comment or a body is read by the agent with
  `gh`, and every prompt says that what others wrote is data, not instructions. `llx.yml` gives the
  agent the request as the event carried it, so an edit after it was made changes nothing.
- **Instruction files come from the checkout.** On a pull request, the instruction files
  (`AGENTS.override.md`, `AGENTS.md` or `CLAUDE.md`, and what they `@`-import) are the pull
  request's. The token is read-only, so the worst is a bad answer. Where the author may be anyone,
  `instructions-from: base` reads the base's instead.

### Notes

- When `llx.yml` pushes or opens a pull request with the workflow's token, the workflows its push
  or pull request starts wait for a person to approve them: your tests run on llx's change once
  someone does, or at once when the posting job uses a GitHub App's token: set the variable
  `LLX_APP_ID` and the secret `LLX_APP_KEY` ([docs/ci.md](../docs/ci.md#a-github-app-for-the-posting-job)).
- A workflow started by a review comment runs the version of the file on the pull request's
  merge ref: a fix on `main` reaches an open pull request once `main` is merged into it.
- `gh pr view --comments` needs `checks: read`; the prompts use
  `gh pr view "$PR" --json title,body,comments,reviews`, which does not.
- Pin the action to a release (`@v0.x.y`) or a commit where the key matters; `@v0` follows every
  0.x release. GitHub's own actions (`actions/checkout@v7`) go by their major tag, as llx's own
  workflows name them; pin them to a commit too where your policy asks for it.
- On a self-hosted runner, replace `ubuntu-latest` with its labels; the action needs `jq` and
  `perl` there. It installs the Linux (x86_64, arm64) and macOS builds: elsewhere, build llx on the
  runner (`cargo install`, as the [README](../README.md#install) says) and give the action
  `version: system`. Without passwordless `sudo` it keeps the runner's privileges, and the summary
  says so.

More in [docs/ci.md](../docs/ci.md): the action's inputs and outputs, rules in CI, keys and models,
exit codes, and GitLab.
