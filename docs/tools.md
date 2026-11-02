# Tools

llx has six tools, and with `--schema` a seventh, `result`, asked for once the work is done. The
set is deliberately minimal: a new capability goes into one of them (as an `action`), or is left
to the shell. The model works as you do at a terminal: it reads with `cat`,
`sed -n` and `rg` and runs the checks in the shell, and changes files with a patch, as free text with
nothing to escape. Its context names the script runtimes found on the session's `PATH` (`python3`, `node`, `bun`, only
those present) in a `Runtimes:` line.

| Tool | What it is for |
| --- | --- |
| [`bash`](#bash) | Commands that run to their end: reading, searching, running checks |
| [`apply_patch`](#apply_patch) | Files added, changed, deleted or moved, with a patch |
| [`grep`](#grep) | A search of the code by what a question means |
| [`ask`](#ask) | A question to you |
| [`agents`](#agents) | Other agents, in parallel |
| [`jobs`](#jobs) | Monitors and schedules |
| [`result`](#result) | Only with `--schema`, after the work: the data in your shape |

A turn's calls run in the order the model wrote them; those whose tool allows it run at once.
Once a call fails, the turn's later calls are answered unrun: they were written expecting it to
succeed. The last call of a reply the output limit cut off is never run as it stands: a cut
command may be missing its end, so it is answered unrun, and a cut patch is applied only up to
the file the cut fell in ([`apply_patch`](#apply_patch)).

## bash

- `sh -c` in the workspace or a `workdir`, with stdin closed. It runs in its own session, with no terminal, so a program that prompts fails at once; git and pagers are told not to prompt or page. It runs with `AGENT=1` (test runners that look for it, such as bun, print only what failed), `NO_COLOR=1` and `TERM=dumb`, and without `FORCE_COLOR` or `CLICOLOR_FORCE`.
- The model reads what Codex's `shell` returns: `Exit code: N`, `Wall time: 0.4 seconds`, then `Output:` with stdout and stderr as one stream, in the order written (one pipe for both) (a timeout reads `Timed out after: …`, a signal `Killed by signal: N`, a stop `Stopped: by the person`; a `workdir` adds `Directory: …`, a cut output `Total output lines: N`).
- A timeout (120 s by default, at most 600 s) or Ctrl-C kills it with everything it started.
- A server started in the background (`cmd > log 2>&1 &`) outlives a call that ends on its own; in the sandbox it cannot open a socket, so it needs `escalate` or an allow rule.
- The output reads as a terminal shows it: colours, titles and links taken out, a line redrawn in place (`\r`, a progress bar) at its last state, a backspace applied; a line repeated is shown once with `... the same line N more times ...`.
- It keeps about 80,000 characters of a command that succeeded (the first 16,000 and the last 64,000, cut at lines), so the files read in one command come whole, and about 40,000 of one that failed (8,000 and 32,000), whose end is what matters, and a line over 2,000 characters is shortened with how many more it had. When anything is left out, the whole output goes to a file (`$TMPDIR/llx-output-<uid>/`, the directory 0700 and each file 0600, kept a day and 512 MiB at most, 64 MiB a file), and the cut is marked ``... lines A-B of N cut (C characters); read them with `sed -n 'A,Bp' <file>` (the whole output is in that file) ...``; when no file could be written, it says to run the command again with its output redirected. A non-zero exit is a normal result.
- It asks the gate. In `default` its commands run in an operating system sandbox that writes only inside the workspace (never its `.git`) and its own scratch `$TMPDIR`, and opens no network; where the workspace is your home directory or holds it, or on Linux in a workspace with a `.git` of its own, they only read. In a read-only mode (`plan`) they write nothing ([modes](modes.md#modes)). A command that fails as the sandbox refuses it is told so, and `escalate: true` asks the person to run it as it is (refused where no one can be asked).
- The key variables of the provider in use (as the catalog names them, such as `DEEPSEEK_API_KEY`) and `OPENROUTER_API_KEY` are left out of a command's environment; other providers' variables stay, so a project's own tests can use them; a provider chosen later with `/login` has its variables left out too, from the next command on. That is no barrier, since a command can still read llx's own environment (`ps -E` on macOS; on Linux llx marks itself non-dumpable, so only root can read `/proc/<llx pid>/environ`, and a hosted CI runner gives passwordless `sudo`) and the saved key file. Where `bash` runs on input from strangers, see [CI security](ci.md#security).

## apply_patch

- A patch as free text, `*** Begin Patch` … `*** End Patch`: `*** Add File`, `*** Delete File`, `*** Update File` (with `*** Move to`), each update's hunks found by their lines and the lines around them. Where the provider takes a tool of free text held to a grammar (the ChatGPT sign-in, for a model the catalog says takes one), it is offered so; elsewhere as a function with the patch in one string. Every file is worked out and asked of the gate before any is written; a hunk that does not fit changes nothing. The one exception is a patch the output limit cut off: the files before the one the cut fell in are applied, and the model is told to send the rest again.
- The model reads Codex's summary: `Success. Updated the following files:` and one line a file, `A path`, `M path` or `D path` (a move is `D old`, `A new`). The diff is shown to you in the terminal, never sent to the model; a failure says why and that no file was changed.
- Writes are atomic (a new temporary file, then a rename) and keep CRLF line endings and permissions; a read-only file is refused, and a file with other hard links is written in place.
- It asks the gate, refuses to write when the file changed while the question was open, and changes only files inside the workspace, never llx's own. In `default` it changes them without asking, but a file inside `.git` is asked; in a read-only mode (`plan`) a patch is refused: a file written cannot be sandboxed.

## grep

- The search [`llx grep`](reference.md#llx-grep) runs, as a tool: `query` (the question, in plain words with any names it knows), `paths` (files and directories, the workspace when none), `patterns` (regexes as `rg` takes them, with `fixed` and `ignore_case`), which keep only the files they match. The files are ranked by the question's words, then the decision model reads the best of them and keeps those that answer.
- It runs in llx's own process, not in a command's sandbox, which has neither the network nor the decision model's key: so the agent never runs `llx grep` through `bash`. It reads the decision model's key as `llx grep` does (`OPENROUTER_API_KEY`, else the key `/login` saved), from the session's environment in the daemon; with none it ranks by the words alone and says so. `--no-advice` turns off the advice only, not the search.
- The model reads up to 8 files with their parts (whole functions where they are short, line-numbered, 120 lines in all), then `also:` and `unsure, read to check:`, and a line with how many files answered, the decision model and the cost; no file answering reads `No file answers`.
- It only reads, so it asks no gate and runs beside the turn's other calls. Every path goes through the workspace, and a file llx keeps to itself (its keys, its sessions) is never listed. The decision model's cost is the task's, under `--budget`. Agents have it too.

## ask

One to three questions to you, in a session only (not in `exec`). Each is shown with up to four options to pick with the arrows or a number (a question that allows several answers toggles them with Space, then Enter), and a last one to type your own answer; Esc skips it, and Ctrl-C stops the task. It asks no gate: it changes nothing.

- A question's `header` (a word or two) names it in a row of tabs above it: `☒` for those answered, `☐` for the one asked and those to come.
- An option's `preview` (a mockup, a snippet) is drawn in a box beside the options while that option is marked, with its description under it; on a narrow terminal, under the options. A preview cannot move the cursor or clear the screen, and past 20 lines the rest are counted.
- Once answered, the call shows each question with its answer (`Which layout? → Sidebar`).

## agents

Starts other agents and works with them, in parallel. The model starts one only when you, or the project's instructions (AGENTS.md), ask for agents, delegation or parallel work: on its own it reads and works itself, which costs less for the same result.

- `spawn` (a task, a short name, and `context`) starts one on a thread of its own and returns at once; `send` tells one more (read at its next step while it works, or a new task once it is done); `wait` (some or all, `timeout_ms`: 30 s by default, 600 s at most) returns the answers that came; `list` shows them; `close` ends one. At most 6 are open and 8 working, a closed one that has not stopped yet among them.
- An agent's answer, its final message, reaches the main agent by itself at its next step as an `<agent_result>` message; a task does not end while its agents work.
- An agent is a session of its own whose parent is the main one: it has the same tools but `agents`, `ask` and `jobs` (one level, and nobody to answer it), offered the same way, and asks the same gate, its approvals marked `[agent name]` and asked one at a time.
- By default an agent is a fork: its conversation is the main one as the last request sent it, then the turn that started it, so it reads nothing again and its first request starts where the provider's prompt cache already holds the main session's. With `context: "fresh"` it opens with the context alone and sees nothing of the main conversation.
- It stops with the task (Ctrl-C, the time limit) or alone (`x` on its row), and what it spends counts in the task's turns and cost (and in `--json`'s `turn` events and `usage`).
- Its tasks are saved as a session that names its parent; `llx sessions` lists the main ones with how many agents each started, and `resume` never picks an agent's.

## jobs

Work that comes back later.

- `monitor` runs a command in the background, asked of the gate and sandboxed as `bash`'s are (`escalate` too), in a session of its own, without the key variables `bash` leaves out and without its `AGENT`, `NO_COLOR` and `TERM`. Each batch of lines it prints (40 lines at most, each cut at 400 characters) reaches the main agent as a `<monitor>` message, at its next step while a task runs, else as a task of its own. It ends when the command exits, after 100 batches, at its deadline (5 min by default, 30 at most; 10 at most in `exec`, where the task waits for it) or with `stop`, and the agent is told.
- `schedule` gives a `prompt` back as a task of its own at a time: `in` (`45m`, `2h`, `1d`), once; or a 5-field `cron` in local time (`*/10 * * * *`, `0 9 * * 1-5`; both day fields restricted match either), once or `recurring` (for 7 days at most). It runs between tasks, behind what you queued, while the session is open (not in `exec`; not restored by `resume`; no jitter).
- At most 8 monitors and 50 jobs. `list`, `stop`.

## result

Not offered while the model works. With `--schema`, once the task has its answer, llx asks the same model once more for a `result` call carrying `data` that matches the JSON Schema, with the call forced; data that does not match goes back to be fixed, up to two times, then the run ends `no_data` (exit 1). How a task ends, and where the data goes, is in the [reference](reference.md#exit-codes).
