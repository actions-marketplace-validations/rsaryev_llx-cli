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
4. Anything else is asked of the person at the terminal (in `exec` too, when it has one); with no one there (CI, cron, an `exec` with no terminal), it is refused, and the agent is told how the person can allow it. Where every tool is allowed whole (`bypass`), there is nothing else: the model is told so, and nothing about refusals.

A **mode** is a named set of those rules (`allow`, `deny`), and what its commands reach (`reach`: `anything`, `workspace` or `read-only`). Three are built in:

| Mode | What it does |
| --- | --- |
| `default` | reaches the workspace: commands run without asking in the sandbox, which writes only inside the workspace (never its `.git`; on Linux they only read) and opens no network; file changes there are made; what must reach further is asked, or refused with no one to ask |
| `plan` | read-only: its commands run without asking, inside the sandbox, and file changes are refused |
| `bypass` (`-y`) | allows `bash`, `apply_patch` and `jobs` |

The sandbox is the operating system's: Seatbelt (`sandbox-exec`) on macOS, and Landlock (ABI 3: Linux 6.2 or later) with a seccomp filter on Linux. In `default` and `plan` a command reads every file but llx's own (its keys, tokens and sessions: on macOS only, as Landlock cannot take a path out of what it lets read), and opening a socket or network connection, or reaching another program (Apple events, launchd, daemons), fails. On Linux before 6.12 a command can still signal your other processes ([SECURITY.md](../SECURITY.md)).

In `default` a command writes inside the workspace and a scratch `$TMPDIR` of its own, never the workspace's `.git` (a hook written there would run outside any sandbox at your next commit), and on macOS never a nested repository's either (any `.git` inside the workspace, in any case); writing anywhere else fails. A command the sandbox stops is told its failure looks like the sandbox's refusal, and the model can run it again with `escalate`: that one is asked of you, and with your yes runs as it is. A patch inside `.git` is asked the same way. Two cases only read: a workspace that is your home directory or holds it, and every workspace on Linux, where Landlock cannot take a path out of a tree it lets write, so no `.git` in it (its own, a nested repository's, or one made anew) could be kept. Where no sandbox can be set up at all (on Linux, a kernel before 6.2 or without Landlock), each command is asked as one that runs as it is. On macOS `cargo test`, `npm test` and builds that write into the project work; on Linux they fail and are escalated, as is a command that downloads (`cargo add`, `npm install`), which needs the network.

In `plan` a command writes no file (but `/dev/null` and a scratch `$TMPDIR` of its own). Deny rules still refuse. Where no sandbox can be set up, the command is refused, never run open. Reading, `git log`, `git diff`, `rg` and tests that write nothing work; a build that writes (`cargo test` into `target/`, `vite build` into `dist/`) fails.

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
- In a session, `/mode` lists the modes, `/mode <name>` switches from the next task on, and Shift+Tab goes round them all without Enter (queued until the current task ends). The agent hears of a change before its next task, in a `<mode_change>` note; its context lists the rules in force.
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
| `apply_patch(tests/)` | a file under any directory named `tests`, at any depth (`src/gen/` from the root) |
| `apply_patch(*.md)` | a Markdown file at any depth |
| `jobs` | every monitor or schedule |

