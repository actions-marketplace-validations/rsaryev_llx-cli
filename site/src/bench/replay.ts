import type { Row, Trial } from './types';

/** A row as it stood `at` seconds into the batch: only the runs that had ended by then count.
 *  Once all have, the row's own figures (the ones bench.py rounded) stand as they are. */
export type Standing = Row & { ended: number; running: boolean };

export function standing(row: Row, at: number): Standing {
  const ended = row.trials.filter((trial) => trial.seconds <= at);
  if (ended.length === row.trials.length) return { ...row, ended: ended.length, running: false };
  const scores = ended.map((trial) => trial.score);
  return {
    ...row,
    ended: ended.length,
    running: true,
    runs: ended.length,
    score: median(scores),
    low: Math.min(...scores),
    high: Math.max(...scores),
    seconds: at,
    cost: known(
      ended.map((trial: Trial) => trial.cost),
      mean,
    ),
    tokens: known(
      ended.map((trial) => trial.tokens),
      median,
    ),
  };
}

/** The board at a moment: rows with a run ended first, ranked as the finished board is ([`ranked`]);
 *  rows still waiting on their first run after, in the order they came. */
export function board(rows: Row[], at: number): Standing[] {
  const all = rows.map((row) => standing(row, at));
  return [...ranked(all.filter((row) => row.ended > 0)), ...all.filter((row) => row.ended === 0)];
}

/** Scores within this share of the total of the best one count as even: less than one agent's
 *  runs differ by among themselves. */
const EVEN = 0.01;

/** Rows by score, those within [`EVEN`] of the best as one, and then by time: of agents that did
 *  the task as well, the faster one is ahead (its cost and tokens follow its turns). */
export function ranked<T extends Pick<Row, 'score' | 'seconds' | 'total'>>(rows: T[]): T[] {
  const best = Math.max(0, ...rows.map((row) => row.score));
  const even = (row: T) => row.score >= best - Math.ceil(EVEN * row.total);
  return [...rows].sort(
    (a, b) => Number(even(b)) - Number(even(a)) || (even(a) ? 0 : b.score - a.score) || a.seconds - b.seconds,
  );
}

/** When the batch's last run ended. */
export const length = (rows: Row[]) => Math.max(0, ...rows.flatMap((row) => row.trials.map((trial) => trial.seconds)));

/** An average of the runs that reported it; null when none did. */
const known = (values: (number | null)[], average: (values: number[]) => number) => {
  const reported = values.filter((value): value is number => value !== null);
  return reported.length ? average(reported) : null;
};
const median = (values: number[]) => {
  if (values.length === 0) return 0;
  const sorted = [...values].sort((a, b) => a - b);
  const middle = Math.floor(sorted.length / 2);
  return sorted.length % 2 ? (sorted[middle] ?? 0) : ((sorted[middle - 1] ?? 0) + (sorted[middle] ?? 0)) / 2;
};
const mean = (values: number[]) => (values.length ? values.reduce((sum, value) => sum + value, 0) / values.length : 0);
