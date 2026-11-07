---
title: "Debug metadata"
description: "The version 1 debug-metadata schema emitted by --debug: sources, functions, sites, locals, sidecar and embedded symbols."
sidebar:
  order: 8
---

`elephc --debug file.php` emits a versioned debug-metadata document next to the
binary (`file.edbg.json`) and embeds the same JSON in the executable. It is the
compiler-side half of the native debugger. The runtime agent reads the embedded
table to resolve site ids to sources, functions and locals; the terminal session
and the [DAP adapter](dap.md) format those values from the same schema.

```bash
elephc --debug hello.php
# writes ./hello and ./hello.edbg.json
```

The document is generated from the same `@file` / `@fn` / `@src` / `@local`
markers that drive source maps and DWARF. The entry source is id `0`; included
and autoloaded files keep their own ids. Two span identities that name the same
physical path collapse to one source entry.

The debug binary also checks a one-byte flag per source-line transition and
calls the preserving `elephc_debug_site` hook only when the site is armed.
`ELEPHC_DEBUG_SITES=1` arms every site and makes `elephc_debug_dump` print the
per-site counters at exit; without it the flags stay zero and the dump is
silent. `elephc debug <source> [--break FILE:LINE]` starts the interactive stdin
session. The runtime keeps a per-thread activation stack from the function
enter/exit hooks, so `bt` prints the current frame and its callers, and
`frame <index>` selects a caller for `locals`/`p`. `next`, `step` and `finish`
use that stack depth to step over, into and out of calls.

## Schema (version 1)

```json
{
  "format": "elephc-debug-metadata",
  "version": 1,
  "source": "hello.php",
  "source_sha256": "86e64f19…",
  "sources": [
    {"id": 0, "path": "hello.php", "sha256": "86e64f19…", "lines": 12, "entry": true}
  ],
  "functions": [
    {
      "id": 0, "name": "add", "symbol": "_fn_add", "source_id": 0,
      "kind": "function", "asm_start": 20, "asm_end": 180,
      "line_start": 3, "line_end": 5, "synthetic": false,
      "flags": [],
      "params": [
        {"name": "a", "php_type": "int", "by_ref": false, "variadic": false},
        {"name": "b", "php_type": "int", "by_ref": false, "variadic": false}
      ]
    }
  ],
  "sites": [
    {"id": 0, "function_id": 0, "source_id": 0, "line": 3, "column": 12,
     "end_line": 3, "end_column": 14, "op": "load_local", "origin": null}
  ],
  "locals": [
    {"function_id": 0, "name": "sum", "frame_offset": 136,
     "php_type": "mixed", "kind": "php_local", "storage": "raw",
     "state_offset": null, "param": null, "by_ref": false, "variadic": false}
  ],
  "classes": [
    {"id": 14, "name": "Point", "parent": "", "abstract": false,
     "properties": [
       {"name": "x", "offset": 8, "php_type": "int", "visibility": "public",
        "by_ref": false, "declared": true},
       {"name": "label", "offset": 40, "php_type": "string",
        "visibility": "private", "by_ref": false, "declared": true}
     ]}
  ]
}
```

### Envelope

| Field | Types | Meaning |
|---|---|---|
| `format` | string | Always `"elephc-debug-metadata"`. |
| `version` | integer | Schema version; currently `1`. |
| `source` | string | Entry file as passed on the command line. |
| `source_sha256` | string \| null | Entry-source hash, duplicated from the source table for convenience. |

### `sources`

| Field | Types | Meaning |
|---|---|---|
| `id` | integer | Source id used by functions, sites and locals. `0` is the entry file. |
| `path` | string | Physical path. |
| `sha256` | string \| null | Source hash at compile time. |
| `lines` | integer \| null | Number of source lines, or null when unreadable. |
| `entry` | boolean | True only for the entry file. |

### `functions`

| Field | Types | Meaning |
|---|---|---|
| `id` | integer | Function id used by sites/locals. |
| `name` | string | PHP-level name (`add`, `Foo::bar`, `main`, or an include variant). |
| `symbol` | string | Emitted assembly entry symbol. |
| `source_id` | integer \| null | Defining source, or null for synthetic bodies. |
| `kind` | string | `main`, `function`, `method`, `closure` or `generator`. |
| `asm_start` / `asm_end` | integer | 1-based assembly-line region. |
| `line_start` / `line_end` | integer \| null | Source-line range covered by sites. |
| `synthetic` | boolean | Compiler-generated body. |
| `flags` | array | `main`, `method`, `closure`, `generator`, `static`, `by_ref`. |
| `params` | array | Parameter name, PHP type, by-ref and variadic flags derived from named local slots. |

### `sites`