- `*` stands for any text, spaces included (the older `bash(ls:*)` reads as `bash(ls *)`); the words before it limit the rule, so put it after the subcommand (`git log *`, not `git * main`).
- A command is **allowed** only when every part of it matches an allow rule: it is split at `&&`, `||`, `;`, `|`, `&` and line breaks, outside quotes. One with `$(…)`, backticks, `<(…)`, a subshell `( … )`, a here-document, a redirect to a file (`> /dev/null` and `2>&1` are fine), `eval`, `exec`, `source`, `sh -c`, `tee` to a file, `find -exec`/`-delete`, `rg --pre`/`--hostname-bin` (ripgrep runs the program they name), or a word the shell builds (`$X`, `${…}`, `$'…'`, an unquoted `{a,b}`), or an unquoted glob where `rg`, `git`, `find`, `sudo` or a shell reads options or a program (`rg foo *`: the shell may fill it with a file named `--pre=sh`; `ls *` and `rg foo -- *` are fine) is never allowed by a pattern: no allow rule matches it, so the mode decides (in `default` and `plan` it runs in the sandbox without asking; in a mode of your own that reaches anything, it is asked, or refused with no one to ask).
- A `git` command that can run another program or write a file is never allowed by a pattern either: an option that names a program (`--receive-pack`, `--upload-pack`, `--exec`, `rebase -x`) or a file to write (`--output`, `archive -o`, `format-patch -o`), config given on the command line (`git -c`, `--config-env`), a remote that is a command (`ext::`), or a subcommand that runs what it is given (`bisect run`, `submodule foreach`, `filter-branch`). A `--` right after an option that takes a value is that value, not the end of the options. `bash(git push *)` does not allow `git push --receive-pack='sh -c …' origin`, and `bash(git log *)` does not allow `git log --output=~/.bashrc`.
- A program is read without its quotes and by its name: `'sh' -c` and `/bin/sh -c` are `sh -c`. `timeout`, `time`, `nice`, `nohup`, `stdbuf`, `env`, `command`, `builtin`, `noglob` and a bare `xargs` (but not before `git`, `rg`, `find`, `sudo` or a shell, whose options the words it reads could be) are looked through, and one that cannot be (`xargs -0`, `time -p`) is never allowed by a pattern; a leading `VAR=value`, or `env` with an assignment or an option, stops an allow. An allow matches `sudo` and `doas` as written (`bash(sudo apt *)`; `bash(npm test)` does not allow `sudo npm test`), and never one told to run a shell (`-s`, `-i`), one whose command an assignment or `env` would stop, or one that runs another.
- A command is **denied** when any part matches a deny rule, inside `$(…)` and past any `VAR=value`, wrapper, `env`, `sudo` or `doas` too: as written, without its quotes, or with its program by name. `bash(git push *)` in deny stops `/usr/bin/git push`, `'git' push` and `sudo -nu root git push`; `bash(sudo *)` stops `sudo rm …` and `nice sudo rm …`. A monitor's command (`jobs`) is held to `bash`'s deny rules too; only a `jobs` rule allows a monitor.
- A deny rule does not look inside the text a shell is given: `bash(rm *)` does not stop `sh -c 'rm …'`. It is a guard on what the model usually writes, not a sandbox; for that, use a read-only mode (`plan`), or run llx in a container. A rule that names an unknown tool is an error at startup (exit 3).

## Approvals

