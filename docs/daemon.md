# llx as a daemon and a thin client

One `llx serve` per config directory (one per user, unless a test or a second config gives
another) owns everything that works: sessions (lanes), agents, jobs, models and sign-ins. `llx` at
a terminal is a client: it draws, reads keys, and speaks to the daemon over a socket. Several
terminals, several projects, one place where work goes on when a terminal closes. `--no-daemon`
and `llx exec` run the same server in their own process, over channels instead of the socket.

This page is for contributors: the design, where it came from, and how it is built. The rules it
follows are ARCHITECTURE.md's W1-W8 and the steps D1-D7.

Sources read: an agent's app-server (its daemon and TUI client), Localix's API server and UI
(`docs/internals/architecture*`, `client-server-sync.md`, the wake queue), and llx's own seam
(`apps/tui/src/event.rs`, `apps/server/src/server/ui.rs`, `desk.rs`).

## What we take, and from whom

| Decision | Taken from | Why |
|---|---|---|
| One daemon per config directory, started by the first client, found at a fixed socket path; a probe with a short timeout, else spawn | An agent app-server (a 50 ms probe) | No setup step; a second terminal attaches |
| Unix socket in a `0700` directory, socket `0600`, no token; `flock` against a second daemon | An agent app-server | Same-user only, std only |
| One client interface, two transports: in process (channels) and remote (socket) | An agent app-server's client | `exec`, tests and `--no-daemon` keep working with no socket |
| A question the daemon asks a person (approval, question, choice, a line) belongs to the session, not the client: sent to every attached client, first answer wins, `person/resolved` tells the others; replayed to a client that attaches; withdrawn when the task stops | An agent app-server (server requests), Localix (HITL replay) | A closed terminal never loses a question; two terminals never both answer |
| The person's input goes to a queue on the daemon (the lane's queue), not a reply channel held by the UI | Localix (wake queue), llx's own `packages/queue` | The daemon owns all state; the client only shows it |
| A bounded event ring; a client that attaches replays what it keeps, and a gap means "read again" | Localix (`epoch:seq`, `transport.gap`) | Reattach without a durable log |
| Live-only events (a call's step) are never replayed | Localix (replay tiers) | Replaying deltas corrupts what is drawn |
| A slow client is cut off, not waited for | An agent app-server, Localix | The daemon never blocks on a terminal |
| Session settings (directory, environment, command line) are the client's, given at its handshake; mode and model live on the session | An agent app-server (thread params), Localix (session cwd) | A lane is a project; two clients, two projects |
| A terminal's signals never reach the daemon (it runs in a session of its own, with no terminal): Ctrl-C at a terminal becomes the `interrupt` command | An agent app-server | One daemon, many terminals |
| No idle exit: the daemon lives until stopped | An agent app-server | Schedules and monitors outlive every terminal |

What we do not take: WebSocket framing (newline-delimited JSON is enough on a local socket, and
adds no dependency), schema generation, experimental API gating, an updater loop (a build
mismatch restarts an idle daemon instead), event sourcing.

## The protocol

`packages/protocol` holds the wire types, and nothing else (L6): the server turns its domain
values into them (`apps/server/src/server/wire.rs`), and the interface knows only them (L7). The
same frames go over channels in process and over the socket (W4); the socket carries one JSON
object per line.

A frame is `{"method": …, "params": …}` (`frame.rs`). A command's `id`, a number, is inside its
params. The smallest handshake, which a server in process accepts (a daemon also needs its own
`build`, and builds a desk only once a `place` is given):

```json
{"method":"command/call","params":{"id":1,"command":{"type":"initialize","version":2}}}
```

From the client (`ToServer`):

- `command/call {id, command}`: a command, answered by exactly one `command/done {id, done}`.
- `person/reply {id, reply}`: the answer to the question `id`, of its kind (`approval`,
  `question`, `choice`, `adjusted`, `line`; every kind but `approval` carries `null` when
  cancelled).

From the server (`ToClient`):

- `command/done {id, done}`: what came of a call, sent only to the connection that made it.
- `session/event {session, event, seq}`: what happened in a lane or an agent (`session` is its
  id), or in the server (`null`). `seq` numbers what the ring keeps; a live-only event has none.
- `session/gap`: what follows is not all that happened (the ring let some go).
- `person/ask {id, session, asked}`: a question for the person, from that session's task:
  `approve`, `question` (one of the `ask` tool's, with its place among them and their headers),
  `choose`, `choose_adjustable` (rows with a value adjusted inline, in optional groups: `/model`'s
  providers) or `line` (a secret stays hidden on the client and travels once).
- `person/resolved {id}`: the question no longer waits: answered elsewhere, or its task stopped.

A command is tagged by `type` (`command.rs`), and names the session it is for (W7):

| Command | What it does | Done |
|---|---|---|
| `initialize {version, place, build}` | The interface's handshake, first and once. `place` (`{cwd, env, args, dir}`: the directory, the environment, the command line and where its relative paths are) is where its sessions work | `initialized {version, max_lanes, commands, modes, background}`: what `/` and `/mode` offer, and whether quitting may move work to the background (a daemon's); a daemon of another build answers `failed` |
| `input {session, text, images, later}` | What the person sent a lane: its next task, words for the task running, or a slash command; `later` keeps it for after the task (Tab) | `ok`, or `failed` for a lane not open |
| `interrupt {session}` | Ctrl-C: stop the lane's task | `interrupted {again}`: `again` when no task ran or it was stopped already, so the interface may quit |
| `halt {id}` | Stop the task of a lane or an agent, and never quit for it | `ok` |
| `stop_job {lane, job}` | Stop a monitor or a schedule | `ok` |
| `close {session}` | Close an idle lane | `ok`, or `failed` for a lane not open |
| `open {target}` | Open a lane (`{"type":"new"}`, or `{"type":"saved","id":…}`), or show it when open | `ok` |
| `list` | This directory's saved sessions (they come as an event) | `ok` |
| `quit_all` | "Exit and stop tasks" | `ok` |
| `leave` | The person left: what waits for them ends with nothing | `ok` |
| `work` | What still runs, for the quit question | `work {running}`: each `{kind, name}`, a kind being `session`, `monitor` or `schedule` |
| `background` | "Move to background and exit" | `ok` |
| `status` | What the daemon is | `status {build, pid, attached, lanes, working, kept, jobs}` |
| `shutdown` | End the daemon, every lane stopped first | `ok` |

`status` and `shutdown` are a daemon's alone: a server in process answers them `failed`. They need
no handshake: `llx serve status`, `llx serve stop` and a client looking for its daemon connect,
send one, and get no desk. A command the desk carries out (`open`, `list`, `quit_all`,
`background`) is answered `failed` once the sessions have ended. A command that panics is answered
`failed` (W8); so is a call the server cannot read, under its id. Any other line that is not a frame is dropped, and the client is
told in a note.

The events (`event.rs`): a session's header, notes and errors, its mode, a task's start and end,
a call and its result (its label and outcome), a step under the call running, the answer, the
earlier tasks of a resumed session, its facts (`updated`: the title), a lane's start and end, the
saved sessions (`saved`), which lane to show, an agent's start and close, tokens spent and the
session's total, agents cleared on a rewind, words put in the composer (`prefill`), the context
left, the jobs, the lane's queue, the words a task took as steering, text for the clipboard
(`copy`), and `quit`, when the terminal is given back.

The protocol's version is `protocol::VERSION`; it grows by optional fields and new commands, and
each side ignores a field it does not know (W3). What the daemon checks is the build: the version,
the binary's size and its time (`daemon::build`; `LLX_BUILD` stands in for it in tests), so a
daemon of another build never serves this client.

## Who owns what

| | Daemon | Client |
|---|---|---|
| Sessions, agents, jobs, queues, history | ✓ | |
| Models, sign-ins, the catalog refresh | ✓ | |
| Keys, composer, drawing, raw mode | | ✓ |
| `$EDITOR`, Ctrl+V (a picture is kept in the shared pasted folder and sent as its path) | | ✓ |
| `/copy` (the daemon sends the text, the client copies it) | | ✓ |
| `@` file list (read by the interface on a thread of its own, its `git` through `within.rs`) | | ✓ |

A session's directory and environment are its client's (`apps/server/src/server/place.rs`):
commands run in the environment of the terminal that opened the session (its `PATH`, an
activated virtualenv, its keys), not the daemon's, less the key variables llx hides from
commands (the chat model's provider's, any provider's switched to, and `OPENROUTER_API_KEY`).
The model a session uses is its client's too: the providers' keys come from that environment
(`providers::Vars`), and so do the options a variable may give (`LLX_PROVIDER`, `LLX_MODEL`,
`LLX_REASONING`, `LLX_NO_ADVICE`) unless the client's command line, as clap reads it, gives the
option (`commands::serve::FROM_ENV`). A daemon is started without these variables, so it never
takes another terminal's.

## Lifecycle

- `llx`: ask the socket the daemon's lock names (`<config>/daemon.lock`, while it is held), else
  `$TMPDIR/llx-<uid>/<FNV-1a of the config directory>.sock` (`/tmp` without `$TMPDIR`; a test
  home gets its own daemon). Its `status`, within 2 s: this build's → attach; another build's and
  idle → `shutdown`, wait for the connection to end, start a new one; another build's and busy →
  the session runs in process, said in a note. None answers → `llx serve` in its own session
  (`setsid`, no terminal, its errors in `<config>/daemon.log`), its ready line awaited 10 s (a lost
  race to another client's daemon is fine); attach.
- `llx serve`: `flock` on `<config>/daemon.lock` (`<config>` is the credential store's
  directory, `~/.config/llx`), retried 2 s for a daemon that is ending, else exit 3; bind; print
  `llx serve: ready (pid N)`; from then on its stdout goes where its errors go; accept clients on
  threads.
- Quitting while work runs asks, as Claude Code does:

  ```text
  Background work is running
  The following will stop when you exit:
    monitor · cargo build --release
  ❯ 1. Exit and stop tasks
    2. Move to background and exit
    3. Stay
  ```

  Only moving to the background keeps them: the sessions go on in the daemon (their tasks,
  monitors and schedules), and the next plain `llx` in that directory comes back to them. A
  terminal closed without that choice stops its sessions' work, as "Exit and stop tasks" does; the
  daemon itself lives on. In process, "Move to background" is not offered.
- The daemon never exits by itself: schedules and monitors must outlive every terminal. It ends
  with `llx serve stop` (`shutdown`: every task stopped, 2 s for them to end, the socket removed,
  the lock file emptied), or with the machine. `llx serve status` says its pid and build, the
  terminals attached, the sessions open and working, those in the background and the jobs.

## How it is built

The steps (ARCHITECTURE.md D1-D7) shipped one at a time, each green: the seam as the protocol, in
process (D1); sessions that carry their place (D2); the socket and `llx serve` (D3); attach and
detach (D4); the split into the `server`, `tui` and `llx` crates (D7). What they built:

**The socket.** `apps/server/src/server/socket.rs`: `bind` in a 0700 directory, the socket 0600;
`accept` on a thread, with a backoff when accepts keep failing; per connection a reader, a pump
and a writer. 64 frames read and not yet served before the reader waits; a whole replay and 1,024
live frames queued for a client, or a write stalled 10 s, before it is cut off. The client's end
is `packages/client`'s `Client::connect`: the same reader loop as in process, and a writer thread
with a 10 s write timeout, so a server that stops reading is gone. The binary finds or starts its
daemon (`apps/cli/src/terminal/daemon.rs`: `status`, `reach`, `spawn`, `stop`) and runs the
interface over the socket (`terminal/remote.rs`; `tui::Start` takes a `Connect`, channels or the
socket). A server that is gone ends the interface. Pty tests run through a daemon per test home;
the harness stops it (`llx serve stop`, then a kill by the pid in the lock file).

**A desk per connection.** Each connection gets a server (`run::serving`, with the daemon's
`Host`) and, once its `initialize` gives a place, a desk built by the same `run::run` as in
process (`At::Served`): the client's command line (`Place.args`, parsed again: its `-C` is the
place's directory, its pictures resolved at `Place.dir`) and environment. The server posts the
place to the desk (`Request::Attach`), which builds its workspace there (`Workspace::with_env`).
The daemon serves sessions only: `exec` and the other commands run in their own process. A client
that goes without quitting stops its desk's lanes (`Request::Gone`), unless it moved them to the
background first (`Request::Background`): the host then keeps the desk by its directory
(`server::host::Kept`).

**Attach and detach.** A `Peer` serves any number of clients (`server/hub.rs`). Every event goes
to each attached client, numbered per peer (`seq`) and kept in a ring of the last 2,000 (and
8 MiB), unless live only: a step, a copy, a quit, a prefill and a list are never kept (`hub.rs`;
the ring is `hub/ring.rs`). Each open session's state is kept outside the ring too, whole: a lane's
start, header and mode, an agent's start and close, the latest queue, total, jobs, context and
facts; a newer one replaces the one before, in the ring too. A lane that ends says so
(`LaneEnded`, from its queue's end) and takes all of its own and its agents' with it; the
interface lets it go, and what is typed for it is refused. A connection is attached as its first
frame comes and given what the ring keeps; when the ring no longer holds all that happened, a
`session/gap` first, then the state of every session the ring no longer holds, so a lane moved to
the background long ago is drawn and typed into again; then the questions waiting. On a gap the
interface clears its transcripts, jobs and questions and draws again from what follows. Every
attach replays the whole ring: a new `llx` has no cursor, and no client reconnects within its own
life. A queue is told only when it changes. A question goes to every client; the first reply wins
and every client gets `person/resolved`. A closed client is dropped, never waited for; the person
is given up on when the last client is.

**The quit question.** `work` lists what runs (each lane working by its title, each monitor and
schedule by name; `server/work.rs` keeps the lanes' part from their events); `initialized`'s
`background` says whether the server keeps work once the interface leaves. `background` posts
`desk::Request::Background`, and the connection's end then leaves the prompts waiting. A later
connection back to those sessions is `serve::rejoin`: attached at once, given the ring and the
questions waiting, then served; it leaves them kept again if it moves them to the background once
more, else their work stops.

**In short.** `llx` at a terminal runs its sessions in one daemon per config directory (found or
started, replaced when of another build and idle); `--no-daemon` and `exec` run in process. The
interface speaks only the protocol (L7), over a socket or channels. A session takes its place from
the client; questions belong to it; its events are numbered and kept in a ring. The catalog is
read again when a refresh wrote a newer one (looked at each minute, models.dev asked at most
hourly). Left for later: `daemon.log` rotation (the log, `logs/llx.log`, has its own); one desk
per directory shared by two terminals at once.

### Desks apart

What is the process's in a terminal's own llx is each desk's in the daemon:

- **Questions.** One question at a time per interface, not per process (ARCHITECTURE.md K3): each
  desk's lanes and agents ask on its own lock (`Shared::asking`); only `exec`'s terminal asks on
  the process's. A question waiting in sessions moved to the background holds nothing else.
- **Idle.** A daemon of another build is stopped only when `Status::idle`: no interface attached,
  no lane open, no desk kept, no task working, no monitor or schedule (`status`'s `kept` and
  `jobs`, absent from an older daemon, whose `lanes` still say it is in use). Otherwise the
  session runs in process, with the note. An interface is attached while its connection lasts
  (`Host::attach` held by the connection's end, `run::Left`), not while its desk does.
- **Coming back.** A client is attached to the sessions kept in its directory only when its
  command line is empty (a plain `llx`) and it is of the daemon's build: a task, `resume`,
  `--model`, `--image`, `-C` or any other option opens a new desk built from it, and the kept
  sessions wait for a plain `llx`. They keep the place they were opened in (environment): their
  work runs there.
- **One desk per session.** The lanes open in every desk are one table (`desk::ending`'s
  `OPEN`): a session open at one terminal is refused at another (`session … is open at another
  terminal`), whether resumed on the command line or from the list; in its own desk it is shown.
- **Waking.** The desk waits on one channel: its requests (forwarded from the connection's) and
  its lanes' ends (`Leaving`), never a timer.
