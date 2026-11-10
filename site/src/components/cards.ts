import { AGENTS_DOCS, CI_DOCS, DOCS, EXAMPLES_DOCS, GITHUB } from '../links';

/** A word's tone in a panel (the terminal's colours); '' is plain. */
export type Tone = 'dim' | 'hi' | 'ok' | 'bad' | 'acc' | 'sel' | '';
/** A cell of a table: its text, or its tone and text. */
export type Cell = string | [Tone, string];
/** A line of a drawn screen: its parts, each with a tone. */
export type Line = [Tone, string][];

/** What a panel shows, a block after a block. A `shell` session or `json` is plain text the
 *  highlighter colours; `out` is a line of output in one tone; `grid` is a table in real columns
 *  (`align`: `l` or `r` for each); `ui` is a screen of the interface, drawn by tone. */
export type Block =
  | { lang: 'shell' | 'json'; text: string }
  | { out: string; tone?: Tone }
  | { grid: Cell[][]; align?: string }
  | { ui: Line[] };

export type Card = { title: string; text: string; panel: Block[]; link?: { href: string; label: string } };

const shell = (text: string): Block => ({ lang: 'shell', text });
const out = (text: string, tone: Tone = 'hi'): Block => ({ out: text, tone });

/** The benchmark card is benchmark/HISTORY.md's runs of 2026-11-10 02:03 and 02:04; the exit codes
 *  are exit.rs; the cost in the pipeline card is an illustration; the rules card
 *  uses llx's own wording of a refusal (packages/tools/src/lib.rs, apps/server/src/rules.rs), and
 *  the search card is a run of rg into llx decide on this repository. */
