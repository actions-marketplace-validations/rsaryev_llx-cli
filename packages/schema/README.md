# schema

A JSON Schema validator for what a language model writes: the schema arrives at run time (as
`llx exec --schema` takes it), is compiled once, and every answer is checked against it, valid
ones with no allocation at all. Every message names the field, the rule and what the data has,
so a model can mend its answer from it:

```text
data.issues[0].line must be >= 1 (is 0)
data.issues[0].severity must be one of ["low","medium","high"] (is "urgent")
data must match exactly one schema of oneOf (matches none)
```

It depends on `serde_json` and on `regex-lite` (for the few patterns its own matcher does not
take), and adds about 230 KB to a release binary built for size; the `jsonschema` crate would
add about 1.9 MB.

## Use

```rust
use schema::Schema;
use serde_json::json;

let schema = Schema::new(json!({
    "type": "object",
    "required": ["verdict"],
    "properties": {"verdict": {"enum": ["approve", "request_changes"]}}
}));
assert!(schema.matches(&json!({"verdict": "approve"})));
assert_eq!(
    schema.mismatches(&json!({"verdict": "maybe"})),
    [r#"data.verdict must be one of ["approve","request_changes"] (is "maybe")"#]
);
// What the schema asks that is not checked here, each named once: none for this one.
assert!(schema.unchecked().is_empty());
```

- `Schema::new(schema)`: compiled once. `Schema::with_documents(schema, &[(uri, document)])`:
  the documents its `$ref`s may reach by URI (nothing is fetched from the network).
- `matches(&data)`: whether the data matches, stopping at the first failure.
- `mismatches(&data)`: every way it does not, the first 20 messages; empty when it matches.
- `unchecked()`: the keywords present that are not checked here. A schema is never taken
  silently as stricter than it is checked.

## What it checks

- **Drafts** 4, 6, 7, 2019-09 and 2020-12, chosen by `$schema` (2020-12 when it names none),
  each with its own rules: draft 4's boolean `exclusiveMinimum`, the `$ref` that hides its
  siblings before 2019-09, `items` as a list before 2020-12, draft 7's `dependencies`; and a
  metaschema's `$vocabulary` when it leaves out the validation vocabulary.
- **Every keyword of validation**: `type`, `enum`, `const`; the bounds and `multipleOf`, exact in
  decimal (`0.0075` is a multiple of `0.0001`), integers exact past 2^53; `minLength` and
  `maxLength` in characters; `pattern`; the array keywords, `uniqueItems` and `contains` with its
  counts; the object keywords, `required`, `dependentRequired` and the property counts.
- **Every applicator**: `allOf`, `anyOf`, `oneOf`, `not`, `if`/`then`/`else`, `properties`,
  `patternProperties`, `additionalProperties`, `propertyNames`, `dependentSchemas`, `items`,
  `prefixItems`, `contains`, `unevaluatedProperties` and `unevaluatedItems`.
- **References**: `$ref` by JSON pointer, `$id`, `$anchor` and `$dynamicRef` inside the
  resource, and into the documents given; a loop with no step into the data is cut.
