import { AGENTS_DOCS, CI_DOCS, DOCS } from '../links';

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

/** The exit codes are exit.rs; the cost in the pipeline card is an illustration; the rules card
 *  uses llx's own wording of a refusal (packages/tools/src/lib.rs). */
export const CARDS: Card[] = [
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
          [['dim', '130'], 'interrupted', ['dim', 'Ctrl-C or SIGTERM (128 + the signal, as shells report it)']],
        ],
        align: 'rll',
      },
    ],
  },
  {
    title: 'A step your pipeline can branch on',
    text: 'llx exec runs one task and prints JSON events; the last one holds the status, your data and the cost. On GitHub, uses: rsaryev/llx@v0 runs the same step and returns the answer, the status and the spend.',
    panel: [
      shell(
        '$ llx exec --json --mode plan --schema @ship.json \\\n    "Safe to ship this diff? Which version bump?" > events.jsonl\n$ echo $?',
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
      { grid: [[['bad', '2'], ['dim', 'exhausted: out of turns, time or money before a result']]] },
    ],
  },
  {
    title: 'Runs under your rules',
    text: 'A mode is a set of rules: what runs without asking, what is refused in every mode. They come from your command line and settings, never from the repository. plan runs read-only in the system sandbox.',
    link: { href: `${DOCS}/modes`, label: 'Modes and trust' },
    panel: [
      {
        grid: [
          [['hi', 'default'], ['dim', 'each command is asked']],
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
    text: 'One task, split: each agent is a session of its own, running at once, answering back to the one that started it. Jobs watch a command or run on a schedule and report back while the work goes on.',
    panel: [
      {
        ui: [
          [['acc', '• '], ['hi', 'Spawned'], ['', ' median · why the median tests fail']],
          [['acc', '↳ '], ['hi', 'Spawned'], ['', ' mean · mean() on an empty list']],
          [['ok', '● '], ['', 'Agent "median" finished · 6s']],
          [['ok', '● '], ['', 'Agent "mean" finished · 6s']],
          [['ok', '• '], ['hi', 'Ran'], ['', " python3 - <<'EOF'  "], ['dim', '# stats.py, the median']],
          [['', '• Fixed '], ['acc', 'stats.py:15'], ['', ': averages the two middle values.']],
        ],
      },
    ],
  },
  {
    title: 'Any provider, any model',
    text: 'Pick one of 1,000+ models from 200+ providers with /model, from models.dev\'s catalog, refreshed as it grows: a ChatGPT sign-in, OpenRouter, or a key for the provider you use. The reasoning level with ← →, saved for every run after.',
    panel: [
      {
        ui: [
          [['hi', '[OpenAI 2]'], ['dim', ' · Anthropic 0 · OpenRouter 5 · xAI 0 · DeepSeek 0']],
          [['dim', 'Search: terra▏  7 matches']],
          [['sel', '❯ GPT-6.1 Terra      gpt-6.1-terra      ← low →']],
          [['', '  GPT-6.1 Terra Pro  gpt-6.1-terra-pro    medium']],
          [['dim', 'tab provider · ←→ reasoning · enter select']],
          [['dim', 'Now openai · gpt-6.1-terra · low']],
        ],
      },
    ],
  },
  {
    title: 'For other agents: decide in a second',
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
    title: 'Five tools, one binary under 4 MB',
    text: 'bash reads, searches and runs the checks; apply_patch changes files; ask puts a question to you; agents and jobs do the rest. No plugins, no skills, no servers to run. At your terminal it is a minimal agent: sign in with /login, then give it a task.',
    link: { href: DOCS, label: 'The docs' },
    panel: [
      {
        grid: [
          [['hi', 'bash'], ['dim', 'read, search, run the checks']],
          [['hi', 'apply_patch'], ['dim', 'add, change, delete, move files']],
          [['hi', 'ask'], ['dim', 'a question to the person']],
          [['hi', 'agents'], ['dim', 'other agents, in parallel']],
          [['hi', 'jobs'], ['dim', 'monitors and schedules']],
          [['acc', 'result'], ['dim', 'the data of --schema, asked for at the end']],
        ],
      },
    ],
  },
];
