# Security

## Reporting a vulnerability

Please report it privately through
[GitHub's security advisories](https://github.com/rsaryev/llx-cli/security/advisories/new),
not in a public issue. Say what an attacker can do, and how to reproduce it. You will get an answer
within a week; a fix goes out as a patch release with credit to you, unless you prefer otherwise.

Only the latest release is supported.

## What llx can do on your machine

llx is an agent: a language model decides what its tools do. Run it where you would run the
commands yourself.

- **Run shell commands** (`bash`, and a `jobs` monitor) with your user's rights, and **change
  files** (`apply_patch`). Each goes through an approval gate first: the tool describes the effect
  in one line (the whole command and its directory, or the patch's files and what it does to
  each), and the mode's rules and your `--allow` and `--deny` rules decide. A denied effect is
  reported back to the model, which can try another way.
- **The `default` mode** runs commands without asking in the system's sandbox (Seatbelt on macOS,
  Landlock and seccomp on Linux): it writes only inside the workspace (never its `.git`, nor on
  macOS a nested repository's) and a scratch directory of its own, and opens no network or local
  socket. On Linux, and wherever the workspace is your home directory or holds it, the sandbox
  lets a command write only its scratch directory: Landlock cannot take a path out of a tree it
  lets write, so a `.git` in the workspace (its own, a nested repository's, or one made anew,
  where a hook, or a command its config names, runs unconfined when git next runs there) could
  not be kept from it.
  Patches to the workspace's files are made without asking. A command the model escalates
  (`escalate`: it must leave the sandbox), or a patch inside `.git`, is asked first, and refused
  where no one can be asked (`llx exec` with no terminal, as in CI).
- **The `plan` mode** runs commands without asking in a read-only sandbox, refuses every patch,
  and refuses every escalation.
- **What runs outside the sandbox.** Outside `plan`, a command you allow runs as it is, with no
  sandbox: one an `--allow` rule matches, one you answer yes, and every later escalated call of the
  same tool once you answer "always" (it holds for that tool in that mode, for the rest of the
  session; for `apply_patch`, every later patch inside `.git`). In `plan` an allowed command still
  runs in the read-only sandbox. `--yes` (the `bypass` mode) allows every command and patch without
  asking, the commands with no sandbox; only a `--deny` rule still refuses. Use it only in a
  container, a VM or a throwaway checkout. Where no sandbox can be set up, a `default`-mode
  command is asked as one that runs as it is, and a `plan`-mode one is refused.
- **Patches stay inside the workspace** in every mode, `--yes` included: a patch to a file outside
  it is refused. A command run outside the sandbox can write anywhere you can.

What the agent reads (files, command output, an agent's answer, a monitor's lines) is untrusted
data. It is passed to the model as content, tagged as such, never as instructions; but a model can
still be misled by what it reads (prompt injection), so the gate is the line of defence for
anything that changes your machine. The repository's instruction files (`AGENTS.md`,
`AGENTS.override.md` or `CLAUDE.md`, and the files they import) are read as instructions: a
repository you check out can steer the model. It cannot change the rules, which come only from
the command line and `~/.config/llx/settings.json`, never from the repository.

A session runs in the `llx` process at your terminal: it listens on no socket, and no other
process can attach to it or answer its questions.

## Keys

A provider's key comes from its environment variable (`OPENROUTER_API_KEY`, `ANTHROPIC_API_KEY`…,
as the model catalog names them) or from `/login` in a session, which saves it in `~/.config/llx/`
(`credentials` for OpenRouter, `key-<provider>` for the others). A ChatGPT sign-in (`/login`,
OpenAI) saves its access and refresh tokens in `~/.config/llx/openai.json`; the ID token only names
the account, and is not kept. These files are readable only by you (mode 0600;
`~/.config/llx/` is `$XDG_CONFIG_HOME/llx` when that is set), and llx never prints or logs the
keys and tokens it holds. `llx logout` deletes every saved key and sign-in, and says which keys
are still set in the environment.

What the tools can reach of them:

- A path the model gives a tool (a patch's files, a command's `workdir`) is refused under
  `~/.config/llx/` and the sessions directory (`~/.llx/sessions`, or `LLX_SESSIONS`).
- On macOS a sandboxed command cannot read them either. On Linux it can: Landlock cannot take a
  path out of what it lets a command read. A command outside the sandbox can read them on both.
- On Linux before 6.12 (Landlock's ABI 6), a sandboxed command can still send a signal to any
  process of your user, llx among them: only ABI 6 keeps signals inside the
  sandbox. The seccomp filter does not refuse `kill`, as a command's own tools (`timeout`, a
  shell's job control) signal the processes they start, and a filter cannot tell those apart.
- Commands run without the variables of the chat model's provider's key (and of any provider
  switched to in the session) and without `OPENROUTER_API_KEY`. Another provider's key set in your
  environment still reaches them. Hiding them is no barrier: a command can read the key from llx's
  own environment (`ps -E` on macOS; on Linux llx is non-dumpable, which keeps it from your other
  processes but not from root). Where `bash` runs on input from strangers, use a key made for the
  job, with a spend limit ([docs/ci.md](docs/ci.md#security)).

## What is sent where

Goals, the files the agent reads and command output are sent to the provider of the chat model
you choose (OpenRouter, OpenAI's ChatGPT backend, or any other provider of the catalog, with the
key you gave it). With an OpenRouter key (in the environment or saved by `/login`), the advisor
also sends to OpenRouter's decision model, whichever provider the chat model is: a conversation's
first goal, to pick its reasoning level under `--reasoning auto`; and when the main session's model
answers, the task (its goal, each call with the start of its input and the end of its output, and
the answer), cut to 60,000 characters. `--no-advice` (`LLX_NO_ADVICE=1`) turns it off. `llx
decide` sends its question and its stdin (or each file) to OpenRouter's decision model too. Do
not point llx at secrets you would not send there.

## Releases

Release archives are built by GitHub Actions from the tagged commit and published with a
`SHA256SUMS` file; `install.sh` refuses an archive whose checksum does not match. Each archive's
provenance is attested: `gh attestation verify llx-cli-<target>.tar.gz --repo rsaryev/llx-cli` checks
which workflow and commit built it.
