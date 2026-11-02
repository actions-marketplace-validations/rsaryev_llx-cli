// The search benchmark's scores: each answer against the files it must and may name, a run's
// table by mode and group, and its line in HISTORY.md (README.md: what a run is judged by).

import * as path from "node:path";

/** A question about the repository and its known answer (queries.json). */
export type Query = {
  id: string;
  group: string;
  query: string;
  rg: string;
  paths?: string[];
  must: string[];
  ok: string[];
};

/** What a mode answered: the files it named, best first, and what the answer took. */
export type Answer = {
  files: string[];
  scores?: number[];
  unsure?: string[];
  millis: number;
  cost: number;
  bytes: number;
  failed?: string;
};

/** An answer scored; `none`, whether it named no file, only for a question with no answer. */
export type Scored = Answer & {
  id: string;
  group: string;
  mode: string;
  r5: number;
  r10: number;
  p5: number;
  rr: number;
  none?: boolean;
};

/** Every mode a run can ask in (README.md): a name it does not know would ask the decision model. */
const MODES = ["words", "judged", "hybrid", "hybrid-words", "rg"];

/** The modes `--mode` names, else words, judged and rg. */
export function modes(named?: string[]): string[] {
  const unknown = named?.find((mode) => !MODES.includes(mode));
  if (unknown) throw new Error(`--mode ${unknown}: one of ${MODES.join(", ")}`);
  return named?.length ? named : ["words", "judged", "rg"];
}

const TEST_DIRS = ["tests", "test", "__tests__", "spec", "fixtures", "testdata"];
const TEST_ENDINGS = ["_tests.rs", "_test.rs", "_test.go", ".test.ts", ".test.js", ".spec.ts", ".spec.js", "_test.py"];

/** A test's file, which the decision model is not given: as `llx grep` tells one (`is_test` in
 * apps/server/src/commands/grep/rank.rs), by the names the usual layouts give one. */
export function isTest(file: string): boolean {
  const lower = file.toLowerCase();
  return (
    TEST_DIRS.some((dir) => lower.startsWith(`${dir}/`) || lower.includes(`/${dir}/`)) ||
    TEST_ENDINGS.some((end) => lower.endsWith(end)) ||
    path.basename(lower).startsWith("test_")
  );
}

export function scored(mode: string, q: Query, a: Answer): Scored {
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
/** The answers to questions that have one: those R@5, R@10 and MRR are measured on. */
const answered = (of: Scored[]) => of.filter((r) => r.none === undefined);

/** A run's table: a row per mode and group, then each mode over all the questions. */
export function table(results: Scored[]): string {
  const groups = [...new Set(results.map((r) => r.group)), "all"];
  const rows = [
    "| mode | group | R@5 | R@10 | P@5 | MRR | none | time | cost | KB read |",
    "|---|---|---|---|---|---|---|---|---|---|",
  ];
  for (const mode of new Set(results.map((r) => r.mode)))
    for (const group of groups) {
      const of = results.filter((r) => r.mode === mode && (group === "all" || r.group === group));
      if (!of.length) continue;
      const some = answered(of);
      const absent = of.filter((r) => r.none !== undefined);
      const cells = [
        mode,
        group,
        some.length ? pct(mean(some.map((r) => r.r5))) : "",
        some.length ? pct(mean(some.map((r) => r.r10))) : "",
        pct(mean(of.map((r) => r.p5))),
        some.length ? mean(some.map((r) => r.rr)).toFixed(2) : "",
        absent.length ? `${absent.filter((r) => r.none).length}/${absent.length}` : "",
        `${(mean(of.map((r) => r.millis)) / 1000).toFixed(2)} s`,
        `$${of.reduce((s, r) => s + r.cost, 0).toFixed(4)}`,
        (mean(of.map((r) => r.bytes)) / 1024).toFixed(1),
      ];
      rows.push(`| ${cells.join(" | ")} |`);
    }
  return rows.join("\n");
}

/** The run's line in HISTORY.md: R@5, P@5 and MRR over all the questions, for each mode of `llx grep`. */
export function recorded(name: string, results: Scored[]): string {
  const line = (mode: string) => {
    const of = results.filter((r) => r.mode === mode);
    const some = answered(of);
    return `${mode} R@5 ${pct(mean(some.map((r) => r.r5)))} P@5 ${pct(mean(of.map((r) => r.p5)))} MRR ${mean(some.map((r) => r.rr)).toFixed(2)}`;
  };
  const modes = [...new Set(results.map((r) => r.mode))].filter((m) => m !== "rg");
  return `- ${name}: ${modes.map(line).join("; ")}\n`;
}