export const CARDS: Card[] = [
  {
    title: 'Measured beside Claude Code',
    text: 'The same task, model and key, started in the same minute, scored by tests written in advance. On the larger task llx finished in 60% of the time for a ninth of the cost; on the small one the two are even. One run of each, with every log in the repository.',
    link: { href: `${GITHUB}/tree/main/benchmark`, label: 'The benchmark and every run' },
    panel: [
      out('Claude Haiku 5.5 · the same key, the same minute', 'dim'),
      {
        grid: [
          [['dim', 'evalkit'], ['hi', 'llx'], ['ok', '238/238'], ['hi', '286 s'], ['dim', '14 turns'], ['hi', '$0.050']],
          [['dim', '238 tests'], ['dim', 'Claude Code'], ['ok', '238/238'], ['dim', '478 s'], ['dim', '25 turns'], ['dim', '$0.46']],
          [['dim', 'shopkit'], ['hi', 'llx'], ['ok', '69/69'], ['hi', '69 s'], ['dim', '5 turns'], ['hi', '$0.012']],
          [['dim', '69 tests'], ['dim', 'Claude Code'], ['ok', '69/69'], ['dim', '67 s'], ['dim', '4 turns'], ['dim', '$0.011']],
        ],
        align: 'llrrrr',
      },
    ],
  },
  {
    title: 'Ends every task with an exit code',
    text: 'A task ends when the model answers; with --schema, llx asks it once more for data matching your JSON Schema. The exit code says how it ended.',
    link: { href: CI_DOCS, label: 'The CI guide' },
    panel: [
      {
        grid: [
          [['dim', '0'], ['ok', 'done'], ['dim', 'the model answered']],
          [['dim', '1'], ['bad', 'no_data'], ['dim', 'the data missed the schema']],
          [['dim', '2'], 'exhausted', ['dim', 'out of turns, time or money']],
          [['dim', '3'], 'error', ['dim', 'no key, no network, a bad argument']],
          [['dim', '130'], 'interrupted', ['dim', 'Ctrl-C, SIGTERM or SIGHUP']],
        ],
        align: 'rll',
      },
    ],
  },
  {
    title: 'A step your pipeline can branch on',
    text: 'llx exec runs one task and prints JSON events; the last one holds the status, your data and the cost. On GitHub, uses: rsaryev/llx-cli@v0 runs the same step and returns the answer, the status and the spend.',
    link: { href: EXAMPLES_DOCS, label: 'Ready-made workflows' },
    panel: [
      shell(
        '$ git diff main... | llx exec --json --mode plan --schema @ship.json \\\n    "Safe to ship this diff? Which version bump?" > events.jsonl\n$ echo $?',
      ),
      out('0', 'ok'),
      shell(
        `$ jq 'select(.type == "run.completed") | {status, exit_code, data, cost_usd: .usage.cost_usd}' events.jsonl`,
      ),
      {
        lang: 'json',
        text: `{
  "status": "done",
  "exit_code": 0,
  "data": { "bump": "patch", "ship": true },
  "cost_usd": 0.0041
}`,
      },
    ],
  },
  {
    title: 'Three limits, the first reached ends the task',
    text: '--max-turns, --timeout (30 minutes by default) and --budget in dollars. A task that runs into one ends with exit 2, exhausted, so a pipeline can tell it from a failure.',
    panel: [
      shell('$ llx exec --timeout 600 --budget 0.50 --max-turns 40 \\\n    "Fix the failing test"\n$ echo $?'),
      { grid: [[['bad', '2'], ['dim', 'exhausted: out of turns, time or money before an answer']]] },
    ],
  },
  {
    title: 'Runs under your rules',
    text: 'A mode is a set of rules and what its commands reach. By default, commands run in the system sandbox, which writes only inside the workspace (never its .git; on Linux they only read) and opens no network; what must reach further is asked. Rules come from your command line and settings, never from the repository, and a deny refuses in every mode.',
    link: { href: `${DOCS}/modes`, label: 'Modes and trust' },
    panel: [
      {
        grid: [
          [['hi', 'default'], ['dim', 'the workspace, in a sandbox; beyond it, asked']],
          [['hi', 'plan'], ['dim', 'read-only, in a sandbox']],
          [['hi', 'bypass'], ['dim', 'everything runs (-y)']],
        ],
      },
      shell(`$ llx exec --allow 'bash(cargo test *)' \\\n    --deny 'bash(git push *)' "Fix the failing test"`),
      {
        ui: [
          [['bad', '• '], ['hi', 'Ran'], ['', ' git push']],
          [['bad', '  └ not allowed: refused by the rule `bash(git push *)`']],
        ],
      },
    ],
  },
  {
    title: 'Agents in parallel, jobs in the background',
    text: 'One task, split: each agent is a session of its own, running at once, answering back to the one that started it. Jobs wait for something to happen, keep a long command running, or run on a schedule, and report back once, while the work goes on.',
    panel: [
      {
        ui: [
          [['acc', '• '], ['hi', 'Spawned'], ['', ' median · why the median tests fail']],
          [['acc', '↳ '], ['hi', 'Spawned'], ['', ' mean · mean() on an empty list']],
          [['ok', '● '], ['', 'Agent "median" finished · 6s']],
          [['ok', '● '], ['', 'Agent "mean" finished · 6s']],
          [['ok', '• '], ['hi', 'Patched'], ['', ' stats.py']],
          [['', '• Fixed '], ['acc', 'stats.py:15'], ['', ': averages the two middle values.']],
        ],
      },
    ],
  },
  {
    title: 'Any provider, any model',
    text: '1,000+ models from 200+ providers, from models.dev\'s catalog, refreshed as it grows. /login signs in to any of them: a ChatGPT sign-in, OpenRouter, or a key for the provider you use. /model switches among those you are signed in to, the reasoning level with ← →, saved for every run after.',
    panel: [
      {
        ui: [
          [['hi', '[OpenAI 2]'], ['dim', ' · Anthropic 0 · OpenRouter 5 · xAI 0 · DeepSeek 0']],
          [['dim', 'Search: terra▏  7 matches']],
          [['sel', '❯ GPT-6.1 Terra      gpt-6.1-terra      ← low →']],
          [['', '  GPT-5.6 Terra      gpt-5.6-terra        auto']],
          [['dim', 'tab provider · ←→ reasoning · enter select · esc clear']],
          [['dim', 'Now openai · gpt-6.1-terra · low (saved as the default in']],
          [['dim', '~/.config/llx/settings.json).']],
        ],
      },
    ],
  },
  {
    title: 'For other agents: decide in about a second',
    text: 'Claude Code, another agent or a script can call llx from its shell. llx decide picks one of the options you give, alone on stdout; llx exec hands a task to a helper on any model.',
    link: { href: AGENTS_DOCS, label: 'llx for other agents' },
    panel: [
      shell('$ git diff | llx decide --choices patch,minor,major \\\n    "Which version bump?"'),
      out('patch', 'ok'),
      shell(`$ echo 'App crashes when the cart is empty' |\n    llx decide --choices bug,feature,question "Label it"`),
      out('bug', 'ok'),
      shell('$ git diff | llx exec --mode plan "A one-line commit message"'),
      out('Fix median calculation for even-length lists', ''),
    ],
  },
  {
    title: 'A search of the code by what it does',
    text: 'rg narrows the files by a few likely words; llx decide reads each and keeps those that do the thing, so code that does it under another name is found and a file that only mentions the words is left out. Two primitives, one pipe: the agent builds the search its question needs.',
    link: { href: AGENTS_DOCS, label: 'In other agents' },
    panel: [
      shell(`$ rg -l -i 'retry|backoff' src |\n    xargs llx decide "Does this file send a failed HTTP request again?"`),
      {
        grid: [
          [['ok', 'yes'], ['hi', 'packages/providers/kit/src/retry.rs']],
          [['ok', 'yes'], ['hi', 'packages/providers/openai/src/responses.rs']],
          [['ok', 'yes'], ['hi', 'packages/providers/openrouter/src/client.rs']],
        ],
      },
      out('12 more files: no (1.2 s, 15 files read by the decision model)', 'dim'),
    ],
  },
  {
    title: 'A skill for any agent',
    text: 'One skill teaches Claude Code, Codex, Cursor and other agents what llx does and when it pays, with examples to run as they are and each command\'s help as the installed llx prints it. They hand reading to a helper and ask llx decide rather than read every file themselves.',
    link: { href: `${GITHUB}/tree/main/skills/llx`, label: 'The skill' },
    panel: [
      shell('$ npx skills add rsaryev/llx-cli'),
      shell('$ llx decide "Bug report or feature request?" \\\n    --choices bug,feature issues/*'),
      {
        grid: [
          [['ok', 'bug'], ['dim', 'issues/01.txt']],
          [['ok', 'bug'], ['dim', 'issues/02.txt']],
          [['acc', 'feature'], ['dim', 'issues/05.txt']],
          [['acc', 'feature'], ['dim', 'issues/06.txt']],
        ],
      },
      out('20 files, one command, none read by Claude: $0.0005', 'dim'),
    ],
  },
  {
    title: 'Five tools, one binary under 4 MB',
    text: 'bash reads, searches and runs the checks; apply_patch changes files; ask puts a question to you; agents and jobs do the rest. It takes no plugins, no skills and no MCP servers. At your terminal it is a minimal agent: sign in with /login, then give it a task.',
    link: { href: DOCS, label: 'The docs' },
    panel: [
      {
        grid: [
          [['hi', 'bash'], ['dim', 'read, search, run the checks']],
          [['hi', 'apply_patch'], ['dim', 'add, change, delete, move files']],
          [['hi', 'ask'], ['dim', 'a question to the person']],
          [['hi', 'agents'], ['dim', 'other agents, in parallel']],
          [['hi', 'jobs'], ['dim', 'monitors, schedules, commands in the background']],
          [['acc', 'result'], ['dim', 'the data of --schema, asked for at the end']],
        ],
      },
    ],
  },
];