Each `@src` marker that carries `site=` becomes a runtime site. Codegen emits
one logical site per `(function, source_id, line)`: the guard stays in the block
where the line first appears, and repeated `@src` markers for the same line keep
their source-map mapping without a site id. The sidecar therefore lists exactly
the sites the generated guard can report.

| Field | Types | Meaning |
|---|---|---|
| `id` | integer | Compiler-assigned site id, matching the generated guard and the runtime counter. |
| `function_id` | integer \| null | Owning function, or null outside a region. |
| `source_id` | integer | Physical source id. |
| `line` / `column` | integer | 1-based source anchor. |
| `end_line` / `end_column` | integer \| null | Exclusive source end. |
| `op` / `origin` | string \| null | EIR opcode and optimizer provenance. |

### `locals`

Named PHP locals with a known frame offset. Hidden compiler temporaries are
omitted.

| Field | Types | Meaning |
|---|---|---|
| `function_id` | integer | Owning function. |
| `name` | string | PHP local or parameter name. |
| `frame_offset` | integer | Address is `frame_pointer - frame_offset`. |
| `php_type` | string | PHP type spelling (`int`, `array<mixed>`, `App\Item`, …). |
| `kind` | string | `php_local`, `static_local`, `ref_cell`, `return_ref_cell`. |
| `storage` | string | Runtime representation: `raw`, `ref_cell`, `dynamic`. |
| `state_offset` | integer \| null | Frame offset of the ref-cell representation state, when present. |
| `param` | integer \| null | Parameter index, or null for a non-parameter local. |
| `by_ref` / `variadic` | boolean | Parameter passing flags. |
| `sensitive` | boolean | Parameter carries PHP's `#[\SensitiveParameter]`; call capture redacts its value. |

### `branches`

One entry per source-level branch point: every leaf of an `if`/`elseif`/`while`/
`do-while`/`for`/ternary condition (including the leaves of `&&`, `||` and `!`),
and one per `switch`. Branch-edge guards share the runtime site-counter table with
line sites, but the metadata keeps them separate from `sites` so line coverage
does not count a branch edge as an executed line.

| Field | Types | Meaning |
|---|---|---|
| `source_id` | integer | Source owning the branch. |
| `line` / `column` | integer | Source position of the leaf condition or switch subject. |
| `kind` | string | `conditional` (two edges) or `switch` (one edge per case label plus default). |
| `edges` | array of integers | Debug site id per source edge, in source order. |
| `then_site` / `else_site` | integer | Conditional-only compatibility spelling of `edges[0]` / `edges[1]`. |

For a conditional, edge 0 is the leaf's true outcome and edge 1 its false outcome;
a negated leaf (`!$x`) still reports the operand's own truth edges, so counts are
independent of the surrounding `if` polarity. For a switch, edges are the `case`
labels in source order followed by the default edge. `test --coverage` sums each
edge's counter. LCOV emits `BRDA:<line>,<branch>,<edge>,<taken>`/`BRF`/`BRH`;
Cobertura emits a `branch="true"` line with `<condition>` children and counts
every edge of a multi-edge switch condition.

### `paths`

One entry per logical condition and switch. A logical path is one full reachable
short-circuit prefix combination; a switch path is one dispatch route. The `site`
field is a dedicated counter for a logical route, or the corresponding branch edge
counter for a switch route.

| Field | Types | Meaning |
|---|---|---|
| `function_id` | integer | Function owning the path group. |
| `source_id` | integer | Source owning the logical condition or switch. |
| `line` / `column` | integer | Source position of the condition or switch subject. |
| `kind` | string | `logical` or `switch`. |
| `truncated` | boolean | Whether the full-prefix plan hit the 128-route cap. Conditions with more than 31 leaves are omitted from this table. |
| `paths` | array | Routes in source order. Each entry is `{ "site": integer, "label": string }`. |

Logical labels list every evaluated leaf outcome followed by the terminal leaf
position, for example `#0=T,#2=T@23:24` (leaf 0 true, leaf 2 true, terminating at
leaf 2) or `#0=F,#1=F@23:16`. Leaf indices are zero-based in source order; a
missing index was short-circuited and never evaluated. Switch labels are `case0`,
`case1`, … followed by `default`. Two nested prefixes that end on the same leaf
edge have different labels and different counters, which is what distinguishes this
from terminal-edge path coverage.

### `classes`

One entry per emitted class. Objects start with the class id at offset `0`; instance
properties follow in 16-byte slots at `offset = 8 + index * 16`. The property `offset`
addresses the value word; the next word (`offset + 8`) carries the string length, the
tagged-scalar tag or the uninitialized typed-property sentinel.

