---
title: "Source maps"
description: "The version 2 JSON source-map schema emitted by --source-map for debugger, profiler, and disassembly tooling."
sidebar:
  order: 7
---

`elephc --source-map file.php` writes a `file.map` sidecar next to the generated
assembly, mapping the assembly back to PHP source. The map is a JSON document
with a versioned, machine-readable schema intended for external tooling
(debuggers, profilers, disassembly viewers).

```bash
elephc --emit-asm --source-map hello.php
# writes hello.s and hello.map
```

For standard debugger/profiler integration without custom tooling, see
[`--debug-info`](output-and-diagnostics.md#--debug-info), which embeds DWARF
line directives in the assembly itself; the two flags compose.

## Schema (version 2)

```json
{
  "format": "elephc-source-map",
  "version": 2,
  "source": "hello.php",
  "source_sha256": "86e64f19…",
  "asm": "hello.s",
  "functions": [
    {"name": "add", "symbol": "_fn_add", "asm_start": 1, "asm_end": 105, "synthetic": false},
    {"name": "main", "symbol": "_main", "asm_start": 4985, "asm_end": 5162, "synthetic": false}
  ],
  "labels": [
    {"name": "_fn_add", "asm_line": 6, "function": 0, "block": null},
    {"name": "_eir_add_entry_0", "asm_line": 23, "function": 0, "block": "entry"}
  ],
  "mappings": [
    {"asm_line": 47, "php_line": 4, "php_col": 15, "php_end_line": 4, "php_end_col": 19,
     "op": "ichecked_add", "origin": null, "function": 0}
  ],
  "lines": [
    {"php_line": 4, "asm_ranges": [[34, 105]]}
  ]
}
```

### Envelope

| Field | Type | Meaning |
|-------|------|---------|
| `format` | string | Always `"elephc-source-map"`. |
| `version` | integer | Schema version; currently `2`. |
| `source` | string | Path of the compiled PHP file, as passed to the CLI. |
| `source_sha256` | string \| null | Lowercase-hex SHA-256 of the PHP file at compile time, so tools can detect a stale map; `null` if the file could not be re-read. |
| `asm` | string | Path of the assembly file the map describes. |

### `functions`

One entry per emitted function body, in assembly order.

| Field | Type | Meaning |
|-------|------|---------|
| `name` | string | PHP-level name (`add`, `Foo::bar`, `main`). |
| `symbol` | string | Assembly entry symbol (`_fn_add`, `_method_Foo_bar`, `_main`). |
| `asm_start` / `asm_end` | integer | 1-based inclusive line range in the `.s` file. |
| `synthetic` | boolean | `true` for compiler-generated bodies with no user-written PHP source: property-default init thunks and builtin class methods (Reflection, SPL, DateTime) linked into the user assembly. Tools that only care about user code can filter on this. |

### `labels`

Assembly label definitions that fall inside a function range — entry labels,
basic-block labels, and epilogue labels. Labels outside function ranges
(data section, runtime glue) are not listed.

| Field | Type | Meaning |
|-------|------|---------|
| `name` | string | Label name without the trailing `:`. |
| `asm_line` | integer | 1-based line of the label definition. |
| `function` | integer | Index into `functions`. |
| `block` | string \| null | EIR basic-block name (`entry`, `loop_body`, …) when the label starts a block; `null` for entry/epilogue symbols. |

### `mappings`

Instruction-level mappings. Each entry describes the assembly emitted for one
EIR instruction that carries a real PHP source span; the mapping line is the
marker comment line, and the instruction's assembly follows it.

| Field | Type | Meaning |
|-------|------|---------|
| `asm_line` | integer | 1-based line of the mapping marker in the `.s` file. |
| `php_line` / `php_col` | integer | 1-based PHP source position (the expression's anchor: its operator or first token). |
| `php_end_line` / `php_end_col` | integer \| null | Exclusive end of the mapped expression (the character after it). `null` when the extent is unknown. |
| `op` | string \| null | EIR opcode spelling (e.g. `ichecked_add`, `call`), or `null` when unknown. |
| `origin` | string \| null | Optimization-pass provenance: `"const_fold"` when the instruction was rewritten to a constant, `"licm"` when it was hoisted out of a loop. `null` for instructions lowered directly from the source. |
| `function` | integer \| null | Index into `functions`, or `null` outside any function. |

### `lines`

The inverse index for breakpoint-style lookups: for each PHP line with at least
one mapping, the assembly line ranges implementing it, sorted by `php_line`.
A mapping's range extends to the next mapping in the same function (or the
function's end); overlapping and adjacent ranges are merged. A PHP line inlined
into several functions gets one range per copy.

| Field | Type | Meaning |
|-------|------|---------|
| `php_line` | integer | 1-based PHP source line. |
| `asm_ranges` | array of `[start, end]` | 1-based inclusive assembly line ranges. |

### Stability contract

- `format` plus integer `version` identify the schema. Consumers should reject
  documents whose `format` differs and treat a higher `version` as a new schema.
- Within version 2, fields are only added, never removed or renamed. Consumers
  must ignore unknown fields.
- All `asm_line` / `asm_start` / `asm_end` values are 1-based lines in the file
  named by `asm`.

## Assembly markers

The map is derived from comment markers codegen leaves in the assembly, so the
`.s` file is self-describing too. Marker values are space-separated `key=value`
tokens:

- `@fn name=<php_name> symbol=<entry_symbol> [synthetic=1]` …
  `@endfn name=<php_name>` bracket each emitted function.
- `@block name=<eir_block>` names the EIR basic block of the next label line.
- `@src line=<L> col=<C> [end=<EL>:<EC>] [op=<opcode>] [origin=<pass>]`
  precedes the assembly of one EIR instruction.
