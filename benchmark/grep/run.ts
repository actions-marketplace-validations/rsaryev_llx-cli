#!/usr/bin/env bun
// The search benchmark: `llx grep` against questions about this repository whose answers are known
// (queries.json), by group, beside `rg` as an agent would first type it.
//
// The repository at the queries' commit is put in a fresh git repository outside this one (so
// `git ls-files` lists what it did then), and each question is asked in each mode (by default
// words, judged and rg):
//
// - `words`:  `llx grep --lexical`, the ranking by the question's words alone (no network);
// - `judged`: `llx grep`, the words' best files read by the decision model (an OpenRouter key);
// - `hybrid`, `hybrid-words`: `llx grep -i -e PATTERN` with the question, the pattern rg is
//   given, with the decision model and without;
// - `rg`:     `rg -c -i PATTERN`, files by their count of matches, the pattern an agent would type.
//
// Each answer is scored against the files it must name (`must`) and those it may (`ok`): recall at
// 5 and 10, precision of its first 5, the reciprocal rank of its first right file, for a question
// with no answer whether it named none, the time, the cost, and the bytes an agent would read
// (score.ts). results/<date>-<hh>-<mm>[-label].json, in UTC, keeps every answer; HISTORY.md a
// line per run (--record).

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { parseArgs } from "node:util";
import { type Answer, type Query, type Scored, isTest, modes, recorded, scored, table } from "./score";

const HERE = import.meta.dir;
const ROOT = path.resolve(HERE, "../..");

const { values: opts } = parseArgs({
  options: {
    llx: { type: "string", default: path.join(ROOT, "target/release/llx") },
    mode: { type: "string", multiple: true },
    only: { type: "string", multiple: true },
    label: { type: "string" },
    // Passed to `llx grep` as they are, to compare its settings: `--grep-arg=--judge --grep-arg=40`.
    "grep-arg": { type: "string", multiple: true },
    record: { type: "boolean", default: false },
    // No model asked: where each file an answer must name stands in the ranking by words, the
    // order the decision model reads files in. `--judge` deeper than the deepest reads for nothing.
    ranks: { type: "boolean", default: false },
  },
});
const MODES = modes(opts.mode);
const spec = JSON.parse(fs.readFileSync(path.join(HERE, "queries.json"), "utf8"));
const queries: Query[] = spec.queries.filter(
  (q: Query) => !opts.only?.length || opts.only.some((o) => q.id.startsWith(o)),
);

function sh(cmd: string[], cwd: string): { out: string; err: string; code: number; millis: number } {
  const started = performance.now();
  const run = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
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
  // `hybrid` and `hybrid-words`: the pattern an agent would give rg first (`rg -i -e`), with the
  // question: the pattern keeps the files, the question ranks them, the model (or not) reads them.
  const lexical = mode === "words" || mode === "hybrid-words" ? ["--lexical"] : [];
  const pattern = mode.startsWith("hybrid") ? ["-i", "-e", q.rg] : [];
  const extra = opts["grep-arg"] ?? [];
  const run = sh([opts.llx!, "grep", "--json", ...lexical, ...pattern, ...extra, q.query, ...paths], repo);
  if (run.code !== 0 && run.code !== 1)
    return { files: [], millis: run.millis, cost: 0, bytes: 0, failed: run.err.trim() };
  const answer = JSON.parse(run.out);
  // With a pattern and no file it matches, nothing is judged: an empty answer, not a failure.
  const nothing = answer.files.length === 0 && run.code === 1;
  if ((mode === "judged" || mode === "hybrid") && answer.by !== "judged" && !nothing)
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

const repo = snapshot(spec.commit);
if (opts.ranks) {
  let deepest = 0;
  for (const q of queries.filter((q) => q.must.length)) {
    const run = sh([opts.llx!, "grep", "--lexical", "-l", "-n", "5000", q.query, ...(q.paths ?? [])], repo);
    // In the order the decision model reads: tests left out, as for a question not about them.
    const listed = run.out.split("\n").filter((f) => f && !isTest(f));
    const ranks = q.must.map((m) => listed.indexOf(m) + 1 || null);
    const found = ranks.filter((r): r is number => r !== null);
    if (found.length) deepest = Math.max(deepest, ...found);
    console.log(`${q.id}\t${listed.length} files\t${ranks.map((r) => r ?? "none").join(" ")}`);
  }
  console.log(`deepest: ${deepest} (none: no word of the question, reached only through links)`);
  process.exit(0);
}
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
if (opts.record) fs.appendFileSync(path.join(HERE, "HISTORY.md"), recorded(name, results));
