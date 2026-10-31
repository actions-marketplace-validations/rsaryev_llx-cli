# Modes, approvals and trust

What llx asks before it acts, how to stop it, and what it does and does not protect.

- [Modes](#modes)
- [Approvals](#approvals)
- [Stopping and quitting](#stopping-and-quitting)
- [Trust boundaries](#trust-boundaries)

## Modes

`bash`, `apply_patch` and `jobs` ask before they act, through one gate. What the gate answers comes from rules, then from what the mode's commands reach:

1. A **deny** rule that matches refuses, and the refusal names the rule.
2. An **allow** rule that matches runs it without asking, as it is (in a read-only mode, still in its sandbox).
3. What the mode's **reach** covers runs without asking, in its sandbox.
4. Anything else is asked of the person at the terminal; with no one there (`exec`, CI), it is refused, and the agent is told how the person can allow it. Where every tool is allowed whole (`bypass`), there is nothing else: the model is told so, and nothing about refusals.

A **mode** is a named set of those rules (`allow`, `deny`), and what its commands reach (`reach`: `anything`, `workspace` or `read-only`). Three are built in:

| Mode | What it does |
| --- | --- |
| `default` | reaches the workspace: commands run without asking in the sandbox, which writes only inside the workspace; file changes there are made; what must reach further is asked, or refused with no one to ask |
| `plan` | read-only: its commands run without asking, inside the sandbox, and file changes are refused |
| `bypass` (`-y`) | allows `bash`, `apply_patch` and `jobs` |

In `default` a command reads every file but llx's own, and writes inside the workspace and a scratch `$TMPDIR` of its own, never the workspace's `.git` (a hook written there would run outside any sandbox at your next commit); writing anywhere else, opening a socket or network connection, or reaching another program fails. A command that fails so is told it looks like the sandbox's refusal, and the model can run it again with `escalate`: that one is asked of you, and with your yes runs as it is. A patch inside `.git` is asked the same way. Two cases only read: a workspace that is your home directory or holds it, and, on Linux, a workspace with a `.git` (Landlock cannot take a path out of a tree it lets write). Where no sandbox can be set up at all, each command is asked as one that runs as it is. `cargo test`, `npm test` and builds that write into the project work; one that downloads (`cargo add`, `npm install`) needs the network, so it is escalated and asked.

In `plan` a command reads every file but llx's own (its keys, tokens and sessions: on macOS; Linux's Landlock cannot take a path out of what it lets read), but writing a file (except `/dev/null` and a scratch `$TMPDIR` of its own), opening any socket or network connection, or reaching another program (Apple events, launchd, daemons) fails. The sandbox is Seatbelt (`sandbox-exec`) on macOS, and Landlock with a seccomp filter on Linux (kernel 5.13 or later). Deny rules still refuse. Where no sandbox can be set up, the command is refused, never run open. Reading, `git log`, `git diff`, `rg` and tests that write nothing work; a build that writes (`cargo test` into `target/`, `vite build` into `dist/`) fails.

Your own go in `modes` in `~/.config/llx/settings.json` (under `XDG_CONFIG_HOME` when set); one with the name of a built-in mode replaces it. `"reach"` says what its commands reach: `"anything"` (the default for a mode of your own: what no rule allows is asked), `"workspace"` (as `default`) or `"read-only"` (as `plan`; the older `"read_only"` key is an error that says so):

```json
{
  "modes": {
    "review": { "allow": ["bash(cargo test *)", "bash(git log *)"], "description": "Review, tests allowed." },
    "audit":  { "reach": "read-only", "description": "Read and search, nothing written." },
    "fix":    { "allow": ["bash(cargo *)"], "deny": ["bash(git push *)"] }
  }
}
```

- `--mode NAME` chooses one, `-y` is `bypass`, else `default`; for a session and for `exec` alike. A name that is not a mode, or a mode whose rules cannot be read, stops the run before a model is called (exit 3).
- `--allow RULE` and `--deny RULE`, repeated, add to the mode's rules for that run: `llx exec --mode review --deny 'bash(cargo publish *)' …`.
- In a session, `/mode` lists the modes, `/mode <name>` switches for the next question, and Shift+Tab goes round them all without Enter (queued until the current task ends). The agent hears of a change before its next task, in a `<mode_change>` note; its context lists the rules in force.
- Rules come from the command line and your settings only, never from the repository: a pull request in CI could otherwise allow itself anything. In a container, mount your settings file read-only (`-v ./llx.json:/root/.config/llx/settings.json:ro`) and pass the key as a variable; `/login` writes to that file, so do not sign in through a read-only one.

### Rules

Rules are written as Claude Code writes them:

| Rule | Matches |
| --- | --- |
| `bash` or `bash(*)` | every command |
| `bash(cargo test)` | that command exactly |
| `bash(cargo test *)` | `cargo test`, `cargo test -q`, …, not `cargo testing` |
| `bash(* --version)` | any program's `--version` |
| `apply_patch` | every file change |
| `apply_patch(docs/**)` | a file under `docs/`, from the workspace root |
| `apply_patch(*.md)` | a Markdown file at any depth |
| `jobs` | every monitor or schedule |

- `*` stands for any text, spaces included; the words before it limit the rule, so put it after the subcommand (`git log *`, not `git * main`).
- A command is **allowed** only when every part of it matches an allow rule: it is split at `&&`, `||`, `;`, `|`, `&` and line breaks, outside quotes. One with `$(…)`, backticks, `<(…)`, a here-document, a redirect to a file (`> /dev/null` and `2>&1` are fine), `eval`, `sh -c`, `tee` to a file or `find -exec`/`-delete` is never allowed by a pattern: it is asked, or refused with no one to ask. So is a `git` command that can run another program: an option that names one (`--receive-pack`, `--upload-pack`, `--exec`, `rebase -x`), config given on the command line (`git -c`, `--config-env`), a remote that is a command (`ext::`), or a subcommand that runs what it is given (`bisect run`, `submodule foreach`, `filter-branch`). `bash(git push *)` does not allow `git push --receive-pack='sh -c …' origin`. A program is read without its quotes and by its name: `'sh' -c` and `/bin/sh -c` are `sh -c`. `timeout`, `time`, `nice`, `nohup`, `stdbuf`, `env` and a bare `xargs` are looked through, and one that cannot be (`xargs -0`, `time -p`) is never allowed by a pattern; a leading `VAR=value`, or `env` with an assignment or an option, stops an allow. An allow matches `sudo` and `doas` as written (`bash(sudo apt *)`; `bash(npm test)` does not allow `sudo npm test`), and never one told to run a shell (`-s`, `-i`), one whose command an assignment or `env` would stop, or one that runs another.
- A command is **denied** when any part matches a deny rule, inside `$(…)` and past any `VAR=value`, wrapper, `env`, `sudo` or `doas` too: as written, without its quotes, or with its program by name. `bash(git push *)` in deny stops `/usr/bin/git push`, `'git' push` and `sudo -nu root git push`; `bash(sudo *)` stops `sudo rm …` and `nice sudo rm …`.
- A deny rule does not look inside the text a shell is given: `bash(rm *)` does not stop `sh -c 'rm …'`. It is a guard on what the model usually writes, not a sandbox; for that, use a read-only mode (`plan`), or run llx in a container. A rule that names an unknown tool is an error at startup (exit 3).

## Approvals

In a session, an approval replaces the prompt with what would happen (`Run this command?`, then the command in full) and three choices: `1. Yes`, `2. Yes, and don't ask again for bash`, `3. No, and tell llx what to do differently`, which opens a line for why.

- Pick with the arrows and Enter, or a number; Esc says no. Letters pick nothing, so words typed as the box opens cannot approve or refuse.
- Line breaks show as `⏎` and control characters are escaped, so what you approve is what runs.
- Outside a session (`exec` with a terminal), the question goes to the terminal itself as one line: `y`, `n`, `n <why>` or `a`, where only the bare word approves. This works while stdin is a pipe, stdout carries JSON and stderr goes to a file.
- A refusal goes back to the model as a failed call with its reason, and it is told not to work around it.

## Stopping and quitting

- **Esc or Ctrl-C stops the task.** The command running is killed with everything it started, an open approval or question ends at once, nothing more is approved, and the task ends at its next model turn as `interrupted` (exit 130 in `exec`), with its session saved.
- **Ctrl-C closes what is open first:** a question says no; the list under the prompt or the ← list closes, and nothing more happens. While a task works, it stops the task and keeps what you were typing. With text typed and no task, it clears the text into the earlier entries (↑ brings it back). At an empty prompt it says `ctrl-c again to quit`; a second Ctrl-C within 2 s quits, any other key cancels. A second Ctrl-C while a task is stopping quits at once.
- **Ctrl-D** at an empty prompt (or `/exit`) quits at once, even while a task works. Esc on an empty prompt says `esc again to rewind`.
- **Quitting while work runs** asks first (`Background work is running`, then each thing that would stop: `session · <its title>`, `monitor · <name>`, `schedule · <name>`):
  - "Exit and stop tasks" stops every session, waits at most 2 s for them and exits (a task whose model request is still in flight then is not recorded);
  - "Move to background and exit", offered only when the sessions run in the daemon, leaves them working there; the next `llx` in that directory comes back to them (what they did meanwhile is drawn, then any question still waiting) and you go on in the same conversation;
  - "Stay" (or Esc) closes the question.
- A terminal closed without that choice stops the work, as "Exit and stop tasks" does. SIGTERM or closing the window stops every session's task (in the daemon, closing the window stops none).

## Trust boundaries

- **Tool output is data, not instructions.** File contents and command output are untrusted. The system prompt says so. This lowers the risk of prompt injection without removing it: approve commands with that in mind, and use `-y` in places where a wrong command costs little.
- **`bash` is a real shell with your permissions.** In `default` its commands run in the operating system's sandbox: they write only inside the workspace (never its `.git`), open no network and reach no other program; what you approve with `escalate`, and everything in `bypass`, runs with no sandbox, and the gate is what stands between the model and your machine. A read-only mode (`plan`) writes no file at all. The workspace's own files are the model's to change in `default`: keep your work committed, so `git` can show and undo what it did. A `workdir` outside the workspace is named as such in the question.
- **Keys and tokens:** `/login` saves an OpenRouter key in `~/.config/llx/credentials` and a ChatGPT sign-in's tokens (access, refresh, and the account id) in `~/.config/llx/openai.json` (under `XDG_CONFIG_HOME` when set), mode 0600, also when an older file was wider; the choice of provider, model and level goes in `settings.json` beside them. Keys and tokens are never printed or logged; a pasted key is read with the echo off and shown only as its masked ends. The ChatGPT access token is refreshed five minutes before it lapses (and once when a request is refused), and the refreshed tokens are saved. `llx logout` removes both files. `OPENROUTER_API_KEY` wins over a saved key when set. Commands start without it, or the key variables of the provider in use (or of one chosen later with `/login`), in their environment, but that is no barrier. A command can read the key from llx's own environment (`ps -E` on macOS; `/proc/<llx pid>/environ` on Linux, which llx closes to other processes of the same user by marking itself non-dumpable, but not to root, and a hosted CI runner gives passwordless `sudo`) or from the saved file. What keeps it from a command is approving each one (not `--yes`, `bypass`, or a rule that allows it), or, on macOS, a read-only mode, whose sandbox leaves llx's own files unread. Where `bash` is allowed on input from strangers, assume the key can leak: use a key made for the job, with a spend limit, and narrow rules ([CI](ci.md#security)).

A long-lived server runs as `cmd > log 2>&1 &`.
