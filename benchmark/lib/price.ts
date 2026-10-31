// What a run costs at the model's price, from llx's catalog (the one source of model facts), the
// same way for every agent: each model request priced by its own tokens.

import * as fs from "node:fs";
import * as path from "node:path";

/** One model request's tokens: `input` includes the `cached` part, `output` the reasoning. */
export type Request = { input: number; cached: number; output: number };

type Rate = { price: number; cache?: { read: number } };
type Priced = Rate & { overrides?: { when: { context?: { gt?: number } }; price: number; cache?: { read: number } }[] };

const CATALOG = path.join(import.meta.dir, "../../packages/providers/models/data/providers");

/** A function from a run's requests to dollars, for `model` of `provider`. */
export function pricer(provider: string, model: string): (requests: Request[]) => number {
  const file = path.join(CATALOG, provider, "models", `${model}.toml`);
  const cost = (Bun.TOML.parse(fs.readFileSync(file, "utf8")) as { cost: { text: { input: Priced; output: Priced } } }).cost.text;
  // The rate for a request with `context` tokens in: an override for long contexts, else the base.
  const at = (rate: Priced, context: number): Rate =>
    rate.overrides?.find((o) => context > (o.when.context?.gt ?? Infinity)) ?? rate;
  return (requests) =>
    requests.reduce((sum, r) => {
      const input = at(cost.input, r.input), output = at(cost.output, r.input);
      const fresh = r.input - r.cached;
      return sum + (fresh * input.price + r.cached * (input.cache?.read ?? input.price) + r.output * output.price) / 1e6;
    }, 0);
}