- **`format`, asserted** (an LLM's answer is what it is for): `date-time`, `date`, `time`,
  `duration`, `email`, `hostname`, `ipv4`, `ipv6`, `uri`, `uri-reference`, `iri`,
  `iri-reference`, `uuid`, `json-pointer`, `relative-json-pointer`, `uri-template` and `regex`.
- **`pattern`** as ECMA-262 reads it (`\d` and `\w` ASCII, `\s` with Unicode's spaces, `.`
  stopping at a line's end), on a string's characters.

Not checked, and named by `unchecked()`: a lookaround or a back-reference in a pattern, the
`idn-hostname` and `idn-email` formats (they need Unicode's IDNA tables), a `$ref` to a document
not given, and a `$dynamicRef` that leaves its resource. Of MaskBench's 11,298 schemas, 21 use
one (0.2%).

## How it is fast

- **Compiled once** into nodes: the types a bit mask, each bound in the order its message says
  it, every `$ref` the node it lands on, an `enum` of many options a hash table, a schema
  with nothing of a kind (no array keyword) a bit that skips the kind.
- **Objects in one pass**: the data's fields and the schema's `properties` are both sorted by
  name, so a cursor meets each field's schema in about one comparison; each required field
  sets a bit as it is met, and `required` is one comparison of masks, nothing looked up.
- **Tagged unions**: a `oneOf` or `anyOf` whose branches each require one field and hold it to
  their own strings (`"type": {"const": "read"}`), as tool calls are written, reads that field
  and tries only the branches its value names.
- **Patterns** by the cheapest matcher that takes them: a comparison or one scan for the common
  shapes (`^[A-Z]{3}$`, `^(small|medium|large)$`), else a position automaton simulated
  bit-parallel, else `regex-lite`; every one linear in the string, none backtracks.
- **`uniqueItems`** through an open-addressed set of the items' hashes (a hash that agrees with
  JSON's equality: `1` and `1.0` alike, an object's fields in any order).
- **Types alone folded**: an `anyOf` of types only, or a `oneOf` of types no two share, is one
  mask; a `$ref` to a schema that asks only a type is that type; an array whose items ask only
  a type, or are such arrays in turn, is checked in one loop over its items.
- **`unevaluated*` settled as it compiles**: where what a schema applies to every value
  (`$ref`, `allOf`) takes every field or item, nothing is left to evaluate and nothing is
  walked; elsewhere what was looked at is marked on the stack, and only once the rest holds.
- **A quick walk first**: `matches` stops at the first failure and writes nothing; only
  `mismatches` on data that failed walks again for every message.

## Conformance

The official [JSON-Schema-Test-Suite](https://github.com/json-schema-org/JSON-Schema-Test-Suite)
(its required tests, the format tests, ECMA-262's regular expressions; `bench/src/bin/suite.rs`):

| draft | ours | jsonschema 0.60 | groups left out (a keyword not checked here) |
|---|---|---|---|
| draft4 | 928/928 | 928/928 | 20 tests |
| draft6 | 1256/1256 | 1256/1256 | 20 tests |
| draft7 | 1584/1584 | 1584/1584 | 145 tests |
| draft2019-09 | 1944/1944 | 1944/1944 | 185 tests |
| draft2020-12 | 1963/1963 | 1963/1963 | 188 tests |

The groups left out ask what is not checked here (`unchecked`): a `format` this crate does not
assert (375), a pattern's lookaround (76), a `$dynamicRef` across resources (73), a `$ref` to
a document not given (34).

[MaskBench](https://github.com/guidance-ai/jsonschemabench) (guidance-ai): 11,298 schemas of real
LLM tasks (function calling, GitHub's, Kubernetes', JSON Schema Store's) with 36,990 answers
labelled valid or not (`bench/src/bin/corpus.rs`):

| | ours | jsonschema 0.60 |
|---|---|---|
| schemas compiled | 11,298 (0 panics) | 11,290 (8 refused) |
| answers judged as labelled | 36,984 of 36,990 (99.98%) | 36,970 (99.95%) |
| compiling every schema | 531.8 ms | 1,106.0 ms (ours 2.08x faster) |
| checking every answer of the 9,779 schemas both check whole | 8.75 ms | 12.32 ms (ours 1.41x faster) |

The two agree on 99.93% of the answers (`bench/out/disagreements.tsv`). 21 schemas (0.19%)
ask something not checked here: a pattern's lookaround (19), a `format` (1), a `$ref` outside
the schema (1). Ours is slower by over 5% on 418 schemas (`bench/out/slower.tsv`), most within
1.2x; timed alone, answer by answer (`bin/schema`), many of them are as fast or faster: the
rest are schemas whose cost is in one field order or one `multipleOf` of a decimal.

## Benchmarks

Against [`jsonschema`](https://github.com/Stranger6667/jsonschema) 0.60 (formats asserted, as
here), both built as llx's release is (`opt-level = "z"`, fat LTO), on an Apple M5 (macOS 26.6). Each time
is the median of 31 rounds; the allocations are counted exactly, and the inputs are fixed, so
both are the same on every run (`bench/src/bin/micro.rs`):

| check | schema | jsonschema 0.60 | ours faster by |
|---|---|---|---|
| review verdict, valid | 404 ns (0 alloc) | 608 ns (0 alloc) | 1.51x |
| review verdict, invalid | 55 ns (0 alloc) | 115 ns (0 alloc) | 2.10x |
| 1,000 rows | 90.32 µs (0 alloc) | 160.15 µs (0 alloc) | 1.77x |
| 500 objects × 20 fields | 96.71 µs (0 alloc) | 403.29 µs (0 alloc) | 4.17x |
| tree of 511 nodes by `$ref` | 17.08 µs (0 alloc) | 30.20 µs (7 alloc) | 1.77x |
| 64 tool calls, `oneOf` of 8 | 6.10 µs (0 alloc) | 8.03 µs (0 alloc) | 1.32x |
| `uniqueItems`, 1,000 strings | 6.35 µs (1 alloc) | 8.82 µs (1 alloc) | 1.39x |
| `enum` of 1,000, the last | 12 ns (0 alloc) | 21 ns (0 alloc) | 1.85x |
| `format: date-time` | 10 ns (0 alloc) | 22 ns (0 alloc) | 2.29x |
| `format: email` | 24 ns (0 alloc) | 169 ns (1 alloc) | 7.10x |
| `format: uuid` | 8 ns (0 alloc) | 11 ns (0 alloc) | 1.31x |
| `format: ipv4` | 9 ns (0 alloc) | 16 ns (0 alloc) | 1.78x |
| `format: uri` | 26 ns (0 alloc) | 41 ns (0 alloc) | 1.58x |
| `pattern: ^[A-Z]{3}$` | 9 ns (0 alloc) | 43 ns (0 alloc) | 4.58x |
| `pattern: ^\d{4}-\d{2}-\d{2}$` | 13 ns (0 alloc) | 44 ns (0 alloc) | 3.55x |
| `pattern: ^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$` | 14 ns (0 alloc) | 44 ns (0 alloc) | 3.21x |
| `pattern: ^(small|medium|large)$` | 10 ns (0 alloc) | 16 ns (0 alloc) | 1.57x |

| compiled once | ours | jsonschema 0.60 | ours faster by |
|---|---|---|---|
| review verdict | 7.03 µs | 15.30 µs | 2.18x |
| tool calls | 29.24 µs | 65.14 µs | 2.23x |

Every message of an invalid verdict: ours 1.46 µs, jsonschema 1.60 µs (1.10x).

Run them:

```bash
cd packages/schema/bench
./fetch.sh                               # the suite and MaskBench, at the commits measured
cargo run --release --bin micro          # the table above
cargo run --release --bin suite          # the official suite, per draft
cargo run --release --bin corpus         # MaskBench: verdicts, compile and check time
cargo run --release --bin schema -- data/jsonschemabench/maskbench/data/<file>.json
```

The bench is a crate of its own, outside the workspace: `jsonschema` and the corpora never
reach llx's build.
