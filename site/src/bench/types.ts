/** One agent on one model: what its runs of the task scored, took and cost. */
export type Row = {
  agent: string;
  /** llx's own row. */
  ours: boolean;
  runs: number;
  /** The median score, and the lowest and highest of the runs, out of `total`. */
  score: number;
  low: number;
  high: number;
  total: number;
  /** Median wall time, in seconds. */
  seconds: number;
  /** Mean cost at the model's API prices, in dollars; null when no run reported its usage. */
  cost: number | null;
  /** Median input tokens sent, cached or not; null when no run reported them. */
  tokens: number | null;
  version: string;
  /** When the rows were run. */
  stamp: string;
  effort: string;
  /** Each run as it ended, in the order it did. */
  trials: Trial[];
};

/** One run: when it ended (every run of a batch starts at once) and what it came to. */
export type Trial = { seconds: number; score: number; cost: number | null; tokens: number | null };

/** Every agent measured on one model. */
export type Board = { id: string; model: string; stamp: string; effort: string; rows: Row[] };