In a session, an approval replaces the prompt with what would happen (`Run this command?`, `Change this file?` or `Allow jobs?`, then the command or the patch's files in full) and three choices: `1. Yes`, `2. Yes, and don't ask again for bash`, `3. No, and tell llx what to do differently`, which opens a line for why. The second allows every later call of that tool that would be asked, for the rest of the session and in this mode only: in `default`, every command the model escalates then runs as it is, with no sandbox.

- Pick with the arrows and Enter, or a number; Esc says no. Letters pick nothing, so words typed as the box opens cannot approve or refuse.
- Line breaks show as `⏎` and control characters are escaped, so what you approve is what runs.
- Outside a session (`exec` with a terminal), the question goes to the terminal itself as one line: `y`, `n`, `n <why>` or `a`, where only the bare word approves. This works while stdin is a pipe, stdout carries JSON and stderr goes to a file.
- A refusal goes back to the model as a failed call with its reason, and it is told not to work around it.

## Stopping and quitting

- **Esc or Ctrl-C stops the task.** The command running is killed with everything it started, an open approval or question ends at once, nothing more is approved, and the task ends at its next model turn as `interrupted`, with its session saved. In `exec`, Ctrl-C, SIGTERM or SIGHUP stops it the same way (exit 130), and a second one quits at once.
- **Ctrl-C closes what is open first:** an approval or a question says no and stops the task that asked (Esc only says no); the list under the prompt or the ← list closes, and nothing more happens. While a task works, it stops the task and keeps what you were typing. With text typed and no task, it clears the text into the earlier entries (↑ brings it back). At an empty prompt it says `ctrl-c again to quit`; a second Ctrl-C within 2 s quits, any other key cancels. A second Ctrl-C while a task is stopping quits at once (asked first while another session works). Each step in order is in [Using llx](usage.md#stepping-back-esc-and-ctrl-c).
- **Ctrl-D** at an empty prompt (or `/exit`) quits, asked first while work runs, as below. Esc twice on an empty prompt with nothing running rewinds (`esc again to rewind`).
- **Quitting while work runs** asks first (`Background work is running`, then each thing that would stop: `session · <its title>`, `monitor · <name>`, `schedule · <name>`):
  - "Exit and stop tasks" stops every session, waits at most 2 s for them and exits (a task whose model request is still in flight then is not recorded);
  - "Move to background and exit", offered only when the sessions run in the daemon, leaves them working there; the next `llx` in that directory comes back to them (what they did meanwhile is drawn, then any question still waiting) and you go on in the same conversation;
  - "Stay" (or Esc) closes the question.
- A terminal closed without that choice (its window closed, SIGTERM or SIGHUP) stops its sessions' work, as "Exit and stop tasks" does, in the daemon too; only "Move to background and exit" keeps it going.

## Trust boundaries

- **Tool output is data, not instructions.** File contents and command output are untrusted. The system prompt says so. This lowers the risk of prompt injection without removing it: approve commands with that in mind, and use `-y` in places where a wrong command costs little.
- **`bash` is a real shell with your permissions.** In `default` its commands run in the operating system's sandbox: they write only inside the workspace (never its `.git`; on Linux they only read), open no network and reach no other program; a command an allow rule names, one you approve with `escalate`, and everything in `bypass` run with no sandbox, and the gate is what stands between the model and your machine. A read-only mode (`plan`) writes no file at all. The workspace's own files are the model's to change in `default`: keep your work committed, so `git` can show and undo what it did. A `workdir` outside the workspace is named as such in the question.
- **Keys and tokens:** `/login` saves an OpenRouter key in `~/.config/llx/credentials`, any other provider's in `~/.config/llx/key-<id>`, and a ChatGPT sign-in's tokens (access, refresh, and the account id) in `~/.config/llx/openai.json` (under `XDG_CONFIG_HOME` when set), mode 0600, also when an older file was wider; the choice of provider, model and level goes in `settings.json` beside them. Keys and tokens are never printed or logged; a pasted key is read with the echo off and shown only as its masked ends. The ChatGPT access token is refreshed five minutes before it lapses (and once when a request is refused), and the refreshed tokens are saved. `llx logout` removes them all. A provider's key variable (`OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`…) wins over its saved key when set.
- **A command and the key:** commands start without `OPENROUTER_API_KEY` or the key variables of the provider in use (or of one chosen later with `/login`) in their environment, but that is no barrier. A command can read the key from llx's own environment (`ps -E` on macOS; `/proc/<llx pid>/environ` on Linux, which llx closes to other processes of the same user by marking itself non-dumpable, but not to root, and a hosted CI runner gives passwordless `sudo`) or from the saved file.
- **The saved files:** on macOS the sandbox keeps them from a command: one of `default` or `plan` cannot read llx's own files (Linux's Landlock cannot take them out of what it lets read); one that runs as it is (a rule allows it, you approve it with `escalate`, or the mode is `bypass`) can. Where `bash` is allowed on input from strangers, assume the key can leak: use a key made for the job, with a spend limit, and narrow rules ([CI](ci.md#security)).
