# llx as a daemon and a thin client (design, built)

One `llx serve` per user owns everything that works: sessions (lanes), agents, jobs, models and
and sign-ins. `llx` at a terminal is a client: it
draws, reads keys, and speaks to the daemon over a socket. Several terminals, several projects,
one place where work goes on when a terminal closes.

Sources read: an agent's app-server (its daemon and TUI client), Localix's API
server and UI (`docs/internals/architecture*`, `client-server-sync.md`, the wake queue), and llx's
own seam (`apps/tui/src/event.rs`, `apps/server/src/server/ui.rs`, `desk.rs`).

## What we take, and from whom

| Decision | Taken from | Why |
|---|---|---|
| One daemon per user, started by the first client, found at a fixed socket path; a probe with a short timeout, else spawn | An agent app-server (a 50 ms probe) | No setup step; a second terminal attaches |
| Unix socket in a `0700` directory, socket `0600`, no token; `flock` against a second daemon | An agent app-server | Same-user only, std only |
| One client interface, two transports: in process (channels) and remote (socket) | An agent app-server's client | `exec`, tests and `--no-daemon` keep working with no socket |
| A request the daemon asks a person (approval, question, choice, a line) belongs to the session, not the client: sent to every attached client, first answer wins, `resolved` tells the others; replayed to a client that attaches; aborted when the task stops | An agent app-server (server requests), Localix (HITL replay) | A closed terminal never loses a question; two terminals never both answer |
| The person's input goes to a queue on the daemon (the lane's queue), not a reply channel held by the UI | Localix (wake queue), llx's own `packages/queue` | The daemon owns all state; the client only shows it |
| A bounded event ring; a client that attaches replays what it keeps, and a gap means "read again" | Localix (`epoch:seq`, `transport.gap`) | Reattach without a durable log |
| Live-only events (a hands' step) are never replayed | Localix (replay tiers) | Replaying deltas corrupts what is drawn |
| A slow client is cut off, not waited for | An agent app-server, Localix | The daemon never blocks on a terminal |
| Session settings (directory, environment, mode, model) live on the session; the client gives them when it opens one | An agent app-server (thread params), Localix (session cwd) | A lane is a project; two clients, two projects |
| Signals never interrupt a task: Ctrl-C becomes `session/interrupt` | An agent app-server | One daemon, many terminals |
| No idle exit: the daemon lives until stopped | An agent app-server | Schedules and monitors outlive every terminal |

What we do not take: WebSocket framing (newline-delimited JSON is enough on a local socket, and
adds no dependency), schema generation, experimental API gating, an updater loop (a version
mismatch restarts an idle daemon instead), event sourcing.

## The protocol

Newline-delimited JSON frames, JSON-RPC shaped: a request `{id, method, params}`, a response
`{id, result | error}`, a notification `{method, params}`. Both sides send requests.

- Handshake: `initialize {version, build, client: {cwd, env, theme, tty, display, options}}` →
  `{daemon_version, sessions}`. A build mismatch: the client asks an idle daemon to stop and starts
  a new one; a busy one keeps it, and the client runs in process this time, saying so.
- Client → daemon: `session/open {target: new | saved(id), cwd, task?}`, `session/attach {id}`,
  `session/detach`, `session/input {id, text, images}` (a task, a steer, a slash
  command), `session/interrupt {id}`, `session/halt {id}`, `session/list`, `request/answer {id,
  answer}`, `daemon/stop`.
- Daemon → client notifications: today's `Event`s, each scoped to a session and numbered
  (`{session, seq, event}`): header, note, task started and ended, call, result, answer, steps,
  jobs, agents, title, context left, queue changes (what waits, what was steered).
- Daemon → client requests: `approve`, `ask`, `choose`, `choose_adjustable`, `line` (a secret
  stays hidden on the client and travels once), each with the session it belongs to; `resolved
  {request}` when one is answered or aborted.

Today's reply channels map one to one: `Ready` becomes the daemon's own input queue, `TakeSteered`
becomes the queue's steer entries, `Approve`/`Ask`/`Choose`/`Line` become daemon → client
requests. Types that cross need serde: `ToolOutput`/`ToolError` (results are sent as their
report), `Approval`, `Question`/`Answer`, `Typed`, `JobRow` (its
`Instant` becomes milliseconds left), `meta::Update`, `picker::Adjustment`.

## Who owns what

| | Daemon | Client |
|---|---|---|
| Sessions, agents, jobs, queues, history | ✓ | |
| Models, sign-ins, the catalog refresh | ✓ | |
| Keys, composer, drawing, raw mode | | ✓ |
| `$EDITOR`, Ctrl+V (a picture is kept in the shared pasted folder and sent as its path) | | ✓ |
| `/copy` (the daemon sends the text, the client copies it) | | ✓ |
| `@` file list | ✓ (asked by the client) | |

A session's directory and environment become the session's (today `Shared.workspace` and the
process environment): commands run in the environment of the client that opened the session
(its `PATH`, an activated virtualenv, its keys), not the daemon's. Its hidden key variables are computed from that environment.

## Lifecycle

- `llx`: probe `$TMPDIR/llx-<uid>/<hash of the config dir>.sock` (a test home gets its own
  daemon); none → spawn `llx serve` detached (`setsid`), wait for its ready line; attach.
- `llx serve`: `flock` on `<config>/llx/daemon.lock` (the credential store's directory), bind, write a ready line, run the sessions on threads.
- Quitting while work runs asks, as Claude Code does:

  ```text
  Background work is running
  The following will stop when you exit:
    shell · Build Electron with siso remote cache
  ❯ 1. Exit and stop tasks
    2. Move to background and exit
    3. Stay
  ```

  Only moving to the background keeps them: the sessions go on in the daemon (their tasks,
  monitors and schedules), and the next `llx` in that directory shows them. A terminal closed
  without that choice stops its sessions' work, as "Exit and stop tasks" does; the daemon itself
  lives on.
- The daemon never exits by itself: schedules and monitors must outlive every terminal. It ends
  with `llx serve stop`, or with the machine.
- `llx serve status|stop`; `--no-daemon` runs everything in process, as today; `exec` always does.

## Steps, each shipped green

1. **The seam as a protocol, in process.** Message types with serde in `packages/protocol`, the
   client interface in `packages/client` with an in-process transport over channels
   (ARCHITECTURE.md L6-L8, W1-W8); the TUI and the desk speak only through it. Reply
   channels become requests with ids and the daemon-side input queue. No behaviour changes; every
   test stays.
2. **Sessions carry their place.** Directory, environment, hidden variables per
   session; `/copy` and the clipboard on the client. Built: `initialize` carries the client's
   `Place {cwd, env}`; the server posts it to the desk (`Request::Attach`), which builds
   its workspace there (`server/place.rs`, `Workspace::with_env`), so `bash`
   and monitors run with the client's environment less the hidden key variables; `/copy` is `Event::Copy`, written by the interface. Still the
   process's: the desk's place is the first client's (one per session, with step 4), and the
   chat model's key is read from the daemon's environment.
3. **The socket.** `llx serve`, discovery, spawn, lock, handshake, the remote transport. Pty tests get a daemon per test home.
   Built so far, not yet used by `llx`: `Client::connect` (the same reader loop as in process; a
   writer thread with a 10 s write timeout, so a server that stops reading is gone), and on the
   server `server/socket.rs` (`bind`, `accept`, `connection`: a reader, a pump and a writer per
   client; a whole replay and 1,024 live frames queued for a client, or a write stalled 10 s,
   before it is cut off; a call it cannot read answered `Failed` under its id; 64 read and not yet served
   before its reader waits) and `daemon.rs` (`socket_path`, `lock` on `<config>/daemon.lock`
   where `<config>` is the credential store's directory; a client's `status` is the binary's,
   `terminal/daemon.rs`). A connection's ends are those
   `serve` and `Peer::new` take in process; a test drives `serve` over a real socket.
   Built: `llx serve` (`commands/serve.rs`) takes the lock (retried 2 s, for a daemon that is
   ending; else exit 3), binds, prints `llx serve: ready (pid N)`, sends its stdout where its
   errors go (`daemon.log`), and runs the accept loop on threads. Each connection gets a server (`run::serving`, with the daemon's
   `Host`) and, once its `initialize` gives a place, a desk built by the same `run::run` as in
   process (`At::Served`): the client's command line (`Place.args`, parsed again; its `-C` is
   the place's directory, its pictures resolved at `Place.dir`) and environment. A client that
   goes without quitting stops its desk's lanes (`desk::ending::Detached::Stop`, the hook for
   moving them to the background). `status` (`Done::Status`: build, pid, attached, lanes open,
   tasks working) and `shutdown` (every task stopped, 2 s for them to end, the socket removed, the lock file emptied). The client, the binary's (`terminal/remote.rs`,
   `terminal/daemon.rs`): `status` with a 2 s wait; this build's → attach; another build's, idle →
   `shutdown`, wait for the connection to end, start a new one; busy → in process, said. None →
   `llx serve` in its own session (`setsid`), the ready line awaited 10 s (a lost race to another
   client's daemon is fine). The build is the version plus the binary's size and time
   (`LLX_BUILD` stands in for it in tests); `initialize` carries it and a daemon refuses another.
   `tui::Start` takes a `Connect` (channels or the socket); a server that is gone ends the
   interface. Pty tests run through a daemon per test home; the harness stops it (`llx serve
   stop`, then a kill by the pid in the lock file).
   The model a session uses is its client's: the providers' keys from its environment, and
   each option a `LLX_*` variable may give (`LLX_PROVIDER`, `LLX_MODEL`, `LLX_REASONING`,
   `LLX_NO_ADVICE`) the client's variable unless its command line, as clap reads it, gives the
   option (`commands::serve::FROM_ENV`; a daemon is started without them).

4. **Attach and detach.** The event ring, pending requests replayed, the quit
   question (stop, move to the background, stay), `status`/`stop`, version restart.
   Built, the server's side: a `Peer` serves any number of clients (`server/hub.rs`). Every
   event goes to each attached client, numbered per peer (`ToClient::Event { seq }`) and kept in
   a ring of the last 2,000 (and 8 MiB), unless live only: a copy, a quit, a
   prefill and a list are never kept (`server/hub/ring.rs`). Each open session's state is kept
   outside the ring too, whole: a lane's start, header and mode, an agent's start and close, the
   latest queue, total, jobs, context and facts; a newer one replaces the one before, in the ring
   too. A lane that ends says so (`Event::LaneEnded`, from its queue's end) and takes all of
   its own and its agents' with it; the interface lets it go, and what is typed for it is refused.
   A connection is attached as its first frame comes (`serve::rejoin`) and given what the ring
   keeps; when the ring no longer holds all that happened, a `ToClient::Gap` first, then the
   state of every session the ring no longer holds, so a lane moved to the background long ago
   is drawn and typed into again; then the questions waiting.
   A queue is told only when it changes. A command that panics is answered `Failed` (W8; the release build unwinds, so this
   holds there too). A question goes to every client; the first reply
   wins and every client gets `Resolved`. A closed client is dropped, never waited for; what
   came of a command goes to its connection alone. `serve` serves `Peer::new`'s client as before;
   `serve::rejoin` serves a later connection to the same peer, detached when it is gone; the
   person is given up on when the last client is. On a gap
   the interface clears its transcripts, jobs and questions and draws again from what follows.
   A cursor to attach from was dropped: a new `llx` has none, and no client reconnects within
   its own life, so every attach replays the whole ring.
   Built: the quit question. `Command::Work`'s `Done::Work` lists what runs (`running`: each
   lane working by its title,
   each monitor and schedule by name; `server/work.rs` keeps the lanes' part from their events);
   `Done::Initialized`'s `background` says whether the server keeps work once the interface
   leaves (false in process, so only "Exit and stop tasks" and "Stay" are offered).
   `Command::Background` posts `desk::Request::Background` and makes `serve` leave the prompts
   waiting when the connection ends (it returns whether it did). The hook for a desk per
   connection: the desk stops every lane's work when its requests' sender is dropped (the
   connection ended) unless `Request::Background` came first; a host needs to do nothing else.
5. and 6. (retired.)

## Built

`llx` at a terminal runs its sessions in one daemon per config directory (`llx serve`, found or
started, replaced when of another build and idle); `--no-daemon` and `exec` run in process. The
interface speaks only the protocol (L7), over a socket or channels. A session takes its place
from the client; questions belong to it (any number of interfaces, the first reply wins, replayed
on attach); its events are numbered and kept in a ring. Quitting while work runs asks: stop,
move to the background (only in the daemon), or stay; the next `llx` in a directory comes back
to the sessions moved to the background there (`server::host::Kept`, `serve::rejoin`). A served
session's providers' keys come from its client's environment (`providers::Vars`, the place's);
the catalog is read again when a refresh wrote a newer one (looked at each minute, models.dev asked
at most hourly); the `LLX_*` variables of its options are the client's too, unless its command line gives
the option. Left for later: `daemon.log` rotation (the log, `logs/llx.log`, has its own); one desk per directory shared by
two terminals at once.

### Desks apart

What is the process's in a terminal's own llx is each desk's in the daemon:

- **Questions.** One question at a time per interface, not per process (ARCHITECTURE.md K3): each
  desk's lanes and agents ask on its own lock (`Shared::asking`); only `exec`'s terminal asks on
  the process's. A question waiting in sessions moved to the background holds nothing else.
- **Idle.** A daemon of another build is stopped only when `Status::idle`: no interface attached,
  no lane open, no desk kept, no task working, no monitor or schedule (`Done::Status`'s `kept`
  and `jobs`, absent from an older daemon, whose `lanes` still say it is in use). Otherwise the
  session runs in process, with the note. An interface is attached while its connection lasts
  (`Host::attach` held by the connection's end, `run::Left`), not while its desk does.
- **Coming back.** A client is attached to the sessions kept in its directory only when its
  command line is empty (a plain `llx`): a task, `resume`, `--model`, `--image`, `-C` or any
  other option opens a new desk built from it, as before, and the kept sessions wait for a plain
  `llx`. They keep the place they were opened in (environment): their work runs there.
- **One desk per session.** The lanes open in every desk are one table (`desk::ending`'s
  `OPEN`): a session open at one terminal is refused at another (`session … is open at another
  terminal`), whether resumed on the command line or from the list; in its own desk it is shown.
- **Waking.** The desk waits on one channel: its requests (forwarded from the connection's) and
  its lanes' ends (`Leaving`), never a timer.
