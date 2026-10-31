#!/usr/bin/env bun
// The benchmark: an agent implements a Bun project against acceptance tests written in advance.
//
// Each run copies tasks/<task>/project into a fresh git repository outside this one and starts an
// agent there (llx, or Codex to compare), then puts back the tests, package.json and tsconfig.json
// from the pristine copy (so an edit through the shell cannot help), runs `bun test`, and records
// whether the task was solved, the time, the turns, the cost at the model's price, the cache, and
// every log of the run. Several agents' runs are interleaved and run at the same time.
//
// results/<date>_<time>-<task>/ holds run.json, summary.md and a directory of logs per run;
// HISTORY.md is made again from all of them. See README.md.

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { parseArgs } from "node:util";
import { type Agent, KINDS, type Options, agent, command, did, extraLogs } from "./lib/agents";
import { type Request, pricer } from "./lib/price";
import { type Result, type Run, history, summary } from "./lib/report";
import { check, prepare, restore, score, sh, tampered } from "./lib/task";

const HERE = import.meta.dir;
const RESULTS = path.join(HERE, "results");
const PROMPT =
  "Implement every module in src/ as README.md specifies. The acceptance tests in test/ are " +
  "fixed: do not change them, package.json or tsconfig.json. Run `bun test` to check your work; " +
  "the task is done when every test passes.";

const gz = (from: string, to: string) => fs.writeFileSync(to, Bun.gzipSync(fs.readFileSync(from)));

/** The logs of a run as they came, and what became of the task after it. */
type Logs = { events: string; stderr: string; extra: { name: string; from: string }[] };
type After = {
  exit: number | null; wall_s: number; pass: number; fail: number; tests: number;
  tampered: string[]; test: string; src: string | null;
};

/** A run's logs kept in `dir`, compressed, and what they say: the one record of a run. */
function keep(dir: string, a: Agent, i: number, logs: Logs, after: After, price: (r: Request[]) => number): Result {
  fs.mkdirSync(dir, { recursive: true });
  const d = did(a.kind, logs.events, logs.stderr, Object.fromEntries(logs.extra.map((x) => [x.name, x.from])));
  gz(logs.events, path.join(dir, "events.jsonl.gz"));
  if (fs.existsSync(logs.stderr)) gz(logs.stderr, path.join(dir, "stderr.txt.gz"));
  for (const x of logs.extra) gz(x.from, path.join(dir, `${x.name}.gz`));
  fs.writeFileSync(path.join(dir, "test.txt.gz"), Bun.gzipSync(Buffer.from(after.test)));
  if (after.src) sh(["tar", "czf", path.join(dir, "src.tar.gz"), "-C", path.dirname(after.src), path.basename(after.src)]);
  fs.writeFileSync(path.join(dir, "answer.md"), d.answer.trim() + "\n");
  const sum = (key: keyof Request) => d.requests.reduce((s, r) => s + r[key], 0);
  const [input, cached] = [sum("input"), sum("cached")];
  return {
    label: a.label, kind: a.kind, run: i, exit: after.exit, wall_s: after.wall_s, pass: after.pass, fail: after.fail,
    solved: after.pass === after.tests, tampered: after.tampered, status: d.status, turns: d.requests.length,
    input_tokens: input, cached_tokens: cached, output_tokens: sum("output"),
    cost_usd: Math.round(price(d.requests) * 1e4) / 1e4, cache_pct: input ? Math.round((100 * cached) / input) : 0,
    calls: d.calls, failed_commands: d.failed_commands, patches: d.patches, failed_patches: d.failed_patches,
    model_s: d.model_s, advised: d.advised,
  };
}

async function one(a: Agent, i: number, o: Options, project: string, scratch: string, out: string, tests: number,
  price: (r: Request[]) => number): Promise<Result> {
  const work = path.join(scratch, `${a.label}-${i}`), logs = path.join(scratch, `${a.label}-${i}.logs`);
  prepare(project, work);
  fs.mkdirSync(logs);
  const [events, stderr] = [path.join(logs, "events.jsonl"), path.join(logs, "stderr.txt")];
  const t0 = performance.now();
  // The agent's own limit, and this one past it for an agent that has none (Codex).
  const p = Bun.spawn(command(a, work, o, PROMPT), {
    stdin: "ignore", stdout: Bun.file(events), stderr: Bun.file(stderr), timeout: (o.timeout + 60) * 1000,
  });
  const exit = await p.exited;
  const wall_s = Math.round((performance.now() - t0) / 100) / 10;
  const touched = tampered(project, work);
  restore(project, work);
  const t = score(work);
  return keep(path.join(out, `${a.label}-${i}`), a, i, { events, stderr, extra: extraLogs(a, events) },
    { exit, wall_s, pass: t.pass, fail: t.fail, tests, tampered: touched, test: t.output, src: path.join(work, "src") }, price);
}

