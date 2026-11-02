#!/usr/bin/env bun
// The search benchmark: `llx grep` against questions about this repository whose answers are known
// (queries.json), by group, beside `rg` as an agent would first type it.
//
// The repository at the queries' commit is put in a fresh git repository outside this one (so
// `git ls-files` lists what it did then), and each question is asked in each mode:
//
// - `words`:  `llx grep --lexical`, the ranking by the question's words alone (no network);
// - `judged`: `llx grep`, the words' best files read by the decision model (an OpenRouter key);
// - `rg`:     `rg -c -i PATTERN`, files by their count of matches, the pattern an agent would type.
//
// Each answer is scored against the files it must name (`must`) and those it may (`ok`): recall at
// 5 and 10, precision of its first 5, the reciprocal rank of its first right file, for a question
// with no answer whether it named none, the time, the cost, and the bytes an agent would read.
// results/<date>_<time>[-label].json keeps every answer; HISTORY.md a line per run (--record).

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { parseArgs } from "node:util";

const HERE = import.meta.dir;
const ROOT = path.resolve(HERE, "../..");

type Query = {
  id: string;
  group: string;
  query: string;
  rg: string;
  paths?: string[];
  must: string[];
  ok: string[];
};
type Answer = {
  files: string[];
  scores?: number[];
  unsure?: string[];
  millis: number;
  cost: number;
  bytes: number;
  failed?: string;
};
type Scored = Answer & {
  id: string;
  group: string;
  mode: string;
  r5: number;
  r10: number;
  p5: number;
  rr: number;
  none?: boolean;
};

const { values: opts } = parseArgs({
  options: {
    llx: { type: "string", default: path.join(ROOT, "target/release/llx") },
    mode: { type: "string", multiple: true },
    only: { type: "string", multiple: true },
    label: { type: "string" },
    // Passed to `llx grep` as they are, to compare its settings: `--grep-arg=--judge --grep-arg=40`.
    "grep-arg": { type: "string", multiple: true },
    record: { type: "boolean", default: false },
  },
});
const MODES = opts.mode?.length ? opts.mode : ["words", "judged", "rg"];
const spec = JSON.parse(fs.readFileSync(path.join(HERE, "queries.json"), "utf8"));
const queries: Query[] = spec.queries.filter(
  (q: Query) => !opts.only?.length || opts.only.some((o) => q.id.startsWith(o)),
);

function sh(cmd: string[], cwd: string): { out: string; err: string; code: number; millis: number } {
  const started = performance.now();
  const run = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe", env: process.env });
  return {
    out: run.stdout.toString(),
    err: run.stderr.toString(),
    code: run.exitCode ?? -1,
    millis: Math.round(performance.now() - started),
  };
}

/** The repository at `commit`, as a git repository of its own in the temporary directory. */
function snapshot(commit: string): string {
  const dir = path.join(os.tmpdir(), `llx-grepbench-${commit}`);
  if (fs.existsSync(path.join(dir, ".git"))) return dir;
  fs.rmSync(dir, { recursive: true, force: true });
  fs.mkdirSync(dir, { recursive: true });
  const archive = Bun.spawnSync(["git", "archive", commit], { cwd: ROOT, stdout: "pipe" });
  if (archive.exitCode !== 0) throw new Error(`git archive ${commit} failed`);
  Bun.spawnSync(["tar", "-x", "-C", dir], { stdin: archive.stdout });
  for (const cmd of [
    ["git", "init", "-q"],
    ["git", "add", "-A"],
    ["git", "-c", "user.email=bench@llx", "-c", "user.name=bench", "commit", "-qm", commit],
  ])
    sh(cmd, dir);
  return dir;
}

function ask(mode: string, q: Query, repo: string): Answer {
  const paths = q.paths ?? [];
  if (mode === "rg") {
    const run = sh(["rg", "-c", "-i", "-e", q.rg, ...paths], repo);
    const files = run.out
      .split("\n")
      .filter(Boolean)
      .map((line) => {
        const at = line.lastIndexOf(":");
        return { file: line.slice(0, at), n: Number(line.slice(at + 1)) };
      })
      .sort((a, b) => b.n - a.n || a.file.localeCompare(b.file))
      .map((hit) => hit.file);
    const read = sh(["rg", "-n", "-i", "-e", q.rg, ...paths], repo);
    return { files, millis: run.millis, cost: 0, bytes: read.out.length };
  }
  const lexical = mode === "words" ? ["--lexical"] : [];
  const extra = opts["grep-arg"] ?? [];
  const run = sh([opts.llx!, "grep", "--json", ...lexical, ...extra, q.query, ...paths], repo);
  if (run.code !== 0 && run.code !== 1)
    return { files: [], millis: run.millis, cost: 0, bytes: 0, failed: run.err.trim() };
  const answer = JSON.parse(run.out);
  if (mode === "judged" && answer.by !== "judged")
    return { files: [], millis: run.millis, cost: 0, bytes: 0, failed: run.err.trim() };
  const files: string[] = answer.files.map((f: { file: string }) => f.file);
  const scores: number[] = answer.files.map((f: { score: number }) => f.score);
  const unsure: string[] = (answer.unsure ?? []).map((f: { file: string }) => f.file);
  const bytes = answer.files.reduce(
    (sum: number, f: { file: string; blocks: { text: string }[] }) =>
      sum + f.file.length + f.blocks.reduce((s, b) => s + b.text.length, 0),
    0,
  );
  return { files, scores, unsure, millis: run.millis, cost: answer.cost_usd ?? 0, bytes };
}

