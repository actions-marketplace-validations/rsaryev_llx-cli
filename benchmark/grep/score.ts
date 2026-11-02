// The search benchmark's scores: each answer against the files it must and may name, a run's
// table by mode and group, and its line in HISTORY.md (README.md: what a run is judged by).

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

/** The modes a run asks in when `--mode` names none. */
export function modes(named?: string[]): string[] {
  return named?.length ? named : ["words", "judged", "rg"];
}

/** A test's file, which the decision model does not read (as `llx grep` tells one). */
export const isTest = (file: string) => /(_tests\.|\/tests?\/)/.test(file);

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
  const all = results.filter((r) => r.mode === "judged" || r.mode === "words");
  const line = (mode: string) => {
    const of = all.filter((r) => r.mode === mode);
    const some = answered(of);
    return `${mode} R@5 ${pct(mean(some.map((r) => r.r5)))} P@5 ${pct(mean(of.map((r) => r.p5)))} MRR ${mean(some.map((r) => r.rr)).toFixed(2)}`;
  };
  const modes = [...new Set(results.map((r) => r.mode))].filter((m) => m !== "rg");
  return `- ${name}: ${modes.map(line).join("; ")}\n`;
}