const stamp = (d: Date, sep: string, mid: string) => {
  const p = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}${mid}${p(d.getHours())}${sep}${p(d.getMinutes())}`;
};

const writeHistory = () => fs.writeFileSync(path.join(HERE, "HISTORY.md"), history(RESULTS));

async function main(): Promise<number> {
  const tasks = fs.readdirSync(path.join(HERE, "tasks")).sort();
  const { values: v } = parseArgs({
    options: {
      task: { type: "string", default: "evalkit" },
      agent: { type: "string", multiple: true },
      repeat: { type: "string", default: "1" },
      provider: { type: "string", default: "openai" },
      model: { type: "string", default: "gpt-6-luna" },
      reasoning: { type: "string", default: "low" },
      timeout: { type: "string", default: "1200" },
      parallel: { type: "string", default: "4" },
      note: { type: "string", default: "" },
      "no-advice": { type: "boolean", default: false },
      check: { type: "boolean", default: false },
      history: { type: "boolean", default: false },
      help: { type: "boolean", short: "h", default: false },
    },
  });
  if (v.help) {
    console.log(`usage: bun benchmark/run.ts [options]  (see benchmark/README.md)
  --task ${tasks.join("|")}     the task (default evalkit)
  --agent [label=]kind[:path]  an agent (${KINDS.join(", ")}); repeat to compare (default: llx, this
                               repository's target/release/llx)
  --repeat N                   runs an agent (default 1)
  --note TEXT                  a line for the summary: what this run is for
  --no-advice                  llx without its advisor (Jev), which needs an OpenRouter key
  --timeout S, --parallel N, --provider, --model, --reasoning
  --check                      only check the task: the reference passes, the stubs next to none
  --history                    only make HISTORY.md again from results/`);
    return 0;
  }
  if (v.history) return (writeHistory(), 0);
  if (!tasks.includes(v.task!)) throw new Error(`--task: one of ${tasks.join(", ")}`);
  const taskDir = path.join(HERE, "tasks", v.task!);
  if (v.check) return check(taskDir);
  const o: Options = { provider: v.provider!, model: v.model!, reasoning: v.reasoning!, timeout: Number(v.timeout), advice: !v["no-advice"] };
  const agents = (v.agent ?? ["llx"]).map(agent);
  if (new Set(agents.map((a) => a.label)).size < agents.length) throw new Error("--agent: two agents with one label");
  const repeat = Number(v.repeat), parallel = Number(v.parallel);
  const project = path.join(taskDir, "project");
  const tests = score(path.join(taskDir, "ref")).pass; // the reference passes every test
  const price = pricer(o.provider, o.model);
  const now = new Date();
  const out = path.join(RESULTS, `${stamp(now, "", "_")}-${v.task}`);
  fs.mkdirSync(out, { recursive: true });
  const macos = os.type() === "Darwin" ? sh(["sw_vers", "-productVersion"]).stdout.trim() : os.release();
  const run: Run = {
    when: stamp(now, ":", " "), task: v.task!, tests, provider: o.provider, model: o.model, reasoning: o.reasoning,
    advice: o.advice, timeout_s: o.timeout, repeat, note: v.note!, machine: `${os.type()} ${os.arch()} ${macos}`,
    load: Math.round(os.loadavg()[0] * 10) / 10, agents, runs: [],
  };
  console.log(`${run.when} ${run.task} (${tests} tests): ${agents.map((a) => a.label).join(", ")}; load ${run.load}; ${out}`);
  const jobs = Array.from({ length: repeat }, (_, i) => agents.map((a) => [a, i] as const)).flat();
  const scratch = fs.mkdtempSync(path.join(os.tmpdir(), "llx-bench-"));
  try {
    let next = 0;
    const worker = async () => {
      while (next < jobs.length) {
        const at = next++;
        const [a, i] = jobs[at];
        run.runs[at] = await one(a, i, o, project, scratch, out, tests, price);
        console.log(JSON.stringify(run.runs[at]));
      }
    };
    await Promise.all(Array.from({ length: Math.min(parallel, jobs.length) }, worker));
  } finally {
    fs.rmSync(scratch, { recursive: true, force: true });
  }
  fs.writeFileSync(path.join(out, "run.json"), JSON.stringify(run, null, 1) + "\n");
  fs.writeFileSync(path.join(out, "summary.md"), summary(run));
  writeHistory();
  console.log("\n" + summary(run));
  return 0;
}

process.exit(await main());
