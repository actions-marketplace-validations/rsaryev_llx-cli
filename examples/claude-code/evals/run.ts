#!/usr/bin/env bun
// The plugin's evals: Claude Code itself (`claude -p`) asked questions about another project's
// code (cases.json), once with this plugin (`--plugin-dir`) and once without, in a copy of that
// project outside this repository. Each run is scored on what Claude did and said:
//
// - which of the plugin's skills it loaded, and whether it read `llx … --help` before it first
//   ran `llx grep` or `llx decide` (the skills point there: the guide is the installed llx's);
// - whether it used what the case expects (`uses`), and nothing it should not;
// - how many of the files the answer must name it named; its turns, time and cost.
//
// The user's own settings and plugins are left out (`--setting-sources project,local`), so the
// plugin under test is the only one; llx's key is the one llx keeps (`/login`) or the
// environment's. Every transcript goes to results/<date>_<time>/.

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";
import { parseArgs } from "node:util";

const HERE = import.meta.dir;
const PLUGIN = path.resolve(HERE, "..");

type Case = { id: string; uses: "grep" | "decide" | "none" | "any"; prompt: string; must: string[] };
type Run = {
  id: string;
  arm: "plugin" | "bare";
  skills: string[];
  commands: string[];
  helpFirst: boolean;
  used: { grep: boolean; decide: boolean };
  found: number;
  must: number;
  turns: number;
  seconds: number;
  cost: number;
  answer: string;
};

const { values: opts } = parseArgs({
  options: {
    only: { type: "string", multiple: true },
    arm: { type: "string", multiple: true },
    model: { type: "string" },
    claude: { type: "string", default: "claude" },
  },
});
const spec = JSON.parse(fs.readFileSync(path.join(HERE, "cases.json"), "utf8"));
const cases: Case[] = spec.cases.filter(
  (c: Case) => !opts.only?.length || opts.only.includes(c.id),
);
const arms = (opts.arm?.length ? opts.arm : ["plugin", "bare"]) as Run["arm"][];

/** The project at the cases' commit, a git repository of its own in the temporary directory. */
function project(): string {
  const dir = path.join(os.tmpdir(), `llx-plugin-evals-${spec.commit}`);
  if (fs.existsSync(path.join(dir, ".git"))) return dir;
  const clone = `${dir}.clone`;
  fs.rmSync(clone, { recursive: true, force: true });
  const run = (cmd: string[], cwd?: string) => {
    const done = Bun.spawnSync(cmd, { cwd, stdout: "pipe", stderr: "pipe" });
    if (done.exitCode !== 0) throw new Error(`${cmd.join(" ")}: ${done.stderr}`);
    return done;
  };
  run(["git", "clone", "-q", "--filter=blob:none", spec.repo, clone]);
  fs.mkdirSync(dir, { recursive: true });
  const archive = run(["git", "archive", spec.commit], clone);
  Bun.spawnSync(["tar", "-x", "-C", dir], { stdin: archive.stdout });
  run(["git", "init", "-q"], dir);
  run(["git", "add", "-A"], dir);
  run(["git", "-c", "user.email=evals@llx", "-c", "user.name=evals", "commit", "-qm", spec.commit], dir);
  fs.rmSync(clone, { recursive: true, force: true });
  return dir;
}