| Field | Types | Meaning |
|---|---|---|
| `id` | integer | Class id stored in every instance header. |
| `name` | string | PHP-visible class name, including its namespace. |
| `parent` | string | Parent class name, or empty for a root declaration. |
| `abstract` | boolean | Whether the declaration is abstract. |
| `dynamic_offset` | integer \| null | Byte offset of the per-object dynamic-property hash, or null when the layout does not reserve one. `stdClass` uses offset `8`; a user class with `N` declared properties uses `8 + N * 16`. |
| `debug_info` | object | `{ "kind": "none" \| "projection" \| "runtime", "entries": [...] }`. A `projection` lists safe `__debugInfo()` entries; `runtime` means the body computes values and the debugger must not invoke it. |
| `property_hooks` | object | Hooked properties keyed by PHP-visible name; omitted when the class declares no hooks. Each entry is `{ "get": bool, "set": bool, "get_by_ref": bool, "virtual": bool }`. `virtual` means the property has hooks but no backing slot anywhere in its ancestry. |
| `properties` | array | Instance properties in layout order. Empty for internal/custom-layout classes, which the debugger shows by name only. |

Each property has `name`, `offset`, `php_type`, `visibility`
(`public`/`protected`/`private`), `by_ref` (slot holds a reference-cell pointer) and
`declared` (typed slot that can be uninitialized). The DAP adapter renders private and
protected properties with a `(private)` / `(protected)` name suffix. Dynamic properties
are read from the hash at `dynamic_offset` (standard hash layout, boxed `Mixed`
values) and named by their key, without the `["key"]` array wrapper.

A `debug_info.entries` element is either a live property read or a safe compile-time
literal:

```json
{"key": "id", "property": "id"}
{"key": "version", "value": "string(5) \"1.0.0\"", "php_type": "string"}
{"key": "flags", "value": "array(3) {[0] => int(1), [1] => int(2), [2] => int(3)}", "php_type": "array"}
```

A property entry reads the declared slot; a literal entry is display-only. The compiler
accepts only a single `return [...]` whose values are direct `$this->property` reads,
scalar literals, negation of a numeric literal, or literal-only nested arrays. The
projection is bounded at three array levels, 16 children per array and 256 string bytes;
anything larger or any computed expression falls back to `runtime`. The declaration is
resolved through its implementation owner, so a class inheriting `__debugInfo()` keeps
the parent projection while property reads still use the child object layout. A
`runtime` method is never called by the debugger: the object shows
`[__debugInfo() runtime]` and its declared/dynamic properties instead.

A `property_hooks` entry is emitted for every property declaring at least one accessor.
The debugger never invokes those accessors: a backed getter's `offset` still addresses
its raw backing value, while a `virtual` property's offset is not readable storage and
the DAP view shows a marker instead. A virtual property is not a valid data-breakpoint
target.

## Embedded table

The same JSON is appended to the generated assembly in a read-only section and
terminated by a NUL:

```asm
.globl elephc_debug_metadata
elephc_debug_metadata:
    .ascii "{…}"
    .byte 0
.globl elephc_debug_metadata_len
elephc_debug_metadata_len:
    .quad 43632
```

macOS prefixes C symbols with `_`. `--debug` marks the metadata symbol as a
linker root (`-Wl,-u` on Linux, `-u` on macOS), so `--gc-sections` /
`-dead_strip` cannot discard it even though no generated code references it.

The sidecar is a projection of the same JSON; a runtime agent can read the
embedded table without looking for the sidecar file.

## Coverage

`elephc test --coverage` compiles the suite with `--debug`, runs it with
`ELEPHC_DEBUG_SITES=1`, and joins the runtime's `elephc-debug-site: <id> <count>`
lines with this metadata's `sites` table. A site is covered when its count is
non-zero; a line with several split-function sites is covered when any of them ran.
The command runs the suite in one process because site counters are process-local,
and prints covered/total lines per source plus the uncovered line numbers. Filtered-out
tests are not compiled and therefore are absent from the denominator.

`--coverage-format text|lcov|cobertura|html` and `--coverage-out FILE` export the
same line/branch data as a text summary, LCOV records, Cobertura XML or a
self-contained HTML page. Repeatable `--coverage-filter PATH` and
`--coverage-exclude PATH` roots select report sources; excluded sources and
their branches are removed before totals are computed. Logical-condition leaves and switch cases are source
edges in the `branches` table; full condition/path coverage is separate work
tracked outside this schema.

## Stability

- `format` plus integer `version` identify the schema. Consumers reject a
  different format and treat a higher version as a new schema.
- Within version 1, fields may be added but not removed or renamed. Consumers
  ignore unknown fields.
- Source ids are compile-local, not stable across rebuilds.
- The metadata is produced by `--debug`; `--debug-info` remains the DWARF-only
  artifact.
