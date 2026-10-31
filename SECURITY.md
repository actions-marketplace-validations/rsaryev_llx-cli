# Security

## Reporting a vulnerability

Please report it privately through
[GitHub's security advisories](https://github.com/rsaryev/llx/security/advisories/new),
not in a public issue. Say what an attacker can do, and how to reproduce it. You will get an answer
within a week; a fix goes out as a patch release with credit to you, unless you prefer otherwise.

Only the latest release is supported.

## What llx can do on your machine

llx is an agent: a language model decides what its tools do. Run it where you would run the
commands yourself.

- **Run shell commands** (`bash`) with your user's rights, and **change files** (`apply_patch`).
  Each of these goes through an approval gate first: the tool describes the effect in one line
  (the whole command, or the patch's files and what it does to each), and it runs only when
  allowed. In `plan` mode commands run without asking inside the system's sandbox, which cannot
  write a file or open a socket, and a patch is refused. A denied effect is reported back to
  the model, which can try another way. Allowing everything up front (`--yes`) skips the gate:
  use it only in a sandbox, a container or a throwaway checkout.

What the agent reads (files, command output) is untrusted data. It is passed to the
model as content, never as instructions, but a model can still be misled by what it reads (prompt
injection), so the gate is the line of defence for anything that changes your machine.

On Unix a session runs in a daemon (`llx serve`), reached over a socket in a directory only you
may enter (`$TMPDIR/llx-<uid>/`, mode 0700, the socket 0600), with no token: it serves the user
who runs it.

## Keys

The OpenRouter key comes from `OPENROUTER_API_KEY` or from `/login` in a session, which saves it
in `~/.config/llx/credentials`. A ChatGPT sign-in (`/login`, OpenAI) saves its access, refresh
and ID tokens in `~/.config/llx/openai.json`. Both files are readable only by you (mode 0600 on
Unix), keys and tokens are never printed or logged, and the tools cannot read anything under
`~/.config/llx/`. `llx logout` deletes both. Goals, page text and command output are sent to the
provider and the models you choose (OpenRouter, OpenAI's ChatGPT backend, or any other provider of the catalog, with the key you gave it); do not point llx
at secrets you would not send there.

## Releases

Release archives are built by GitHub Actions from the tagged commit and published with a
`SHA256SUMS` file; `install.sh` refuses an archive whose checksum does not match.
