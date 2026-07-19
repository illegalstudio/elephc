# Source Maps v2 — Design

Roadmap item: "Source maps v2 — richer mappings for functions / expressions / labels
and a more stable machine-readable schema for external tooling".

## Goals

1. **Function mappings** — external tools can find which assembly range implements
   which PHP function/method/closure, and its entry symbol.
2. **Expression mappings** — each instruction-level mapping carries the EIR opcode
   that produced the assembly, so tools can distinguish e.g. a call from a store on
   the same source line.
3. **Label mappings** — assembly labels (block labels, epilogues) are listed with
   their line and owning function, so disassembly/profiler tooling can anchor on them.
4. **Stable schema** — a versioned, documented JSON contract (`format` +
   integer `version`) that external tooling can rely on; documented in
   `docs/compiling/source-maps.md`.

## Assembly markers (codegen → map generator contract)

Markers are ordinary line comments (target comment prefix agnostic; parsed by
substring, as in v1):

- `@fn name=<php_name> symbol=<entry_symbol>` — emitted immediately before a
  function's prologue. Emitted for user functions, closures, class methods,
  generator functions (covers constructor + body emission), and `main`
  (or the web handler under `--web`).
- `@endfn name=<php_name>` — emitted immediately after the function's epilogue.
- `@src line=<L> col=<C> op=<opcode>` — the v1 marker extended with the EIR
  opcode spelling (`Op::name()`). The parser accepts the marker with or without
  `op=` so older assembly remains parseable.

Labels need no new markers: the generator recognizes assembly label lines
(`<ident>:` at column 0) that fall inside a `@fn`..`@endfn` range. Labels outside
function ranges (data section, runtime glue) are intentionally excluded.

## v2 JSON schema

```json
{
  "format": "elephc-source-map",
  "version": 2,
  "source": "hello.php",
  "asm": "hello.s",
  "functions": [
    {"name": "main", "symbol": "_main", "asm_start": 12, "asm_end": 90}
  ],
  "labels": [
    {"name": "_php_foo_epilogue", "asm_line": 55, "function": 1}
  ],
  "mappings": [
    {"asm_line": 14, "php_line": 3, "php_col": 5, "op": "store_local", "function": 0}
  ]
}
```

- All `asm_line` / `asm_start` / `asm_end` values are 1-based lines in the `.s` file.
- `function` is an index into `functions`, or `null` for mappings outside any
  marked function.
- `op` is the EIR opcode spelling, or `null` when the marker carries none.
- `asm_start` is the `@fn` marker line; `asm_end` is the `@endfn` marker line
  (or the last assembly line if the end marker is missing).
- Schema evolution rule (documented contract): within `version` 2, fields are
  only added, never removed or renamed; consumers must ignore unknown fields.

This replaces the v1 output (`elephc-source-map-v1`, flat `entries` array).
The CLI flag stays `--source-map`.

## Implementation

- `src/codegen/block_emit.rs` — emit `@fn`/`@endfn` in `emit_user_function`,
  `emit_class_method` (both including their generator paths), and
  `emit_main_function`; add `op=` to the `@src` marker.
- `src/source_map.rs` — rewrite generation to the v2 schema; keep local JSON
  escaping; unit tests over synthetic assembly text.
- `src/pipeline.rs` — pass the asm output path through for the `asm` field.
- `tests/codegen/cli.rs` — update the `--source-map` CLI test for v2.
- Docs: new `docs/compiling/source-maps.md` (schema contract), update
  `docs/compiling/output-and-diagnostics.md`, CHANGELOG, ROADMAP checkbox.

## Extensions (second iteration — "implementa tutto")

1. **Expression end spans** — `Span` gained exclusive `end_line`/`end_col`; the
   lexer records token extents, the parser widens binary/assignment/call spans
   through their last token (start stays anchored → diagnostics unchanged).
   Markers carry `end=<EL>:<EC>`; mappings expose `php_end_line`/`php_end_col`.
2. **Inverse index** — new top-level `lines` array: per PHP line, merged
   1-based inclusive `[start, end]` assembly ranges (breakpoint-style lookup).
3. **`--debug-info` (DWARF)** — post-codegen injection reusing the markers:
   `.file`/`.loc` line table plus hand-encoded `.debug_abbrev`/`.debug_info`
   (DWARF32 v4 CU + one `DW_TAG_subprogram` per `@fn` region). The CU MUST
   carry `DW_AT_comp_dir` or ld64 emits no debug map (N_OSO) and lldb sees
   nothing. macOS: `dsymutil` bakes a `.dSYM` after linking (object kept as
   fallback on failure); Linux: line tables link into the binary.
4. **Optimization provenance** — `Instruction.origin` set by const-fold
   (`convert_to_const`) and LICM (`move_instruction`); markers carry
   `origin=<pass>`; mappings expose it; the EIR printer shows `; origin: p`.
5. **Synthetic flag** — `FunctionFlags.is_synthetic` set for propinit thunks
   and builtin (Reflection/SPL/DateTime) method lowering; `@fn` markers carry
   `synthetic=1`; functions expose `"synthetic": bool`.
6. **Source checksum** — `source_sha256` (local FIPS 180-4 implementation, no
   new dependency) for staleness detection.
7. **Block labels** — `@block name=<eir block>` markers before each block
   label; label entries expose `"block"`.
