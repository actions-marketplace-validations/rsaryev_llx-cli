# Breaking changes in an HTTP API

What a change does to an API's existing clients, read by llx: everything a spec diff shows, and
what it never can.

A structural tool (a spec diff, oasdiff, a home-made rule set) compares the spec's shape: paths,
parameters, types, required lists, enums. Many changes that break clients keep that shape:

- a field's unit changes (`amount` in cents instead of major units), said only in its description;
- a boundary turns inclusive, a default changes, a time zone is assumed;
- the code that serves the spec changes behaviour: `skip` becomes a page number, the order flips;
- a status or an enum value starts to mean something else.

llx reads the whole change (`git diff`), the spec around it and the code behind the endpoints, and
answers with JSON: one entry per change, `structural` or `semantic`, breaking or not, why, and
where. It runs read only.

## Files

| File | What it is |
|---|---|
| [check.sh](check.sh) | The check: `check.sh [SPEC] [BASE]` in the repository. Writes `breaking-changes.json`, prints a line per change, exits 0 (nothing breaks), 1 (something breaks) or 2 (llx could not finish). |
| [prompt.md](prompt.md) | The prompt, with `{SPEC}` and `{BASE}` filled in by `check.sh`. Add what your API's clients rely on. |
| [schema.json](schema.json) | The answer's shape. |
| [gitlab-ci.yml](gitlab-ci.yml) | A GitLab CI job for a branch with an open merge request. |
| [demo/](demo) | A small orders API (`spec/`, `src/`), a change with 13 breaking and 3 safe edits (`change.patch`), `run.sh` to check it, and a recorded run: [output.txt](demo/output.txt) and [result.json](demo/result.json). |

## Run it

The demo, in a throwaway repository (needs llx signed in, `git` and `jq`):

```bash
sh examples/breaking-changes/demo/run.sh
```

On your repository, after `git fetch`:

```bash
sh path/to/check.sh src/spec origin/main
```

```text
BREAKS  structural  POST /orders: Success response code changed from 201 to 200 for 'Order created'.
BREAKS  semantic    POST /orders/search: 'skip' changed meaning: it was an item offset and is now a page index, so the slice offset is skip * limit.
safe    structural  GET /stats/charts: New optional query parameter 'currency' (type string) added.
...
```

Each entry of `breaking-changes.json` also says why and where:

```json
{
  "endpoint": "GET /orders/{orderId}, POST /orders (response), POST /orders/search (items)",
  "change": "Order.amount meaning changed from major units (e.g. 12.50) to minor units/cents (e.g. 1250). Type stays number.",
  "kind": "semantic",
  "breaking": true,
  "why": "Same field, same type, but values are 100x larger; clients that read or display the amount as major units will misreport totals silently.",
  "where": "spec/schemas/Order.yml:17 (was 'The total in major units, e.g. 12.50')"
}
```

### In CI

GitLab: [gitlab-ci.yml](gitlab-ci.yml). The job runs in a branch pipeline when a merge request is
open (`$CI_OPEN_MERGE_REQUESTS`), so it needs no merge request pipelines. It uses a glibc image:
the llx Linux build does not run on Alpine.

GitHub, with the action installing llx:

```yaml
- uses: actions/checkout@v7
  with: { fetch-depth: 0 }
- uses: rsaryev/llx@v0
  with: { install-only: true }
- run: sh ci/breaking-changes/check.sh src/spec "origin/${{ github.base_ref }}"
  env:
    OPENROUTER_API_KEY: ${{ secrets.OPENROUTER_API_KEY }}
```

To post the answer on the merge or pull request, read `breaking-changes.json` with `jq` in a later
job that holds the write token and no model key, as the [GitHub examples](../README.md) do.

## Results

**The demo** ([output.txt](demo/output.txt), [result.json](demo/result.json)): llx found all 13 breaking edits, the 4 semantic ones among them,
and called the 3 safe ones safe. Two edits to one field (`limit`: a string now, at most 3
characters) came back as one entry. 38 s, 6 turns, $0.006 with `anthropic/claude-haiku-5.5`.

| Edit | Kind | llx |
|---|---|---|
| `POST /orders` answers 200 instead of 201 | structural | breaking |
| `PUT /orders/{orderId}` removed | structural | breaking |
| a required `currency` in the create request | structural | breaking |
| `quoteDate` no longer required in `Order` | structural | breaking |
| `ROLLBACK` removed from `OrderStatus` | structural | breaking |
| `limit` a string, then `maxLength: 3` | structural | breaking (one entry) |
| query `before` renamed `from` | structural | breaking |
| query `top` now required | structural | breaking |
| `amount` in cents, by its description | semantic | breaking |
| `before` inclusive, by its description | semantic | breaking |
| `skip` a page number, in `src/search.js` | semantic | breaking |
| search results oldest first, in `src/search.js` | semantic | breaking |
| an optional `note` in `Order` | structural | safe |
| an optional query `currency` | structural | safe |
| a summary reworded | semantic | safe |

**A real service.** The same check, without the semantic part, on a production microservice's
spec (about 1,400 lines in 40 files joined by `$ref`), next to that team's own rule-based checker
(six rules over a structural diff):

| | Rule-based checker | llx |
|---|---|---|
| A real commit that renamed two endpoints' paths | 2 of 2 | 2 of 2, with the new paths and the SDK methods renamed |
| 9 planted breaking edits | 6 of 9: missed a query parameter made required, one renamed, and a new `maxLength` | 9 of 9 |
| 3 planted safe edits | no false alarm | no false alarm |
| Output | 20 lines for 6 problems (each repeated wherever a `$ref` is used); 201 → 200 reported as "Endpoint removed" | one entry per change, with why |
| Fails the pipeline | never: it writes a report and exits 0 | `breaking` decides |
| Time, cost | under a second, free | 33 to 47 s, $0.004 to $0.008 |

## Limits

- It is a model's reading, not a proof. Before you rely on it, run it on 10 to 20 past commits of
  your specs and compare with what your reviewers said.
- Keep the structural tool too. It is free and exact for what it can see, and a good input: put
  its output in the prompt, or run it first.
- llx sees what is in the repository: a behaviour that lives in another service, or in data, is
  out of its reach. Say in `prompt.md` what your clients rely on (the SDKs, the validation, the
  consumers you know of).
- The spec, its descriptions and the code are data to the agent, never instructions; the job holds
  no token that can write.
