// A run's summary.md, and HISTORY.md made again from every run's run.json.

import * as fs from "node:fs";
import * as path from "node:path";
import type { Agent } from "./agents";

/** One agent's run of the task, as run.json keeps it. */
export type Result = {
  label: string; kind: string; run: number; exit: number | null; wall_s: number;
  pass: number; fail: number; solved: boolean; tampered: string[]; status: string | null;
  turns: number; input_tokens: number; cached_tokens: number; output_tokens: number;
  cost_usd: number; cache_pct: number; calls: number; failed_commands: number;
  patches: number; failed_patches: number; model_s: number | null; advised: number | null;
};

/** A run of run.ts: what was run, where, and each agent's results. */
export type Run = {
  when: string; task: string; tests: number; provider: string; model: string; reasoning: string;
  advice: boolean; timeout_s: number; repeat: number; note: string; machine: string; load: number | string;
  agents: Agent[]; runs: Result[];
};

const shown = (x: number | null, places = 1) => (x === null ? "—" : String(Math.round(x * 10 ** places) / 10 ** places));
const version = (a: Agent) => a.commit ?? a.version;
const table = (head: string[], rows: (string | number)[][]) =>
  [`| ${head.join(" | ")} |`, `|${"---|".repeat(head.length)}`, ...rows.map((r) => `| ${r.join(" | ")} |`)];

/** The median, rounded to four places. */
function median(values: number[]): number {
  const v = [...values].sort((a, b) => a - b), mid = Math.floor(v.length / 2);
  const m = v.length % 2 ? v[mid] : (v[mid - 1] + v[mid]) / 2;
  return Math.round(m * 1e4) / 1e4;
}

export function summary(run: Run): string {
  const lines = [`# ${run.task}, ${run.when}`, ""];
  if (run.note) lines.push(run.note, "");
  lines.push(
    `${run.tests} tests; ${run.model} (${run.provider}), reasoning ${run.reasoning}; llx's advisor (Jev) ` +
      `${run.advice ? "on" : "off"}; ${run.timeout_s} s a run, runs an agent: ${run.repeat}; ${run.machine}, load ${run.load}.`,
    "",
    ...table(["agent", "kind", "version", "bytes", "sha256"],
      run.agents.map((a) => [a.label, a.kind, version(a), a.bytes ?? "—", a.sha256 ?? "—"])),
    "",
    ...table(
      ["agent", "run", "solved", "pass", "wall s", "model s", "the rest s", "turns", "Jev", "cost $", "cache %",
        "input tokens", "output tokens", "calls", "failed commands", "patches", "failed patches", "tampered"],
      run.runs.map((r) => [
        r.label, r.run, r.solved ? "yes" : "no", `${r.pass}/${run.tests}`, shown(r.wall_s), shown(r.model_s),
        r.model_s === null ? "—" : shown(r.wall_s - r.model_s), r.turns, r.advised ?? "—", r.cost_usd.toFixed(4),
        r.cache_pct, r.input_tokens, r.output_tokens, r.calls, r.failed_commands, r.patches, r.failed_patches,
        r.tampered.length,
      ]),
    ),
    "",
    "Each run's logs are in its directory: `events.jsonl.gz` (the agent's own events), `stderr.txt.gz`, " +
      "`session.jsonl.gz` (Codex's session), `test.txt.gz` (`bun test` after the fixed files were restored), " +
      "`src.tar.gz` (the code it wrote) and `answer.md` (its last words).",
  );
  return lines.join("\n") + "\n";
}

/** HISTORY.md: a row per agent per run of run.ts, its medians over the runs. */
export function history(results: string): string {
  const rows: (string | number)[][] = [];
  for (const name of fs.readdirSync(results).sort()) {
    const file = path.join(results, name, "run.json");
    if (!fs.existsSync(file)) continue;
    const run: Run = JSON.parse(fs.readFileSync(file, "utf8"));
    for (const a of run.agents) {
      const rs = run.runs.filter((r) => r.label === a.label);
      const med = (key: (r: Result) => number) => median(rs.map(key));
      const jev = a.kind !== "llx" ? "—" : run.advice ? `on, ${med((r) => r.advised ?? 0)}` : "off";
      rows.push([
        `[${run.when}](results/${name}/summary.md)`, run.task, a.label, version(a), jev, rs.length,
        `${rs.filter((r) => r.solved).length}/${rs.length}`, `${med((r) => r.pass)}, ${Math.min(...rs.map((r) => r.pass))} of ${run.tests}`,
        med((r) => r.wall_s), med((r) => r.turns), med((r) => r.cost_usd).toFixed(4), med((r) => r.cache_pct),
        med((r) => r.output_tokens), run.load,
      ]);
    }
  }
  return [
    "# Benchmark history", "",
    "A row per agent per run of `run.ts`, its medians over the runs: whether it solved the task " +
      "(every test passed), the time it took, the turns (model requests), llx's advisor (Jev): on or " +
      "off, and how often it sent the model back, the cost at the model's price, the share of input " +
      "read from the prompt cache. Made again from `results/*/run.json` by each run " +
      "(`bun benchmark/run.ts --history`).", "",
    ...table(["when", "task", "agent", "version", "Jev", "runs", "solved", "pass (median, min)", "wall s", "turns",
      "cost $", "cache %", "output tokens", "load"], rows),
  ].join("\n") + "\n";
}
