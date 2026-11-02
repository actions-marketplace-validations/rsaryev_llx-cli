# MODELS.md

How llx knows models: a Rust port of [models.dev](https://github.com/anomalyco/models.dev)
(branch `dev`), studied from a clone of the repository (commit `b2ac700`, 2026-10). This is the
reference for `packages/providers/models`; ARCHITECTURE.md A4 makes it the one source of
model facts.

## models.dev, as it is built

**Two kinds of files, TOML, schema 2** (`AGENTS.md` of models.dev; `packages/core/src/schema-v2.ts`):

| | Lab model | Provider model |
|---|---|---|
| Path | `models/<lab>/<model>.toml` | `providers/<provider>/models/<model>.toml` (a `/` in the id is a folder) |
| Holds | facts true on every host: `name`, `description`, `family`, `open_weights`, `knowledge`, `release_date`, `[modalities]`, `[capabilities]` (`tools`, `reasoning.supported`, `temperature`…), `[limit]` | how one host serves it: reasoning controls, `cache`, `structured_output`, `system_updates`, `effort_updates`, `service_tier`, `[cost.*]`, `status`, `[api.*]`, and only real overrides |
| Identity | the file path (never an `id` field) | the file path; `base_model = "<lab>/<model>"` names the lab file |

A provider file of a model the provider did not make **must** use `base_model`; a full inline
definition is only for a lab serving its own model or a model unique to the host.

**Resolution** (`generate.ts` `mergeBaseModel`, `generate-v2.ts` `withV2Fields`):

1. Start from the lab model without `id`, `benchmarks`, `license`, `links`, `weights`.
2. Deep-merge the provider file over it: tables merge key by key; arrays and primitives replace.
3. Drop each dotted path in `base_model_omit` (e.g. `limit.input`).
4. `modalities` are the provider file's when it has them, else the lab's.
5. `id` is the provider file's path, `type` defaults to `chat`. (models.dev also sets `canonical_id`
   to the `base_model`; llx keeps no such field, nor a provider's `doc`.)

**`provider.toml`**: `name`, `npm` (the AI SDK package: it decides the protocol), `env` (the key
variables), `api` (the base address; required for `@ai-sdk/openai-compatible`), `doc`,
`opencodeai`.

**APIs** (`generate-v2.ts` `toApiV2`, `resolveProtocol`, `toModelApis`): a model's `[api.<protocol>]`
tables are its whole list of APIs, in order, replacing the provider's default. A table's
`base_url` comes from the table, else the provider's `api`, else the SDK's default address
(`DEFAULT_NPM_BASE_URLS`, e.g. `@ai-sdk/xai` → `https://api.x.ai/v1`); a trailing `/` is dropped.
Without tables, the one default API's protocol comes from `npm`:

| `npm` | protocol |
|---|---|
| `@ai-sdk/anthropic` | `messages` |
| `@ai-sdk/openai`, `@ai-sdk/azure`, `@ai-sdk/xai` | `responses` (`chat-completions` with `shape = "completions"`) |
| `@ai-sdk/google` | `generate-content` |
| anything else (`@ai-sdk/openai-compatible`, OpenRouter, Groq, Mistral…) | `chat-completions` |

`compatibility.reasoning_field` names where a chat-completions host returns reasoning
(`reasoning_content`, `reasoning`); `replay_reasoning` says whether it must be sent back.

**Reasoning controls** (`capabilities.reasoning`, per host): `supported`; `effort` (graded, from
`none`, `minimal`, `low`, `medium`, `high`, `xhigh`, `max`, sorted low to high: `none` is off);
`toggle = true` (on/off by a request field); `budget = { min, max }` (a real reasoning-token
budget); `default = { type = "effort", value = "high" }` (optional). None of these: always on, no
control.

**Cache and mid-conversation flags** (per host): `cache = false`, or `implicit` and/or `explicit`
(`ttl`, `max_breakpoints`, `targets`, `min_tokens`). `system_updates`: a system message mid-
conversation is accepted in place, the cache kept. `effort_updates`: a per-turn effort change
keeps the cache. `tools.updates`: tools can be added mid-conversation, the cache kept. Published
as `true`/`false` (an unset flag is `false`).

**Cost**: USD per million tokens. `[cost.text.input]` `price`, `cache = { read, write }`;
`[cost.text.output]` `price`; `[[...overrides]]` by `when = { context = { gt = 272_000 } }` (the
long-context price band; the reason llx's default request ceiling is 272k);
`[[cost.multipliers]]` by service tier.

**Status**: `alpha`, `beta`, `deprecated`. **Type**: `chat` (default), `image`, `video`,
`embedding`, `reranking`, `decision`, `transcription`, `speech`, `realtime`.

**Published** (`packages/web/src/server.ts`, `packages/function/src/worker.ts`):

- `https://models.dev/api.json`: v1, flattened and frozen ("never break v1"): `reasoning_options`,
  `limit`, `cost.input`… (5.9 MB, ETag).
- `https://models.dev/experimental/v2.0/api.json`: v2, resolved: per provider `{id, name, env,
  doc, models}`, per model the merged lab + host fields, `capabilities` (reasoning with `default`,
  `cache`, `system_updates`, `effort_updates`, `tools.updates`), `limit`, the structured `cost`, and
  `api` with every protocol's `base_url` (9.8 MB, ETag). Marked experimental.

## llx's port: `packages/providers/models`

- **Data:** `data/` holds models.dev's own files, verbatim and with their source comments, for the
  providers llx lists first and the lab files they name: `data/providers/<id>/provider.toml`,
  `data/providers/<id>/models/...`, `data/models/<lab>/...`. `scripts/models_sync.py <clone>`
  copies them from a models.dev clone (`git clone -b dev`), keeping per provider the newest models
  llx can work with, and every decision model it serves (`llx decide` takes one); nothing in
  `data/` is edited by hand.
- **Build:** `build.rs` resolves the files as models.dev does (the rules above, in Rust) into one
  document in the shape of the v2 API, cut by the catalog's types (`types.rs`) as a fetched
  catalog is (a model that does not read as one left out), built into the crate.
- **Run:** `types.rs` types that shape (unknown fields ignored), and `provider.rs` reads a
  provider as an index of raw JSON, each model decoded the first time it is asked for (one that
  does not parse is left out, never the catalog). `source.rs` fetches the v2 API at most once an
  hour with its ETag, keeps it cut down in `<config>/models.json` (the ETag in `models.etag`), and
  lays it over the built-in data: a running process, the daemon too, reads it again within a
  minute of a refresh writing a newer one. Nothing waits on the network; `LLX_MODELS_REFRESH=0`
  turns the refresh off.
- **What llx reads:** a model is usable when its `type` is `chat`, `tools.supported`, `text` is
  among its outputs and it is not `deprecated`. It is called over a protocol only at a fixed
  address (an `http(s)://` `base_url` with no `${…}`). A provider's models are listed newest first
  by `release_date`. Its window is `limit.input`, else `limit.context`; its output limit is
  `limit.output`. Its levels come from `capabilities.reasoning` (effort list, toggle → off and
  on, budget → low, medium, high, and off when its `min` allows) and its default from `default`,
  else medium or the nearest below, else the lowest it takes. A provider is called over
  `chat-completions` (any host), but OpenAI with a ChatGPT sign-in over `responses`, which also
  offers the models served over it alone. A `decision` model that is not `deprecated` is kept for
  `llx decide`: the app's default one (`DECISION_DEFAULT`) must be among them.
- **Free-text tools:** whether a model takes a tool of free text held to a Lark grammar (how
  `apply_patch` is offered over the Responses API) is the API's `capabilities.tools.freeform`,
  else the model's own `capabilities.tools.freeform`. models.dev's v2 API does not publish it, so
  a fetched catalog asks the built-in files.
- **Prompt-cache marks:** whether a model caches only at marked breakpoints (`takes_marks`) is read
  from its id's vendor (`anthropic/`, `google/gemini`, `qwen/`): the catalog's `cache.explicit`
  covers other mechanisms too, so it cannot say this.
- **Price:** `cost.text.input.price`, `cost.text.input.cache.read` and `cost.text.output.price`,
  with the last long-context band (`overrides`, `when.context.gt` or `gte`) the request's input
  reaches. A completion whose provider reports no cost (a ChatGPT sign-in) is priced from them, so
  `--budget` and `/usage` count it.
- **Left out:** `cache` and the `*_updates` flags are not kept: the catalog's types drop them,
  built in or fetched.

## Fidelity

`cargo test -p models` checks the resolution on small fixtures. With a fetched v2 API,
`MODELS_DEV_V2=api.json cargo test -p models -- --ignored` compares every built-in model with
models.dev's own resolution of it, field by field, and fails on any difference in what llx reads.