function scored(mode: string, q: Query, a: Answer): Scored {
  const right = (file: string) =>
    q.must.includes(file) || q.ok.some((glob) => new Bun.Glob(glob).match(file));
  const found = (k: number) =>
    q.must.length ? q.must.filter((m) => a.files.slice(0, k).includes(m)).length / q.must.length : 0;
  const first = a.files.slice(0, 5);
  const p5 = first.length ? first.filter(right).length / first.length : q.must.length ? 0 : 1;
  const at = a.files.findIndex((file) => q.must.includes(file));
  const base = { ...a, id: q.id, group: q.group, mode, r5: found(5), r10: found(10), p5, rr: 0 };
  if (!q.must.length) return { ...base, none: a.files.length === 0 };
  return { ...base, rr: at < 0 ? 0 : 1 / (at + 1) };
}

const mean = (xs: number[]) => (xs.length ? xs.reduce((s, x) => s + x, 0) / xs.length : 0);
const pct = (x: number) => `${Math.round(x * 100)}%`;

function table(results: Scored[]): string {
  const groups = [...new Set(queries.map((q) => q.group)), "all"];
  const rows = ["| mode | group | R@5 | R@10 | P@5 | MRR | none | time | cost | KB read |", "|---|---|---|---|---|---|---|---|---|---|"];
  for (const mode of MODES)
    for (const group of groups) {
      const of = results.filter((r) => r.mode === mode && (group === "all" || r.group === group));
      const answered = of.filter((r) => r.none === undefined);
      const absent = of.filter((r) => r.none !== undefined);
      if (!of.length) continue;
      rows.push(
        [
          mode,
          group,
          answered.length ? pct(mean(answered.map((r) => r.r5))) : "",
          answered.length ? pct(mean(answered.map((r) => r.r10))) : "",
          pct(mean(of.map((r) => r.p5))),
          answered.length ? mean(answered.map((r) => r.rr)).toFixed(2) : "",
          absent.length ? `${absent.filter((r) => r.none).length}/${absent.length}` : "",
          `${(mean(of.map((r) => r.millis)) / 1000).toFixed(2)} s`,
          `$${of.reduce((s, r) => s + r.cost, 0).toFixed(4)}`,
          (mean(of.map((r) => r.bytes)) / 1024).toFixed(1),
        ].join(" | ").replace(/^/, "| ").replace(/$/, " |"),
      );
    }
  return rows.join("\n");
}

const repo = snapshot(spec.commit);
const results: Scored[] = [];
for (const q of queries)
  for (const mode of MODES) {
    const answer = ask(mode, q, repo);
    const result = scored(mode, q, answer);
    results.push(result);
    const missed = q.must.filter((m) => !answer.files.slice(0, 10).includes(m));
    const note = answer.failed ? ` FAILED ${answer.failed}` : missed.length ? ` missed ${missed.join(" ")}` : "";
    console.error(`${q.id} ${mode}: ${answer.files.slice(0, 5).join(" ")}${note}`);
  }
const summary = table(results);
console.log(summary);
const stamp = new Date().toISOString().replace(/[:T]/g, "-").slice(0, 16);
const name = `${stamp}${opts.label ? `-${opts.label}` : ""}`;
fs.mkdirSync(path.join(HERE, "results"), { recursive: true });
fs.writeFileSync(
  path.join(HERE, "results", `${name}.json`),
  JSON.stringify({ commit: spec.commit, modes: MODES, results }, null, 1),
);
if (opts.record) {
  const all = results.filter((r) => r.mode === "judged" || r.mode === "words");
  const line = (mode: string) => {
    const of = all.filter((r) => r.mode === mode);
    const answered = of.filter((r) => r.none === undefined);
    return `${mode} R@5 ${pct(mean(answered.map((r) => r.r5)))} P@5 ${pct(mean(of.map((r) => r.p5)))} MRR ${mean(answered.map((r) => r.rr)).toFixed(2)}`;
  };
  const row = `- ${name}: ${MODES.filter((m) => m !== "rg").map(line).join("; ")}\n`;
  fs.appendFileSync(path.join(HERE, "HISTORY.md"), row);
}