function ask(c: Case, arm: Run["arm"], repo: string, log: string): Run {
  const args = [
    opts.claude!,
    "-p",
    c.prompt,
    "--output-format",
    "stream-json",
    "--verbose",
    "--setting-sources",
    "project,local",
    ...(arm === "plugin" ? ["--plugin-dir", PLUGIN] : []),
    ...(opts.model ? ["--model", opts.model] : []),
  ];
  const started = performance.now();
  const done = Bun.spawnSync(args, { cwd: repo, stdout: "pipe", stderr: "pipe", env: process.env });
  const seconds = (performance.now() - started) / 1000;
  const text = done.stdout.toString();
  fs.writeFileSync(log, text);
  const events = text
    .split("\n")
    .filter(Boolean)
    .flatMap((line) => {
      try {
        return [JSON.parse(line)];
      } catch {
        return [];
      }
    });
  const calls = events
    .filter((e) => e.type === "assistant")
    .flatMap((e) => e.message?.content ?? [])
    .filter((part: { type: string }) => part.type === "tool_use");
  const skills = calls
    .filter((call: { name: string }) => call.name === "Skill")
    .map((call: { input: { skill?: string } }) => call.input.skill ?? "?");
  const commands: string[] = calls
    .filter((call: { name: string }) => call.name === "Bash")
    .map((call: { input: { command?: string } }) => call.input.command ?? "");
  const runs = (tool: string) => (command: string) =>
    new RegExp(`(^|[;&|(]\\s*)llx ${tool}\\b`).test(command);
  const first = (tool: string) => commands.findIndex(runs(tool));
  const help = (tool: string) =>
    commands.findIndex((command) => runs(tool)(command) && /--help\b/.test(command));
  const real = (tool: string) =>
    commands.findIndex((command) => runs(tool)(command) && !/--help\b|--check\b/.test(command));
  const helpFirst = ["grep", "decide"].every((tool) => {
    const at = real(tool);
    return at < 0 || (help(tool) >= 0 && help(tool) < at);
  });
  const result = events.find((e) => e.type === "result") ?? {};
  const answer: string = result.result ?? "";
  return {
    id: c.id,
    arm,
    skills,
    commands,
    helpFirst,
    used: { grep: real("grep") >= 0, decide: real("decide") >= 0 },
    found: c.must.filter((file) => answer.includes(file) || answer.includes(path.basename(file)))
      .length,
    must: c.must.length,
    turns: result.num_turns ?? 0,
    seconds: Math.round(seconds),
    cost: result.total_cost_usd ?? 0,
    answer,
  };
}

/** Whether a run did what its case expects of the plugin. */
function expected(c: Case, run: Run): string {
  if (run.arm === "bare") return "—";
  if (c.uses === "any") return run.helpFirst ? "yes" : "no";
  const wanted = c.uses === "none" ? !run.used.grep && !run.used.decide : run.used[c.uses];
  return wanted && run.helpFirst ? "yes" : "no";
}

const repo = project();
const stamp = new Date().toISOString().slice(0, 16).replace("T", "_").replace(":", "");
const out = path.join(HERE, "results", stamp);
fs.mkdirSync(out, { recursive: true });
const runs: Run[] = [];
for (const c of cases)
  for (const arm of arms) {
    const run = ask(c, arm, repo, path.join(out, `${c.id}-${arm}.jsonl`));
    runs.push(run);
    console.error(
      `${c.id} ${arm}: ${run.found}/${run.must} files, ${run.turns} turns, ${run.seconds} s, $${run.cost.toFixed(3)}; skills [${run.skills.join(", ")}]; llx: ${run.commands.filter((x) => /\bllx\b/.test(x)).join(" ⏎ ") || "none"}`,
    );
  }
const rows = runs.map((run) => {
  const c = cases.find((c) => c.id === run.id)!;
  return `| ${run.id} | ${c.uses} | ${run.arm} | ${expected(c, run)} | ${run.skills.join(", ") || "—"} | ${run.found}/${run.must} | ${run.turns} | ${run.seconds} | ${run.cost.toFixed(3)} |`;
});
const summary = [
  `# Plugin evals, ${stamp}`,
  "",
  `${spec.repo} at ${spec.commit}.`,
  "",
  "| case | uses | arm | as expected | skills | files | turns | s | $ |",
  "|---|---|---|---|---|---|---|---|---|",
  ...rows,
  "",
].join("\n");
fs.writeFileSync(path.join(out, "summary.md"), summary);
fs.writeFileSync(path.join(out, "runs.json"), JSON.stringify(runs, null, 1));
console.log(summary);
