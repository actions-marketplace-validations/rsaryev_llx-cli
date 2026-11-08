// What a run costs at the model's price, from llx's catalog (the one source of model facts), the
// same way for every agent: each model request priced by its own tokens.

import * as fs from "node:fs";
import * as os from "node:os";
import * as path from "node:path";

/** One model request's tokens: `input` includes the `cached` part, `output` the reasoning. */
export type Request = { input: number; cached: number; output: number };

type Rate = { price: number; cache?: { read: number } };
type Priced = Rate & { overrides?: { when: { context?: { gt?: number } }; price: number; cache?: { read: number } }[] };

const CATALOG = path.join(import.meta.dir, "../../packages/providers/models/data/providers");

type Cost = { text: { input: Priced; output: Priced } };

/** The model's price in the catalog llx fetched from models.dev (`<config>/models.json`), which
 * llx itself reads first: the built-in files are a cut of it, and miss most of OpenRouter's. */
function fetched(provider: string, model: string): Cost | undefined {
  const config = process.env.XDG_CONFIG_HOME ?? path.join(os.homedir(), ".config");
  try {
    const catalog = JSON.parse(fs.readFileSync(path.join(config, "llx", "models.json"), "utf8"));
    return catalog[provider]?.models?.[model]?.cost;
  } catch {
    return undefined;
  }
}

/** The model's price in the built-in files. */
function builtIn(provider: string, model: string): Cost {
  const file = path.join(CATALOG, provider, "models", `${model}.toml`);
  return (Bun.TOML.parse(fs.readFileSync(file, "utf8")) as { cost: Cost }).cost;
}

/** A function from a run's requests to dollars, for `model` of `provider`. */
export function pricer(provider: string, model: string): (requests: Request[]) => number {
  const cost = (fetched(provider, model) ?? builtIn(provider, model)).text;
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
